  # Tools — implementer notes

Rust Cloudflare Worker at `tools.mycelium.markets`. See `README.md` for the
HTTP contract and route table.

## Layout

```
src/
  lib.rs            # worker entry + router + uptime wrapper
  util.rs           # fetch w/ AbortController timeout + shared helpers
  uptime.rs         # detect_soft_error + record() → public.uptime_record
  tools/
    search_web.rs           # DuckDuckGo Lite SERP scrape
    search_news.rs          # Google News RSS
    fetch_url.rs            # Jina Reader + raw HTML fallback
    encyclopedia.rs         # Wikipedia + Grokipedia (parallel merge)
    prediction_markets.rs   # Polymarket + Manifold + Kalshi (parallel merge)
```

Routes are wired in `src/lib.rs`'s `Router` block. Every `/v1/*` POST handler
returns `Result<Response>`; the wrapper in `fetch()` reads the response body,
sniffs for soft `{error}` payloads, and `ctx.wait_until`s a post to
`public.uptime_record` so the caller never waits.

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
- **Structured JSON, not preformatted strings.** Site-side wrappers do the
  LangChain `formatX` step; external callers render however they like.

## Uptime self-reporting

`src/lib.rs` `fetch()` records `(tool, ms, status, error)` after the response
goes out. `tool_from_path` extracts the slug after `/v1/`. Soft errors are
detected by JSON-parsing the body for an `error` field. `SUPABASE_URL` and
`SUPABASE_PUBLISHABLE_KEY` are bound as `[vars]` in `wrangler.toml` — the
publishable key is the same one Site ships in its browser bundle, so it's
not a secret. 404s skip recording. Records reflect real agent traffic; idle
tools render as "no traffic in <window>" at Site `/admin/health`.

## Adding a tool

1. New file `src/tools/<name>.rs` exporting `pub async fn run(req: Request) -> Result<Response>`.
2. Add `pub mod <name>;` to `src/tools/mod.rs`.
3. Add a `.post_async("/v1/<name>", …)` line to the router in `src/lib.rs`.
4. **Run the root `edit-tools` skill** (`/Users/benny/Code/Mycelium/.claude/skills/edit-tools`)
   for the cross-repo checklist — Site wrapper in `Site/lib/agents/tools/`,
   research-tools-skill description, and any MCP/Spore plumbing.

## Not in scope

`search_markets` stays on Site — it hits Supabase via the service-role
client and depends on per-request auth context. Do not move it here.
