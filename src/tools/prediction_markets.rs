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
struct Item {
    source: String,
    question: String,
    url: String,
    probability_pct: Option<f64>,
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

    let (poly, manif, kal) = futures::join!(
        polymarket_search(&query, limit),
        manifold_search(&query, limit),
        kalshi_search(&query, limit),
    );

    let merged = interleave(vec![poly, manif, kal], limit);
    Ok(serde_json::to_vec(&Resp { results: merged })?)
}

fn interleave(sources: Vec<Vec<Item>>, limit: usize) -> Vec<Item> {
    let mut out = Vec::with_capacity(limit);
    let mut iters: Vec<_> = sources.into_iter().map(|v| v.into_iter()).collect();
    loop {
        let mut pushed = false;
        for it in iters.iter_mut() {
            if let Some(x) = it.next() {
                out.push(x);
                pushed = true;
                if out.len() >= limit {
                    return out;
                }
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

fn map_poly(m: &PolyMarket, event_slug: Option<&str>) -> Vec<Item> {
    let question = m.question.trim();
    if question.is_empty() || m.closed || m.archived {
        return vec![];
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
    let volume = num(&m.volume);

    let is_binary = outcomes.len() == 2
        && outcomes.iter().any(|o| o.eq_ignore_ascii_case("yes"))
        && outcomes.iter().any(|o| o.eq_ignore_ascii_case("no"));

    if is_binary {
        let yes_idx = outcomes
            .iter()
            .position(|o| o.eq_ignore_ascii_case("yes"))
            .unwrap_or(0);
        let yes_price = prices.get(yes_idx).copied().unwrap_or(0.0);
        return vec![Item {
            source: "polymarket".to_string(),
            question: question.to_string(),
            url,
            probability_pct: Some((yes_price * 100.0).clamp(0.0, 100.0)),
            end_date: m.end_date.clone(),
            volume,
        }];
    }

    if outcomes.is_empty() {
        return vec![Item {
            source: "polymarket".to_string(),
            question: question.to_string(),
            url,
            probability_pct: None,
            end_date: m.end_date.clone(),
            volume,
        }];
    }

    let per_outcome_volume = volume / outcomes.len() as f64;
    outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| Item {
            source: "polymarket".to_string(),
            question: format!("{} — {}", question, o),
            url: url.clone(),
            probability_pct: Some(
                (prices.get(i).copied().unwrap_or(0.0) * 100.0).clamp(0.0, 100.0),
            ),
            end_date: m.end_date.clone(),
            volume: per_outcome_volume,
        })
        .collect()
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
            for mapped in map_poly(m, ev.slug.as_deref()) {
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

#[derive(Deserialize)]
struct KalshiMarket {
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
struct KalshiEvent {
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
    markets: Vec<KalshiMarket>,
}

#[derive(Deserialize)]
struct KalshiResp {
    #[serde(default)]
    events: Vec<KalshiEvent>,
}

async fn kalshi_search(query: &str, limit: usize) -> Vec<Item> {
    let url = "https://api.elections.kalshi.com/trade-api/v2/events?status=open&with_nested_markets=true&limit=200";
    let json = match get_json(url, BOT_UA, TIMEOUT_DEFAULT_MS).await {
        Ok(v) => v,
        Err(e) => {
            console_log!("kalshi_search failed: {}", e);
            return vec![];
        }
    };
    let parsed: KalshiResp = serde_json::from_value(json).unwrap_or(KalshiResp { events: vec![] });
    let q = query.to_lowercase();
    let mut out = Vec::new();
    'outer: for ev in &parsed.events {
        let event_match = ev.title.to_lowercase().contains(&q)
            || ev.sub_title.to_lowercase().contains(&q)
            || ev.event_ticker.to_lowercase().contains(&q)
            || ev.series_ticker.to_lowercase().contains(&q);
        for m in &ev.markets {
            if m.status != "active" {
                continue;
            }
            let market_match = m.title.to_lowercase().contains(&q)
                || m.yes_sub_title.to_lowercase().contains(&q)
                || m.ticker.to_lowercase().contains(&q);
            if !event_match && !market_match {
                continue;
            }
            let question = if ev.mutually_exclusive && !m.yes_sub_title.is_empty() {
                format!("{} — {}", ev.title.trim(), m.yes_sub_title.trim())
            } else if !m.title.is_empty() {
                m.title.trim().to_string()
            } else {
                ev.title.trim().to_string()
            };
            if question.is_empty() {
                continue;
            }
            let url = if !ev.series_ticker.is_empty() && !ev.event_ticker.is_empty() {
                format!(
                    "https://kalshi.com/markets/{}/{}",
                    ev.series_ticker, ev.event_ticker
                )
            } else {
                "https://kalshi.com/".to_string()
            };
            let last_price: f64 = m.last_price_dollars.parse().unwrap_or(0.0);
            let probability_pct = if last_price > 0.0 {
                Some((last_price * 100.0).clamp(0.0, 100.0))
            } else {
                None
            };
            let volume: f64 = m.volume_fp.parse().unwrap_or(0.0);
            out.push(Item {
                source: "kalshi".to_string(),
                question,
                url,
                probability_pct,
                end_date: m.close_time.clone(),
                volume,
            });
            if out.len() >= limit {
                break 'outer;
            }
        }
    }
    out
}
