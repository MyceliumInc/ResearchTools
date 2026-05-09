use crate::util::{cache_or, get_json, strip_tags, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Clone)]
struct Item {
    source: String,
    title: String,
    snippet: String,
    url: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "encyclopedia_search", 300, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let query = body.query.trim().to_string();
    if query.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(5).clamp(1, 25);

    let (wiki, grok) = futures::join!(wiki_search(&query, limit), grok_search(&query, limit));

    let merged = interleave(wiki, grok, limit);
    Ok(serde_json::to_vec(&Resp { results: merged })?)
}

fn interleave(a: Vec<Item>, b: Vec<Item>, limit: usize) -> Vec<Item> {
    let mut out = Vec::with_capacity(limit);
    let mut ai = a.into_iter();
    let mut bi = b.into_iter();
    loop {
        let mut pushed = false;
        if let Some(x) = ai.next() {
            out.push(x);
            pushed = true;
            if out.len() >= limit {
                break;
            }
        }
        if let Some(y) = bi.next() {
            out.push(y);
            pushed = true;
            if out.len() >= limit {
                break;
            }
        }
        if !pushed {
            break;
        }
    }
    out
}

async fn wiki_search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://en.wikipedia.org/w/rest.php/v1/search/page?q={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_FAST_MS).await {
        Ok(v) => v,
        Err(e) => {
            console_log!("wiki_search failed: {}", e);
            return vec![];
        }
    };
    let mut out = Vec::new();
    if let Some(arr) = json.get("pages").and_then(|v| v.as_array()) {
        for p in arr {
            let key = p.get("key").and_then(|v| v.as_str()).unwrap_or("").trim();
            let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
            if key.is_empty() || title.is_empty() {
                continue;
            }
            let description = p
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let excerpt_raw = p
                .get("excerpt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let excerpt = strip_tags(excerpt_raw);
            let mut parts = Vec::new();
            if !description.is_empty() {
                parts.push(description.to_string());
            }
            if !excerpt.is_empty() {
                parts.push(excerpt);
            }
            out.push(Item {
                source: "wikipedia".to_string(),
                title: title.to_string(),
                snippet: parts.join(" — "),
                url: format!("https://en.wikipedia.org/wiki/{}", key),
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

async fn grok_search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://grokipedia.com/api/typeahead?query={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_FAST_MS).await {
        Ok(v) => v,
        Err(e) => {
            console_log!("grok_search failed: {}", e);
            return vec![];
        }
    };
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
                source: "grokipedia".to_string(),
                title: title.to_string(),
                snippet,
                url: format!("https://grokipedia.com/page/{}", slug),
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}
