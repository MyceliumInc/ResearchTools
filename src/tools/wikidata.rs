use crate::util::{cache_or, error_response, get_text, BOT_UA, TIMEOUT_SLOW_MS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
}

#[derive(Serialize)]
struct Resp {
    head: Head,
    rows: Vec<HashMap<String, String>>,
}

#[derive(Serialize)]
struct Head {
    vars: Vec<String>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "wikidata_sparql", 600, execute).await
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let q = body.query.trim();
    if q.is_empty() {
        return Ok(serde_json::to_vec(&Resp {
            head: Head { vars: vec![] },
            rows: vec![],
        })?);
    }
    if q.len() > 8000 {
        // Surface as soft error JSON.
        let resp = error_response("SPARQL query exceeds 8000 char limit")?;
        let body = resp_to_bytes(resp).await?;
        return Ok(body);
    }

    let url = format!(
        "https://query.wikidata.org/sparql?format=json&query={}",
        urlencoding::encode(q)
    );
    let text = match get_text(
        &url,
        BOT_UA,
        &[("Accept", "application/sparql-results+json")],
        TIMEOUT_SLOW_MS,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            let resp = error_response(format!("Wikidata SPARQL failed: {}", e))?;
            return resp_to_bytes(resp).await;
        }
    };

    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let resp = error_response(format!("Wikidata returned non-JSON: {}", e))?;
            return resp_to_bytes(resp).await;
        }
    };

    let vars: Vec<String> = v
        .get("head")
        .and_then(|h| h.get("vars"))
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut rows: Vec<HashMap<String, String>> = Vec::new();
    if let Some(bindings) = v
        .get("results")
        .and_then(|r| r.get("bindings"))
        .and_then(|b| b.as_array())
    {
        for b in bindings {
            let mut row: HashMap<String, String> = HashMap::new();
            if let Some(obj) = b.as_object() {
                for (k, val) in obj {
                    let s = val
                        .get("value")
                        .and_then(|vv| vv.as_str())
                        .unwrap_or("")
                        .to_string();
                    row.insert(k.clone(), s);
                }
            }
            rows.push(row);
        }
    }

    Ok(serde_json::to_vec(&Resp {
        head: Head { vars },
        rows,
    })?)
}

async fn resp_to_bytes(mut r: Response) -> Result<Vec<u8>> {
    let text = r.text().await?;
    Ok(text.into_bytes())
}
