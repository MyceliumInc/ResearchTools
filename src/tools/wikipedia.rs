use crate::util::{cache_or, get_json_status, BOT_UA, TIMEOUT_FAST_MS};
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

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "wikipedia_summary", 600, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let slug = body.title.replace(' ', "_");
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        urlencoding::encode(&slug)
    );
    let resp = match get_json_status(&url, BOT_UA, TIMEOUT_FAST_MS).await {
        Ok((404, _)) | Ok((_, None)) => Resp { summary: String::new() },
        Ok((status, _)) if status >= 400 => {
            return Err(Error::RustError(format!("Wikipedia fetch failed: HTTP {}", status)));
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
            Resp { summary: parts.join("\n\n") }
        }
        Err(e) => return Err(Error::RustError(format!("Wikipedia fetch failed: {}", e))),
    };
    Ok(serde_json::to_vec(&resp)?)
}
