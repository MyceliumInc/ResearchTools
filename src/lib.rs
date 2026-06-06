use worker::*;

mod cache;
mod docs;
mod http;
mod telemetry;
mod text;
mod tools;

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let method = req.method().to_string();
    let path = req.path();
    let tool = telemetry::tool_from_path(&path);
    let sink = telemetry::sink(&env);
    let start = Date::now().as_millis();
    console_log!("→ {} {}", method, path);

    let result = Router::new()
        .get("/", |_, _| Response::ok("ok"))
        .get("/docs", |_, _| docs::page())
        .post_async("/v1/web", |req, ctx| async move {
            tools::web::run(req, ctx).await
        })
        .post_async("/v1/news", |req, _| async move {
            tools::news::run(req).await
        })
        .post_async("/v1/fetch", |req, _| async move {
            tools::fetch::run(req).await
        })
        .post_async("/v1/encyclopedia", |req, _| async move {
            tools::encyclopedia::run(req).await
        })
        .post_async("/v1/predictions", |req, _| async move {
            tools::predictions::run(req).await
        })
        .post_async("/v1/pizza", |req, _| async move {
            tools::pizza::run(req).await
        })
        .post_async("/v1/stocks", |req, _| async move {
            tools::stocks::run(req).await
        })
        .post_async("/v1/breaking", |req, _| async move {
            tools::breaking::run(req).await
        })
        .post_async("/v1/doomsday", |req, _| async move {
            tools::doomsday::run(req).await
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
            if let (Some(tool), Some(sink)) = (tool, sink) {
                let bytes = resp.bytes().await.unwrap_or_default();
                let headers = resp.headers().clone();
                let error = if status >= 400 {
                    Some(format!("HTTP {}", status))
                } else {
                    telemetry::detect_soft_error(&bytes)
                };
                console_log!("← {} {} {} {}ms", method, path, status, ms);
                ctx.wait_until(telemetry::record(
                    sink,
                    telemetry::Outcome {
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
        Err(error) => {
            console_log!("✗ {} {} {}ms err={}", method, path, ms, error);
            if let (Some(tool), Some(sink)) = (tool, sink) {
                let msg = text::truncate(&error.to_string(), 300);
                ctx.wait_until(telemetry::record(
                    sink,
                    telemetry::Outcome {
                        tool,
                        ms,
                        status: 500,
                        error: Some(msg),
                    },
                ));
            }
            Err(error)
        }
    }
}
