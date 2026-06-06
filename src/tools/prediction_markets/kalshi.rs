use super::Item;
use crate::http::{get_typed, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::Deserialize;
use worker::*;

#[derive(Deserialize)]
struct Market {
    #[serde(default)]
    ticker: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    yes_sub_title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    last_price_dollars: String,
    #[serde(default)]
    volume_fp: String,
    #[serde(default)]
    close_time: Option<String>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(default)]
    event_ticker: String,
    #[serde(default)]
    series_ticker: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    sub_title: String,
    #[serde(default)]
    mutually_exclusive: bool,
    #[serde(default)]
    markets: Vec<Market>,
}

#[derive(Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    events: Vec<Event>,
}

pub async fn search(query: &str, limit: usize) -> Vec<Item> {
    let url = "https://api.elections.kalshi.com/trade-api/v2/events?status=open&with_nested_markets=true&limit=200";
    let parsed: SearchResponse = match get_typed(url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(value) => value,
        Err(error) => {
            console_log!("kalshi search failed: {}", error);
            return vec![];
        }
    };
    let needle = query.to_lowercase();
    let mut out = Vec::new();
    'outer: for event in &parsed.events {
        let event_match = event.title.to_lowercase().contains(&needle)
            || event.sub_title.to_lowercase().contains(&needle)
            || event.event_ticker.to_lowercase().contains(&needle)
            || event.series_ticker.to_lowercase().contains(&needle);
        for market in &event.markets {
            if market.status != "active" {
                continue;
            }
            let market_match = market.title.to_lowercase().contains(&needle)
                || market.yes_sub_title.to_lowercase().contains(&needle)
                || market.ticker.to_lowercase().contains(&needle);
            if !event_match && !market_match {
                continue;
            }
            let question = if event.mutually_exclusive && !market.yes_sub_title.is_empty() {
                format!("{} — {}", event.title.trim(), market.yes_sub_title.trim())
            } else if !market.title.is_empty() {
                market.title.trim().to_string()
            } else {
                event.title.trim().to_string()
            };
            if question.is_empty() {
                continue;
            }
            let url = if !event.series_ticker.is_empty() && !event.event_ticker.is_empty() {
                format!(
                    "https://kalshi.com/markets/{}/{}",
                    event.series_ticker, event.event_ticker
                )
            } else {
                "https://kalshi.com/".to_string()
            };
            let last_price: f64 = market.last_price_dollars.parse().unwrap_or(0.0);
            let probability_pct = if last_price > 0.0 {
                Some((last_price * 100.0).clamp(0.0, 100.0))
            } else {
                None
            };
            let volume: f64 = market.volume_fp.parse().unwrap_or(0.0);
            out.push(Item {
                source: "kalshi".to_string(),
                question,
                url,
                probability_pct,
                end_date: market.close_time.clone(),
                volume,
            });
            if out.len() >= limit {
                break 'outer;
            }
        }
    }
    out
}
