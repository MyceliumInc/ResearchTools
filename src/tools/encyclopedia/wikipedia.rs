use super::Item;
use crate::http::{get_json, BOT_UA, TIMEOUT_FAST_MS};
use crate::text::strip_tags;
use worker::*;

pub async fn search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://en.wikipedia.org/w/rest.php/v1/search/page?q={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_FAST_MS).await {
        Ok(value) => value,
        Err(error) => {
            console_log!("wikipedia search failed: {}", error);
            return vec![];
        }
    };
    let mut out = Vec::new();
    if let Some(pages) = json.get("pages").and_then(|value| value.as_array()) {
        for page in pages {
            let key = page.get("key").and_then(|value| value.as_str()).unwrap_or("").trim();
            let title = page.get("title").and_then(|value| value.as_str()).unwrap_or("").trim();
            if key.is_empty() || title.is_empty() {
                continue;
            }
            let description = page
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            let excerpt = strip_tags(
                page.get("excerpt")
                    .and_then(|value| value.as_str())
                    .unwrap_or(""),
            );
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
