# Tools — Mycelium HTTP tools worker

Rust Cloudflare Worker hosting the research tools shared across Mycelium
agents. Deployed at `tools.mycelium.markets`. See `README.md` for the HTTP
contract.

## Why Rust

- Parsing-heavy (DDG HTML, Google News RSS, Polymarket pagination) — Rust uses
  ~3–5× less CPU-ms per request than the old TS implementations.
- Single source of truth for Site + MCP + external callers.
- Deployed as WASM to Cloudflare Workers; same edge as Site and MCP.

## Layout

- `src/lib.rs` — `#[event(fetch)]` entry; routes each `/v1/*` to a handler.
- `src/tools/*.rs` — one file per tool. Each exports a single
  `pub async fn run(req: Request) -> Result<Response>` handler.

## Conventions

- **No auth.** Endpoint is public; we're on Workers paid tier and the tools
  scrape public sources. Don't add bearer tokens here without a reason.
- **No URL response caching in v1.** Fetch upstream fresh each call. If
  latency becomes a problem, add KV in front of the slowest upstream first.
- **Errors as HTTP 200 + `{error}`.** Match the TS tools' "Search failed:
  ..." UX so the LLM sees a soft error rather than a retry-able 5xx.
- **Structured JSON out, not pre-formatted strings.** The Site wrappers own
  the `formatX` step for LangChain output.
- **Upstream timeouts mirror the old TS values** (6–20s). Enforced via
  `AbortController` in `src/util.rs`.
- **No `unsafe`, no `panic!` in handler paths.** Surface upstream failures
  via the error JSON.
- **Parallel fan-out** uses `futures::future::join_all` when a tool needs to
  multiplex upstream calls.

## Dependencies

Kept minimal to shrink the WASM bundle:

- `worker` — Cloudflare Workers bindings.
- `serde`, `serde_json` — request/response JSON.
- `regex` — DDG HTML + HTML tag stripping (matches TS behavior exactly).
- `quick-xml` — Google News RSS.
- `urlencoding` — query-string escaping.
- `futures` — `join_all` for parallel upstream fan-out.

No `scraper` / `html5ever` — overkill for the small number of HTML endpoints
we touch, and they inflate the WASM bundle by >1 MB.

## Adding a tool

1. Create `src/tools/<name>.rs` with a `pub async fn run(mut req: Request) -> Result<Response>`.
2. Add `pub mod <name>;` to `src/tools/mod.rs`.
3. Wire `/v1/<name>` in `src/lib.rs`'s router.
4. Update `README.md`'s route table.
5. Add a wrapper in `Site/lib/agents/tools/<name>.ts` that calls the new
   endpoint and preserves the LangChain `tool(...)` + `formatX(...)` shape.

## Commands

```bash
cd Tools
bun run dev        # wrangler dev (requires worker-build installed: cargo install -q worker-build)
bun run deploy     # wrangler deploy
cargo test         # unit tests (parsers)
```

Compat date `2026-04-01`. Custom domain: `tools.mycelium.markets`.
