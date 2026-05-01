use crate::util::{cache_or, error_response, get_text, strip_tags, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct WebResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<WebResult>,
}

pub async fn run(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let key = match ctx.secret("BRAVE_API_KEY") {
        Ok(s) => s.to_string(),
        Err(_) => return error_response("Search failed: BRAVE_API_KEY not configured"),
    };
    cache_or(req, "search_web", 300, move |body| execute(body, key)).await
}

async fn execute(raw: Vec<u8>, key: String) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Err(Error::RustError("Search failed: empty query".into()));
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 20);

    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}&text_decorations=false&safesearch=moderate&country=US",
        urlencoding::encode(trimmed),
        limit,
    );

    let text = get_text(
        &url,
        BOT_UA,
        &[
            ("Accept", "application/json"),
            ("X-Subscription-Token", &key),
        ],
        TIMEOUT_DEFAULT_MS,
    )
    .await
    .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| Error::RustError(format!("Search failed: bad json: {}", e)))?;

    let mut results: Vec<WebResult> = Vec::new();
    if let Some(arr) = json.pointer("/web/results").and_then(|v| v.as_array()) {
        for item in arr.iter().take(limit) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .map(strip_tags)
                .unwrap_or_default();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(strip_tags)
                .unwrap_or_default();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            results.push(WebResult { title, url, snippet });
        }
    }

    Ok(serde_json::to_vec(&Resp { results })?)
}
