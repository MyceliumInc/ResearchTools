use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct Item {
    slug: String,
    title: String,
    snippet: String,
    url: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "grokipedia_search", 300, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(5).clamp(1, 25);
    let url = format!(
        "https://grokipedia.com/api/typeahead?query={}&limit={}",
        urlencoding::encode(trimmed),
        limit
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_FAST_MS)
        .await
        .map_err(|e| Error::RustError(format!("Grokipedia search failed: {}", e)))?;

    let mut out = Vec::new();
    if let Some(arr) = json.get("results").and_then(|v| v.as_array()) {
        for r in arr {
            let slug = r.get("slug").and_then(|v| v.as_str()).unwrap_or("").trim();
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
            if slug.is_empty() || title.is_empty() {
                continue;
            }
            let snippet = r
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            out.push(Item {
                slug: slug.to_string(),
                title: title.to_string(),
                snippet,
                url: format!("https://grokipedia.com/page/{}", slug),
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(serde_json::to_vec(&Resp { results: out })?)
}
