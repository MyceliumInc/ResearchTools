use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {}

#[derive(Debug, Serialize)]
struct PentagonResult {
    // overview
    headline: String,
    defcon_level: u32,
    defcon_severity: f64,
    overall_index: u32,

    // spike state
    active_spikes: u32,
    spike_events: Vec<SpikeEvent>,

    // data quality
    data_freshness: String,
    open_places: u32,
    total_places: u32,
    sustained: bool,
    sentinel: bool,

    // per place
    place_data: Vec<PlaceData>,

    // provenance
    source_url: &'static str,

    // spike breakdown
    places_above_150: u32,
    places_above_200: u32,
}

#[derive(Debug, Serialize)]
struct PlaceData {
    place_name: String,
    current_popularity: Option<u32>,
    percentage_of_usual: Option<u32>,
    spike_magnitude: Option<String>,
    data_source: Option<String>,
}

#[derive(Debug, Serialize)]
struct SpikeEvent {
    place_name: String,
    current_popularity: u32,
    percentage_of_usual: u32,
    spike_magnitude: String,
    data_source: String,
    minutes_ago: u32,
}

#[derive(Debug, Deserialize)]
struct Raw {
    overall_index: u32,
    defcon_level: u32,
    defcon_details: RawDefconDetails,
    active_spikes: u32,
    has_active_spikes: bool,
    timestamp: String,
    method: String,
    data_freshness: String,
    data: Vec<RawPlace>,
    events: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawDefconDetails {
    at_time: String,
    defcon_severity_decimal: f64,
    raw_index: f64,
    smoothed_index: f64,
    open_places: u32,
    total_places: u32,
    intensity_score: f64,
    breadth_score: u32,
    night_multiplier: f64,
    persistence_factor: f64,
    places_above_150: u32,
    places_above_200: u32,
    high_count: u32,
    extreme_count: u32,
    max_pct: u32,
    max_current_popularity: u32,
    sustained: bool,
    sentinel: bool,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawPlace {
    name: String,
    current_popularity: Option<u32>,
    percentage_of_usual: Option<u32>,
    is_spike: bool,
    spike_magnitude: Option<String>,
    data_freshness: String,
    data_source: Option<String>,
}

#[derive(Debug, Deserialize)]
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
            spike_events: raw.events.into_iter().filter_map(try_event).collect(),
            data_freshness: raw.data_freshness,
            open_places: raw.defcon_details.open_places,
            total_places: raw.defcon_details.total_places,
            sustained: raw.defcon_details.sustained,
            sentinel: raw.defcon_details.sentinel,
            place_data: raw
                .data
                .into_iter()
                .filter(|p| p.current_popularity.is_some() || p.percentage_of_usual.is_some())
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
    let spike_word = if raw.active_spikes == 1 {
        "spike"
    } else {
        "spikes"
    };
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

fn try_event(raw: RawEvent) -> Option<SpikeEvent> {
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
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;

    let ts = Date::now().as_millis();
    let url = format!("https://www.pizzint.watch/api/dashboard-data?_t={}", ts);

    let json = get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS).await?;

    let raw_data: Raw = serde_json::from_value(json)
        .map_err(|e| Error::RustError(format!("upstream shape changed: {}", e)))?;
    let result: PentagonResult = raw_data.into();

    Ok(serde_json::to_vec(&result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_sample_into_pentagon_result() {
        // 1. Load the JSON fixture as a string
        let json = include_str!("fixtures/pentagon_pizza_sample.json");

        // 2. Deserialize into Raw
        let raw: Raw = serde_json::from_str(json).expect("deserialization failed");

        // 3. Transform via From
        let result: PentagonResult = raw.into();

        // Top-level scalars
        assert_eq!(result.defcon_level, 3);
        assert_eq!(result.active_spikes, 2);
        assert_eq!(result.overall_index, 31);

        // From defcon_details, hoisted up
        assert_eq!(result.open_places, 5);
        assert_eq!(result.total_places, 6);
        assert_eq!(result.sentinel, false);

        // Vec lengths (catches filter and From-impl bugs)
        assert_eq!(result.spike_events.len(), 2);
        // place_data length depends on how aggressively your filter drops places — check the JSON

        // Headline (catches the build_headline function)
        assert!(result.headline.contains("DEFCON 3"));
        assert!(result.headline.contains("2 current spikes"));
        assert!(result.headline.contains("fresh")); // since data_freshness is "fresh"

        // First spike event (catches try_event)
        let first = &result.spike_events[0];
        assert_eq!(first.place_name, "District Pizza Palace"); // first in events array
        assert_eq!(first.percentage_of_usual, 257);
    }
}