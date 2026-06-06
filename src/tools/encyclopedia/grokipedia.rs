use super::Item;
use crate::http::{get_json, BOT_UA, TIMEOUT_FAST_MS};
use worker::*;

pub async fn search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://grokipedia.com/api/typeahead?query={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_FAST_MS).await {
        Ok(value) => value,
        Err(error) => {
            console_log!("grokipedia search failed: {}", error);
            return vec![];
        }
    };
    let mut out = Vec::new();
    if let Some(results) = json.get("results").and_then(|value| value.as_array()) {
        for result in results {
            let slug = result.get("slug").and_then(|value| value.as_str()).unwrap_or("").trim();
            let title = result.get("title").and_then(|value| value.as_str()).unwrap_or("").trim();
            if slug.is_empty() || title.is_empty() {
                continue;
            }
            let snippet = result
                .get("snippet")
                .and_then(|value| value.as_str())
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
