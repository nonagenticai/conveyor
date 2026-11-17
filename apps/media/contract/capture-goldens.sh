#!/usr/bin/env bash
# Capture the golden request/response set for one reference.
#
#   capture-goldens.sh --service NAME --live http://127.0.0.1:PORT [--out DIR]
#
# `cases.txt` is pipe-separated, five fields, one case per line; `#` starts a comment:
#
#   name | METHOD | path | headers (JSON object or empty) | body (JSON or empty)
#
# The body field may not contain a `|`. Headers are how the tenant contract is pinned: every case
# that needs one names it here rather than relying on a default the reference would supply — a
# sentinel tenant in the goldens would prove nothing about the rewrite honouring the header.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
service=""; live=""; out=""
while [ $# -gt 0 ]; do
    case "$1" in
        --service) service="${2:-}"; shift 2 ;;
        --live) live="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) break ;;
    esac
done
[ -n "$service" ] && [ -n "$live" ] || { echo "usage: $0 --service NAME --live URL [--out DIR]" >&2; exit 2; }
out="${out:-$here/$service}"
mkdir -p "$out/golden"

while IFS='|' read -r name method path headers body; do
    case "$name" in ''|'#'*) continue ;; esac
    printf '%s %s' "$method" "$path" > "$out/golden/$name.request"
    [ -n "$headers" ] && printf '\n%s' "$headers" >> "$out/golden/$name.request"
    [ -n "$body" ] && printf '\n%s' "$body" >> "$out/golden/$name.request"
    hdrs=(-H 'content-type: application/json')
    if [ -n "$headers" ]; then
        while IFS=$'\t' read -r k v; do hdrs+=(-H "$k: $v"); done < <(printf '%s' "$headers" | jq -r 'to_entries[]|[.key,.value]|@tsv')
    fi
    if [ -n "$body" ]; then
        curl -sS --max-time 180 -X "$method" "${hdrs[@]}" --data "$body" \
             -o "$out/golden/$name.body" -D "$out/golden/$name.headers" "$live$path"
    else
        curl -sS --max-time 180 -X "$method" "${hdrs[@]}" \
             -o "$out/golden/$name.body" -D "$out/golden/$name.headers" "$live$path"
    fi
    awk 'NR==1{print $2}' "$out/golden/$name.headers" > "$out/golden/$name.status"
    awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[ ]*/,""); sub(/\r$/,""); print; exit}' \
        "$out/golden/$name.headers" > "$out/golden/$name.content-type"
    rm -f "$out/golden/$name.headers"
    echo "captured $name ($(cat "$out/golden/$name.status"))"
done < "$out/cases.txt"
