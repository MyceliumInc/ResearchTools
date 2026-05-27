use crate::util::{get_text, send_request_timed, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use worker::*;

#[derive(Serialize, Deserialize, Clone)]
struct RawData {
    headline: String,
    url: String,
    description: Option<String>,
    pub_date_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct Raw {
    embedding: Vec<f32>,
    raw_data: RawData,
}

#[derive(Serialize)]
struct ReturnThis {
    stories: Vec<Story>,
}

#[derive(Serialize)]
struct Story {
    headline: String,
    url: String,
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

fn compute_epsilon(articles: &[Raw], percentile: f32) -> f32 {
    let mut distances = Vec::new();
    for i in 0..articles.len() {
        for j in (i + 1)..articles.len() {
            distances.push(cosine_distance(
                &articles[i].embedding,
                &articles[j].embedding,
            ));
        }
    }
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((distances.len() as f32) * percentile).floor() as usize;
    distances.get(idx).cloned().unwrap_or(0.3)
}

pub async fn run(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let api_key = ctx
        .secret("OPENROUTER_API_KEY")
        .map_err(|_| Error::RustError("missing OPENROUTER_API_KEY".to_string()))?
        .to_string();
    let articles = fetch_articles().await;
    if articles.is_empty() {
        return Response::from_json(&ReturnThis { stories: vec![] });
    }
    let embedded = embed_articles(&articles, &api_key).await?;
    let epsilon = compute_epsilon(&embedded, 0.1);
    let clusters = cluster(&embedded, epsilon, 2);
    let stories = pick_representatives(clusters, &embedded);
    let body = ReturnThis { stories };
    Response::from_json(&body)
}

const FEEDS: &[&str] = &[
    "https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en",
    "https://feeds.bbci.co.uk/news/rss.xml",
    "https://rss.nytimes.com/services/xml/rss/nyt/HomePage.xml",
    "https://feeds.npr.org/1001/rss.xml",
    "https://www.theguardian.com/world/rss",
    "http://rss.cnn.com/rss/cnn_topstories.rss",
    "https://feeds.washingtonpost.com/rss/world",
    "https://www.aljazeera.com/xml/rss/all.xml",
];

const SIX_HOURS_MS: u64 = 6 * 60 * 60 * 1000;

async fn fetch_articles() -> Vec<RawData> {
    let futures: Vec<_> = FEEDS.iter().map(|url| fetch_feed(url)).collect();
    let results = futures::future::join_all(futures).await;
    console_log!(
        "feed results: {} feeds, articles: {}",
        results.len(),
        results.iter().map(|v| v.len()).sum::<usize>()
    );

    let now = Date::now().as_millis();
    let cutoff = now.saturating_sub(SIX_HOURS_MS);

    let mut seen_urls = std::collections::HashSet::new();
    let mut articles = Vec::new();

    for feed_articles in results {
        for article in feed_articles {
            if article.pub_date_ms.unwrap_or(cutoff) >= cutoff
                && seen_urls.insert(article.url.clone())
            {
                articles.push(article);
            }
        }
    }

    articles
}

async fn fetch_feed(url: &str) -> Vec<RawData> {
    let text = match get_text(url, BOT_UA, &[], TIMEOUT_DEFAULT_MS).await {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let mut articles = Vec::new();

    for chunk in text.split("<item>").skip(1) {
        let headline = extract_tag(&chunk, "title");
        let url = extract_tag(&chunk, "link");
        let description = extract_tag(&chunk, "description");
        let pub_date = extract_tag(&chunk, "pubDate").and_then(|s| parse_pub_date_ms(strip_cdata(s)));
        if let (Some(h), Some(u)) = (headline, url) {
            let raw_data: RawData = RawData {
                headline: strip_cdata(h).to_string(),
                url: u.replace("&amp;", "&"),
                description: description.map(|s| strip_cdata(s).to_string()),
                pub_date_ms: pub_date,
            };
            articles.push(raw_data);
        }
    }

    articles
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

async fn embed_articles(articles: &[RawData], api_key: &str) -> Result<Vec<Raw>> {
    let headlines: Vec<&str> = articles.iter().map(|a| a.headline.as_str()).collect();
    let body = serde_json::json!({
    "model": "openai/text-embedding-3-small",
    "input": headlines,
    });
    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", api_key.to_string()))?;
    headers.set("Content-Type", "application/json")?;
    let mut req = RequestInit::new();
    req.with_method(Method::Post).with_headers(headers);
    req.with_body(Some(serde_json::to_string(&body)?.into()));
    let request = Request::new_with_init("https://openrouter.ai/api/v1/embeddings", &req)?;
    let mut response: Response = send_request_timed(request, TIMEOUT_DEFAULT_MS).await?;
    let json: serde_json::Value = serde_json::from_str(&response.text().await?)?;
    let data = json["data"]
        .as_array()
        .ok_or_else(|| Error::RustError("no data in response".to_string()))?;
    let raw: Vec<Raw> = data
        .iter()
        .zip(articles.iter())
        .map(|(embedding, article)| {
            let embedding_vec = embedding["embedding"]
                .as_array()
                .ok_or_else(|| Error::RustError("no embedding in response".to_string()))?;
            let embedding_f32: Vec<f32> = embedding_vec
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            Ok(Raw {
                embedding: embedding_f32,
                raw_data: article.clone(),
            })
        })
        .collect::<Result<Vec<Raw>, Error>>()?;
    Ok(raw)
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a * mag_b == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (mag_a * mag_b))
}

fn cluster(articles: &[Raw], epsilon: f32, min_points: usize) -> Vec<Vec<usize>> {
    let mut visited: Vec<bool> = vec![false; articles.len()];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for (i, article) in articles.iter().enumerate() {
        if visited[i] == true {
            continue;
        }
        let neighbors = (0..articles.len())
            .filter(|&j| cosine_distance(&article.embedding, &articles[j].embedding) < epsilon)
            .collect::<Vec<usize>>();
        if (neighbors.len() < min_points) {
            continue;
        }
        let mut cluster = vec![i];
        let mut queue: VecDeque<usize> = neighbors.into();
        visited[i] = true;
        while let Some(j) = queue.pop_front() {
            if (visited[j] == true) {
                continue;
            }
            visited[j] = true;
            cluster.push(j);

            let j_neighbors: Vec<usize> = (0..articles.len())
                .filter(|&k| {
                    cosine_distance(&articles[k].embedding, &articles[j].embedding) < epsilon
                })
                .collect::<Vec<usize>>();
            if (j_neighbors.len() >= min_points) {
                queue.extend(j_neighbors)
            }
        }
        clusters.push(cluster);
    }
    clusters
}

fn pick_representatives(clusters: Vec<Vec<usize>>, articles: &[Raw]) -> Vec<Story> {
    let mut stories: Vec<Story> = Vec::new();

    for cluster in clusters {
        let mut centroid: Vec<f32> = vec![0.0f32; articles[cluster[0]].embedding.len()];
        for &idx in &cluster {
            for (i, &val) in articles[idx].embedding.iter().enumerate() {
                centroid[i] += val;
            }
        }
        for val in &mut centroid {
            *val /= cluster.len() as f32;
        }
        let rep = cluster.iter().min_by(|&&a, &&b| {
            cosine_distance(&articles[a].embedding, &centroid)
                .partial_cmp(&cosine_distance(&articles[b].embedding, &centroid))
                .unwrap()
        });

        if let Some(&idx) = rep {
            stories.push(Story {
                headline: articles[idx].raw_data.headline.clone(),
                url: articles[idx].raw_data.url.clone(),
            });
        }
    }
    stories
}
