use crate::util::{error_response, strip_html_doc, BOT_UA};
use serde::{Deserialize, Serialize};
use worker::*;

const FETCH_CAP: usize = 3500;

#[derive(Deserialize)]
struct Req {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
}

#[derive(Serialize)]
struct Resp {
    text: String,
    source: &'static str,
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            break;
        }
        out.push(c);
    }
    out
}

async fn fetch_text(url: &str, user_agent: &str, accept: Option<&str>) -> Result<(u16, String)> {
    let mut headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    if let Some(a) = accept {
        headers.set("Accept", a)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    let status = resp.status_code();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let cap = body.max_chars.unwrap_or(FETCH_CAP).clamp(100, 50_000);

    let reader = format!("https://r.jina.ai/{}", body.url);
    match fetch_text(&reader, BOT_UA, Some("text/plain")).await {
        Ok((status, text)) if status < 400 => {
            let out = if text.is_empty() {
                "Empty page.".to_string()
            } else {
                truncate(&text, cap)
            };
            return Response::from_json(&Resp {
                text: out,
                source: "jina",
            });
        }
        _ => {}
    }

    match fetch_text(&body.url, BOT_UA, None).await {
        Ok((status, _)) if status >= 400 => {
            return error_response(format!("Fetch failed: HTTP {}", status));
        }
        Ok((_, raw)) => {
            let stripped = strip_html_doc(&raw);
            let out = if stripped.is_empty() {
                "Empty page.".to_string()
            } else {
                truncate(&stripped, cap)
            };
            Response::from_json(&Resp {
                text: out,
                source: "raw",
            })
        }
        Err(e) => error_response(format!("Fetch failed: {}", e)),
    }
}
