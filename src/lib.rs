use worker::*;

mod tools;
mod uptime;
mod util;

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let method = req.method().to_string();
    let path = req.path();
    let tool = uptime::tool_from_path(&path);
    let supabase = uptime::supabase_config(&env);
    let start = Date::now().as_millis();
    console_log!("→ {} {}", method, path);

    let result = Router::new()
        .get("/", |_, _| Response::ok("ok"))
        .post_async("/v1/search_web", |req, ctx| async move {
            tools::search_web::run(req, ctx).await
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
        .post_async("/v1/polymarket_search", |req, _| async move {
            tools::polymarket::run(req).await
        })
        .post_async("/v1/manifold_search", |req, _| async move {
            tools::manifold::run(req).await
        })
        .post_async("/v1/usgs_earthquakes", |req, _| async move {
            tools::usgs_earthquakes::run(req).await
        })
        .post_async("/v1/sec_filings", |req, _| async move {
            tools::sec::run(req).await
        })
        .post_async("/v1/weather_forecast", |req, _| async move {
            tools::weather::run(req).await
        })
        .run(req, env)
        .await;

    let ms = (Date::now().as_millis() - start) as u32;

    match result {
        Ok(mut resp) => {
            let status = resp.status_code();
            if let (Some(tool), Some((url, key))) = (tool, supabase) {
                let bytes = resp.bytes().await.unwrap_or_default();
                let headers = resp.headers().clone();
                let error = if status >= 400 {
                    Some(format!("HTTP {}", status))
                } else {
                    uptime::detect_soft_error(&bytes)
                };
                console_log!("← {} {} {} {}ms", method, path, status, ms);
                ctx.wait_until(uptime::record(
                    url,
                    key,
                    uptime::Outcome {
                        tool,
                        ms,
                        status,
                        error,
                    },
                ));
                Response::from_bytes(bytes).map(|r| r.with_status(status).with_headers(headers))
            } else {
                console_log!("← {} {} {} {}ms", method, path, status, ms);
                Ok(resp)
            }
        }
        Err(e) => {
            console_log!("✗ {} {} {}ms err={}", method, path, ms, e);
            if let (Some(tool), Some((url, key))) = (tool, supabase) {
                let msg = uptime::truncate(&e.to_string(), 300);
                ctx.wait_until(uptime::record(
                    url,
                    key,
                    uptime::Outcome {
                        tool,
                        ms,
                        status: 500,
                        error: Some(msg),
                    },
                ));
            }
            Err(e)
        }
    }
}
