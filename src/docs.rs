use worker::*;

pub fn page() -> Result<Response> {
    let headers = Headers::new();
    headers.set("content-type", "text/html; charset=utf-8")?;
    headers.set("cache-control", "public, max-age=300")?;
    Ok(Response::ok(HTML)?.with_headers(headers))
}

const HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Research Tools — API spec</title>
<style>
  :root {
    --bg: #fafaf7;
    --fg: #1a1a1a;
    --muted: #666;
    --rule: #e5e3dc;
    --code-bg: #f1efe9;
    --accent: #5a4a2a;
    --link: #2d5a8a;
    --card: #ffffff;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: var(--bg); color: var(--fg); }
  body {
    font: 15px/1.55 ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    max-width: 780px;
    margin: 0 auto;
    padding: 48px 24px 96px;
  }
  header { border-bottom: 1px solid var(--rule); padding-bottom: 20px; margin-bottom: 32px; }
  h1 { font-size: 22px; margin: 0 0 4px; letter-spacing: -0.01em; }
  h2 { font-size: 17px; margin: 40px 0 12px; padding-top: 24px; border-top: 1px solid var(--rule); letter-spacing: -0.005em; }
  h3 { font-size: 13px; margin: 18px 0 6px; font-weight: 600; color: var(--accent); text-transform: uppercase; letter-spacing: 0.05em; }
  h2:first-of-type { border-top: none; padding-top: 0; }
  p { margin: 8px 0 12px; }
  .sub { color: var(--muted); font-size: 13px; }
  a { color: var(--link); text-decoration: none; }
  a:hover { text-decoration: underline; }
  code, pre { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
  code { background: var(--code-bg); padding: 1px 5px; border-radius: 3px; font-size: 13px; }
  pre {
    background: var(--code-bg);
    padding: 12px 14px;
    border-radius: 4px;
    overflow-x: auto;
    font-size: 12.5px;
    line-height: 1.5;
    margin: 8px 0 12px;
  }
  pre code { background: transparent; padding: 0; font-size: inherit; }
  table { border-collapse: collapse; width: 100%; margin: 10px 0 16px; font-size: 13.5px; }
  th, td { text-align: left; padding: 7px 10px; border-bottom: 1px solid var(--rule); vertical-align: top; }
  th { font-weight: 600; color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
  td code { font-size: 12.5px; }
  .method { display: inline-block; padding: 1px 6px; border-radius: 3px; background: #e8d9b8; color: #5a4a2a; font-size: 11px; font-weight: 700; letter-spacing: 0.04em; vertical-align: 1px; }
  .nav { font-size: 13px; color: var(--muted); margin: 4px 0 0; line-height: 2; }
  .nav a { display: inline-block; margin-right: 6px; padding: 2px 8px; border: 1px solid var(--rule); border-radius: 999px; background: var(--card); }
  .nav a:hover { text-decoration: none; border-color: var(--accent); color: var(--accent); }
  footer { margin-top: 64px; padding-top: 16px; border-top: 1px solid var(--rule); font-size: 12px; color: var(--muted); }

  details.ep {
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--card);
    margin: 10px 0;
    scroll-margin-top: 16px;
    overflow: hidden;
  }
  details.ep + details.ep { margin-top: 8px; }
  details.ep[open] { border-color: #d8d3c4; box-shadow: 0 1px 2px rgba(0,0,0,0.03); }
  details.ep:target { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
  details.ep > summary {
    list-style: none;
    cursor: pointer;
    padding: 13px 16px 13px 38px;
    position: relative;
    display: block;
    user-select: none;
  }
  details.ep > summary::-webkit-details-marker { display: none; }
  details.ep > summary::before {
    content: "";
    position: absolute;
    left: 16px;
    top: 50%;
    width: 6px;
    height: 6px;
    border-right: 1.5px solid var(--accent);
    border-bottom: 1.5px solid var(--accent);
    transform: translateY(-50%) rotate(-45deg);
    transition: transform 0.15s ease;
  }
  details.ep[open] > summary::before { transform: translateY(-50%) rotate(45deg); }
  details.ep > summary:hover { background: #f7f5ef; }
  .sline { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; }
  .route { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; font-size: 13.5px; font-weight: 600; color: var(--accent); }
  .sdesc { color: var(--muted); font-size: 13px; }
  .ep-body { padding: 4px 16px 18px; border-top: 1px solid var(--rule); }
  .ep-body > h3:first-child { margin-top: 14px; }
  .ep-body > p { font-size: 14px; }
</style>
</head>
<body>

<header>
  <h1>Research Tools</h1>
  <p class="sub">Public HTTP research endpoints for LLM agents. No auth, structured JSON in and out. Click any endpoint to expand its contract.</p>
  <p class="nav">
    <a href="#search_web">search_web</a>
    <a href="#search_news">search_news</a>
    <a href="#fetch_url">fetch_url</a>
    <a href="#encyclopedia_search">encyclopedia_search</a>
    <a href="#prediction_market_search">prediction_market_search</a>
    <a href="#stock_quote">stock_quote</a>
    <a href="#breaking_news">breaking_news</a>
    <a href="#pentagon_pizza">pentagon_pizza</a>
  </p>
</header>

<h2>Overview</h2>
<p>Every endpoint is <code>POST application/json</code> and returns <code>application/json</code>. There is no authentication &mdash; the service is public and only scrapes/queries public sources. Open to any caller. Set <code>BASE</code> to the worker origin (<code>https://tools.mycelium.markets</code>) for the curl snippets below.</p>

<h3>Health</h3>
<pre><code>GET /        &rarr; "ok"
GET /docs    &rarr; this page</code></pre>

<h3>Error contract</h3>
<p>Upstream failures return <strong>HTTP 200</strong> with <code>{"error": "&lt;message&gt;"}</code> so a calling LLM doesn't trigger a retry loop on a transient scrape failure. Bad request bodies return 4xx; worker bugs return 5xx.</p>
<pre><code>{ "error": "Search failed: upstream timeout" }</code></pre>

<h3>Routes</h3>
<table>
  <thead><tr><th>Endpoint</th><th>Default limit</th><th>Max</th></tr></thead>
  <tbody>
    <tr><td><a href="#search_web"><code>/v1/search_web</code></a></td><td>8</td><td>20</td></tr>
    <tr><td><a href="#search_news"><code>/v1/search_news</code></a></td><td>10</td><td>50</td></tr>
    <tr><td><a href="#fetch_url"><code>/v1/fetch_url</code></a></td><td>3500 chars</td><td>50000</td></tr>
    <tr><td><a href="#encyclopedia_search"><code>/v1/encyclopedia_search</code></a></td><td>5</td><td>25</td></tr>
    <tr><td><a href="#prediction_market_search"><code>/v1/prediction_market_search</code></a></td><td>8</td><td>50</td></tr>
    <tr><td><a href="#stock_quote"><code>/v1/stock_quote</code></a></td><td>1 symbol</td><td>25 symbols</td></tr>
    <tr><td><a href="#breaking_news"><code>/v1/breaking_news</code></a></td><td>up to 20 stories</td><td>&mdash;</td></tr>
    <tr><td><a href="#pentagon_pizza"><code>/v1/pentagon_pizza</code></a></td><td>no params</td><td>&mdash;</td></tr>
  </tbody>
</table>

<h2>Endpoints</h2>

<details class="ep" id="search_web">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/search_web</span></span>
    <span class="sdesc">Web search &mdash; Exa if keyed, else a keyless Mojeek &rarr; DuckDuckGo &rarr; Marginalia chain.</span>
  </summary>
  <div class="ep-body">
    <p>Uses the <a href="https://exa.ai">Exa Search API</a> if <code>EXA_API_KEY</code> is configured on the worker; otherwise falls back to a keyless chain (Mojeek &rarr; DuckDuckGo &rarr; Marginalia) where the first provider returning results wins. Returns a flat list of organic results regardless of backend.</p>
    <h3>Request</h3>
    <pre><code>{
  "query": "OpenAI o1 release date",
  "limit": 8
}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "results": [
    {
      "title": "OpenAI o1 — official announcement",
      "url":   "https://openai.com/...",
      "snippet": "We're releasing o1, a new series of reasoning..."
    }
  ]
}</code></pre>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/search_web \
  -H 'content-type: application/json' \
  -d '{"query":"OpenAI o1 release date","limit":5}'</code></pre>
  </div>
</details>

<details class="ep" id="search_news">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/search_news</span></span>
    <span class="sdesc">News search via Bing News RSS, recency-sorted.</span>
  </summary>
  <div class="ep-body">
    <p>News search via Bing News RSS. Items are recency-sorted by the upstream feed.</p>
    <h3>Request</h3>
    <pre><code>{
  "query": "Federal Reserve rate decision",
  "limit": 10
}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "items": [
    {
      "title":    "Fed holds rates steady at March meeting",
      "link":     "https://news.example.com/...",
      "pub_date": "Wed, 19 Mar 2025 18:00:00 GMT"
    }
  ]
}</code></pre>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/search_news \
  -H 'content-type: application/json' \
  -d '{"query":"Federal Reserve rate decision","limit":10}'</code></pre>
  </div>
</details>

<details class="ep" id="fetch_url">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/fetch_url</span></span>
    <span class="sdesc">Fetch one page as cleaned text &mdash; Jina Reader, then raw HTML fallback.</span>
  </summary>
  <div class="ep-body">
    <p>Fetch a single page and return cleaned text. Tries <a href="https://jina.ai/reader">Jina Reader</a> first for readable extraction, falls back to a raw HTML fetch + tag stripper. <code>source</code> in the response indicates which path served the result.</p>
    <h3>Request</h3>
    <pre><code>{
  "url": "https://example.com/article",
  "max_chars": 3500
}</code></pre>
    <p><code>max_chars</code> is clamped to <code>[100, 50000]</code> and defaults to <code>3500</code>.</p>
    <h3>Response</h3>
    <pre><code>{
  "text":   "Article body, stripped to plain text...",
  "source": "jina"
}</code></pre>
    <p><code>source</code> is <code>"jina"</code> or <code>"raw"</code>.</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/fetch_url \
  -H 'content-type: application/json' \
  -d '{"url":"https://example.com/article","max_chars":2000}'</code></pre>
  </div>
</details>

<details class="ep" id="encyclopedia_search">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/encyclopedia_search</span></span>
    <span class="sdesc">Merged reference lookup across Wikipedia + Grokipedia.</span>
  </summary>
  <div class="ep-body">
    <p>Parallel search across Wikipedia and Grokipedia. Results from both sources are merged into a single flat list, each tagged with its <code>source</code>.</p>
    <h3>Request</h3>
    <pre><code>{
  "query": "LMSR market maker",
  "limit": 5
}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "results": [
    {
      "source":  "wikipedia",
      "title":   "Logarithmic market scoring rule",
      "snippet": "An automated market-making mechanism...",
      "url":     "https://en.wikipedia.org/wiki/..."
    },
    {
      "source":  "grokipedia",
      "title":   "LMSR",
      "snippet": "...",
      "url":     "https://grokipedia.com/..."
    }
  ]
}</code></pre>
    <p><code>source</code> is <code>"wikipedia"</code> or <code>"grokipedia"</code>.</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/encyclopedia_search \
  -H 'content-type: application/json' \
  -d '{"query":"LMSR market maker","limit":5}'</code></pre>
  </div>
</details>

<details class="ep" id="prediction_market_search">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/prediction_market_search</span></span>
    <span class="sdesc">Cross-venue sentiment &mdash; Polymarket + Manifold + Kalshi.</span>
  </summary>
  <div class="ep-body">
    <p>Parallel search across Polymarket, Manifold, and Kalshi. Returns binary-market quotes as a YES probability percentage. Useful for sentiment lookup against external venues.</p>
    <h3>Request</h3>
    <pre><code>{
  "query": "2028 US presidential election",
  "limit": 8
}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "results": [
    {
      "source":          "polymarket",
      "question":        "Will candidate X win the 2028 US presidential election?",
      "url":             "https://polymarket.com/event/...",
      "probability_pct": 32.5,
      "end_date":        "2028-11-07T00:00:00Z",
      "volume":          1245678.0
    }
  ]
}</code></pre>
    <p><code>source</code> is <code>"polymarket"</code>, <code>"manifold"</code>, or <code>"kalshi"</code>. <code>probability_pct</code> may be <code>null</code> when the upstream market hasn't priced. <code>volume</code> is in the upstream venue's native units (typically USD).</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/prediction_market_search \
  -H 'content-type: application/json' \
  -d '{"query":"2028 US presidential election","limit":8}'</code></pre>
  </div>
</details>

<details class="ep" id="stock_quote">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/stock_quote</span></span>
    <span class="sdesc">Keyless equity quotes from CNBC &mdash; up to 25 symbols per call.</span>
  </summary>
  <div class="ep-body">
    <p>Fetch detailed stock quotes for a list of symbols, sourced from CNBC. <strong>No API key required.</strong> Up to <strong>25 symbols</strong> are resolved in a single upstream call. Successful quotes and per-symbol errors come back in the same response.</p>
    <h3>Request</h3>
    <pre><code>{
  "symbols": ["AAPL", "MSFT"]
}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "quotes": [
    {
      "symbol":         "AAPL",
      "name":           "Apple Inc",
      "price":          307.34,
      "change":         -3.89,
      "change_pct":     -1.25,
      "open":           312.86,
      "high":           315.17,
      "low":            307.15,
      "previous_close": 311.23,
      "volume":         60277341,
      "market_cap":     "4.514T",
      "pe":             37.33,
      "eps":            8.23,
      "dividend_yield": 0.35,
      "currency":       "USD",
      "exchange":       "NASDAQ",
      "market_status":  "POST_MKT",
      "as_of":          "06/05/26 EDT",
      "source":         "cnbc"
    }
  ],
  "errors": [
    { "symbol": "ZZZZ", "message": "symbol not found" }
  ]
}</code></pre>
    <p>Any numeric field may be <code>null</code> when the upstream omits it. <code>market_cap</code> is a display string (e.g. <code>"4.514T"</code>), not a number. <code>errors</code> lists symbols that could not be resolved, each with a message. <code>source</code> is currently always <code>"cnbc"</code>.</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/stock_quote \
  -H 'content-type: application/json' \
  -d '{"symbols":["AAPL","MSFT"]}'</code></pre>
  </div>
</details>

<details class="ep" id="breaking_news">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/breaking_news</span></span>
    <span class="sdesc">What the wire is covering right now &mdash; multi-outlet clustered headlines.</span>
  </summary>
  <div class="ep-body">
    <p>Surfaces stories the wire is covering <em>right now</em>. Pulls headlines from a handful of major outlets (BBC, NYT, Guardian, NPR, CNN, Al Jazeera, Sky News), groups near-duplicates via entity-blocked Jaccard clustering on stemmed tokens, and returns one representative per cluster. No embedding model, no LLM &mdash; pure token math, sub-second cold latency. Responses are edge-cached for 30 seconds.</p>
    <p>A story only appears if it&apos;s covered by at least <strong>2 distinct outlets</strong> within the last 12 hours. Sorted by source coverage (most-covered first).</p>
    <h3>Request</h3>
    <pre><code>{}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "stories": [
    {
      "headline": "Five people stuck in flooded cave in Laos for week found alive",
      "url":      "https://news.sky.com/story/...",
      "source":   "SkyNews",
      "sources":  4
    }
  ]
}</code></pre>
    <p><code>source</code> is the outlet that supplied the representative headline (earliest scoop, non-live-blog URL preferred). <code>sources</code> is the count of distinct outlets covering this story.</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/breaking_news \
  -H 'content-type: application/json' \
  -d '{}'</code></pre>
  </div>
</details>

<details class="ep" id="pentagon_pizza">
  <summary>
    <span class="sline"><span class="method">POST</span><span class="route">/v1/pentagon_pizza</span></span>
    <span class="sdesc">Novelty geopolitical signal &mdash; pizzeria activity near the Pentagon.</span>
  </summary>
  <div class="ep-body">
    <p>The <strong>Pentagon Pizza Index</strong>: a tongue-in-cheek geopolitical-activity signal derived from the live Google&nbsp;Maps popularity of pizzerias near the Pentagon, via <a href="https://www.pizzint.watch">pizzint.watch</a>. The folk theory is that late-night spikes in nearby pizza demand track unusual activity at the building. Lower <code>defcon_level</code> = more unusual activity. Strictly for fun &mdash; not a real intelligence source. Responses are edge-cached for 60 seconds.</p>
    <h3>Request</h3>
    <pre><code>{}</code></pre>
    <h3>Response</h3>
    <pre><code>{
  "headline": "data: fresh - DEFCON 5 - 0 current spikes with 12/14 places open",
  "defcon_level": 5,
  "defcon_severity": 4.8,
  "overall_index": 42,
  "active_spikes": 0,
  "spike_events": [
    {
      "place_name":         "...",
      "current_popularity": 80,
      "percentage_of_usual": 210,
      "spike_magnitude":    "...",
      "data_source":        "...",
      "minutes_ago":        12
    }
  ],
  "data_freshness": "fresh",
  "open_places": 12,
  "total_places": 14,
  "sustained": false,
  "sentinel": false,
  "place_data": [
    {
      "place_name":         "...",
      "current_popularity": 55,
      "percentage_of_usual": 120,
      "spike_magnitude":    "...",
      "data_source":        "..."
    }
  ],
  "source_url": "https://www.pizzint.watch/api/dashboard-data",
  "places_above_150": 1,
  "places_above_200": 0
}</code></pre>
    <p><code>defcon_level</code> runs 5 (calm) down to 1 (most unusual). <code>spike_events</code> lists places whose current popularity is anomalously high; <code>place_data</code> carries the per-pizzeria snapshot. <code>data_freshness</code> reflects how recent the upstream readings are.</p>
    <h3>curl</h3>
    <pre><code>curl -s $BASE/v1/pentagon_pizza \
  -H 'content-type: application/json' \
  -d '{}'</code></pre>
  </div>
</details>

<h2>Operational notes</h2>
<ul>
  <li><strong>Telemetry.</strong> If <code>TELEMETRY_URL</code> is configured, each <code>/v1/*</code> handler POSTs <code>{tool, ms, status, error}</code> to that URL via <code>ctx.waitUntil</code> after responding &mdash; no caller-visible latency. Opt-in; unset by default.</li>
  <li><strong>Caching.</strong> Identical request bodies are served from a short edge cache: <code>search_web</code> and <code>encyclopedia_search</code> 5 min, <code>prediction_market_search</code> and <code>pentagon_pizza</code> 1 min, <code>breaking_news</code> 30 s, <code>stock_quote</code> 5 s. <code>search_news</code> and <code>fetch_url</code> fetch fresh on each call.</li>
  <li><strong>Rate limits.</strong> None enforced today. Be reasonable.</li>
  <li><strong>Stability.</strong> v1 routes and JSON shapes won't break. New fields will always be additive.</li>
</ul>

<footer>
  Worker: Rust + WASM on Cloudflare
</footer>

</body>
</html>"##;
