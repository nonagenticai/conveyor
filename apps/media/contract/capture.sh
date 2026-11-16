#!/usr/bin/env bash
# Capture one reference's OpenAPI lock and its provenance.
#
#   capture.sh --service NAME --live http://127.0.0.1:PORT --image-digest sha256:... \
#              [--source-commit SHA] [--out DIR]
#
# The lock is what the Rust rewrite in `apps/media` must serve. It is taken from the reference
# service, never from the rewrite: a lock regenerated from the implementation cannot fail for the
# reason the check exists.
#
# No service address is stored: the URL is supplied at capture time.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
service=""; live=""; digest=""; commit=""; out=""
while [ $# -gt 0 ]; do
    case "$1" in
        --service) service="${2:-}"; shift 2 ;;
        --live) live="${2:-}"; shift 2 ;;
        --image-digest) digest="${2:-}"; shift 2 ;;
        --source-commit) commit="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) break ;;
    esac
done
[ -n "$service" ] && [ -n "$live" ] || { echo "usage: $0 --service NAME --live URL [--image-digest ...] [--source-commit ...] [--out DIR]" >&2; exit 2; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 1; }
out="${out:-$here/$service}"
mkdir -p "$out"

tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT
curl -sS -m 30 "$live/openapi.json" -o "$tmp"
jq -e '.paths' "$tmp" >/dev/null || { echo "$live/openapi.json is not an OpenAPI document" >&2; exit 1; }
jq -S . "$tmp" > "$out/openapi.lock.json"

ops="$(jq '[.paths | to_entries[] | .key as $p | .value | keys[] | select(. as $m | ["get","put","post","delete","patch"] | index($m)) | "\(.|ascii_upcase) \($p)"] | sort | .[]' -r "$out/openapi.lock.json")"
jq -n --arg service "$service" --arg digest "$digest" --arg commit "$commit" \
      --arg taken "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson ops "$(printf '%s\n' "$ops" | jq -R . | jq -s .)" \
      '{service:$service, image_digest:$digest, source_commit:$commit, taken:$taken,
        operations:($ops|length), paths:([$ops[]|split(" ")[1]]|unique|length), op_list:$ops}' \
      > "$out/manifest.json"

printf '%s\n' "$ops" > "$out/operations.txt"
echo "$service: $(printf '%s\n' "$ops" | wc -l | tr -d ' ') operations, $(jq '.paths|length' "$out/openapi.lock.json") paths -> $out"
