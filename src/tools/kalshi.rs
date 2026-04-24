use crate::util::{dollars_to_cents, error_response, get_json, score, to_number, tokens, BOT_UA};
use futures::future::join_all;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use worker::*;

const BASE: &str = "https://api.elections.kalshi.com/trade-api/v2";
const SERIES_TTL_MS: f64 = 10.0 * 60.0 * 1000.0;

static SERIES_CACHE: Lazy<Mutex<Option<(f64, Vec<Value>)>>> = Lazy::new(|| Mutex::new(None));

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    top_series: Option<usize>,
}

#[derive(Serialize)]
struct KalshiResult {
    ticker: String,
    event_ticker: String,
    series_ticker: String,
    title: String,
    subtitle: String,
    yes_bid_cents: i64,
    yes_ask_cents: i64,
    no_bid_cents: i64,
    no_ask_cents: i64,
    last_price_cents: i64,
    volume_24h: f64,
    open_interest: f64,
    close_time: Option<String>,
    category: String,
    url: String,
    score: f64,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<KalshiResult>,
}

async fn fetch_all_series() -> Result<Vec<Value>> {
    let now = Date::now().as_millis() as f64;
    if let Ok(guard) = SERIES_CACHE.lock() {
        if let Some((ts, ref cached)) = *guard {
            if now - ts < SERIES_TTL_MS {
                return Ok(cached.clone());
            }
        }
    }
    let v = get_json(&format!("{}/series", BASE), BOT_UA).await?;
    let list = v
        .get("series")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    if let Ok(mut guard) = SERIES_CACHE.lock() {
        *guard = Some((now, list.clone()));
    }
    Ok(list)
}

async fn fetch_events(series_ticker: &str) -> Vec<Value> {
    let url = format!(
        "{}/events?series_ticker={}&status=open&with_nested_markets=true&limit=200",
        BASE,
        urlencoding::encode(series_ticker)
    );
    match get_json(&url, BOT_UA).await {
        Ok(v) => v
            .get("events")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn pick_market(markets: &[Value], needles: &[String]) -> Option<Value> {
    if markets.is_empty() {
        return None;
    }
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, m) in markets.iter().enumerate() {
        let title = m.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let subtitle = m.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
        let yes_sub = m.get("yes_sub_title").and_then(|v| v.as_str()).unwrap_or("");
        let hay = format!("{} {} {}", title, subtitle, yes_sub);
        let s = score(&hay, needles);
        let vol = to_number(m.get("volume_24h_fp"));
        match best {
            None => best = Some((i, s, vol)),
            Some((_, bs, bv)) if s > bs || (s == bs && vol > bv) => {
                best = Some((i, s, vol))
            }
            _ => {}
        }
    }
    let (idx, _, _) = best?;
    Some(markets[idx].clone())
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let trimmed = body.query.trim().to_string();
    if trimmed.is_empty() {
        return Response::from_json(&Resp { results: vec![] });
    }
    let needles = tokens(&trimmed);
    if needles.is_empty() {
        return Response::from_json(&Resp { results: vec![] });
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 50);
    let top_series = body.top_series.unwrap_or(10).clamp(1, 50);

    let series = match fetch_all_series().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("Kalshi search failed: {}", e)),
    };

    struct Ranked {
        s: Value,
        score: f64,
        title_len: usize,
    }

    let mut ranked: Vec<Ranked> = series
        .into_iter()
        .filter_map(|s| {
            let ticker = s
                .get("ticker")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if ticker.is_empty() {
                return None;
            }
            let title = s
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tags = s
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let hay = format!("{} {} {}", title, ticker, tags);
            let sc = score(&hay, &needles);
            if sc <= 0.0 {
                return None;
            }
            let title_len = if title.is_empty() { 999 } else { title.len() };
            Some(Ranked {
                s,
                score: sc,
                title_len,
            })
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.title_len.cmp(&b.title_len))
    });
    ranked.truncate(top_series);

    if ranked.is_empty() {
        return Response::from_json(&Resp { results: vec![] });
    }

    let futs: Vec<_> = ranked
        .iter()
        .map(|r| {
            let t = r
                .s
                .get("ticker")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            async move { fetch_events(&t).await }
        })
        .collect();
    let event_lists = join_all(futs).await;

    let mut scored: Vec<KalshiResult> = Vec::new();
    for (i, events) in event_lists.iter().enumerate() {
        let series_score = ranked[i].score;
        let series_ticker_default = ranked[i]
            .s
            .get("ticker")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let series_category_default = ranked[i]
            .s
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        for ev in events {
            let markets = ev
                .get("markets")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let Some(m) = pick_market(&markets, &needles) else {
                continue;
            };
            let ev_title = ev.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let ev_sub = ev.get("sub_title").and_then(|v| v.as_str()).unwrap_or("");
            let m_title = m.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let m_sub = m.get("subtitle").and_then(|v| v.as_str()).unwrap_or("");
            let ev_text = format!("{} {} {} {}", ev_title, ev_sub, m_title, m_sub);
            let ev_score = score(&ev_text, &needles);
            let combined = if ev_score > series_score {
                ev_score
            } else {
                series_score
            };

            let ticker = m
                .get("ticker")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let event_ticker = ev
                .get("event_ticker")
                .and_then(|v| v.as_str())
                .or_else(|| m.get("event_ticker").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            let series_ticker = ev
                .get("series_ticker")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| series_ticker_default.clone());
            let category = ev
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| series_category_default.clone());
            let url = if !event_ticker.is_empty() {
                format!(
                    "https://kalshi.com/markets/{}/{}",
                    series_ticker.to_lowercase(),
                    event_ticker.to_lowercase()
                )
            } else {
                "https://kalshi.com/".to_string()
            };
            let subtitle = if !ev_sub.is_empty() {
                ev_sub.to_string()
            } else if !m_sub.is_empty() {
                m_sub.to_string()
            } else {
                m.get("yes_sub_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let title = if !ev_title.is_empty() {
                ev_title.to_string()
            } else {
                m_title.to_string()
            };
            let close_time = ev
                .get("close_time")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    m.get("close_time")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });

            scored.push(KalshiResult {
                ticker,
                event_ticker,
                series_ticker,
                title,
                subtitle,
                yes_bid_cents: dollars_to_cents(m.get("yes_bid_dollars")),
                yes_ask_cents: dollars_to_cents(m.get("yes_ask_dollars")),
                no_bid_cents: dollars_to_cents(m.get("no_bid_dollars")),
                no_ask_cents: dollars_to_cents(m.get("no_ask_dollars")),
                last_price_cents: dollars_to_cents(m.get("last_price_dollars")),
                volume_24h: to_number(m.get("volume_24h_fp")),
                open_interest: to_number(m.get("open_interest_fp")),
                close_time,
                category,
                url,
                score: combined,
            });
        }
    }

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.volume_24h
                    .partial_cmp(&a.volume_24h)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    scored.truncate(limit);
    Response::from_json(&Resp { results: scored })
}
