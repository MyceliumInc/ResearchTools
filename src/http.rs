use futures::future::{select, Either};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use worker::*;

pub const BOT_UA: &str = "ResearchToolsBot/1.0";

pub const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub const TIMEOUT_FAST_MS: u64 = 6_000;
pub const TIMEOUT_DEFAULT_MS: u64 = 10_000;
pub const TIMEOUT_SLOW_MS: u64 = 20_000;

pub fn build_request(
    url: &str,
    method: Method,
    headers: &[(&str, &str)],
    body: Option<String>,
) -> Result<Request> {
    let header_map = Headers::new();
    for (name, value) in headers {
        header_map.set(name, value)?;
    }
    let mut init = RequestInit::new();
    init.with_method(method).with_headers(header_map);
    if let Some(content) = body {
        init.with_body(Some(content.into()));
    }
    Request::new_with_init(url, &init)
}

pub fn error_response(message: impl Into<String>) -> Result<Response> {
    #[derive(Serialize)]
    struct SoftError {
        error: String,
    }
    Response::from_json(&SoftError {
        error: message.into(),
    })
}

pub async fn send_request_timed(request: Request, timeout_ms: u64) -> Result<Response> {
    with_timeout(Fetch::Request(request).send(), timeout_ms).await
}

pub async fn get_text(
    url: &str,
    user_agent: &str,
    extra: &[(&str, &str)],
    timeout_ms: u64,
) -> Result<String> {
    let mut headers = vec![("User-Agent", user_agent)];
    headers.extend_from_slice(extra);
    let request = build_request(url, Method::Get, &headers, None)?;
    let mut response = send_request_timed(request, timeout_ms).await?;
    let status = response.status_code();
    if status >= 400 {
        return Err(Error::RustError(format!("HTTP {}", status)));
    }
    with_timeout(response.text(), timeout_ms).await
}

pub async fn get_json(
    url: &str,
    user_agent: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value> {
    let text = get_text(url, user_agent, &[("Accept", "application/json")], timeout_ms).await?;
    serde_json::from_str(&text).map_err(|error| Error::RustError(format!("bad json: {}", error)))
}

pub async fn get_typed<T: DeserializeOwned>(
    url: &str,
    user_agent: &str,
    timeout_ms: u64,
) -> Result<T> {
    let text = get_text(url, user_agent, &[("Accept", "application/json")], timeout_ms).await?;
    serde_json::from_str(&text).map_err(|error| Error::RustError(format!("bad json: {}", error)))
}

async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let delay = Delay::from(Duration::from_millis(timeout_ms));
    match select(Box::pin(future), Box::pin(delay)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err(Error::RustError(format!(
            "upstream timeout after {}ms",
            timeout_ms
        ))),
    }
}
