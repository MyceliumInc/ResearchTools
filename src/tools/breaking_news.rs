use crate::util::{cache_or, get_text, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use worker::*;

#[derive(Serialize)]
struct Out {
    stories: Vec<Story>,
}

#[derive(Serialize)]
struct Story {
    headline: String,
    url: String,
    source: &'static str,
    sources: usize,
}

const FEEDS: &[(&str, &str)] = &[
    ("BBC", "https://feeds.bbci.co.uk/news/rss.xml"),
    ("NYT", "https://rss.nytimes.com/services/xml/rss/nyt/HomePage.xml"),
    ("Guardian", "https://www.theguardian.com/world/rss"),
    ("NPR", "https://feeds.npr.org/1001/rss.xml"),
    ("CNN", "http://rss.cnn.com/rss/cnn_topstories.rss"),
    ("AlJazeera", "https://www.aljazeera.com/xml/rss/all.xml"),
    ("SkyNews", "https://feeds.skynews.com/feeds/rss/world.xml"),
];

const CUTOFF_MS: u64 = 12 * 60 * 60 * 1000;
const JACCARD_THRESHOLD: f32 = 0.20;
const MIN_SOURCES: usize = 2;
const MAX_STORIES: usize = 20;

struct Item {
    headline: String,
    url: String,
    source: &'static str,
    pub_date_ms: u64,
    entities: Vec<String>,
    tokens: Vec<String>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "breaking_news", 30, |_body| async move {
        let stories = pipeline().await;
        serde_json::to_vec(&Out { stories })
            .map_err(|e| Error::RustError(format!("serialize: {}", e)))
    })
    .await
}

async fn pipeline() -> Vec<Story> {
    let futures: Vec<_> = FEEDS
        .iter()
        .map(|(name, url)| fetch_feed(name, url))
        .collect();
    let results = futures::future::join_all(futures).await;

    let now = Date::now().as_millis();
    let cutoff = now.saturating_sub(CUTOFF_MS);

    let mut items: Vec<Item> = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut dropped_old = 0usize;
    let mut dropped_dup = 0usize;
    for batch in results {
        for it in batch {
            if it.pub_date_ms != 0 && it.pub_date_ms < cutoff {
                dropped_old += 1;
                continue;
            }
            if !seen_urls.insert(it.url.clone()) {
                dropped_dup += 1;
                continue;
            }
            items.push(it);
        }
    }
    console_log!(
        "breaking_news: items={} dropped_old={} dropped_dup={}",
        items.len(),
        dropped_old,
        dropped_dup
    );
    if items.is_empty() {
        return vec![];
    }

    let clusters = cluster_items(&items);
    let sizes: Vec<usize> = clusters.iter().map(|c| c.len()).collect();
    console_log!(
        "breaking_news: raw_clusters={} sizes={:?}",
        clusters.len(),
        sizes
    );
    for (ci, cluster) in clusters.iter().enumerate() {
        let members: Vec<String> = cluster
            .iter()
            .map(|&i| format!("[{}] {}", items[i].source, items[i].headline))
            .collect();
        console_log!("breaking_news: cluster #{}: {:?}", ci, members);
    }

    let mut stories: Vec<Story> = clusters
        .into_iter()
        .filter_map(|cluster| {
            let mut sources: HashSet<&str> = HashSet::new();
            for &i in &cluster {
                sources.insert(items[i].source);
            }
            if sources.len() < MIN_SOURCES {
                return None;
            }
            let rep = cluster
                .iter()
                .min_by_key(|&&i| {
                    let live_penalty: u8 = if is_live_blog_url(&items[i].url) { 1 } else { 0 };
                    let date_key = if items[i].pub_date_ms == 0 {
                        u64::MAX
                    } else {
                        items[i].pub_date_ms
                    };
                    (live_penalty, date_key, items[i].headline.len())
                })?;
            Some(Story {
                headline: items[*rep].headline.clone(),
                url: items[*rep].url.clone(),
                source: items[*rep].source,
                sources: sources.len(),
            })
        })
        .collect();

    stories.sort_by(|a, b| b.sources.cmp(&a.sources));
    stories.truncate(MAX_STORIES);
    console_log!("breaking_news: returning {} stories", stories.len());
    stories
}

fn cluster_items(items: &[Item]) -> Vec<Vec<usize>> {
    let n = items.len();
    if n == 0 {
        return vec![];
    }
    let mut by_entity: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        for e in &it.entities {
            by_entity.entry(e.as_str()).or_default().push(i);
        }
    }
    let mut parent: Vec<usize> = (0..n).collect();
    let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut near_misses: Vec<(f32, usize, usize)> = Vec::new();
    for postings in by_entity.values() {
        if postings.len() < 2 {
            continue;
        }
        for i in 0..postings.len() {
            for j in (i + 1)..postings.len() {
                let a = postings[i].min(postings[j]);
                let b = postings[i].max(postings[j]);
                if items[a].source == items[b].source {
                    continue;
                }
                if !seen_pairs.insert((a, b)) {
                    continue;
                }
                let j_val = jaccard(&items[a].tokens, &items[b].tokens);
                if j_val >= JACCARD_THRESHOLD {
                    let ra = find(&mut parent, a);
                    let rb = find(&mut parent, b);
                    if ra != rb {
                        parent[ra] = rb;
                    }
                } else if j_val >= 0.15 {
                    near_misses.push((j_val, a, b));
                }
            }
        }
    }
    near_misses.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    for (j, a, b) in near_misses.iter().take(5) {
        console_log!(
            "breaking_news: near-miss j={:.3} [{}] {} ~~ [{}] {}",
            j,
            items[*a].source,
            items[*a].headline,
            items[*b].source,
            items[*b].headline
        );
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().filter(|g| g.len() >= 2).collect()
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let sb: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let inter = sa.intersection(&sb).count() as f32;
    let uni = sa.union(&sb).count() as f32;
    if uni == 0.0 {
        0.0
    } else {
        inter / uni
    }
}

async fn fetch_feed(source: &'static str, url: &str) -> Vec<Item> {
    let text = match get_text(url, BOT_UA, &[], TIMEOUT_DEFAULT_MS).await {
        Ok(t) => t,
        Err(e) => {
            console_log!("breaking_news: feed {} fetch failed: {:?}", source, e);
            return vec![];
        }
    };
    let mut out = Vec::new();
    for chunk in text.split("<item>").skip(1) {
        let raw_h = match extract_tag(chunk, "title") {
            Some(s) => strip_cdata(s).to_string(),
            None => continue,
        };
        let raw_u = match extract_tag(chunk, "link") {
            Some(s) => s.replace("&amp;", "&"),
            None => continue,
        };
        let raw_d = extract_tag(chunk, "description")
            .map(|s| strip_cdata(s).to_string())
            .unwrap_or_default();
        let pub_date_ms = extract_tag(chunk, "pubDate")
            .and_then(|s| parse_pub_date_ms(strip_cdata(s)))
            .unwrap_or(0);
        let entities = extract_entities(&raw_h, &raw_d);
        let tokens = make_tokens(&normalize(&raw_h));
        out.push(Item {
            headline: clean_headline(&raw_h),
            url: raw_u,
            source,
            pub_date_ms,
            entities,
            tokens,
        });
    }
    console_log!("breaking_news: {} parsed {}", source, out.len());
    out
}

fn extract_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

fn strip_cdata(s: &str) -> &str {
    s.strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(s)
}

const ENTITY_STOPS: &[&str] = &[
    "The", "A", "An", "But", "And", "Or", "For", "In", "On", "At", "To", "Of", "It", "Is",
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    "January", "February", "March", "April", "May", "June", "July", "August",
    "September", "October", "November", "December",
    "News", "Live", "Breaking", "Update", "Watch", "Read", "How", "Why", "What", "When", "Where",
    "Photos", "Video", "Opinion", "Editorial",
];

fn is_entity_stop(s: &str) -> bool {
    ENTITY_STOPS.iter().any(|w| *w == s)
}

fn extract_entities(headline: &str, description: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for text in [headline, description] {
        let bytes = text.as_bytes();
        let n = bytes.len();
        let mut i = 0;
        while i < n {
            if !bytes[i].is_ascii_uppercase() {
                i += 1;
                continue;
            }
            let word_start = i;
            i += 1;
            while i < n && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'\'') {
                i += 1;
            }
            let s = match std::str::from_utf8(&bytes[word_start..i]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if is_entity_stop(s) {
                continue;
            }
            let all_upper = s.bytes().all(|b| b.is_ascii_uppercase() || b == b'\'');
            if s.len() < 3 && !all_upper {
                continue;
            }
            if s.len() < 2 {
                continue;
            }
            let lc = s.to_lowercase();
            if lc.len() < 2 {
                continue;
            }
            if seen.insert(lc.clone()) {
                out.push(lc);
            }
        }
        let mut k = 0;
        let bn = bytes.len();
        while k < bn {
            if bytes[k].is_ascii_digit() {
                let s = k;
                while k < bn && (bytes[k].is_ascii_digit() || bytes[k] == b',' || bytes[k] == b'.') {
                    k += 1;
                }
                if let Ok(num) = std::str::from_utf8(&bytes[s..k]) {
                    if num.len() >= 2 {
                        let key = num.to_string();
                        if seen.insert(key.clone()) {
                            out.push(key);
                        }
                    }
                }
            } else {
                k += 1;
            }
        }
    }
    out
}

fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}

const STOPWORDS: &[&str] = &[
    "the","a","an","and","or","but","of","in","on","at","to","for","with","by","from","up","about","into","over","after","is","are","was","were","be","been","being","have","has","had","do","does","did","will","would","could","should","may","might","can","this","that","these","those","you","he","she","it","we","they","what","which","who","whom","whose","when","where","why","how","as","if","than","then","also","just","not","no","so","very","more","most","much","many","some","any","all","each","every","other","another","such","new","says","said","say","its","his","her","their","our","my","your"
];

fn is_stopword(s: &str) -> bool {
    STOPWORDS.iter().any(|w| *w == s)
}

fn stem(t: &str) -> String {
    let mut s = t.to_string();
    for suf in ["ies", "ing", "ed", "s"] {
        if s.len() > suf.len() + 2 && s.ends_with(suf) {
            s.truncate(s.len() - suf.len());
            if suf == "ies" {
                s.push('y');
            }
            return s;
        }
    }
    s
}

fn make_tokens(normalized: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for t in normalized.split_whitespace() {
        if t.len() < 2 || is_stopword(t) {
            continue;
        }
        let s = stem(t);
        if s.len() < 2 {
            continue;
        }
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    out
}

const SUFFIX_PATTERNS: &[&str] = &[
    " - BBC News", " | BBC News",
    " - The New York Times", " | The New York Times",
    " - The Guardian", " | The Guardian",
    " - NPR", " | NPR",
    " - CNN", " | CNN", " | CNN.com",
    " - Al Jazeera", " | Al Jazeera",
    " - The Washington Post", " | The Washington Post",
    " - Sky News", " | Sky News",
    " – live", " - live", " | live", " - Live Updates", " — live updates",
];

fn is_live_blog_url(url: &str) -> bool {
    let lc = url.to_ascii_lowercase();
    lc.contains("/live/") || lc.contains("live-updates") || lc.contains("live-news") || lc.contains("liveblog")
}

fn clean_headline(h: &str) -> String {
    let mut out = h.to_string();
    for s in SUFFIX_PATTERNS {
        if let Some(rest) = out.strip_suffix(s) {
            out = rest.to_string();
            break;
        }
    }
    out.trim().to_string()
}

fn parse_pub_date_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = match s.find(',') {
        Some(i) => s[i + 1..].trim(),
        None => s,
    };
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let day: i64 = parts[0].parse().ok()?;
    let month: i64 = match parts[1] {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4,
        "May" => 5, "Jun" => 6, "Jul" => 7, "Aug" => 8,
        "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts[2].parse().ok()?;
    let time: Vec<&str> = parts[3].split(':').collect();
    if time.len() < 2 {
        return None;
    }
    let h: i64 = time[0].parse().ok()?;
    let m: i64 = time[1].parse().ok()?;
    let sec: i64 = time.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let tz: i64 = match parts[4] {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        z if z.starts_with('+') || z.starts_with('-') => {
            let sign: i64 = if z.starts_with('-') { -1 } else { 1 };
            let n: i64 = z[1..].parse().ok()?;
            sign * ((n / 100) * 3600 + (n % 100) * 60)
        }
        _ => 0,
    };
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + h * 3600 + m * 60 + sec - tz;
    if secs < 0 {
        return None;
    }
    Some((secs as u64) * 1000)
}

