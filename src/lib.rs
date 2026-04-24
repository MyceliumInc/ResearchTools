use worker::*;

mod util;
mod tools;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let method = req.method().to_string();
    let path = req.path();
    let start = Date::now().as_millis();
    console_log!("→ {} {}", method, path);

    let result = Router::new()
        .get("/", |_, _| Response::ok("ok"))
        .post_async("/v1/search_web", |req, _| async move {
            tools::search_web::run(req).await
        })
        .post_async("/v1/search_news", |req, _| async move {
            tools::search_news::run(req).await
        })
        .post_async("/v1/fetch_url", |req, _| async move {
            tools::fetch_url::run(req).await
        })
        .post_async("/v1/wikipedia_summary", |req, _| async move {
            tools::wikipedia::run(req).await
        })
        .post_async("/v1/grokipedia_search", |req, _| async move {
            tools::grokipedia::run(req).await
        })
        .post_async("/v1/kalshi_search", |req, _| async move {
            tools::kalshi::run(req).await
        })
        .post_async("/v1/polymarket_search", |req, _| async move {
            tools::polymarket::run(req).await
        })
        .post_async("/v1/manifold_search", |req, _| async move {
            tools::manifold::run(req).await
        })
        .post_async("/v1/usgs_earthquakes", |req, _| async move {
            tools::usgs_earthquakes::run(req).await
        })
        .post_async("/v1/reddit_search", |req, _| async move {
            tools::reddit::run(req).await
        })
        .post_async("/v1/wikidata_sparql", |req, _| async move {
            tools::wikidata::run(req).await
        })
        .post_async("/v1/sec_filings", |req, _| async move {
            tools::sec::run(req).await
        })
        .post_async("/v1/weather_forecast", |req, _| async move {
            tools::weather::run(req).await
        })
        .run(req, env)
        .await;

    let ms = Date::now().as_millis() - start;
    match &result {
        Ok(resp) => console_log!(
            "← {} {} {} {}ms",
            method,
            path,
            resp.status_code(),
            ms
        ),
        Err(e) => console_log!("✗ {} {} {}ms err={}", method, path, ms, e),
    }
    result
}
