use crate::http::error_response;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use worker::*;

pub async fn cache_or<Handler, Fut>(
    mut req: Request,
    tool: &str,
    ttl_seconds: u32,
    handler: Handler,
) -> Result<Response>
where
    Handler: FnOnce(Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>>>,
{
    let body = req.bytes().await.unwrap_or_default();
    if let Some(hit) = lookup(tool, &body).await {
        return Ok(hit);
    }
    match handler(body.clone()).await {
        Ok(payload) => store(tool, &body, ttl_seconds, &payload).await,
        Err(error) => error_response(error.to_string()),
    }
}

fn key(tool: &str, body: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    format!(
        "https://research-tools-cache.internal/{}?h={:016x}",
        tool,
        hasher.finish()
    )
}

async fn lookup(tool: &str, body: &[u8]) -> Option<Response> {
    Cache::default().get(&key(tool, body), true).await.ok().flatten()
}

async fn store(tool: &str, body: &[u8], ttl_seconds: u32, payload: &[u8]) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Cache-Control", &format!("public, max-age={}", ttl_seconds))?;
    let cached = Response::from_bytes(payload.to_vec())?.with_headers(headers.clone());
    let _ = Cache::default().put(&key(tool, body), cached).await;
    Response::from_bytes(payload.to_vec()).map(|response| response.with_headers(headers))
}
