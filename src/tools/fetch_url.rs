use crate::util::{error_response, send_request_timed, strip_html_doc, BOT_UA, TIMEOUT_SLOW_MS};
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
    let headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    if let Some(a) = accept {
        headers.set("Accept", a)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = send_request_timed(request, TIMEOUT_SLOW_MS).await?;
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
    let (jina, raw) = futures::future::join(
        fetch_text(&reader, BOT_UA, Some("text/plain")),
        fetch_text(&body.url, BOT_UA, None),
    )
    .await;

    let jina_text = match &jina {
        Ok((status, text)) if !text.trim().is_empty() => Some((*status, text.clone())),
        _ => None,
    };
    let raw_stripped = match &raw {
        Ok((status, text)) => {
            let stripped = strip_html_doc(text);
            if stripped.trim().is_empty() {
                None
            } else {
                Some((*status, stripped))
            }
        }
        _ => None,
    };

    if let Some((status, text)) = &jina_text {
        if *status < 400 {
            return Response::from_json(&Resp {
                text: truncate(text, cap),
                source: "jina",
            });
        }
    }
    if let Some((status, text)) = &raw_stripped {
        if *status < 400 {
            return Response::from_json(&Resp {
                text: truncate(text, cap),
                source: "raw",
            });
        }
    }
    if let Some((_, text)) = jina_text {
        return Response::from_json(&Resp {
            text: truncate(&text, cap),
            source: "jina",
        });
    }
    if let Some((_, text)) = raw_stripped {
        return Response::from_json(&Resp {
            text: truncate(&text, cap),
            source: "raw",
        });
    }

    let detail = match (&jina, &raw) {
        (Err(e), _) => format!("Fetch failed: {}", e),
        (_, Err(e)) => format!("Fetch failed: {}", e),
        (Ok((s1, _)), Ok((s2, _))) => format!("Fetch failed: HTTP {}/{}", s1, s2),
    };
    error_response(detail)
}
