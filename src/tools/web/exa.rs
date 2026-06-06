use super::WebResult;
use crate::http::{build_request, send_request_timed, BOT_UA, TIMEOUT_DEFAULT_MS};
use crate::text::strip_tags;
use worker::*;

pub async fn search(query: &str, limit: usize, key: &str) -> Result<Vec<WebResult>> {
    let payload = serde_json::json!({
        "query": query,
        "type": "auto",
        "numResults": limit,
        "contents": { "highlights": true },
    });

    let headers = [
        ("User-Agent", BOT_UA),
        ("Accept", "application/json"),
        ("Content-Type", "application/json"),
        ("x-api-key", key),
    ];
    let request = build_request(
        "https://api.exa.ai/search",
        Method::Post,
        &headers,
        Some(payload.to_string()),
    )
    .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;

    let mut response = send_request_timed(request, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;
    if response.status_code() >= 400 {
        return Err(Error::RustError(format!(
            "Search failed: HTTP {}",
            response.status_code()
        )));
    }
    let text = response
        .text()
        .await
        .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| Error::RustError(format!("Search failed: bad json: {}", error)))?;

    let mut results: Vec<WebResult> = Vec::new();
    if let Some(items) = json.get("results").and_then(|value| value.as_array()) {
        for item in items.iter().take(limit) {
            let title = item
                .get("title")
                .and_then(|value| value.as_str())
                .map(strip_tags)
                .unwrap_or_default();
            let url = item
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = item
                .get("highlights")
                .and_then(|value| value.as_array())
                .map(|highlights| {
                    highlights
                        .iter()
                        .filter_map(|highlight| highlight.as_str())
                        .map(strip_tags)
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join(" … ")
                })
                .filter(|text| !text.is_empty())
                .or_else(|| {
                    item.get("summary")
                        .and_then(|value| value.as_str())
                        .map(strip_tags)
                })
                .unwrap_or_default();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            results.push(WebResult { title, url, snippet });
        }
    }
    Ok(results)
}
