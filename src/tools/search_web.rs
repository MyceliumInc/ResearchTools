use crate::util::{cache_or, send_request_timed, strip_tags, TIMEOUT_DEFAULT_MS, UA};
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

fn extract_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {}=", name);
    let lower = tag.to_ascii_lowercase();
    let start = lower.find(&needle)?;
    let rest = &tag[start + needle.len()..];
    let (quote, body) = match rest.as_bytes().first()? {
        b'"' => ('"', &rest[1..]),
        b'\'' => ('\'', &rest[1..]),
        _ => return None,
    };
    let end = body.find(quote)?;
    Some(&body[..end])
}

fn find_blocks<'a>(html: &'a str, elem: &str, class: &str) -> Vec<(&'a str, &'a str)> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{}", elem);
    let close = format!("</{}>", elem);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = lower[i..].find(&open) {
        let tag_start = i + rel;
        let tag_end = match html[tag_start..].find('>') {
            Some(p) => tag_start + p,
            None => break,
        };
        let tag = &html[tag_start..=tag_end];
        let body_start = tag_end + 1;
        let body_end = match lower[body_start..].find(&close) {
            Some(p) => body_start + p,
            None => break,
        };
        let body = &html[body_start..body_end];
        let cls = extract_attr(tag, "class").unwrap_or("");
        if cls.split_ascii_whitespace().any(|c| c == class) {
            out.push((tag, body));
        }
        i = body_end + close.len();
    }
    out
}

fn parse_ddg(html: &str) -> Vec<WebResult> {
    let mut anchors: Vec<(String, String)> = Vec::new();
    for (tag, body) in find_blocks(html, "a", "result-link") {
        let href = extract_attr(tag, "href").unwrap_or("");
        if href.to_ascii_lowercase().contains("duckduckgo.com/y.js") {
            continue;
        }
        anchors.push((href.to_string(), strip_tags(body)));
    }

    let snippets: Vec<String> = find_blocks(html, "td", "result-snippet")
        .into_iter()
        .map(|(_, body)| strip_tags(body))
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

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "search_web", 300, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
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

    let request = Request::new_with_init("https://lite.duckduckgo.com/lite/", &init)
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;
    let mut resp = send_request_timed(request, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;
    if resp.status_code() >= 400 {
        return Err(Error::RustError(format!("Search failed: HTTP {}", resp.status_code())));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;
    let mut results = parse_ddg(&html);
    results.truncate(limit);
    Ok(serde_json::to_vec(&Resp { results })?)
}
