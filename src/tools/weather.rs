use crate::util::{cache_or, error_response, get_json_status, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    lat: f64,
    lon: f64,
}

#[derive(Serialize)]
struct Location {
    city: String,
    state: String,
    lat: f64,
    lon: f64,
}

#[derive(Serialize)]
struct Period {
    name: String,
    start_time: String,
    end_time: String,
    temperature: Option<i64>,
    temperature_unit: String,
    wind_speed: String,
    wind_direction: String,
    short_forecast: String,
    detailed_forecast: String,
    precip_probability_pct: Option<i64>,
}

#[derive(Serialize)]
struct Resp {
    location: Location,
    periods: Vec<Period>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "weather_forecast", 300, execute).await
}

async fn resp_to_bytes(mut r: Response) -> Result<Vec<u8>> {
    let text = r.text().await?;
    Ok(text.into_bytes())
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;

    let points_url = format!("https://api.weather.gov/points/{},{}", body.lat, body.lon);
    let (status, maybe) = get_json_status(&points_url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("NWS points failed: {}", e)))?;

    if status == 404 {
        let resp = error_response("NWS only covers US locations")?;
        return resp_to_bytes(resp).await;
    }
    let points = match maybe {
        Some(v) => v,
        None => {
            let resp = error_response(format!("NWS points returned HTTP {}", status))?;
            return resp_to_bytes(resp).await;
        }
    };

    let forecast_url = points
        .get("properties")
        .and_then(|p| p.get("forecast"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if forecast_url.is_empty() {
        let resp = error_response("NWS points missing forecast URL")?;
        return resp_to_bytes(resp).await;
    }

    let city = points
        .get("properties")
        .and_then(|p| p.get("relativeLocation"))
        .and_then(|r| r.get("properties"))
        .and_then(|p| p.get("city"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let state = points
        .get("properties")
        .and_then(|p| p.get("relativeLocation"))
        .and_then(|r| r.get("properties"))
        .and_then(|p| p.get("state"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (fstatus, fmaybe) = get_json_status(&forecast_url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("NWS forecast failed: {}", e)))?;
    let forecast = match fmaybe {
        Some(v) => v,
        None => {
            let resp = error_response(format!("NWS forecast returned HTTP {}", fstatus))?;
            return resp_to_bytes(resp).await;
        }
    };

    let mut periods = Vec::new();
    if let Some(arr) = forecast
        .get("properties")
        .and_then(|p| p.get("periods"))
        .and_then(|a| a.as_array())
    {
        for p in arr.iter().take(14) {
            let precip = p
                .get("probabilityOfPrecipitation")
                .and_then(|o| o.get("value"))
                .and_then(|v| v.as_i64());
            periods.push(Period {
                name: p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                start_time: p
                    .get("startTime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                end_time: p
                    .get("endTime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                temperature: p.get("temperature").and_then(|v| v.as_i64()),
                temperature_unit: p
                    .get("temperatureUnit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                wind_speed: p
                    .get("windSpeed")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                wind_direction: p
                    .get("windDirection")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                short_forecast: p
                    .get("shortForecast")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                detailed_forecast: p
                    .get("detailedForecast")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                precip_probability_pct: precip,
            });
        }
    }

    let resp = Resp {
        location: Location {
            city,
            state,
            lat: body.lat,
            lon: body.lon,
        },
        periods,
    };
    Ok(serde_json::to_vec(&resp)?)
}
