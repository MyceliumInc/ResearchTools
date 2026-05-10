use crate::util::{error_response, get_text, BOT_UA, TIMEOUT_DEFAULT_MS};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize)]
struct Req {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Serialize)]
struct NewsItem {
    title: String,
    link: String,
    pub_date: String,
}

#[derive(Serialize)]
struct Resp {
    items: Vec<NewsItem>,
}

fn parse_rss(xml: &str) -> Vec<NewsItem> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut current: Option<NewsItem> = None;
    let mut field: Option<&'static str> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"item" => {
                    current = Some(NewsItem {
                        title: String::new(),
                        link: String::new(),
                        pub_date: String::new(),
                    });
                }
                b"title" => field = Some("title"),
                b"link" => field = Some("link"),
                b"pubDate" => field = Some("pub_date"),
                _ => field = None,
            },
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"item" {
                    if let Some(item) = current.take() {
                        if !item.title.is_empty() && !item.link.is_empty() {
                            items.push(item);
                        }
                    }
                }
                field = None;
            }
            Ok(Event::Text(t)) => {
                if let (Some(f), Some(item)) = (field, current.as_mut()) {
                    let txt = t.unescape().unwrap_or_default().into_owned();
                    match f {
                        "title" => item.title.push_str(&txt),
                        "link" => item.link.push_str(&txt),
                        "pub_date" => item.pub_date.push_str(&txt),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if let (Some(f), Some(item)) = (field, current.as_mut()) {
                    let txt = String::from_utf8_lossy(t.as_ref()).into_owned();
                    match f {
                        "title" => item.title.push_str(&txt),
                        "link" => item.link.push_str(&txt),
                        "pub_date" => item.pub_date.push_str(&txt),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    items
}

async fn try_source(url: &str) -> Result<Vec<NewsItem>> {
    let xml = get_text(url, BOT_UA, &[], TIMEOUT_DEFAULT_MS).await?;
    Ok(parse_rss(&xml))
}

pub async fn run(mut req: Request) -> Result<Response> {
    let body: Req = match req.json().await {
        Ok(v) => v,
        Err(e) => return error_response(format!("bad request: {}", e)),
    };
    let limit = body.limit.unwrap_or(10).clamp(1, 50);
    let q = urlencoding::encode(&body.query);

    let bing = format!("https://www.bing.com/news/search?q={}&format=rss", q);

    let items = match try_source(&bing).await {
        Ok(v) => v,
        Err(e) => return error_response(format!("News search failed: {}", e)),
    };

    let mut items = items;
    items.truncate(limit);
    Response::from_json(&Resp { items })
}
