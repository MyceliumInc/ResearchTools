use super::WebResult;
use crate::http::{get_text, BROWSER_UA, TIMEOUT_FAST_MS};
use crate::text::{extract_attr, has_class, strip_tags};
use worker::*;

pub async fn search(query: &str, limit: usize) -> Result<Vec<WebResult>> {
    let url = format!(
        "https://www.mojeek.com/search?q={}",
        urlencoding::encode(query)
    );
    let html = get_text(&url, BROWSER_UA, &[], TIMEOUT_FAST_MS)
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
        if has_class(tag, "title") {
            let url = extract_attr(tag, "href");
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</a>") {
                Some(position) => body_start + position,
                None => break,
            };
            let title = strip_tags(&html[body_start..body_end]);
            if url.starts_with("http") && !title.is_empty() {
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
    while let Some(offset) = html[cursor..].find("<p") {
        let start = cursor + offset;
        let after_open = start + 2;
        let tag_end = match html[after_open..].find('>') {
            Some(position) => after_open + position,
            None => break,
        };
        let tag = &html[start..tag_end];
        if has_class(tag, "s") {
            let body_start = tag_end + 1;
            let body_end = match html[body_start..].find("</p>") {
                Some(position) => body_start + position,
                None => break,
            };
            snippets.push(strip_tags(&html[body_start..body_end]));
            cursor = body_end + 4;
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
