use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrArray {
    Array(Vec<String>),
    Encoded(String),
    Missing,
}

impl Default for StringOrArray {
    fn default() -> Self {
        Self::Missing
    }
}

impl StringOrArray {
    fn to_vec(&self) -> Vec<String> {
        match self {
            Self::Array(v) => v.clone(),
            Self::Encoded(s) => serde_json::from_str(s).unwrap_or_default(),
            Self::Missing => vec![],
        }
    }
}

#[derive(Deserialize)]
struct UpstreamMarket {
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
    #[serde(default)]
    liquidity: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UpstreamEvent {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    markets: Vec<UpstreamMarket>,
}

#[derive(Deserialize)]
struct UpstreamResp {
    #[serde(default)]
    events: Vec<UpstreamEvent>,
}

#[derive(Serialize)]
struct Outcome {
    outcome: String,
    price: f64,
}

#[derive(Serialize)]
struct PolyResult {
    slug: String,
    question: String,
    end_date: Option<String>,
    volume: f64,
    liquidity: f64,
    outcomes: Vec<Outcome>,
    url: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<PolyResult>,
}

fn num(raw: &Option<serde_json::Value>) -> f64 {
    match raw {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn map_market(m: &UpstreamMarket, event_slug: Option<&str>) -> Option<PolyResult> {
    let question = m.question.trim();
    if question.is_empty() || m.closed || m.archived {
        return None;
    }
    let slug = if !m.slug.is_empty() {
        m.slug.clone()
    } else {
        event_slug.unwrap_or("").to_string()
    };
    let outcomes = m.outcomes.to_vec();
    let prices: Vec<f64> = m
        .outcome_prices
        .to_vec()
        .iter()
        .map(|s| s.parse().unwrap_or(0.0))
        .collect();
    let paired = outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| Outcome {
            outcome: o,
            price: prices.get(i).copied().unwrap_or(0.0),
        })
        .collect();
    let url = if !slug.is_empty() {
        format!("https://polymarket.com/event/{}", slug)
    } else {
        "https://polymarket.com/".to_string()
    };
    Some(PolyResult {
        slug,
        question: question.to_string(),
        end_date: m.end_date.clone(),
        volume: num(&m.volume),
        liquidity: num(&m.liquidity),
        outcomes: paired,
        url,
    })
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "polymarket_search", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 50);
    let url = format!(
        "https://gamma-api.polymarket.com/public-search?q={}&limit_per_type={}&events_status=active",
        urlencoding::encode(trimmed),
        limit
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("Polymarket search failed: {}", e)))?;
    let parsed: UpstreamResp = serde_json::from_value(json).unwrap_or(UpstreamResp { events: vec![] });

    let mut out = Vec::new();
    'outer: for ev in &parsed.events {
        for m in &ev.markets {
            if let Some(mapped) = map_market(m, ev.slug.as_deref()) {
                out.push(mapped);
                if out.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    Ok(serde_json::to_vec(&Resp { results: out })?)
}
