use crate::http::{build_request, send_request_timed};
use crate::text::truncate;
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
        Ok(encoded) => encoded,
        Err(error) => {
            console_log!("[telemetry] payload encode failed: {}", error);
            return;
        }
    };

    let bearer = sink.auth.as_ref().map(|auth| format!("Bearer {}", auth));
    let mut headers: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
    if let (Some(bearer), Some(auth)) = (&bearer, &sink.auth) {
        headers.push(("Authorization", bearer));
        headers.push(("apikey", auth));
    }

    let request = match build_request(&sink.url, Method::Post, &headers, Some(body)) {
        Ok(request) => request,
        Err(error) => {
            console_log!("[telemetry] request build failed: {}", error);
            return;
        }
    };

    match send_request_timed(request, TIMEOUT_MS).await {
        Ok(mut response) => {
            let status = response.status_code();
            if status >= 300 {
                let detail = response.text().await.unwrap_or_default();
                console_log!(
                    "[telemetry] record {} returned {} body={}",
                    outcome.tool,
                    status,
                    truncate(&detail, 200)
                );
            }
        }
        Err(error) => {
            console_log!("[telemetry] record {} failed: {}", outcome.tool, error);
        }
    }
}
