  # Tools — implementer notes

Rust Cloudflare Worker exposing public HTTP research endpoints for LLM
agents. See `README.md` for the HTTP contract and route table.

## Layout

Each callable (HTTP-routed) tool is a directory under `tools/`. Its `mod.rs`
is the orchestrator — it owns `run()`, the shared result struct, and the
merge — and each upstream provider is a sibling file in the same directory
exporting `pub async fn search(...)`. Single-source tools are just a `mod.rs`.

```
src/
  lib.rs            # worker entry + router + telemetry wrapper
  http.rs           # build_request + fetch helpers (get_text/get_json/get_typed), timeouts
  cache.rs          # cache_or(): body-keyed edge cache wrapper
  text.rs           # strip_tags/strip_html_doc, tag+attr extraction, truncate
  telemetry.rs      # detect_soft_error + record() → optional webhook
  docs.rs           # /docs HTML
  tools/
    mod.rs                  # callable-tool module list + generic interleave()
    search_web/
      mod.rs                # Exa if keyed, else keyless fallback chain
      exa.rs                # Exa Search API
      mojeek.rs             # Mojeek scrape (keyless, primary fallback)
      duckduckgo.rs         # DuckDuckGo Lite scrape (keyless)
      marginalia.rs         # Marginalia public JSON API (keyless)
    search_news/mod.rs      # Bing News RSS
    fetch_url/mod.rs        # Jina Reader + raw HTML fallback
    encyclopedia/
      mod.rs                # merges Wikipedia + Grokipedia (parallel)
      wikipedia.rs
      grokipedia.rs
    prediction_markets/
      mod.rs                # merges Polymarket + Manifold + Kalshi (parallel)
      polymarket.rs
      manifold.rs
      kalshi.rs
    pentagon_pizza/mod.rs   # pentagon pizza index
    stock_quote/mod.rs      # Finnhub quotes (needs FINNHUB_API_KEY)
    breaking_news/
      mod.rs                # multi-feed RSS → entity-blocked Jaccard cluster
      extract.rs            # tokenize / entities / headline cleanup / jaccard
      dates.rs              # RFC-2822 pubDate parsing
```

Routes are wired in `src/lib.rs`'s `Router` block. Every `/v1/*` POST handler
returns `Result<Response>`; the wrapper in `fetch()` reads the response body,
sniffs for soft `{error}` payloads, and `ctx.wait_until`s a POST to the
configured telemetry webhook so the caller never waits.

## Conventions

- **No auth, by design.** Endpoint is public; tools only scrape public sources.
- **Edge-cached by request body.** `cache.rs`'s `cache_or` keys the Cloudflare
  Cache API on `(tool, hash(body))` so identical calls are served from the
  edge. TTLs are per-tool (see `/docs`); `search_news` and `fetch_url` opt out
  and fetch fresh.
- **Errors as HTTP 200 + `{error}`.** Upstream failures surface as a soft
  error so the LLM doesn't trigger a retry loop. Bad input → 4xx; worker
  bugs → 5xx.
- **Upstream timeouts** via a `select` race against a `Delay` in
  `src/http.rs` (6–20s).
- **No `unsafe`, no `panic!` in handler paths.** Surface failures via the
  error JSON.
- **No `scraper` / `html5ever` / `regex` deps.** They inflate the WASM bundle.
  Use byte-pattern matching + `quick-xml` instead. Current deps: `worker`,
  `serde`, `serde_json`, `quick-xml`, `urlencoding`, `futures`.
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

1. New directory `src/tools/<name>/` with a `mod.rs` exporting `pub async fn run(req: Request) -> Result<Response>`; put per-provider subtools in sibling files inside it.
2. Add `pub mod <name>;` to `src/tools/mod.rs`.
3. Add a `.post_async("/v1/<name>", …)` line to the router in `src/lib.rs`.
