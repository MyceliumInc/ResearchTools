use crate::cache::cache_or;
use crate::http::{get_typed, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {}

#[derive(Serialize)]
struct PentagonResult {
    headline: String,
    defcon_level: u32,
    defcon_severity: f64,
    overall_index: u32,

    active_spikes: u32,
    spike_events: Vec<SpikeEvent>,

    data_freshness: String,
    open_places: u32,
    total_places: u32,
    sustained: bool,
    sentinel: bool,

    place_data: Vec<PlaceData>,

    source_url: &'static str,

    places_above_150: u32,
    places_above_200: u32,
}

#[derive(Serialize)]
struct PlaceData {
    place_name: String,
    current_popularity: Option<u32>,
    percentage_of_usual: Option<u32>,
    spike_magnitude: Option<String>,
    data_source: Option<String>,
}

#[derive(Serialize)]
struct SpikeEvent {
    place_name: String,
    current_popularity: u32,
    percentage_of_usual: u32,
    spike_magnitude: String,
    data_source: String,
    minutes_ago: u32,
}

#[derive(Deserialize)]
struct Raw {
    overall_index: u32,
    defcon_level: u32,
    defcon_details: RawDefconDetails,
    active_spikes: u32,
    data_freshness: String,
    data: Vec<RawPlace>,
    events: Vec<RawEvent>,
}

#[derive(Deserialize)]
struct RawDefconDetails {
    defcon_severity_decimal: f64,
    open_places: u32,
    total_places: u32,
    places_above_150: u32,
    places_above_200: u32,
    sustained: bool,
    sentinel: bool,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    current_popularity: Option<u32>,
    percentage_of_usual: Option<u32>,
    spike_magnitude: Option<String>,
    data_source: Option<String>,
}

#[derive(Deserialize)]
struct RawEvent {
    place_name: String,
    current_popularity: Option<u32>,
    percentage_of_usual: Option<u32>,
    spike_magnitude: Option<String>,
    data_source: Option<String>,
    minutes_ago: u32,
}

impl From<RawPlace> for PlaceData {
    fn from(raw: RawPlace) -> Self {
        PlaceData {
            place_name: raw.name,
            current_popularity: raw.current_popularity,
            percentage_of_usual: raw.percentage_of_usual,
            spike_magnitude: raw.spike_magnitude,
            data_source: raw.data_source,
        }
    }
}

impl From<Raw> for PentagonResult {
    fn from(raw: Raw) -> Self {
        PentagonResult {
            headline: build_headline(&raw),
            defcon_level: raw.defcon_level,
            defcon_severity: raw.defcon_details.defcon_severity_decimal,
            overall_index: raw.overall_index,
            active_spikes: raw.active_spikes,
            spike_events: raw.events.into_iter().filter_map(map_event).collect(),
            data_freshness: raw.data_freshness,
            open_places: raw.defcon_details.open_places,
            total_places: raw.defcon_details.total_places,
            sustained: raw.defcon_details.sustained,
            sentinel: raw.defcon_details.sentinel,
            place_data: raw
                .data
                .into_iter()
                .filter(|place| {
                    place.current_popularity.is_some() || place.percentage_of_usual.is_some()
                })
                .map(PlaceData::from)
                .collect(),
            source_url: "https://www.pizzint.watch/api/dashboard-data",
            places_above_150: raw.defcon_details.places_above_150,
            places_above_200: raw.defcon_details.places_above_200,
        }
    }
}

fn build_headline(raw: &Raw) -> String {
    let freshness = match raw.data_freshness.as_str() {
        "fresh" => "fresh",
        _ => "STALE",
    };
    let spike_word = if raw.active_spikes == 1 { "spike" } else { "spikes" };
    format!(
        "data: {} - DEFCON {} - {} current {} with {}/{} places open",
        freshness,
        raw.defcon_level,
        raw.active_spikes,
        spike_word,
        raw.defcon_details.open_places,
        raw.defcon_details.total_places
    )
}

fn map_event(raw: RawEvent) -> Option<SpikeEvent> {
    Some(SpikeEvent {
        place_name: raw.place_name,
        current_popularity: raw.current_popularity?,
        percentage_of_usual: raw.percentage_of_usual?,
        spike_magnitude: raw.spike_magnitude?,
        data_source: raw.data_source?,
        minutes_ago: raw.minutes_ago,
    })
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "pentagon_pizza", 60, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let _body: Req = serde_json::from_slice(&raw)
        .map_err(|error| Error::RustError(format!("bad request: {}", error)))?;

    let timestamp = Date::now().as_millis();
    let url = format!("https://www.pizzint.watch/api/dashboard-data?_t={}", timestamp);

    let upstream: Raw = get_typed(&url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|error| Error::RustError(format!("upstream shape changed: {}", error)))?;
    let result: PentagonResult = upstream.into();

    Ok(serde_json::to_vec(&result)?)
}
