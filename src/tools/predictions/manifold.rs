use super::Item;
use crate::http::{get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
use worker::*;

pub async fn search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://api.manifold.markets/v0/search-markets?term={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(value) => value,
        Err(error) => {
            console_log!("manifold search failed: {}", error);
            return vec![];
        }
    };
    let mut out = Vec::new();
    if let Some(markets) = json.as_array() {
        for market in markets {
            let question = market
                .get("question")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if question.is_empty() {
                continue;
            }
            let is_resolved = market
                .get("isResolved")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if is_resolved {
                continue;
            }
            let url = market
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let outcome_type = market
                .get("outcomeType")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let probability_pct = if outcome_type == "BINARY" {
                market
                    .get("probability")
                    .and_then(|value| value.as_f64())
                    .map(|probability| (probability * 100.0).round().clamp(0.0, 100.0))
            } else {
                None
            };
            let end_date = market
                .get("closeTime")
                .and_then(|value| value.as_i64())
                .and_then(millis_to_iso);
            let volume = market.get("volume").and_then(|value| value.as_f64()).unwrap_or(0.0);

            out.push(Item {
                source: "manifold".to_string(),
                question,
                url,
                probability_pct,
                end_date,
                volume,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

fn millis_to_iso(millis: i64) -> Option<String> {
    if millis <= 0 {
        return None;
    }
    Some(Date::from(DateInit::Millis(millis as u64)).to_string())
}
