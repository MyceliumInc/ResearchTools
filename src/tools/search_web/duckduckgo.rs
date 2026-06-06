use super::WebResult;
use crate::http::{build_request, send_request_timed, BROWSER_UA, TIMEOUT_FAST_MS};
use crate::text::{extract_attr, has_class, strip_tags};
use worker::*;

pub async fn search(query: &str, limit: usize) -> Result<Vec<WebResult>> {
    let form = format!("q={}&b=&kl=us-en", urlencoding::encode(query));
    let headers = [
        ("User-Agent", BROWSER_UA),
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Origin", "https://html.duckduckgo.com"),
        ("Referer", "https://html.duckduckgo.com/"),
    ];
    let request = build_request(
        "https://lite.duckduckgo.com/lite/",
        Method::Post,
        &headers,
        Some(form),
    )
    .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;

    let mut response = send_request_timed(request, TIMEOUT_FAST_MS)
        .await
        .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;
    if response.status_code() >= 400 {
        return Err(Error::RustError(format!(
            "Search failed: HTTP {}",
            response.status_code()
        )));
    }
    let html = response
        .text()
        .await
        .map_err(|error| Error::RustError(format!("Search failed: {}", error)))?;

    let mut results = parse(&html);
    results.truncate(limit);
    Ok(results)
}

fn parse(html: &str) -> Vec<WebResult> {
    let mut anchors: Vec<WebResult> = Vec::new();
    let mut snippets: Vec<String> = Vec::new();

    let mut cursor = 0;
    while let Some(offset) = html[cursor..].find("<a") {
        let start = cursor + offset;
        let after_open = start + 2;
        let tag_end = match html[after_open..].find('>') {
            Some(position) => after_open + position,
            None => break,
        };
        let tag = &html[start..tag_end];
        if has_class(tag, "result-link") {
            let href = extract_attr(tag, "href");
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</a>") {
                Some(position) => body_start + position,
                None => break,
            };
            let title = strip_tags(&html[body_start..body_end]);
            let url = unwrap_redirect(&href);
            if !url.is_empty() && !title.is_empty() && !is_promo(&href) {
                anchors.push(WebResult {
                    title,
                    url,
                    snippet: String::new(),
                });
            }
            cursor = body_end + 4;
        } else {
            cursor = tag_end + 1;
        }
    }

    let mut cursor = 0;
    while let Some(offset) = html[cursor..].find("<td") {
        let start = cursor + offset;
        let after_open = start + 3;
        let tag_end = match html[after_open..].find('>') {
            Some(position) => after_open + position,
            None => break,
        };
        let tag = &html[start..tag_end];
        if has_class(tag, "result-snippet") {
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</td>") {
                Some(position) => body_start + position,
                None => break,
            };
            snippets.push(strip_tags(&html[body_start..body_end]));
            cursor = body_end + 5;
        } else {
            cursor = tag_end + 1;
        }
    }

    for (index, anchor) in anchors.iter_mut().enumerate() {
        if let Some(snippet) = snippets.get(index) {
            anchor.snippet = snippet.clone();
        }
    }
    anchors
}

fn is_promo(href: &str) -> bool {
    href.contains("duckduckgo.com/y.js")
}

fn unwrap_redirect(raw: &str) -> String {
    if let Some(index) = raw.find("uddg=") {
        let rest = &raw[index + 5..];
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
