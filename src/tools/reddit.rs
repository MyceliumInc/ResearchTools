use crate::util::{cache_or, get_json, strip_tags, BOT_UA, TIMEOUT_FAST_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    subreddit: Option<String>,
}

#[derive(Serialize)]
struct Item {
    title: String,
    subreddit: String,
    permalink: String,
    url: String,
    score: i64,
    num_comments: i64,
    created: String,
    author: String,
    snippet: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "reddit_search", 60, execute).await
}

fn valid_sort(s: &str) -> bool {
    matches!(s, "relevance" | "hot" | "new" | "top" | "comments")
}

fn valid_time(s: &str) -> bool {
    matches!(s, "hour" | "day" | "week" | "month" | "year" | "all")
}

fn iso_from_unix(ts: f64) -> String {
    let secs = ts as i64;
    if secs <= 0 {
        return String::new();
    }
    // Minimal ISO8601 UTC rendering without chrono.
    let days_since_epoch = secs / 86_400;
    let seconds_of_day = secs % 86_400;
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;

    let mut year = 1970i64;
    let mut days = days_since_epoch;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let yd = if leap { 366 } else { 365 };
        if days >= yd {
            days -= yd;
            year += 1;
        } else {
            break;
        }
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0usize;
    let mut day = days;
    while month < 12 && day >= months[month] {
        day -= months[month];
        month += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month + 1,
        day + 1,
        hour,
        minute,
        second
    )
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let q = body.query.trim();
    if q.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(10).clamp(1, 50);
    let sort = body.sort.as_deref().unwrap_or("relevance");
    let sort = if valid_sort(sort) { sort } else { "relevance" };
    let time = body.time.as_deref().unwrap_or("week");
    let time = if valid_time(time) { time } else { "week" };

    let url = match body.subreddit.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(sub) => format!(
            "https://www.reddit.com/r/{}/search.json?q={}&restrict_sr=1&limit={}&sort={}&t={}&raw_json=1",
            urlencoding::encode(sub),
            urlencoding::encode(q),
            limit,
            sort,
            time
        ),
        None => format!(
            "https://www.reddit.com/search.json?q={}&limit={}&sort={}&t={}&raw_json=1",
            urlencoding::encode(q),
            limit,
            sort,
            time
        ),
    };

    let json = get_json(&url, BOT_UA, TIMEOUT_FAST_MS)
        .await
        .map_err(|e| Error::RustError(format!("Reddit search failed: {}", e)))?;

    let mut out = Vec::new();
    if let Some(children) = json
        .get("data")
        .and_then(|d| d.get("children"))
        .and_then(|c| c.as_array())
    {
        for c in children {
            let d = match c.get("data") {
                Some(v) => v,
                None => continue,
            };
            if d.get("over_18").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            let title = d.get("title").and_then(|v| v.as_str()).unwrap_or("").trim();
            if title.is_empty() {
                continue;
            }
            let subreddit = d
                .get("subreddit")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let permalink_raw = d.get("permalink").and_then(|v| v.as_str()).unwrap_or("");
            let permalink = if permalink_raw.is_empty() {
                String::new()
            } else {
                format!("https://www.reddit.com{}", permalink_raw)
            };
            let url_field = d
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let score = d.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
            let num_comments = d.get("num_comments").and_then(|v| v.as_i64()).unwrap_or(0);
            let created_utc = d.get("created_utc").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let author = d
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let selftext = d.get("selftext").and_then(|v| v.as_str()).unwrap_or("");
            let cleaned = strip_tags(selftext);
            let snippet: String = cleaned.chars().take(400).collect();

            out.push(Item {
                title: title.to_string(),
                subreddit,
                permalink,
                url: url_field,
                score,
                num_comments,
                created: iso_from_unix(created_utc),
                author,
                snippet,
            });
            if out.len() >= limit {
                break;
            }
        }
    }

    Ok(serde_json::to_vec(&Resp { results: out })?)
}
