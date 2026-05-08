use serde::Serialize;
use worker::*;

const ERROR_MAX_CHARS: usize = 300;
const TIMEOUT_MS: u64 = 4_000;

#[derive(Serialize)]
struct Payload<'a> {
    p_tool: &'a str,
    p_ms: i32,
    p_status: i32,
    p_error: &'a str,
}

pub struct Outcome {
    pub tool: String,
    pub ms: u32,
    pub status: u16,
    pub error: Option<String>,
}

pub fn supabase_config(env: &Env) -> Option<(String, String)> {
    let url = env.var("SUPABASE_URL").ok()?.to_string();
    let key = env.var("SUPABASE_PUBLISHABLE_KEY").ok()?.to_string();
    if url.is_empty() || key.is_empty() {
        return None;
    }
    Some((url, key))
}

pub fn detect_soft_error(bytes: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let msg = value.get("error")?.as_str()?;
    if msg.is_empty() {
        return None;
    }
    Some(truncate(msg, ERROR_MAX_CHARS))
}

pub fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn tool_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/v1/")?;
    let slug = rest.split('/').next()?;
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}

pub async fn record(url: String, key: String, outcome: Outcome) {
    let body = match serde_json::to_string(&Payload {
        p_tool: &outcome.tool,
        p_ms: outcome.ms as i32,
        p_status: outcome.status as i32,
        p_error: outcome.error.as_deref().unwrap_or(""),
    }) {
        Ok(s) => s,
        Err(e) => {
            console_log!("[uptime] payload encode failed: {}", e);
            return;
        }
    };

    let endpoint = format!("{}/rest/v1/rpc/uptime_record", url.trim_end_matches('/'));
    let headers = Headers::new();
    if headers.set("Content-Type", "application/json").is_err()
        || headers.set("apikey", &key).is_err()
        || headers
            .set("Authorization", &format!("Bearer {}", key))
            .is_err()
    {
        console_log!("[uptime] header build failed");
        return;
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(wasm_bindgen::JsValue::from_str(&body)));

    let request = match Request::new_with_init(&endpoint, &init) {
        Ok(r) => r,
        Err(e) => {
            console_log!("[uptime] request build failed: {}", e);
            return;
        }
    };

    match crate::util::send_request_timed(request, TIMEOUT_MS).await {
        Ok(mut resp) => {
            let status = resp.status_code();
            if status >= 300 {
                let detail = resp.text().await.unwrap_or_default();
                console_log!(
                    "[uptime] record {} returned {} body={}",
                    outcome.tool,
                    status,
                    truncate(&detail, 200)
                );
            }
        }
        Err(e) => {
            console_log!("[uptime] record {} failed: {}", outcome.tool, e);
        }
    }
}
