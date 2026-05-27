  # Tools — implementer notes

Rust Cloudflare Worker exposing public HTTP research endpoints for LLM
agents. See `README.md` for the HTTP contract and route table.

## Layout

```
src/
  lib.rs            # worker entry + router + telemetry wrapper
  util.rs           # fetch w/ AbortController timeout + shared helpers
  telemetry.rs      # detect_soft_error + record() → optional webhook
  docs.rs           # /docs HTML
  tools/
    search_web.rs           # Exa Search API w/ DuckDuckGo Lite fallback
    search_news.rs          # Google News RSS
    fetch_url.rs            # Jina Reader + raw HTML fallback
    encyclopedia.rs         # Wikipedia + Grokipedia (parallel merge)
    prediction_markets.rs   # Polymarket + Manifold + Kalshi (parallel merge)
    pentagon_pizza.rs       # pentagon pizza index
```

Routes are wired in `src/lib.rs`'s `Router` block. Every `/v1/*` POST handler
returns `Result<Response>`; the wrapper in `fetch()` reads the response body,
sniffs for soft `{error}` payloads, and `ctx.wait_until`s a POST to the
configured telemetry webhook so the caller never waits.

## Conventions

- **No auth, by design.** Endpoint is public; tools only scrape public sources.
- **No URL response caching in v1.** Fetch upstream fresh each call. Add
  Cloudflare Cache API later if needed.
- **Errors as HTTP 200 + `{error}`.** Upstream failures surface as a soft
  error so the LLM doesn't trigger a retry loop. Bad input → 4xx; worker
  bugs → 5xx.
- **Upstream timeouts** via `AbortController` in `src/util.rs` (6–20s).
- **No `unsafe`, no `panic!` in handler paths.** Surface failures via the
  error JSON.
- **No `scraper` / `html5ever` / `regex` deps.** They inflate the WASM bundle.
  Use byte-pattern matching + `quick-xml` instead. Current deps: `worker`,
  `serde`, `serde_json`, `quick-xml`, `urlencoding`, `futures`, `once_cell`.
- **`nodejs_compat` is NOT required** — pure-Rust WASM worker.
- **Structured JSON, not preformatted strings.** Callers render however
  they like.

## Telemetry self-reporting

`src/lib.rs` `fetch()` records `(tool, ms, status, error)` after the response
goes out. `tool_from_path` extracts the slug after `/v1/`. Soft errors are
detected by JSON-parsing the body for an `error` field. The recorder is
opt-in: if `TELEMETRY_URL` is unset, no record is sent. `TELEMETRY_AUTH`
(optional) is sent as both `Authorization: Bearer <value>` and `apikey:
<value>` headers. 404s skip recording.

## Adding a tool

1. New file `src/tools/<name>.rs` exporting `pub async fn run(req: Request) -> Result<Response>`.
2. Add `pub mod <name>;` to `src/tools/mod.rs`.
3. Add a `.post_async("/v1/<name>", …)` line to the router in `src/lib.rs`.
