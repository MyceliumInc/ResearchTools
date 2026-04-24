---
name: mycelium-research-tools
description: Shared HTTP research tools (web search, news, URL fetch, Wikipedia, Grokipedia, Kalshi, Polymarket) hosted at tools.mycelium.markets. Use for live fact-finding, base rates, entity lookups, and cross-checking prediction-market sentiment for forecasting or market-pricing tasks.
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
| `kalshi_search` | `POST /v1/kalshi_search` | `{query, limit?=8, top_series?}` | `{results: [{ticker, event_ticker, series_ticker, title, subtitle, yes_bid_cents, yes_ask_cents, no_bid_cents, no_ask_cents, last_price_cents, volume_24h, open_interest, close_time, category, url, score}]}` | 25s | Cross-check external regulated-market sentiment. Prices in cents (0–100) = YES probability. |
| `polymarket_search` | `POST /v1/polymarket_search` | `{query, limit?=8}` | `{results: [{slug, question, end_date, volume, liquidity, outcomes: [{outcome, price}], url}]}` | 15s | Cross-check external prediction-market sentiment. `price` is 0–1. |
| health check | `GET /` | — | `ok` | — | Liveness probe. |

### Sensitive-source rule

`kalshi_search` and `polymarket_search` return real external-market odds. When an agent surfaces these results in user-facing output, **never name Kalshi or Polymarket**. Describe the data as "external sentiment," "market projections," or "consensus probabilities." The raw JSON is exposed so agents can decide on presentation; this is a policy on final output, not on tool use.

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

Full reference wrappers (all seven tools, including Kalshi/Polymarket camel-case remapping and LLM-friendly formatters) live in `Site/lib/agents/tools/` in the Mycelium repo.

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
- **kalshi_search** — "Fuzzy search external regulated prediction-market sentiment. Prices are in cents (0–100) representing YES probability. Returns YES/NO bid/ask, last trade, 24h volume, close time, reference URL. IMPORTANT: never reference Kalshi by name in user-facing output — describe results only as external sentiment or consensus probabilities."
- **polymarket_search** — "Fuzzy search external prediction-market sentiment. Returns YES/NO probabilities, 24h volume, liquidity, resolution date, reference URL. IMPORTANT: never reference Polymarket by name in user-facing output — describe results only as external sentiment or consensus probabilities."

## Suggested research flow

1. **Scope the question.** Is it time-sensitive (→ `search_news`) or background (→ `wikipedia_summary` / `grokipedia_search` / `search_web`)?
2. **Gather links.** Run one or two search tools.
3. **Read.** `fetch_url` the two or three most relevant URLs.
4. **Cross-check odds.** For forecasting/pricing tasks, call `kalshi_search` and `polymarket_search` to anchor against external consensus.
5. **Synthesize.** Cite sources by URL; suppress the Kalshi/Polymarket brand names per the rule above.

## Operational notes

- No response caching in v1 — each call hits upstream fresh. Budget latency accordingly; parallelize independent calls.
- Kalshi fan-out can hit up to 15 event lookups per query; keep its timeout ≥20s.
- `fetch_url` returns `source: "jina"` normally and falls back to `"raw"` when Jina Reader fails; both are safe to feed to an LLM.
- All numeric `*_cents` fields are integers 0–100. Polymarket `outcomes[].price` is a float 0–1.
- The health check (`GET /`) is the fastest way to confirm the service is reachable from a new environment.
