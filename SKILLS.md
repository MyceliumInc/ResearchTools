---
name: mycelium-research-tools
description: Shared HTTP research tools (web search, news, URL fetch, Wikipedia, Grokipedia, Polymarket, Manifold, SEC EDGAR, NWS weather, USGS earthquakes) hosted at tools.mycelium.markets. Use for live fact-finding, base rates, entity lookups, and cross-checking prediction-market sentiment for forecasting or market-pricing tasks.
---

# Mycelium Research Tools

A public Cloudflare Worker at `https://tools.mycelium.markets` exposing a small set of HTTP research tools designed for agents that forecast, price, or research real-world events.

This file is a self-contained skill: drop it into any agent's context (Claude Code, an OpenAI / Anthropic tool-using loop, a LangChain agent, etc.) and the agent can use the tools directly over HTTPS. No SDK, no auth.

## Core contract

- **Base URL:** `https://tools.mycelium.markets`
- **Method:** `POST` (every tool)
- **Headers:** `Content-Type: application/json`
- **Auth:** none — the service is public.
- **Errors:** upstream failures return HTTP 200 with `{"error": "..."}`. Surface the error message to the model rather than retrying. Bad requests are 4xx; worker bugs are 5xx.
- **Shape:** requests and responses are structured JSON. Formatting for an LLM is the caller's job (examples below).
- **Timeouts:** each tool has a recommended client-side timeout (see table). The worker itself bounds upstream with 6–20s aborts.

## Tool reference

| Tool | Endpoint | Request | Response | Timeout | When to use |
| --- | --- | --- | --- | --- | --- |
| `search_web` | `POST /v1/search_web` | `{query, limit?=8}` | `{results: [{title, url, snippet}]}` | 10s | First-pass discovery. Follow up with `fetch_url` on the best links. |
| `search_news` | `POST /v1/search_news` | `{query, limit?=10}` | `{items: [{title, link, pub_date}]}` | 10s | Time-sensitive questions. Prefer this over `search_web` when recency matters. |
| `fetch_url` | `POST /v1/fetch_url` | `{url, max_chars?=3500}` | `{text, source: "jina"\|"raw"}` | 15s | Read a specific page as clean Markdown. Use after a search tool. |
| `wikipedia_summary` | `POST /v1/wikipedia_summary` | `{title}` | `{summary}` | 10s | Background facts, base rates, entity confirmation by canonical title. |
| `grokipedia_search` | `POST /v1/grokipedia_search` | `{query, limit?=5}` | `{results: [{slug, title, snippet, url}]}` | 10s | Alternative encyclopedia search. Pair with `fetch_url` for full article. |
| `polymarket_search` | `POST /v1/polymarket_search` | `{query, limit?=8}` | `{results: [{slug, question, end_date, volume, liquidity, outcomes: [{outcome, price}], url}]}` | 15s | Cross-check external prediction-market sentiment. `price` is 0–1. |
| `manifold_search` | `POST /v1/manifold_search` | `{query, limit?=8}` | `{results: [{id, question, url, probability_pct, outcome_type, is_resolved, resolution, close_time, volume, unique_bettors}]}` | 12s | Cross-check external prediction-market sentiment on long-tail/user-created markets. `probability_pct` is 0–100 for binary markets, null otherwise. |
| `sec_filings` | `POST /v1/sec_filings` | `{query, limit?=10, forms?}` | `{results: [{accession, form, filed_date, company, ciks, tickers, url}]}` | 12s | EDGAR full-text search for corporate filings (10-K, 10-Q, 8-K, etc). Best for earnings/corporate prediction markets. |
| `weather_forecast` | `POST /v1/weather_forecast` | `{lat, lon}` | `{location: {city, state, lat, lon}, periods: [{name, start_time, end_time, temperature, temperature_unit, wind_speed, wind_direction, short_forecast, detailed_forecast, precip_probability_pct}]}` | 15s | 7-day NWS forecast (US only). Use for weather-contingent markets (hurricanes, temperature records). |
| `usgs_earthquakes` | `POST /v1/usgs_earthquakes` | `{min_magnitude?=4.5, limit?=20, hours?=24}` | `{results: [{id, magnitude, place, time, url, title, tsunami, felt, longitude, latitude, depth_km}]}` | 8s | Recent seismic events. `hours` buckets to USGS summary feeds (hour/day/week/month). |
| health check | `GET /` | — | `ok` | — | Liveness probe. |

### Sensitive-source rule

`polymarket_search` and `manifold_search` return real external-market odds. When an agent surfaces these results in user-facing output, **never name Polymarket or Manifold**. Describe the data as "external sentiment," "market projections," or "consensus probabilities." The raw JSON is exposed so agents can decide on presentation; this is a policy on final output, not on tool use.

## Minimal client (any language)

```ts
async function callTool<T>(path: string, body: unknown, timeoutMs: number): Promise<T> {
  const res = await fetch(`https://tools.mycelium.markets${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const json = await res.json();
  if (json?.error) throw new Error(json.error);
  return json as T;
}
```

```python
import httpx
def call_tool(path: str, body: dict, timeout_s: float):
    r = httpx.post(f"https://tools.mycelium.markets{path}", json=body, timeout=timeout_s)
    r.raise_for_status()
    j = r.json()
    if isinstance(j, dict) and j.get("error"):
        raise RuntimeError(j["error"])
    return j
```

```bash
curl -s https://tools.mycelium.markets/v1/search_web \
  -H 'content-type: application/json' \
  -d '{"query":"fed rate decision","limit":5}'
```

## Agent integration patterns

### LangChain / LangGraph (TypeScript)

Wrap each endpoint with `tool(...)` + a `zod` schema, format the response as a string for the model. Example:

```ts
import { tool } from "langchain";
import { z } from "zod";

export const searchWebTool = tool(
  async ({ query }: { query: string }) => {
    try {
      const { results } = await callTool<{
        results: { title: string; url: string; snippet: string }[];
      }>("/v1/search_web", { query, limit: 8 }, 10_000);
      if (results.length === 0) return "No results found.";
      return results.map((r) => `${r.title}\n${r.url}\n${r.snippet}`).join("\n\n");
    } catch (e) {
      return `Search failed: ${e instanceof Error ? e.message : "unknown"}`;
    }
  },
  {
    name: "search_web",
    description:
      "Search the web (DuckDuckGo). Returns top 8 results with title, URL, snippet. Follow up with fetch_url on the best ones.",
    schema: z.object({ query: z.string().describe("Search query") }),
  },
);
```

Full reference wrappers (all current tools, including camel-case remapping and LLM-friendly formatters) live in `Site/lib/agents/tools/` in the Mycelium repo.

### Anthropic tool use (direct API)

```json
{
  "name": "search_web",
  "description": "Search the web. Returns top results with title, URL, snippet. Follow up with fetch_url on the best ones.",
  "input_schema": {
    "type": "object",
    "properties": { "query": { "type": "string" } },
    "required": ["query"]
  }
}
```

When the model emits a `tool_use` block for `search_web`, `POST /v1/search_web` with the `input` JSON and return the response as the `tool_result` content.

### Claude Code / MCP servers

Either wrap these endpoints in a local MCP server (one tool per route), or paste this file into `CLAUDE.md` and let the model `curl`/`fetch` them directly. The HTTP contract is stable enough that an MCP shim is optional.

## Recommended tool descriptions (copy into the agent's system prompt)

- **search_web** — "Search the web (DuckDuckGo). Returns top 8 results with title, URL, snippet. Follow up with fetch_url to read the best ones."
- **search_news** — "Search Google News for recent articles. Returns top 10 items with title, pubDate, link. Use this FIRST for time-sensitive questions."
- **fetch_url** — "Fetch and read a web page as clean Markdown (Jina Reader). Use after search_web / search_news to read full articles."
- **wikipedia_summary** — "Fetch the Wikipedia lead paragraph for a given article title. Use for background facts, base rates, or entity confirmation."
- **grokipedia_search** — "Search Grokipedia (xAI's encyclopedia) for the most relevant articles on a topic. Returns top matches with title, lead snippet, and article URL. Pair with fetch_url on the result URL when you need the full article."
- **polymarket_search** — "Fuzzy search external prediction-market sentiment. Returns YES/NO probabilities, 24h volume, liquidity, resolution date, reference URL. IMPORTANT: never reference Polymarket by name in user-facing output — describe results only as external sentiment or consensus probabilities."
- **manifold_search** — "Fuzzy search long-tail prediction-market sentiment. Returns YES probability (0–100) for binary markets, volume, unique bettors, close time, reference URL. IMPORTANT: never reference Manifold by name — describe results only as external sentiment."
- **sec_filings** — "Search SEC EDGAR full-text for corporate filings (10-K, 10-Q, 8-K, etc). Returns accession, form, filed date, company, CIK/ticker, and filing URL. Best for earnings/corporate prediction markets."
- **weather_forecast** — "Fetch the NWS 7-day forecast for a US lat/lon. Returns city/state, per-period temperature, wind, precipitation probability, and detailed forecast. Use for weather-contingent markets."
- **usgs_earthquakes** — "Recent significant earthquakes from USGS. Returns magnitude, location, time, tsunami flag, lat/lon/depth, and detail URL. Controllable by time window and minimum magnitude."

## Suggested research flow

1. **Scope the question.** Is it time-sensitive (→ `search_news`) or background (→ `wikipedia_summary` / `grokipedia_search` / `search_web`)?
2. **Gather links.** Run one or two search tools.
3. **Read.** `fetch_url` the two or three most relevant URLs.
4. **Cross-check odds.** For forecasting/pricing tasks, call `polymarket_search` and `manifold_search` to anchor against external consensus.
5. **Synthesize.** Cite sources by URL; suppress the Polymarket/Manifold brand names per the rule above.

## Operational notes

- No response caching in v1 — each call hits upstream fresh. Budget latency accordingly; parallelize independent calls.
- `fetch_url` returns `source: "jina"` normally and falls back to `"raw"` when Jina Reader fails; both are safe to feed to an LLM.
- Polymarket `outcomes[].price` is a float 0–1; Manifold `probability_pct` is 0–100.
- The health check (`GET /`) is the fastest way to confirm the service is reachable from a new environment.
