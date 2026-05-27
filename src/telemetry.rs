use serde::Serialize;
use worker::*;

const ERROR_MAX_CHARS: usize = 300;
const TIMEOUT_MS: u64 = 4_000;

#[derive(Serialize)]
struct Payload<'a> {
    tool: &'a str,
    ms: i32,
    status: i32,
    error: &'a str,
}

pub struct Outcome {
    pub tool: String,
    pub ms: u32,
    pub status: u16,
    pub error: Option<String>,
}

pub struct Sink {
    pub url: String,
    pub auth: Option<String>,
}

pub fn sink(env: &Env) -> Option<Sink> {
    let url = env.var("TELEMETRY_URL").ok()?.to_string();
    if url.is_empty() {
        return None;
    }
    let auth = env
        .var("TELEMETRY_AUTH")
        .ok()
        .map(|v| v.to_string())
        .filter(|s| !s.is_empty());
    Some(Sink { url, auth })
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

pub async fn record(sink: Sink, outcome: Outcome) {
    let body = match serde_json::to_string(&Payload {
        tool: &outcome.tool,
        ms: outcome.ms as i32,
        status: outcome.status as i32,
        error: outcome.error.as_deref().unwrap_or(""),
    }) {
        Ok(s) => s,
        Err(e) => {
            console_log!("[telemetry] payload encode failed: {}", e);
            return;
        }
    };

    let headers = Headers::new();
    if headers.set("Content-Type", "application/json").is_err() {
        console_log!("[telemetry] header build failed");
        return;
    }
    if let Some(auth) = &sink.auth {
        let _ = headers.set("Authorization", &format!("Bearer {}", auth));
        let _ = headers.set("apikey", auth);
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(wasm_bindgen::JsValue::from_str(&body)));

    let request = match Request::new_with_init(&sink.url, &init) {
        Ok(r) => r,
        Err(e) => {
            console_log!("[telemetry] request build failed: {}", e);
            return;
        }
    };

    match crate::util::send_request_timed(request, TIMEOUT_MS).await {
        Ok(mut resp) => {
            let status = resp.status_code();
            if status >= 300 {
                let detail = resp.text().await.unwrap_or_default();
                console_log!(
                    "[telemetry] record {} returned {} body={}",
                    outcome.tool,
                    status,
                    truncate(&detail, 200)
                );
            }
        }
        Err(e) => {
            console_log!("[telemetry] record {} failed: {}", outcome.tool, e);
        }
    }
}
