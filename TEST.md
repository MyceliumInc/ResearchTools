# Endpoint smoke + latency script

Live-tests every `/v1/*` endpoint using `curl`. Times each call via `curl`'s
`-w` timing format, validates the JSON shape with `jq`, and prints PASS/FAIL
per endpoint.

Run all of them:

```bash
BASE="${BASE:-http://127.0.0.1:8787}"

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
check encyclopedia_search /v1/encyclopedia_search '{"query":"2028 United States presidential election","limit":3}' '.results | type == "array" and length > 0'
check prediction_market_search /v1/prediction_market_search '{"query":"2028 presidential election","limit":3}' '.results | type == "array"'
```

Override `BASE` to test a deployed instance:

```bash
BASE=https://your-worker.example.com bash test.sh
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
