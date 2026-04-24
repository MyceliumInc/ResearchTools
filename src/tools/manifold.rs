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
    id: String,
    question: String,
    url: String,
    probability_pct: Option<i32>,
    outcome_type: String,
    is_resolved: bool,
    resolution: Option<String>,
    close_time: Option<String>,
    volume: f64,
    unique_bettors: i64,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "manifold_search", 60, execute).await
}

fn ms_to_iso(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    let date = Date::from(DateInit::Millis(ms as u64));
    Some(date.to_string())
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 25);
    let url = format!(
        "https://api.manifold.markets/v0/search-markets?term={}&limit={}",
        urlencoding::encode(trimmed),
        limit
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("Manifold search failed: {}", e)))?;

    let mut out = Vec::new();
    if let Some(arr) = json.as_array() {
        for m in arr {
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let question = m
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() || question.is_empty() {
                continue;
            }
            let url = m
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let outcome_type = m
                .get("outcomeType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_resolved = m
                .get("isResolved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let resolution = m
                .get("resolution")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let probability_pct =
                if outcome_type == "BINARY" && !is_resolved {
                    m.get("probability")
                        .and_then(|v| v.as_f64())
                        .map(|p| (p * 100.0).round().clamp(0.0, 100.0) as i32)
                } else {
                    None
                };
            let close_time = m
                .get("closeTime")
                .and_then(|v| v.as_i64())
                .and_then(ms_to_iso);
            let volume = m.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let unique_bettors = m
                .get("uniqueBettorCount")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            out.push(Item {
                id,
                question,
                url,
                probability_pct,
                outcome_type,
                is_resolved,
                resolution,
                close_time,
                volume,
                unique_bettors,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(serde_json::to_vec(&Resp { results: out })?)
}
