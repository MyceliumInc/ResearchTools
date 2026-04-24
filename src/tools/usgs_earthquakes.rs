use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    #[serde(default)]
    min_magnitude: Option<f64>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    hours: Option<u32>,
}

#[derive(Serialize)]
struct Item {
    id: String,
    magnitude: f64,
    place: String,
    time: Option<String>,
    time_ms: i64,
    url: String,
    title: String,
    tsunami: bool,
    felt: Option<i64>,
    longitude: f64,
    latitude: f64,
    depth_km: f64,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "usgs_earthquakes", 60, execute).await
}

fn ms_to_iso(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    let date = Date::from(DateInit::Millis(ms as u64));
    Some(date.to_string())
}

fn window_for(hours: u32) -> &'static str {
    match hours {
        0..=1 => "hour",
        2..=24 => "day",
        25..=168 => "week",
        _ => "month",
    }
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = if raw.is_empty() {
        Req {
            min_magnitude: None,
            limit: None,
            hours: None,
        }
    } else {
        serde_json::from_slice(&raw)
            .map_err(|e| Error::RustError(format!("bad request: {}", e)))?
    };
    let min_mag = body.min_magnitude.unwrap_or(4.5);
    let limit = body.limit.unwrap_or(20).clamp(1, 100);
    let hours = body.hours.unwrap_or(24).clamp(1, 720);
    let window = window_for(hours);

    let url = format!(
        "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_{}.geojson",
        window
    );
    let json = get_json(&url, BOT_UA, TIMEOUT_FAST_MS)
        .await
        .map_err(|e| Error::RustError(format!("USGS fetch failed: {}", e)))?;

    let mut rows: Vec<Item> = Vec::new();
    if let Some(features) = json.get("features").and_then(|v| v.as_array()) {
        for f in features {
            let id = f
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let props = match f.get("properties") {
                Some(p) => p,
                None => continue,
            };
            let magnitude = props.get("mag").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if magnitude < min_mag {
                continue;
            }
            let place = props
                .get("place")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let time_ms = props.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
            let time = ms_to_iso(time_ms);
            let url_prop = props
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = props
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tsunami = props
                .get("tsunami")
                .and_then(|v| v.as_i64())
                .map(|n| n != 0)
                .unwrap_or(false);
            let felt = props.get("felt").and_then(|v| v.as_i64());

            let coords = f
                .get("geometry")
                .and_then(|g| g.get("coordinates"))
                .and_then(|c| c.as_array());
            let longitude = coords
                .and_then(|a| a.first())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let latitude = coords
                .and_then(|a| a.get(1))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let depth_km = coords
                .and_then(|a| a.get(2))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            rows.push(Item {
                id,
                magnitude,
                place,
                time,
                time_ms,
                url: url_prop,
                title,
                tsunami,
                felt,
                longitude,
                latitude,
                depth_km,
            });
        }
    }
    rows.sort_by(|a, b| b.time_ms.cmp(&a.time_ms));
    rows.truncate(limit);
    Ok(serde_json::to_vec(&Resp { results: rows })?)
}
