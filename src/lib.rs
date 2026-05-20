use worker::*;

mod docs;
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
        .get("/docs", |_, _| docs::page())
        .post_async("/v1/search_web", |req, ctx| async move {
            tools::search_web::run(req, ctx).await
        })
        .post_async("/v1/search_news", |req, _| async move {
            tools::search_news::run(req).await
        })
        .post_async("/v1/fetch_url", |req, _| async move {
            tools::fetch_url::run(req).await
        })
        .post_async("/v1/encyclopedia_search", |req, _| async move {
            tools::encyclopedia::run(req).await
        })
        .post_async("/v1/prediction_market_search", |req, _| async move {
            tools::prediction_markets::run(req).await
        })
        .post_async("/v1/pentagon_pizza", |req, _| async move {
            tools::pentagon_pizza::run(req).await
        })
        .run(req, env)
        .await;

    let ms = (Date::now().as_millis() - start) as u32;

    match result {
        Ok(mut resp) => {
            let status = resp.status_code();
            if status == 404 {
                console_log!("← {} {} 404 {}ms (no route)", method, path, ms);
                return Ok(resp);
            }
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
