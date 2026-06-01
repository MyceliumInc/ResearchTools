use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    symbols: Vec<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FinnhubQuote {
    #[serde(rename = "c")]
    current_price: f64,
    #[serde(default, rename = "d")]
    change: Option<f64>,
    #[serde(default, rename = "dp")]
    change_pct: Option<f64>,
    #[serde(rename = "h")]
    high: f64,
    #[serde(rename = "l")]
    low: f64,
    #[serde(rename = "o")]
    open: f64,
    #[serde(rename = "pc")]
    previous_close: f64,
    #[serde(rename = "t")]
    timestamp_secs: i64,
}

#[derive(Serialize)]
struct StockQuote {
    symbol: String,
    price: Option<f64>,
    change: Option<f64>,
    change_pct: Option<f64>,
    previous_close: Option<f64>,
    currency: String,
    as_of: String,
    source: String,
}

#[derive(Serialize)]
struct QuoteError {
    symbol: String,
    message: String,
}

#[derive(Serialize)]
struct Resp {
    quotes: Vec<StockQuote>,
    errors: Vec<QuoteError>,
}

pub async fn run(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let key = ctx
        .secret("FINNHUB_API_KEY")
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    cache_or(req, "stock_quote", 5, move |body| execute(body, key)).await
}

async fn execute(raw: Vec<u8>, key: Option<String>) -> Result<Vec<u8>> {
    let key = match key {
        Some(k) => k,
        None => return Err(Error::RustError("Finnhub API key not configured".into())),
    };
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    if body.symbols.is_empty() {
        return Err(Error::RustError("no symbols provided".into()));
    }
    if body.symbols.len() > 25 {
        return Err(Error::RustError("too many symbols (max 25)".into()));
    }

    let mut quotes: Vec<StockQuote> = Vec::new();
    let mut errors: Vec<QuoteError> = Vec::new();
    let futures: Vec<_> = body.symbols.iter().map(|s| fetch_one(s, &key)).collect();
    for r in futures::future::join_all(futures).await {
        match r {
            Ok(q) => quotes.push(q),
            Err((symbol, message)) => errors.push(QuoteError { symbol, message }),
        }
    }

    Ok(serde_json::to_vec(&Resp { quotes, errors })?)
}

fn map_finnhub(symbol: &str, fh: FinnhubQuote) -> Result<StockQuote, String> {
    if fh.change.is_none() || fh.change_pct.is_none() {
        return Err("symbol not found".to_string());
    }
    let date = worker::Date::from(worker::DateInit::Millis((fh.timestamp_secs as u64) * 1000));
    Ok(StockQuote {
        symbol: symbol.to_string(),
        price: Some(fh.current_price),
        change: fh.change,
        change_pct: fh.change_pct,
        previous_close: Some(fh.previous_close),
        currency: "USD".to_string(),
        as_of: date.to_string(),
        source: "finnhub".to_string(),
    })
}

async fn fetch_one(symbol: &str, key: &str) -> Result<StockQuote, (String, String)> {
    let url = format!(
        "https://finnhub.io/api/v1/quote?symbol={}&token={}",
        urlencoding::encode(symbol),
        key
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_FAST_MS)
        .await
        .map_err(|e| (symbol.to_string(), e.to_string()))?;
    let resp: FinnhubQuote = serde_json::from_value(json)
        .map_err(|e| (symbol.to_string(), format!("invalid response: {}", e)))?;

    map_finnhub(symbol, resp).map_err(|e| (symbol.to_string(), e))
}
