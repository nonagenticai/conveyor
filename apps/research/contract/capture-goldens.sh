#!/usr/bin/env bash
# Capture the golden request/response set from a reference web-scraper.
#
#   capture-goldens.sh --live http://HOST:PORT --fixtures http://HOST:PORT [--out DIR]
#
# `--fixtures` is the base URL of the synthetic corpus in `fixtures/` (any static file server:
# `python3 -m http.server` over that directory). The reference service must permit that host through
# its SSRF guard (`SSRF_ALLOWED_HOSTS='["127.0.0.1"]'` when the corpus is served on loopback) — the
# guard's own refusal is captured separately by the `blocked_address` case.
#
# Every payload is synthetic: the corpus is in this tree, and no real host is named anywhere.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
fixtures=""
out="$here/webscraper/golden"

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --fixtures) fixtures="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) break ;;
    esac
done

[ -n "$live" ] || { echo "usage: $0 --live URL --fixtures URL [--out DIR]" >&2; exit 2; }
[ -n "$fixtures" ] || { echo "usage: $0 --live URL --fixtures URL [--out DIR]" >&2; exit 2; }
command -v curl >/dev/null 2>&1 || { echo "missing curl: install it first" >&2; exit 1; }

mkdir -p "$out"

while IFS='|' read -r name method path body; do
    case "$name" in ''|'#'*) continue ;; esac
    printf '%s %s' "$method" "$path" > "$out/$name.request"
    rendered="${body//\{BASE\}/$fixtures}"
    if [ -n "$rendered" ]; then
        printf '\n%s' "$rendered" >> "$out/$name.request"
        curl -sS --max-time 120 -X "$method" -H 'content-type: application/json' \
            --data "$rendered" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    else
        curl -sS --max-time 120 -X "$method" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    fi
    awk 'NR==1{print $2}' "$out/$name.headers" > "$out/$name.status"
    awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[ ]*/,""); sub(/\r$/,""); print; exit}' \
        "$out/$name.headers" > "$out/$name.content-type"
    rm -f "$out/$name.headers"
    echo "captured $name ($(cat "$out/$name.status"))"
done < "$here/cases.txt"
