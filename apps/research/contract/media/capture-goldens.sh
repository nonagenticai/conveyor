#!/usr/bin/env bash
# Capture the golden request/response set from a reference media-discovery.
#
#   capture-goldens.sh --live http://HOST:PORT [--out DIR]
#
# Boot the reference with `../run-local.sh` (every adapter disabled, no licence token, a scratch
# Postgres). Every payload is synthetic: the fixed ids in `cases.txt` are the nil-uuid form and the
# only rows the run creates are the ones it deletes again.
#
# `{COLLECTION_ID}` in a path is substituted from the body of the `collection_create` case, which
# runs earlier in the file.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
out="$here/golden"

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) break ;;
    esac
done

[ -n "$live" ] || { echo "usage: $0 --live URL [--out DIR]" >&2; exit 2; }
command -v curl >/dev/null 2>&1 || { echo "missing curl: install it first" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "missing jq: install it first" >&2; exit 1; }

mkdir -p "$out"
collection_id=""

# The subject of the items happy-path cases: the first row in `seed.sql`, which must have been
# loaded into the reference's scratch database before this script runs.
item_id="10000000-0000-7000-8000-000000000001"

while IFS='|' read -r name method path body; do
    case "$name" in ''|'#'*) continue ;; esac
    path="${path//\{COLLECTION_ID\}/$collection_id}"
    path="${path//\{ITEM_ID\}/$item_id}"
    printf '%s %s' "$method" "$path" > "$out/$name.request"
    if [ -n "$body" ]; then
        printf '\n%s' "$body" >> "$out/$name.request"
        curl -sS --max-time 120 -X "$method" -H 'content-type: application/json' \
            --data "$body" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    else
        curl -sS --max-time 120 -X "$method" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    fi
    awk 'NR==1{print $2}' "$out/$name.headers" > "$out/$name.status"
    awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[ ]*/,""); sub(/\r$/,""); print; exit}' \
        "$out/$name.headers" > "$out/$name.content-type"
    rm -f "$out/$name.headers"
    if [ "$name" = "collection_create" ]; then
        collection_id="$(jq -r '.id // empty' "$out/$name.body")"
        [ -n "$collection_id" ] || { echo "collection_create returned no id" >&2; exit 1; }
    fi
    echo "captured $name ($(cat "$out/$name.status"))"
done < "$here/cases.txt"
