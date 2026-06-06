mod duckduckgo;
mod exa;
mod marginalia;
mod mojeek;

use crate::cache::cache_or;
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct WebResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<WebResult>,
}

pub async fn run(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let exa_key = ctx
        .secret("EXA_API_KEY")
        .ok()
        .map(|secret| secret.to_string())
        .filter(|secret| !secret.is_empty());
    cache_or(req, "search_web", 300, move |body| execute(body, exa_key)).await
}

async fn execute(raw: Vec<u8>, exa_key: Option<String>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;
    let query = body.query.trim();
    if query.is_empty() {
        return Err(Error::RustError("Search failed: empty query".into()));
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 20);

    let results = match exa_key {
        Some(key) => exa::search(query, limit, &key).await?,
        None => fallback(query, limit).await?,
    };

    Ok(serde_json::to_vec(&Resp { results })?)
}

async fn fallback(query: &str, limit: usize) -> Result<Vec<WebResult>> {
    let mut empty_ok = false;
    let mut last_error: Option<Error> = None;

    if let Some(results) = consider(
        mojeek::search(query, limit).await,
        "mojeek",
        &mut empty_ok,
        &mut last_error,
    ) {
        return Ok(results);
    }
    if let Some(results) = consider(
        duckduckgo::search(query, limit).await,
        "duckduckgo",
        &mut empty_ok,
        &mut last_error,
    ) {
        return Ok(results);
    }
    if let Some(results) = consider(
        marginalia::search(query, limit).await,
        "marginalia",
        &mut empty_ok,
        &mut last_error,
    ) {
        return Ok(results);
    }

    if empty_ok {
        return Ok(Vec::new());
    }
    Err(last_error
        .unwrap_or_else(|| Error::RustError("Search failed: no provider returned results".into())))
}

fn consider(
    outcome: Result<Vec<WebResult>>,
    provider: &str,
    empty_ok: &mut bool,
    last_error: &mut Option<Error>,
) -> Option<Vec<WebResult>> {
    match outcome {
        Ok(results) if !results.is_empty() => Some(results),
        Ok(_) => {
            *empty_ok = true;
            None
        }
        Err(error) => {
            console_log!("search_web: {} failed: {}", provider, error);
            *last_error = Some(error);
            None
        }
    }
}
