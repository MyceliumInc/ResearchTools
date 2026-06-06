use crate::http::{build_request, error_response, send_request_timed, BOT_UA, TIMEOUT_SLOW_MS};
use crate::text::{strip_html_doc, truncate};
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

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(value) => value,
        Err(error) => return error_response(format!("bad request: {}", error)),
    };
    let cap = body.max_chars.unwrap_or(FETCH_CAP).clamp(100, 50_000);

    let reader = format!("https://r.jina.ai/{}", body.url);
    let (jina, raw) = futures::future::join(
        fetch_text(&reader, Some("text/plain")),
        fetch_text(&body.url, None),
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
        (Err(error), _) => format!("Fetch failed: {}", error),
        (_, Err(error)) => format!("Fetch failed: {}", error),
        (Ok((jina_status, _)), Ok((raw_status, _))) => {
            format!("Fetch failed: HTTP {}/{}", jina_status, raw_status)
        }
    };
    error_response(detail)
}

async fn fetch_text(url: &str, accept: Option<&str>) -> Result<(u16, String)> {
    let mut headers = vec![("User-Agent", BOT_UA)];
    if let Some(value) = accept {
        headers.push(("Accept", value));
    }
    let request = build_request(url, Method::Get, &headers, None)?;
    let mut response = send_request_timed(request, TIMEOUT_SLOW_MS).await?;
    let status = response.status_code();
    let text = response.text().await.unwrap_or_default();
    Ok((status, text))
}
