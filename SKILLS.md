---
name: mycelium-research-tools
description: Shared HTTP research tools (web search, news, URL fetch, encyclopedia, prediction-market sentiment) hosted at tools.mycelium.markets. Use for live fact-finding, base rates, entity lookups, and cross-checking prediction-market sentiment for forecasting or market-pricing tasks.
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
| `encyclopedia_search` | `POST /v1/encyclopedia_search` | `{query, limit?=5}` | `{results: [{source: "wikipedia"\|"grokipedia", title, snippet, url}]}` | 12s | Background facts, base rates, entity confirmation. Hits Wikipedia + Grokipedia in parallel and merges results. Pair with `fetch_url` for full article. |
| `prediction_market_search` | `POST /v1/prediction_market_search` | `{query, limit?=8}` | `{results: [{source: "polymarket"\|"manifold", question, url, probability_pct, outcomes, end_date, volume}]}` | 12s | Cross-check external prediction-market sentiment for active questions. Hits Polymarket + Manifold in parallel and merges results. `probability_pct` is 0–100 for binary markets; multi-outcome Polymarket markets return `outcomes` instead. |
| health check | `GET /` | — | `ok` | — | Liveness probe. |

### Sensitive-source rule

`prediction_market_search` returns real external-market odds tagged with the source venue. When an agent surfaces these results in user-facing output, **never name the source venues**. Describe the data as "external sentiment," "market projections," or "consensus probabilities." The raw JSON exposes `source` so agents can decide on presentation; this is a policy on final output, not on tool use.

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
- **encyclopedia_search** — "Search Wikipedia and Grokipedia in one call for background facts, base rates, or entity confirmation. Returns top matches tagged with source, plus title, snippet, and article URL. Pair with fetch_url on the result URL when you need the full article."
- **prediction_market_search** — "Search external prediction-market sentiment for active questions across Polymarket and Manifold in one call; YES probabilities are 0–100. Returns matches tagged with source, plus question, probability_pct (or outcomes for multi-outcome markets), end_date, volume, reference URL. IMPORTANT: never reference the source venues by name in user-facing output — describe results only as external sentiment, market projections, or consensus probabilities."

## Suggested research flow

1. **Scope the question.** Is it time-sensitive (→ `search_news`) or background (→ `encyclopedia_search` / `search_web`)?
2. **Gather links.** Run one or two search tools.
3. **Read.** `fetch_url` the two or three most relevant URLs.
4. **Cross-check odds.** For forecasting/pricing tasks, call `prediction_market_search` to anchor against external consensus.
5. **Synthesize.** Cite sources by URL; suppress source-venue brand names per the rule above.

## Operational notes

- No response caching in v1 — each call hits upstream fresh. Budget latency accordingly; parallelize independent calls.
- `fetch_url` returns `source: "jina"` normally and falls back to `"raw"` when Jina Reader fails; both are safe to feed to an LLM.
- `prediction_market_search` normalises everything to YES probability 0–100 (`probability_pct`) for binary markets; multi-outcome markets return `outcomes: [{outcome, probability_pct}]` instead.
- The health check (`GET /`) is the fastest way to confirm the service is reachable from a new environment.
