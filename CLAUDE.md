# Tools — Mycelium HTTP tools worker

Rust Cloudflare Worker at `tools.mycelium.markets`. See `README.md` for the
HTTP contract, route table, and commands.

## Implementer notes

- **No auth, by design.** The endpoint is public; tools scrape public sources.
  Do not add bearer tokens without a reason.
- **No URL response caching in v1.** Fetch upstream fresh each call. If you
  add caching later, use the Cloudflare Cache API.
- **Errors as HTTP 200 + `{error}`.** Upstream failures surface as a soft
  error so the LLM doesn't trigger a retry loop. Bad requests return 4xx;
  worker bugs return 5xx.
- **Upstream timeouts** are enforced via `AbortController` in `src/util.rs`
  (6–20s, mirroring the old TS values).
- **No `unsafe`, no `panic!` in handler paths.** Surface failures via the
  error JSON.
- **No `scraper` / `html5ever`.** They inflate the WASM bundle by >1 MB; use
  `regex` patterns and `quick-xml` instead.
- **`nodejs_compat` is NOT required** — pure-Rust WASM worker.

## Uptime

Each `/v1/*` handler self-reports `(tool, ms, status, error)` to
`helpers.uptime` via `ctx.wait_until` after responding — the central wrapper
in `src/lib.rs` reads the response body, sniffs for soft `{error}` payloads,
and posts to `public.uptime_record` (anon-callable `SECURITY DEFINER` RPC)
without adding latency to the caller. Records reflect real agent traffic;
tools that aren't called show as "no traffic in <window>" at `/admin/health`.

`SUPABASE_URL` and `SUPABASE_PUBLISHABLE_KEY` are bound as `[vars]` in
`wrangler.toml`. The publishable key is the same one shipped in Site's
browser bundle, so it's not a secret. Cache-hit responses are recorded too —
their low ms reflects reality.
