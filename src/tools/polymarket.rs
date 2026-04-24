use crate::util::{error_response, get_json, to_number, BOT_UA};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
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

fn to_string_array(v: Option<&Value>) -> Vec<String> {
    let Some(v) = v else { return vec![] };
    match v {
        Value::Array(arr) => arr
            .iter()
            .map(|x| match x {
                Value::String(s) => s.clone(),
                _ => x.to_string(),
            })
            .collect(),
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(arr)) => arr
                .iter()
                .map(|x| match x {
                    Value::String(s) => s.clone(),
                    _ => x.to_string(),
                })
                .collect(),
            _ => vec![],
        },
        _ => vec![],
    }
}

fn map_market(m: &Value, event_slug: Option<&str>) -> Option<PolyResult> {
    let question = m.get("question").and_then(|v| v.as_str())?.trim();
    if question.is_empty() {
        return None;
    }
    let closed = m.get("closed").and_then(|v| v.as_bool()).unwrap_or(false);
    let archived = m.get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
    if closed || archived {
        return None;
    }
    let slug = m
        .get("slug")
        .and_then(|v| v.as_str())
        .or(event_slug)
        .unwrap_or("")
        .to_string();
    let outcomes = to_string_array(m.get("outcomes"));
    let prices: Vec<f64> = to_string_array(m.get("outcomePrices"))
        .iter()
        .map(|s| s.parse::<f64>().unwrap_or(0.0))
        .collect();
    let paired = outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| Outcome {
            outcome: o,
            price: prices.get(i).copied().unwrap_or(0.0),
        })
        .collect();
    let end_date = m
        .get("endDate")
        .and_then(|v| v.as_str())
        .or_else(|| m.get("end_date").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let url = if !slug.is_empty() {
        format!("https://polymarket.com/event/{}", slug)
    } else {
        "https://polymarket.com/".to_string()
    };
    Some(PolyResult {
        slug,
        question: question.to_string(),
        end_date,
        volume: to_number(m.get("volume")),
        liquidity: to_number(m.get("liquidity")),
        outcomes: paired,
        url,
    })
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let trimmed = body.query.trim();
    if trimmed.is_empty() {
        return Response::from_json(&Resp { results: vec![] });
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 50);
    let url = format!(
        "https://gamma-api.polymarket.com/public-search?q={}&limit_per_type={}&events_status=active",
        urlencoding::encode(trimmed),
        limit
    );
    let json = match get_json(&url, BOT_UA).await {
        Ok(v) => v,
        Err(e) => return error_response(format!("Polymarket search failed: {}", e)),
    };
    let mut out = Vec::new();
    if let Some(events) = json.get("events").and_then(|v| v.as_array()) {
        'outer: for ev in events {
            let event_slug = ev.get("slug").and_then(|v| v.as_str());
            if let Some(markets) = ev.get("markets").and_then(|v| v.as_array()) {
                for m in markets {
                    if let Some(mapped) = map_market(m, event_slug) {
                        out.push(mapped);
                        if out.len() >= limit {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    Response::from_json(&Resp { results: out })
}
