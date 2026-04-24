use crate::util::{error_response, strip_tags, UA};
use regex::Regex;
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct WebResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<WebResult>,
}

fn unwrap_ddg_redirect(raw: &str) -> String {
    if let Some(idx) = raw.find("uddg=") {
        let rest = &raw[idx + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        if let Ok(decoded) = urlencoding::decode(&rest[..end]) {
            return decoded.into_owned();
        }
        return String::new();
    }
    if raw.starts_with("http") {
        return raw.to_string();
    }
    if raw.starts_with("//") {
        return format!("https:{}", raw);
    }
    String::new()
}

fn parse_ddg(html: &str) -> Vec<WebResult> {
    let anchor_re = Regex::new(
        r#"(?is)<a[^>]*href="([^"]+)"[^>]*class=['"]result-link['"][^>]*>(.*?)</a>"#,
    )
    .unwrap();
    let snippet_re =
        Regex::new(r#"(?is)<td[^>]*class=['"]result-snippet['"][^>]*>(.*?)</td>"#).unwrap();
    let ddg_js = Regex::new(r"(?i)duckduckgo\.com/y\.js").unwrap();

    let mut anchors: Vec<(String, String)> = Vec::new();
    for cap in anchor_re.captures_iter(html) {
        let raw_url = cap.get(1).map_or("", |m| m.as_str());
        if ddg_js.is_match(raw_url) {
            continue;
        }
        let title = strip_tags(cap.get(2).map_or("", |m| m.as_str()));
        anchors.push((raw_url.to_string(), title));
    }

    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .map(|c| strip_tags(c.get(1).map_or("", |m| m.as_str())))
        .collect();

    let mut results = Vec::new();
    for (i, (raw_url, title)) in anchors.into_iter().enumerate() {
        let url = unwrap_ddg_redirect(&raw_url);
        if url.is_empty() || title.is_empty() {
            continue;
        }
        results.push(WebResult {
            title,
            url,
            snippet: snippets.get(i).cloned().unwrap_or_default(),
        });
    }
    results
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let limit = body.limit.unwrap_or(8).clamp(1, 25);
    let form = format!("q={}&b=&kl=us-en", urlencoding::encode(&body.query));

    let headers = Headers::new();
    headers.set("User-Agent", UA)?;
    headers.set(
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
    )?;
    headers.set("Accept-Language", "en-US,en;q=0.9")?;
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;
    headers.set("Origin", "https://html.duckduckgo.com")?;
    headers.set("Referer", "https://html.duckduckgo.com/")?;
    headers.set("Upgrade-Insecure-Requests", "1")?;
    headers.set("Sec-Fetch-Dest", "document")?;
    headers.set("Sec-Fetch-Mode", "navigate")?;
    headers.set("Sec-Fetch-Site", "same-origin")?;
    headers.set("Sec-Fetch-User", "?1")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(form.into()));

    let request = match Request::new_with_init("https://lite.duckduckgo.com/lite/", &init) {
        Ok(r) => r,
        Err(e) => return error_response(format!("Search failed: {}", e)),
    };
    let mut resp = match Fetch::Request(request).send().await {
        Ok(r) => r,
        Err(e) => return error_response(format!("Search failed: {}", e)),
    };
    if resp.status_code() >= 400 {
        return error_response(format!("Search failed: HTTP {}", resp.status_code()));
    }
    let html = match resp.text().await {
        Ok(t) => t,
        Err(e) => return error_response(format!("Search failed: {}", e)),
    };
    let mut results = parse_ddg(&html);
    results.truncate(limit);
    Response::from_json(&Resp { results })
}
