use super::WebResult;
use crate::http::{get_typed, BOT_UA, TIMEOUT_FAST_MS};
use crate::text::strip_tags;
use serde::Deserialize;
use worker::*;

#[derive(Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
}

pub async fn search(query: &str, limit: usize) -> Result<Vec<WebResult>> {
    let url = format!(
        "https://api.marginalia.nu/public/search/{}",
        urlencoding::encode(query)
    );
    let parsed: SearchResponse = get_typed(&url, BOT_UA, TIMEOUT_FAST_MS)
        .await
        .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;

    let mut results: Vec<WebResult> = Vec::new();
    for entry in parsed.results.into_iter().take(limit) {
        let title = strip_tags(&entry.title);
        if entry.url.is_empty() || title.is_empty() {
            continue;
        }
        results.push(WebResult {
            title,
            url: entry.url,
            snippet: strip_tags(&entry.description),
        });
    }
    Ok(results)
}
