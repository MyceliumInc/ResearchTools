mod kalshi;
mod manifold;
mod polymarket;

use crate::cache::cache_or;
use crate::tools::interleave;
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct Item {
    pub source: String,
    pub question: String,
    pub url: String,
    pub probability_pct: Option<f64>,
    pub end_date: Option<String>,
    pub volume: f64,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "predictions", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;
    let query = body.query.trim();
    if query.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 50);

    let (poly, manif, kal) = futures::join!(
        polymarket::search(query, limit),
        manifold::search(query, limit),
        kalshi::search(query, limit),
    );

    let merged = interleave(vec![poly, manif, kal], limit);
    Ok(serde_json::to_vec(&Resp { results: merged })?)
}
