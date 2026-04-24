use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
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
    id: i64,
    title: String,
    url: String,
    status: Option<String>,
    probability_pct: Option<i32>,
    question_type: Option<String>,
    published_at: Option<String>,
    close_time: Option<String>,
    resolution: Option<String>,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "metaculus_search", 300, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 20);
    let url = format!(
        "https://www.metaculus.com/api/posts/?search={}&limit={}&order_by=-hotness",
        urlencoding::encode(trimmed),
        limit
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("Metaculus search failed: {}", e)))?;

    let mut out = Vec::new();
    if let Some(arr) = json.get("results").and_then(|v| v.as_array()) {
        for p in arr {
            let id = p.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let title = p
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id == 0 || title.is_empty() {
                continue;
            }
            let url_title = p
                .get("url_title")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let url = if url_title.is_empty() {
                format!("https://www.metaculus.com/questions/{}/", id)
            } else {
                format!("https://www.metaculus.com/questions/{}/{}/", id, url_title)
            };
            let status = p
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let published_at = p
                .get("published_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let close_time = p
                .get("scheduled_close_time")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let question = p.get("question");
            let question_type = question
                .and_then(|q| q.get("type"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let resolution = question
                .and_then(|q| q.get("resolution"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let probability_pct = if question_type.as_deref() == Some("binary") {
                question
                    .and_then(|q| q.get("aggregations"))
                    .and_then(|a| a.get("recency_weighted"))
                    .and_then(|r| r.get("latest"))
                    .and_then(|l| l.get("centers"))
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_f64())
                    .map(|p| (p * 100.0).round().clamp(0.0, 100.0) as i32)
            } else {
                None
            };

            out.push(Item {
                id,
                title,
                url,
                status,
                probability_pct,
                question_type,
                published_at,
                close_time,
                resolution,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(serde_json::to_vec(&Resp { results: out })?)
}
