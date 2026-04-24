use crate::util::{cache_or, get_text, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    timespan: Option<String>,
}

#[derive(Serialize)]
struct Item {
    title: String,
    url: String,
    domain: String,
    seen_date: String,
    language: String,
    source_country: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "gdelt_search", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(10).clamp(1, 75);
    let timespan = body
        .timespan
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("24h");

    let url = format!(
        "https://api.gdeltproject.org/api/v2/doc/doc?query={}&mode=ArtList&format=json&maxrecords={}&timespan={}&sort=DateDesc",
        urlencoding::encode(trimmed),
        limit,
        urlencoding::encode(timespan)
    );

    let text = match get_text(&url, BOT_UA, &[], TIMEOUT_DEFAULT_MS).await {
        Ok(t) => t,
        Err(e) => return Err(Error::RustError(format!("GDELT search failed: {}", e))),
    };

    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(serde_json::to_vec(&Resp { results: vec![] })?),
    };

    let mut out = Vec::new();
    if let Some(arr) = json.get("articles").and_then(|v| v.as_array()) {
        for a in arr {
            let title = a
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let url = a
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let domain = a
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let seen_date = a
                .get("seendate")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let language = a
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let source_country = a
                .get("sourcecountry")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(Item {
                title,
                url,
                domain,
                seen_date,
                language,
                source_country,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(serde_json::to_vec(&Resp { results: out })?)
}
