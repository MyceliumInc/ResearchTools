use serde::Serialize;
use worker::*;

pub const UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub const BOT_UA: &str = "MyceliumBot/1.0";

pub fn error_response(message: impl Into<String>) -> Result<Response> {
    #[derive(Serialize)]
    struct E {
        error: String,
    }
    Response::from_json(&E { error: message.into() })
}

pub async fn get_text(url: &str, user_agent: &str, extra: &[(&str, &str)]) -> Result<String> {
    let headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    for (k, v) in extra {
        headers.set(k, v)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    if resp.status_code() >= 400 {
        return Err(Error::RustError(format!("HTTP {}", resp.status_code())));
    }
    resp.text().await
}

pub async fn get_json(url: &str, user_agent: &str) -> Result<serde_json::Value> {
    let text = get_text(
        url,
        user_agent,
        &[("Accept", "application/json")],
    )
    .await?;
    serde_json::from_str(&text).map_err(|e| Error::RustError(format!("bad json: {}", e)))
}

pub async fn get_json_status(
    url: &str,
    user_agent: &str,
) -> Result<(u16, Option<serde_json::Value>)> {
    let headers = Headers::new();
    headers.set("User-Agent", user_agent)?;
    headers.set("Accept", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    let status = resp.status_code();
    if status >= 400 {
        return Ok((status, None));
    }
    let text = resp.text().await?;
    let v = serde_json::from_str(&text)
        .map_err(|e| Error::RustError(format!("bad json: {}", e)))?;
    Ok((status, Some(v)))
}

pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn strip_html_doc(s: &str) -> String {
    // Remove <script>...</script> and <style>...</style> blocks first.
    let re_script = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let re_style = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let stage1 = re_script.replace_all(s, "");
    let stage2 = re_style.replace_all(&stage1, "");
    strip_tags(&stage2)
}

pub fn to_number(raw: Option<&serde_json::Value>) -> f64 {
    match raw {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn dollars_to_cents(raw: Option<&serde_json::Value>) -> i64 {
    let n = to_number(raw);
    if n.is_finite() {
        (n * 100.0).round() as i64
    } else {
        0
    }
}

pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

pub fn score(haystack: &str, needles: &[String]) -> f64 {
    if needles.is_empty() {
        return 0.0;
    }
    let hay = haystack.to_lowercase();
    let mut hits = 0;
    for n in needles {
        if hay.contains(n) {
            hits += 1;
        }
    }
    hits as f64 / needles.len() as f64
}
