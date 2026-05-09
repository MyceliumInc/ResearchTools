use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Clone)]
struct Outcome {
    outcome: String,
    probability_pct: f64,
}

#[derive(Serialize, Clone)]
struct Item {
    source: String,
    question: String,
    url: String,
    probability_pct: Option<f64>,
    outcomes: Option<Vec<Outcome>>,
    end_date: Option<String>,
    volume: f64,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "prediction_market_search", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let query = body.query.trim().to_string();
    if query.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 50);

    let (poly, manif) = futures::join!(
        polymarket_search(&query, limit),
        manifold_search(&query, limit)
    );

    let merged = interleave(poly, manif, limit);
    Ok(serde_json::to_vec(&Resp { results: merged })?)
}

fn interleave(a: Vec<Item>, b: Vec<Item>, limit: usize) -> Vec<Item> {
    let mut out = Vec::with_capacity(limit);
    let mut ai = a.into_iter();
    let mut bi = b.into_iter();
    loop {
        let mut pushed = false;
        if let Some(x) = ai.next() {
            out.push(x);
            pushed = true;
            if out.len() >= limit {
                break;
            }
        }
        if let Some(y) = bi.next() {
            out.push(y);
            pushed = true;
            if out.len() >= limit {
                break;
            }
        }
        if !pushed {
            break;
        }
    }
    out
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
struct PolyMarket {
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
struct PolyEvent {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    markets: Vec<PolyMarket>,
}

#[derive(Deserialize)]
struct PolyResp {
    #[serde(default)]
    events: Vec<PolyEvent>,
}

fn num(raw: &Option<serde_json::Value>) -> f64 {
    match raw {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn map_poly(m: &PolyMarket, event_slug: Option<&str>) -> Option<Item> {
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
    let url = if !slug.is_empty() {
        format!("https://polymarket.com/event/{}", slug)
    } else {
        "https://polymarket.com/".to_string()
    };

    let is_binary = outcomes.len() == 2
        && outcomes
            .iter()
            .any(|o| o.eq_ignore_ascii_case("yes"))
        && outcomes
            .iter()
            .any(|o| o.eq_ignore_ascii_case("no"));

    let (probability_pct, outcomes_out) = if is_binary {
        let yes_idx = outcomes
            .iter()
            .position(|o| o.eq_ignore_ascii_case("yes"))
            .unwrap_or(0);
        let yes_price = prices.get(yes_idx).copied().unwrap_or(0.0);
        (Some((yes_price * 100.0).clamp(0.0, 100.0)), None)
    } else if outcomes.is_empty() {
        (None, None)
    } else {
        let paired: Vec<Outcome> = outcomes
            .into_iter()
            .enumerate()
            .map(|(i, o)| Outcome {
                outcome: o,
                probability_pct: (prices.get(i).copied().unwrap_or(0.0) * 100.0).clamp(0.0, 100.0),
            })
            .collect();
        (None, Some(paired))
    };

    Some(Item {
        source: "polymarket".to_string(),
        question: question.to_string(),
        url,
        probability_pct,
        outcomes: outcomes_out,
        end_date: m.end_date.clone(),
        volume: num(&m.volume),
    })
}

async fn polymarket_search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://gamma-api.polymarket.com/public-search?q={}&limit_per_type={}&events_status=active",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(v) => v,
        Err(e) => {
            console_log!("polymarket_search failed: {}", e);
            return vec![];
        }
    };
    let parsed: PolyResp = serde_json::from_value(json).unwrap_or(PolyResp { events: vec![] });
    let mut out = Vec::new();
    'outer: for ev in &parsed.events {
        for m in &ev.markets {
            if let Some(mapped) = map_poly(m, ev.slug.as_deref()) {
                out.push(mapped);
                if out.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    out
}

fn ms_to_iso(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    let date = Date::from(DateInit::Millis(ms as u64));
    Some(date.to_string())
}

async fn manifold_search(query: &str, limit: usize) -> Vec<Item> {
    let url = format!(
        "https://api.manifold.markets/v0/search-markets?term={}&limit={}",
        urlencoding::encode(query),
        limit
    );
    let json = match get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(v) => v,
        Err(e) => {
            console_log!("manifold_search failed: {}", e);
            return vec![];
        }
    };
    let mut out = Vec::new();
    if let Some(arr) = json.as_array() {
        for m in arr {
            let question = m
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if question.is_empty() {
                continue;
            }
            let is_resolved = m
                .get("isResolved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_resolved {
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
                .unwrap_or("");
            let probability_pct = if outcome_type == "BINARY" {
                m.get("probability")
                    .and_then(|v| v.as_f64())
                    .map(|p| (p * 100.0).round().clamp(0.0, 100.0))
            } else {
                None
            };
            let end_date = m
                .get("closeTime")
                .and_then(|v| v.as_i64())
                .and_then(ms_to_iso);
            let volume = m.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0);

            out.push(Item {
                source: "manifold".to_string(),
                question,
                url,
                probability_pct,
                outcomes: None,
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
