# Tools endpoint smoke + latency script

Live-tests every `/v1/*` endpoint against `tools.mycelium.markets` using
`curl`. Times each call via `curl`'s `-w` timing format, validates the JSON
shape with `jq`, and prints PASS/FAIL per endpoint.

Run all of them:

```bash
BASE="${BASE:-https://tools.mycelium.markets}"

fmt='time_total=%{time_total}s  http=%{http_code}  size=%{size_download}B\n'

check() {
  local name="$1" path="$2" body="$3" jqtest="$4"
  local tmp; tmp=$(mktemp)
  local timing
  timing=$(curl -sS -o "$tmp" -w "$fmt" -X POST "$BASE$path" \
    -H 'Content-Type: application/json' -d "$body")
  local status="fail"
  if jq -e "$jqtest" < "$tmp" > /dev/null 2>&1; then status="PASS"; else status="FAIL"; fi
  printf '%-22s %s  %s\n' "$name" "$status" "$timing"
  if [ "$status" = "FAIL" ]; then
    echo "  body: $(head -c 400 < "$tmp")"
  fi
  rm -f "$tmp"
}

echo "== root =="
curl -sS -o /dev/null -w "$fmt" "$BASE/"

echo
echo "== endpoints =="
check search_web        /v1/search_web        '{"query":"cloudflare workers","limit":3}'        '.results | type == "array" and length > 0'
check search_news       /v1/search_news       '{"query":"bitcoin","limit":3}'                   '.items   | type == "array"'
check fetch_url         /v1/fetch_url         '{"url":"https://www.example.com","max_chars":500}' '.text    | type == "string" and length > 0'
check wikipedia_summary /v1/wikipedia_summary '{"title":"Cloudflare"}'                          '.summary | type == "string" and length > 0'
check grokipedia_search /v1/grokipedia_search '{"query":"ethereum","limit":3}'                  '.results | type == "array"'
check kalshi_search     /v1/kalshi_search     '{"query":"election","limit":3}'                  '.results | type == "array"'
check polymarket_search /v1/polymarket_search '{"query":"election","limit":3}'                  '.results | type == "array"'
check manifold_search   /v1/manifold_search   '{"query":"bitcoin","limit":3}'                   '.results | type == "array"'
check metaculus_search  /v1/metaculus_search  '{"query":"ai","limit":3}'                        '.results | type == "array"'
check usgs_earthquakes  /v1/usgs_earthquakes  '{"min_magnitude":2.5,"hours":168,"limit":5}'     '.results | type == "array"'
check gdelt_search      /v1/gdelt_search      '{"query":"inflation","limit":5,"timespan":"24h"}' '.results | type == "array"'
check reddit_search     /v1/reddit_search     '{"query":"bitcoin","limit":3,"sort":"top","time":"week"}' '.results | type == "array"'
check wikidata_sparql   /v1/wikidata_sparql   '{"query":"SELECT ?item ?itemLabel WHERE { ?item wdt:P31 wd:Q3624078 . SERVICE wikibase:label { bd:serviceParam wikibase:language \"en\" . } } LIMIT 3"}' '.rows | type == "array"'
check sec_filings       /v1/sec_filings       '{"query":"Apple","limit":3,"forms":["10-K"]}'    '.results | type == "array"'
check weather_forecast  /v1/weather_forecast  '{"lat":40.7128,"lon":-74.0060}'                  '.periods | type == "array" and length > 0'
```

## Repeat N times (latency sample)

```bash
for i in 1 2 3 4 5; do
  curl -sS -o /dev/null -w 'search_web  %{time_total}s\n' -X POST "$BASE/v1/search_web" \
    -H 'Content-Type: application/json' -d '{"query":"cloudflare workers","limit":3}'
done
```

## Notes

- Endpoints always return HTTP 200. Upstream failures come back as
  `{"error":"..."}` — a FAIL here means the JSON shape check failed (either
  error payload or malformed response).
- `search_news` sometimes 503s upstream from Google News — retry before
  calling it broken.
