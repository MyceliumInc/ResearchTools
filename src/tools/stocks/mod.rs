use crate::cache::cache_or;
use crate::http::{get_typed, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    symbols: Vec<String>,
}

#[derive(Deserialize)]
struct FormattedQuote {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    last: Option<String>,
    #[serde(default)]
    change: Option<String>,
    #[serde(default)]
    change_pct: Option<String>,
    #[serde(default)]
    open: Option<String>,
    #[serde(default)]
    high: Option<String>,
    #[serde(default)]
    low: Option<String>,
    #[serde(default)]
    previous_day_closing: Option<String>,
    #[serde(default)]
    volume: Option<String>,
    #[serde(default)]
    #[serde(rename = "mktcapView")]
    mktcap_view: Option<String>,
    #[serde(default)]
    pe: Option<String>,
    #[serde(default)]
    eps: Option<String>,
    #[serde(default)]
    dividendyield: Option<String>,
    #[serde(default)]
    #[serde(rename = "currencyCode")]
    currency_code: Option<String>,
    #[serde(default)]
    exchange: Option<String>,
    #[serde(default)]
    curmktstatus: Option<String>,
    #[serde(default)]
    last_timedate: Option<String>,
    #[serde(default)]
    code: Option<u32>,
}

#[derive(Deserialize)]
struct QuoteResult {
    #[serde(default)]
    #[serde(rename = "FormattedQuote")]
    quotes: Vec<FormattedQuote>,
}

#[derive(Deserialize)]
struct CnbcResponse {
    #[serde(rename = "FormattedQuoteResult")]
    result: QuoteResult,
}

#[derive(Serialize)]
struct StockQuote {
    symbol: String,
    name: Option<String>,
    price: Option<f64>,
    change: Option<f64>,
    change_pct: Option<f64>,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    previous_close: Option<f64>,
    volume: Option<i64>,
    market_cap: Option<String>,
    pe: Option<f64>,
    eps: Option<f64>,
    dividend_yield: Option<f64>,
    currency: Option<String>,
    exchange: Option<String>,
    market_status: Option<String>,
    as_of: Option<String>,
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

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "stocks", 5, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;
    if body.symbols.is_empty() {
        return Err(Error::RustError("no symbols provided".into()));
    }
    if body.symbols.len() > 25 {
        return Err(Error::RustError("too many symbols (max 25)".into()));
    }

    let joined = body
        .symbols
        .iter()
        .map(|symbol| symbol.trim().to_uppercase())
        .collect::<Vec<_>>()
        .join("|");
    let url = format!(
        "https://quote.cnbc.com/quote-html-webservice/restQuote/symbolType/symbol?symbols={}&requestMethod=itv&noform=1&partnerId=2&fund=1&exthrs=1&output=json",
        urlencoding::encode(&joined)
    );
    let response: CnbcResponse = get_typed(&url, BOT_UA, TIMEOUT_FAST_MS).await?;

    let mut quotes: Vec<StockQuote> = Vec::new();
    let mut errors: Vec<QuoteError> = Vec::new();
    for entry in response.result.quotes {
        let symbol = entry
            .symbol
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_default();
        let missing_last = entry
            .last
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true);
        if entry.code == Some(1) || missing_last {
            errors.push(QuoteError {
                symbol,
                message: "symbol not found".to_string(),
            });
            continue;
        }
        quotes.push(map_quote(symbol, entry));
    }

    Ok(serde_json::to_vec(&Resp { quotes, errors })?)
}

fn map_quote(symbol: String, entry: FormattedQuote) -> StockQuote {
    StockQuote {
        symbol,
        name: clean(entry.name),
        price: entry.last.as_deref().and_then(parse_number),
        change: entry.change.as_deref().and_then(parse_number),
        change_pct: entry.change_pct.as_deref().and_then(parse_number),
        open: entry.open.as_deref().and_then(parse_number),
        high: entry.high.as_deref().and_then(parse_number),
        low: entry.low.as_deref().and_then(parse_number),
        previous_close: entry.previous_day_closing.as_deref().and_then(parse_number),
        volume: entry.volume.as_deref().and_then(parse_volume),
        market_cap: clean(entry.mktcap_view),
        pe: entry.pe.as_deref().and_then(parse_number),
        eps: entry.eps.as_deref().and_then(parse_number),
        dividend_yield: entry.dividendyield.as_deref().and_then(parse_number),
        currency: clean(entry.currency_code),
        exchange: clean(entry.exchange),
        market_status: clean(entry.curmktstatus),
        as_of: clean(entry.last_timedate),
        source: "cnbc".to_string(),
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

fn parse_number(raw: &str) -> Option<f64> {
    let trimmed: String = raw
        .trim()
        .chars()
        .filter(|character| !matches!(character, '%' | ',' | '$'))
        .collect();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse().ok()
}

fn parse_volume(raw: &str) -> Option<i64> {
    let trimmed: String = raw.trim().chars().filter(|character| *character != ',').collect();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse().ok()
}
