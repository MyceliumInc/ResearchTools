use crate::util::{cache_or, get_json, BOT_UA, TIMEOUT_DEFAULT_MS};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    forms: Option<Vec<String>>,
}

#[derive(Serialize)]
struct Item {
    accession: String,
    form: String,
    filed_date: String,
    company: String,
    ciks: Vec<String>,
    tickers: Vec<String>,
    url: String,
}

#[derive(Serialize)]
struct Resp {
    results: Vec<Item>,
}

pub async fn run(req: Request) -> Result<Response> {
    cache_or(req, "sec_filings", 300, execute).await
}

fn strip_leading_zeros(s: &str) -> String {
    let t = s.trim_start_matches('0');
    if t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    }
}

fn string_array(v: Option<&serde_json::Value>) -> Vec<String> {
    let mut out = Vec::new();
    match v {
        Some(serde_json::Value::Array(arr)) => {
            for x in arr {
                if let Some(s) = x.as_str() {
                    if !s.is_empty() {
                        out.push(s.to_string());
                    }
                }
            }
        }
        Some(serde_json::Value::String(s)) => {
            if !s.is_empty() {
                out.push(s.clone());
            }
        }
        _ => {}
    }
    out
}

async fn execute(raw: Vec<u8>) -> Result<Vec<u8>> {
    let body: Req = serde_json::from_slice(&raw)
        .map_err(|e| Error::RustError(format!("bad request: {}", e)))?;
    let q = body.query.trim();
    if q.is_empty() {
        return Ok(serde_json::to_vec(&Resp { results: vec![] })?);
    }
    let limit = body.limit.unwrap_or(10).clamp(1, 50);

    let mut url = format!(
        "https://efts.sec.gov/LATEST/search-index?q={}",
        urlencoding::encode(q)
    );
    if let Some(forms) = body.forms.as_ref() {
        let joined: Vec<String> = forms
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !joined.is_empty() {
            url.push_str(&format!("&forms={}", urlencoding::encode(&joined.join(","))));
        }
    }

    let json = get_json(&url, BOT_UA, TIMEOUT_DEFAULT_MS)
        .await
        .map_err(|e| Error::RustError(format!("SEC EDGAR failed: {}", e)))?;

    let mut out = Vec::new();
    if let Some(hits) = json
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|a| a.as_array())
    {
        for h in hits {
            let source = match h.get("_source") {
                Some(v) => v,
                None => continue,
            };
            let id = h.get("_id").and_then(|v| v.as_str()).unwrap_or("");
            let adsh = source
                .get("adsh")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let accession = if !adsh.is_empty() {
                adsh
            } else {
                // _id is often "<accession>:<primary_doc>"
                id.split(':').next().unwrap_or("").to_string()
            };
            let form = source
                .get("form")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let filed_date = source
                .get("file_date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let company = source
                .get("display_names")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    source
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                });
            let ciks = string_array(source.get("ciks"));
            let tickers = string_array(source.get("tickers"));

            let url = if !accession.is_empty() && !ciks.is_empty() {
                let stripped_accession = accession.replace('-', "");
                let cik = strip_leading_zeros(&ciks[0]);
                format!(
                    "https://www.sec.gov/Archives/edgar/data/{}/{}/{}-index.htm",
                    cik, stripped_accession, accession
                )
            } else {
                String::new()
            };

            out.push(Item {
                accession,
                form,
                filed_date,
                company,
                ciks,
                tickers,
                url,
            });
            if out.len() >= limit {
                break;
            }
        }
    }

    Ok(serde_json::to_vec(&Resp { results: out })?)
}
