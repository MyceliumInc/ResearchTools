use crate::util::{error_response, get_json_status, BOT_UA};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    title: String,
}

#[derive(Serialize)]
struct Resp {
    summary: String,
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let slug = body.title.replace(' ', "_");
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        urlencoding::encode(&slug)
    );
    match get_json_status(&url, BOT_UA).await {
        Ok((404, _)) => Response::from_json(&Resp {
            summary: String::new(),
        }),
        Ok((status, _)) if status >= 400 => {
            error_response(format!("Wikipedia fetch failed: HTTP {}", status))
        }
        Ok((_, Some(v))) => {
            let description = v.get("description").and_then(|d| d.as_str()).unwrap_or("");
            let extract = v.get("extract").and_then(|d| d.as_str()).unwrap_or("");
            let mut parts = Vec::new();
            if !description.is_empty() {
                parts.push(description.to_string());
            }
            if !extract.is_empty() {
                parts.push(extract.to_string());
            }
            Response::from_json(&Resp {
                summary: parts.join("\n\n"),
            })
        }
        Ok((_, None)) => Response::from_json(&Resp {
            summary: String::new(),
        }),
        Err(e) => error_response(format!("Wikipedia fetch failed: {}", e)),
    }
}
