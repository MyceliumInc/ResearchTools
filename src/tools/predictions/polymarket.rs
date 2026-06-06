use super::Item;
use crate::http::{get_typed, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::Deserialize;
use worker::*;

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum StringOrArray {
    Array(Vec<String>),
    Encoded(String),
    #[default]
    Missing,
}

impl StringOrArray {
    fn to_vec(&self) -> Vec<String> {
        match self {
            Self::Array(values) => values.clone(),
            Self::Encoded(encoded) => serde_json::from_str(encoded).unwrap_or_default(),
            Self::Missing => vec![],
        }
    }
}

#[derive(Deserialize)]
struct Market {
    #[serde(default)]
    question: String,
    #[serde(default)]
    closed: bool,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    outcomes: StringOrArray,
    #[serde(default, rename = "outcomePrices")]
    outcome_prices: StringOrArray,
    #[serde(default, rename = "endDate", alias = "end_date")]
    end_date: Option<String>,
    #[serde(default)]
    volume: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    markets: Vec<Market>,
}

#[derive(Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    events: Vec<Event>,
}

pub async fn search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://gamma-api.polymarket.com/public-search?q={}&limit_per_type={}&events_status=active",
        urlencoding::encode(query),
        limit
    );
    let parsed: SearchResponse = match get_typed(&url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(value) => value,
        Err(error) => {
            console_log!("polymarket search failed: {}", error);
            return vec![];
        }
    };
    let mut out = Vec::new();
    'outer: for event in &parsed.events {
        for market in &event.markets {
            for item in map_market(market, event.slug.as_deref()) {
                out.push(item);
                if out.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    out
}

fn map_market(market: &Market, event_slug: Option<&str>) -> Vec<Item> {
    let question = market.question.trim();
    if question.is_empty() || market.closed || market.archived {
        return vec![];
    }
    let slug = if !market.slug.is_empty() {
        market.slug.clone()
    } else {
        event_slug.unwrap_or("").to_string()
    };
    let outcomes = market.outcomes.to_vec();
    let prices: Vec<f64> = market
        .outcome_prices
        .to_vec()
        .iter()
        .map(|price| price.parse().unwrap_or(0.0))
        .collect();
    let url = if !slug.is_empty() {
        format!("https://polymarket.com/event/{}", slug)
    } else {
        "https://polymarket.com/".to_string()
    };
    let volume = number(&market.volume);

    let is_binary = outcomes.len() == 2
        && outcomes.iter().any(|outcome| outcome.eq_ignore_ascii_case("yes"))
        && outcomes.iter().any(|outcome| outcome.eq_ignore_ascii_case("no"));

    if is_binary {
        let yes_index = outcomes
            .iter()
            .position(|outcome| outcome.eq_ignore_ascii_case("yes"))
            .unwrap_or(0);
        let yes_price = prices.get(yes_index).copied().unwrap_or(0.0);
        return vec![Item {
            source: "polymarket".to_string(),
            question: question.to_string(),
            url,
            probability_pct: Some((yes_price * 100.0).clamp(0.0, 100.0)),
            end_date: market.end_date.clone(),
            volume,
        }];
    }

    if outcomes.is_empty() {
        return vec![Item {
            source: "polymarket".to_string(),
            question: question.to_string(),
            url,
            probability_pct: None,
            end_date: market.end_date.clone(),
            volume,
        }];
    }

    let per_outcome_volume = volume / outcomes.len() as f64;
    outcomes
        .into_iter()
        .enumerate()
        .map(|(index, outcome)| Item {
            source: "polymarket".to_string(),
            question: format!("{} — {}", question, outcome),
            url: url.clone(),
            probability_pct: Some(
                (prices.get(index).copied().unwrap_or(0.0) * 100.0).clamp(0.0, 100.0),
            ),
            end_date: market.end_date.clone(),
            volume: per_outcome_volume,
        })
        .collect()
}

fn number(raw: &Option<serde_json::Value>) -> f64 {
    match raw {
        Some(serde_json::Value::Number(value)) => value.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(value)) => value.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
