use futures::future::{select, Either};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use worker::*;

pub const BOT_UA: &str = "MyceliumBot/1.0";

pub const TIMEOUT_FAST_MS: u64 = 6_000;
pub const TIMEOUT_DEFAULT_MS: u64 = 10_000;
pub const TIMEOUT_SLOW_MS: u64 = 20_000;

fn body_hash(body: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    h.finish()
}

fn cache_key(tool: &str, body: &[u8]) -> String {
    format!("https://tools-cache.internal/{}?h={:016x}", tool, body_hash(body))
}

async fn cache_lookup(tool: &str, body: &[u8]) -> Option<Response> {
    let key = cache_key(tool, body);
    Cache::default().get(&key, true).await.ok().flatten()
}

async fn cache_store(tool: &str, body: &[u8], ttl_s: u32, payload: &[u8]) -> Result<Response> {
    let key = cache_key(tool, body);
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Cache-Control", &format!("public, max-age={}", ttl_s))?;
    let to_cache = Response::from_bytes(payload.to_vec())?.with_headers(headers.clone());
    let _ = Cache::default().put(&key, to_cache).await;
    Response::from_bytes(payload.to_vec()).map(|r| r.with_headers(headers))
}

pub async fn cache_or<F, Fut>(
    mut req: Request,
    tool: &str,
    ttl_s: u32,
    handler: F,
) -> Result<Response>
where
    F: FnOnce(Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>>>,
{
    let body = req.bytes().await.unwrap_or_default();
    if let Some(hit) = cache_lookup(tool, &body).await {
        return Ok(hit);
    }
    match handler(body.clone()).await {
        Ok(payload) => cache_store(tool, &body, ttl_s, &payload).await,
        Err(e) => error_response(e.to_string()),
    }
}

pub fn error_response(message: impl Into<String>) -> Result<Response> {
    #[derive(Serialize)]
    struct E {
        error: String,
    }
    Response::from_json(&E {
        error: message.into(),
    })
}

async fn with_timeout<F, T>(fut: F, ms: u64) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let delay = Delay::from(Duration::from_millis(ms));
    match select(Box::pin(fut), Box::pin(delay)).await {
        Either::Left((res, _)) => res,
        Either::Right(_) => Err(Error::RustError(format!("upstream timeout after {}ms", ms))),
    }
}

async fn send_with_timeout(req: Request, ms: u64) -> Result<Response> {
    with_timeout(Fetch::Request(req).send(), ms).await
}

pub async fn get_text(
    url: &str,
    user_agent: &str,
    extra: &[(&str, &str)],
    timeout_ms: u64,
) -> Result<String> {
    let headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    for (k, v) in extra {
        headers.set(k, v)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = send_with_timeout(request, timeout_ms).await?;
    if resp.status_code() >= 400 {
        return Err(Error::RustError(format!("HTTP {}", resp.status_code())));
    }
    with_timeout(resp.text(), timeout_ms).await
}

pub async fn get_json(
    url: &str,
    user_agent: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value> {
    let text = get_text(url, user_agent, &[("Accept", "application/json")], timeout_ms).await?;
    serde_json::from_str(&text).map_err(|e| Error::RustError(format!("bad json: {}", e)))
}

pub async fn get_json_status(
    url: &str,
    user_agent: &str,
    timeout_ms: u64,
) -> Result<(u16, Option<serde_json::Value>)> {
    let headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    headers.set("Accept", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = send_with_timeout(request, timeout_ms).await?;
    let status = resp.status_code();
    if status >= 400 {
        return Ok((status, None));
    }
    let text = with_timeout(resp.text(), timeout_ms).await?;
    let v = serde_json::from_str(&text)
        .map_err(|e| Error::RustError(format!("bad json: {}", e)))?;
    Ok((status, Some(v)))
}

pub async fn send_request_timed(req: Request, timeout_ms: u64) -> Result<Response> {
    send_with_timeout(req, timeout_ms).await
}

pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn strip_html_doc(s: &str) -> String {
    let cleaned = strip_block(s, "script");
    let cleaned = strip_block(&cleaned, "style");
    strip_tags(&cleaned)
}

fn strip_block(s: &str, tag: &str) -> String {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        match lower[i..].find(&open) {
            Some(rel_start) => {
                let start = i + rel_start;
                out.push_str(&s[i..start]);
                match lower[start..].find(&close) {
                    Some(rel_end) => {
                        i = start + rel_end + close.len();
                    }
                    None => break,
                }
            }
            None => {
                out.push_str(&s[i..]);
                break;
            }
        }
    }
    out
}

