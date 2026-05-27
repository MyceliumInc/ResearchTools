use crate::util::{
    cache_or, send_request_timed, strip_tags, BOT_UA, TIMEOUT_DEFAULT_MS,
};
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

pub async fn run(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let exa = ctx
        .secret("EXA_API_KEY")
        .ok()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    cache_or(req, "search_web", 300, move |body| execute(body, exa)).await
}

async fn execute(raw: Vec<u8>, exa: Option<String>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let query = body.query.trim();
    if query.is_empty() {
        return Err(Error::RustError("Search failed: empty query".into()));
    }
    let limit = body.limit.unwrap_or(8).clamp(1, 20);

    let results = match exa {
        Some(key) => search_exa(query, limit, &key).await?,
        None => search_ddg(query, limit).await?,
    };

    Ok(serde_json::to_vec(&Resp { results })?)
}

async fn search_exa(query: &str, limit: usize, key: &str) -> Result<Vec<WebResult>> {
    let payload = serde_json::json!({
        "query": query,
        "type": "auto",
        "numResults": limit,
        "contents": { "highlights": true },
    });

    let headers = Headers::new();
    headers.set("User-Agent", BOT_UA)?;
    headers.set("Accept", "application/json")?;
    headers.set("Content-Type", "application/json")?;
    headers.set("x-api-key", key)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(payload.to_string().into()));

    let request = Request::new_with_init("https://api.exa.ai/search", &init)
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;
    let mut resp = send_request_timed(request, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;
    if resp.status_code() >= 400 {
        return Err(Error::RustError(format!(
            "Search failed: HTTP {}",
            resp.status_code()
        )));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| Error::RustError(format!("Search failed: bad json: {}", e)))?;

    let mut results: Vec<WebResult> = Vec::new();
    if let Some(arr) = json.get("results").and_then(|v| v.as_array()) {
        for item in arr.iter().take(limit) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .map(strip_tags)
                .unwrap_or_default();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let snippet = item
                .get("highlights")
                .and_then(|v| v.as_array())
                .map(|hs| {
                    hs.iter()
                        .filter_map(|h| h.as_str())
                        .map(strip_tags)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" … ")
                })
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    item.get("summary")
                        .and_then(|v| v.as_str())
                        .map(strip_tags)
                })
                .unwrap_or_default();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            results.push(WebResult { title, url, snippet });
        }
    }
    Ok(results)
}

async fn search_ddg(query: &str, limit: usize) -> Result<Vec<WebResult>> {
    let form = format!("q={}&b=&kl=us-en", urlencoding::encode(query));

    let headers = Headers::new();
    headers.set("User-Agent", BOT_UA)?;
    headers.set(
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    )?;
    headers.set("Accept-Language", "en-US,en;q=0.9")?;
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;
    headers.set("Origin", "https://html.duckduckgo.com")?;
    headers.set("Referer", "https://html.duckduckgo.com/")?;

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
        return Err(Error::RustError(format!(
            "Search failed: HTTP {}",
            resp.status_code()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::RustError(format!("Search failed: {}", e)))?;

    let mut results = parse_ddg(&html);
    results.truncate(limit);
    Ok(results)
}

fn parse_ddg(html: &str) -> Vec<WebResult> {
    let mut anchors: Vec<WebResult> = Vec::new();
    let mut snippets: Vec<String> = Vec::new();

    let mut i = 0;
    while let Some(rel) = html[i..].find("<a") {
        let start = i + rel;
        let after_a = start + 2;
        let tag_end = match html[after_a..].find('>') {
            Some(p) => after_a + p,
            None => break,
        };
        let tag = &html[start..tag_end];
        if has_class(tag, "result-link") {
            let href = extract_attr(tag, "href");
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</a>") {
                Some(p) => body_start + p,
                None => break,
            };
            let title = strip_tags(&html[body_start..body_end]);
            let url = unwrap_ddg_redirect(&href);
            if !url.is_empty() && !title.is_empty() && !is_ddg_promo(&href) {
                anchors.push(WebResult { title, url, snippet: String::new() });
            }
            i = body_end + 4;
        } else {
            i = tag_end + 1;
        }
    }

    let mut j = 0;
    while let Some(rel) = html[j..].find("<td") {
        let start = j + rel;
        let after = start + 3;
        let tag_end = match html[after..].find('>') {
            Some(p) => after + p,
            None => break,
        };
        let tag = &html[start..tag_end];
        if has_class(tag, "result-snippet") {
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</td>") {
                Some(p) => body_start + p,
                None => break,
            };
            snippets.push(strip_tags(&html[body_start..body_end]));
            j = body_end + 5;
        } else {
            j = tag_end + 1;
        }
    }

    for (idx, a) in anchors.iter_mut().enumerate() {
        if let Some(s) = snippets.get(idx) {
            a.snippet = s.clone();
        }
    }
    anchors
}

fn has_class(tag: &str, name: &str) -> bool {
    tag.contains(&format!("class=\"{}\"", name))
        || tag.contains(&format!("class='{}'", name))
}

fn extract_attr(tag: &str, name: &str) -> String {
    for delim in ['"', '\''] {
        let needle = format!("{}={}", name, delim);
        if let Some(start) = tag.find(&needle) {
            let after = &tag[start + needle.len()..];
            if let Some(end) = after.find(delim) {
                return after[..end].to_string();
            }
        }
    }
    String::new()
}

fn is_ddg_promo(href: &str) -> bool {
    href.contains("duckduckgo.com/y.js")
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
