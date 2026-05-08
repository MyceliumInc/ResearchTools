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

The Site Worker probes every endpoint here every 15 minutes and logs to
`helpers.uptime`; results surface at `/admin/health`. See root `CLAUDE.md`.
