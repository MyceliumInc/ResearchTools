mod dates;
mod extract;

use crate::cache::cache_or;
use dates::parse_pub_date_ms;
use extract::{
    clean_headline, extract_entities, is_live_blog_url, jaccard, make_tokens, normalize,
};
use crate::http::{get_text, BOT_UA, TIMEOUT_DEFAULT_MS};
use crate::text::{extract_xml_tag, strip_cdata};
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
    cache_or(req, "breaking", 30, |_body| async move {
        let stories = pipeline().await;
        serde_json::to_vec(&Out { stories })
            .map_err(|error| Error::RustError(format!("serialize: {}", error)))
    })
    .await
}

async fn pipeline() -> Vec<Story> {
    let pending: Vec<_> = FEEDS.iter().map(|(name, url)| fetch_feed(name, url)).collect();
    let batches = futures::future::join_all(pending).await;

    let cutoff = Date::now().as_millis().saturating_sub(CUTOFF_MS);
    let mut items: Vec<Item> = Vec::new();
    let mut seen_urls = HashSet::new();
    for batch in batches {
        for item in batch {
            if item.pub_date_ms != 0 && item.pub_date_ms < cutoff {
                continue;
            }
            if !seen_urls.insert(item.url.clone()) {
                continue;
            }
            items.push(item);
        }
    }
    if items.is_empty() {
        return vec![];
    }

    let clusters = cluster_items(&items);
    let mut stories: Vec<Story> = clusters
        .into_iter()
        .filter_map(|cluster| {
            let mut sources: HashSet<&str> = HashSet::new();
            for &index in &cluster {
                sources.insert(items[index].source);
            }
            if sources.len() < MIN_SOURCES {
                return None;
            }
            let representative = cluster.iter().min_by_key(|&&index| {
                let live_penalty: u8 = if is_live_blog_url(&items[index].url) { 1 } else { 0 };
                let date_key = if items[index].pub_date_ms == 0 {
                    u64::MAX
                } else {
                    items[index].pub_date_ms
                };
                (live_penalty, date_key, items[index].headline.len())
            })?;
            Some(Story {
                headline: items[*representative].headline.clone(),
                url: items[*representative].url.clone(),
                source: items[*representative].source,
                sources: sources.len(),
            })
        })
        .collect();

    stories.sort_by_key(|story| std::cmp::Reverse(story.sources));
    stories.truncate(MAX_STORIES);
    stories
}

fn cluster_items(items: &[Item]) -> Vec<Vec<usize>> {
    let count = items.len();
    if count == 0 {
        return vec![];
    }
    let mut by_entity: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        for entity in &item.entities {
            by_entity.entry(entity.as_str()).or_default().push(index);
        }
    }
    let mut parent: Vec<usize> = (0..count).collect();
    let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
    for postings in by_entity.values() {
        if postings.len() < 2 {
            continue;
        }
        for first in 0..postings.len() {
            for second in (first + 1)..postings.len() {
                let lower = postings[first].min(postings[second]);
                let higher = postings[first].max(postings[second]);
                if items[lower].source == items[higher].source {
                    continue;
                }
                if !seen_pairs.insert((lower, higher)) {
                    continue;
                }
                if jaccard(&items[lower].tokens, &items[higher].tokens) >= JACCARD_THRESHOLD {
                    let root_lower = find(&mut parent, lower);
                    let root_higher = find(&mut parent, higher);
                    if root_lower != root_higher {
                        parent[root_lower] = root_higher;
                    }
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..count {
        let root = find(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }
    groups.into_values().filter(|group| group.len() >= 2).collect()
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

async fn fetch_feed(source: &'static str, url: &str) -> Vec<Item> {
    let text = match get_text(url, BOT_UA, &[], TIMEOUT_DEFAULT_MS).await {
        Ok(text) => text,
        Err(error) => {
            console_log!("breaking_news: feed {} fetch failed: {:?}", source, error);
            return vec![];
        }
    };
    let mut out = Vec::new();
    for chunk in text.split("<item>").skip(1) {
        let headline_raw = match extract_xml_tag(chunk, "title") {
            Some(value) => strip_cdata(value).to_string(),
            None => continue,
        };
        let link = match extract_xml_tag(chunk, "link") {
            Some(value) => value.replace("&amp;", "&"),
            None => continue,
        };
        let description = extract_xml_tag(chunk, "description")
            .map(|value| strip_cdata(value).to_string())
            .unwrap_or_default();
        let pub_date_ms = extract_xml_tag(chunk, "pubDate")
            .and_then(|value| parse_pub_date_ms(strip_cdata(value)))
            .unwrap_or(0);
        let entities = extract_entities(&headline_raw, &description);
        let tokens = make_tokens(&normalize(&headline_raw));
        out.push(Item {
            headline: clean_headline(&headline_raw),
            url: link,
            source,
            pub_date_ms,
            entities,
            tokens,
        });
    }
    out
}
