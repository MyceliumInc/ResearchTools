use crate::cache::cache_or;
use crate::http::{get_typed, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {}

#[derive(Deserialize)]
struct RawMarket {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    volume: f64,
    #[serde(default)]
    volume_24h: f64,
    #[serde(default, rename = "endDate")]
    end_date: Option<i64>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    markets: Vec<RawMarket>,
    #[serde(default)]
    timestamp: String,
}

#[derive(Serialize)]
struct DoomsdayMarket {
    label: String,
    slug: String,
    region: String,
    url: String,
    probability_pct: f64,
    volume: f64,
    volume_24h: f64,
    end_date_ms: Option<i64>,
    image: Option<String>,
}

#[derive(Serialize)]
struct Resp {
    markets: Vec<DoomsdayMarket>,
    timestamp: String,
    source_url: &'static str,
}

impl From<RawMarket> for DoomsdayMarket {
    fn from(raw: RawMarket) -> Self {
        let url = if raw.slug.is_empty() {
            "https://polymarket.com/".to_string()
        } else {
            format!("https://polymarket.com/event/{}", raw.slug)
        };
        DoomsdayMarket {
            label: raw.label,
            slug: raw.slug,
            region: raw.region,
            url,
            probability_pct: (raw.price * 100.0).clamp(0.0, 100.0),
            volume: raw.volume,
            volume_24h: raw.volume_24h,
            end_date_ms: raw.end_date,
            image: raw.image,
        }
    }
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "doomsday", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let _body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;

    let url = "https://www.pizzint.watch/api/neh-index/doomsday";
    let upstream: Raw = get_typed(url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|error| Error::RustError(format!("upstream shape changed: {}", error)))?;

    let markets: Vec<DoomsdayMarket> = upstream
        .markets
        .into_iter()
        .filter(|market| !market.label.trim().is_empty())
        .map(DoomsdayMarket::from)
        .collect();

    Ok(serde_json::to_vec(&Resp {
        markets,
        timestamp: upstream.timestamp,
        source_url: "https://www.pizzint.watch/api/neh-index/doomsday",
    })?)
}
