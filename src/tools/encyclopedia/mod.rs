mod grokipedia;
mod wikipedia;

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
    pub title: String,
    pub snippet: String,
    pub url: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "encyclopedia", 300, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;
    let query = body.query.trim();
    if query.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(5).clamp(1, 25);

    let (wiki, grok) = futures::join!(
        wikipedia::search(query, limit),
        grokipedia::search(query, limit)
    );

    let merged = interleave(vec![wiki, grok], limit);
    Ok(serde_json::to_vec(&Resp { results: merged })?)
}
