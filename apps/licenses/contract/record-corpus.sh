#!/usr/bin/env bash
# Record a probe corpus from a live service, in the format shadow.sh replays.
#
#   record-corpus.sh --base URL --out FILE [--cases DIR]
#
# The request set is the committed golden set (one .request per case), so the corpus probes
# exactly the contract this unit is measured against. Each line is
#   {"id","method","path","headers":{},"request_body","status","content_type","body"}
# with `status`/`content_type`/`body` as the live service answered them.
#
# ⛔ Record from the PINNED OLD SERVICE (contract/manifest.json). A corpus recorded from the
# candidate proves nothing — shadow.sh's header says the same. This is a LIVE capture: bodies
# may carry deployment-specific values, so the corpus itself is never committed; only
# shadow-report.json (counts and hashed case ids) is.
#
# Exit: 0 recorded; 2 usage/transport; 3 no cases (a vacuous zero is not a pass).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
base=""
out=""
cases="$here/golden"

usage() {
    sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --base) base="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        --cases) cases="${2:-}"; shift 2 ;;
        *) usage ;;
    esac
done

[ -n "$base" ] && [ -n "$out" ] || usage
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
[ -d "$cases" ] || { echo "no cases dir: $cases" >&2; exit 2; }
base="${base%/}"

n=0
: > "$out"
for req in "$cases"/*.request; do
    [ -f "$req" ] || continue
    name="$(basename "$req" .request)"
    method="$(sed -n '1p' "$req")"
    path="$(sed -n '2p' "$req")"
    headers_json='{}'
    request_body=""

    # The .request file is a curl-like argv after the first two lines: `-H <header>` and
    # `--data-raw <body>` pairs, one value per line.
    pending=""
    while IFS= read -r line; do
        if [ "$pending" = "H" ]; then
            key="${line%%:*}"
            val="${line#*: }"
            headers_json="$(jq -c --arg k "$key" --arg v "$val" '. + {($k): $v}' <<<"$headers_json")"
            pending=""
            continue
        fi
        if [ "$pending" = "B" ]; then
            request_body="$line"
            pending=""
            continue
        fi
        case "$line" in
            -H) pending="H" ;;
            --data-raw|--data) pending="B" ;;
        esac
    done < <(sed -n '3,$p' "$req")

    body_file="$(mktemp)"
    head_file="$(mktemp)"
    args=(-sS -m 30 -X "$method" -o "$body_file" -D "$head_file" -w '%{http_code}')
    while IFS=$'\t' read -r k v; do
        [ -n "$k" ] && args+=(-H "$k: $v")
    done < <(jq -r 'to_entries[] | "\(.key)\t\(.value)"' <<<"$headers_json")
    [ -n "$request_body" ] && args+=(--data "$request_body")

    status="$(curl "${args[@]}" "$base$path" 2>/dev/null || echo 000)"
    ctype="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' "$head_file" | tr -d '\r')"
    body="$(cat "$body_file")"

    jq -cn --arg id "$name" --arg method "$method" --arg path "$path" \
        --argjson headers "$headers_json" --arg request_body "$request_body" \
        --arg status "$status" --arg content_type "${ctype:-application/json}" \
        --arg body "$body" \
        '{id:$id, method:$method, path:$path, headers:$headers, request_body:$request_body,
          status:($status|tonumber), content_type:$content_type, body:$body}' >> "$out"
    rm -f "$body_file" "$head_file"
    n=$((n + 1))
done

[ "$n" -gt 0 ] || { echo "no .request cases in $cases — a vacuous zero is not a pass" >&2; exit 3; }
echo "recorded $n case(s) from $base -> $out" >&2
