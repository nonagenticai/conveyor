#!/usr/bin/env bash
# Capture the live inference-router contract: OpenAPI lock + provenance.
#
#   capture.sh --live http://HOST:PORT [--image-digest sha256:...] [--out DIR]
#
# The lock is the normalised document the Rust rewrite must serve; the manifest
# records the capture time, the deployed image digest and the lock's own sha256.
# No service address is stored: the URL is supplied at capture time.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
digest=""
out="$here"

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --image-digest) digest="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) echo "usage: $0 --live URL [--image-digest sha256:...] [--out DIR]" >&2; exit 2 ;;
    esac
done

usage() { echo "usage: $0 --live URL [--image-digest sha256:...] [--out DIR]" >&2; exit 2; }
[ -n "$live" ] || usage
command -v jq >/dev/null 2>&1 || { echo "missing jq: install it first" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "missing curl: install it first" >&2; exit 1; }

mkdir -p "$out"
raw="$(mktemp)"
trap 'rm -f "$raw"' EXIT

curl -fsS --max-time 30 "$live/openapi.json" -o "$raw"
jq -S . "$raw" > "$out/openapi.lock.json"

if command -v shasum >/dev/null 2>&1; then
    lock_sha="$(shasum -a 256 "$out/openapi.lock.json" | cut -d' ' -f1)"
else
    lock_sha="$(sha256sum "$out/openapi.lock.json" | cut -d' ' -f1)"
fi

jq -n \
    --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg digest "$digest" \
    --arg sha "$lock_sha" \
    --argjson paths "$(jq '.paths | length' "$out/openapi.lock.json")" \
    --argjson schemas "$(jq '.components.schemas | length' "$out/openapi.lock.json")" \
    '{service:"inference-router", captured_at:$at, image_digest:$digest, lock_sha256:$sha,
      openapi:"3.1.0", paths:$paths, schemas:$schemas}' \
    > "$out/manifest.json"

echo "wrote $out/openapi.lock.json ($lock_sha) and $out/manifest.json"
