# Mycelium Tools

Shared HTTP tools service for Mycelium agents. Rust Cloudflare Worker at
`tools.mycelium.markets`. Exposes the research/sentiment tools (search, fetch,
Wikipedia, Wikidata, Grokipedia, Google News, Polymarket, Manifold, SEC
EDGAR, NWS weather, USGS earthquakes) that used to live inside
`Site/lib/agents/tools/`.

Both the Site agents (Creator, Pricer) and external callers (MCP, partner
bots) hit the same endpoints — one source of truth, fast edge deploys, no
duplicated implementations.

## Why this exists

- **One impl, many callers.** Site's Pricer, MCP agents, and anyone else use
  the same tools by URL.
- **Edge-local + fast.** Rust + WASM on the same Cloudflare POPs as Site and
  MCP; tool calls never leave Cloudflare's network when invoked from another
  Worker.

## Endpoints

All endpoints are `POST`, accept `application/json`, and return
`application/json`. No authentication — the service is public.

Base URL: `https://tools.mycelium.markets`

| Route | Request | Response |
| --- | --- | --- |
| `POST /v1/search_web` | `{query, limit?}` | `{results: [{title, url, snippet}]}` |
| `POST /v1/search_news` | `{query, limit?}` | `{items: [{title, link, pub_date}]}` |
| `POST /v1/fetch_url` | `{url, max_chars?}` | `{text, source: "jina" \| "raw"}` |
| `POST /v1/wikipedia_summary` | `{title}` | `{summary}` |
| `POST /v1/wikidata_sparql` | `{query}` | `{results: [...]}` |
| `POST /v1/grokipedia_search` | `{query, limit?}` | `{results: [{slug, title, snippet, url}]}` |
| `POST /v1/polymarket_search` | `{query, limit?}` | `{results: [...]}` |
| `POST /v1/manifold_search` | `{query, limit?}` | `{results: [...]}` |
| `POST /v1/sec_filings` | `{query, limit?}` | `{results: [...]}` |
| `POST /v1/weather_forecast` | `{lat, lon}` | `{periods: [...]}` |
| `POST /v1/usgs_earthquakes` | `{min_magnitude?, limit?}` | `{quakes: [...]}` |
| `GET /` | — | `ok` |

**Error contract.** Upstream failures return HTTP 200 with
`{"error": "<message>"}` so callers can surface a soft error to the LLM
without a retry loop. Bad requests return 4xx; worker bugs return 5xx.

Default limits: 8 (search_web, polymarket, manifold), 10 (search_news,
sec_filings), 5 (grokipedia), 20 (usgs_earthquakes), 3500 chars (fetch_url).
Wikidata SPARQL queries are capped at 8000 chars.

## Shape notes

Responses are **structured JSON**, not pre-formatted strings. Site-side
wrappers (`Site/lib/agents/tools/*.ts`) handle the LangChain `formatX` step.
External callers can render however they like.

Polymarket / Manifold responses intentionally expose the upstream slug, URL,
and prices so callers can decide how to present them. The "never reference
Polymarket/Manifold by name" rule is enforced in the Site wrappers' LangChain
tool descriptions, not here.

## Layout

```
src/
  lib.rs            # worker entry + router
  util.rs           # fetch w/ timeout + shared helpers
  tools/
    mod.rs
    search_web.rs       # DuckDuckGo Lite SERP scrape
    search_news.rs      # Google News RSS
    fetch_url.rs        # Jina Reader + raw HTML fallback
    wikipedia.rs        # Wikipedia REST summary
    wikidata.rs         # Wikidata SPARQL
    grokipedia.rs       # Grokipedia typeahead
    polymarket.rs       # Gamma public-search
    manifold.rs         # Manifold markets search
    sec.rs              # SEC EDGAR full-text search
    weather.rs          # NWS forecast
    usgs_earthquakes.rs # USGS recent quakes
```

## Commands

```bash
cd Tools
cargo install -q worker-build      # once
bun run dev        # wrangler dev
bun run deploy     # wrangler deploy
cargo test         # unit tests
```

`wrangler.toml` pins `main = "build/worker/shim.mjs"`, which `worker-build`
generates from the Rust crate. `nodejs_compat` is NOT required — this is a
pure-Rust WASM worker.

## Adding a tool

1. New file `src/tools/<name>.rs` exporting `pub async fn run(req: Request) -> Result<Response>`.
2. Add a `pub mod <name>;` line to `src/tools/mod.rs`.
3. Add a route in `src/lib.rs`'s `Router` block.
4. Add a thin wrapper in `Site/lib/agents/tools/<name>.ts` that calls the new
   endpoint and keeps the `formatX` + LangChain `tool(...)` shape.

## Not in scope

`search_markets` stays on Site — it hits Supabase via the service-role client
and depends on per-request auth context. Do not move it here.
