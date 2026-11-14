#!/usr/bin/env bash
# Capture the live licenses contract: OpenAPI lock + provenance.
#
#   capture.sh --live URL [--image-digest sha256:...] [--out DIR] [--compare FILE]
#
# The lock is the normalised document the Rust rewrite must serve; the manifest
# records the capture time, the deployed image digest and the lock's own sha256.
# No service address is stored: the URL is supplied at capture time.
#
# --compare takes a third-party snapshot (for example the old service's committed
# docs/openapi.json) and writes a structural diff report beside the manifest.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
digest=""
out="$here"
compare=""

usage() {
    echo "usage: $0 --live URL [--image-digest sha256:...] [--out DIR] [--compare FILE]" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --image-digest) digest="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        --compare) compare="${2:-}"; shift 2 ;;
        *) usage ;;
    esac
done

[ -n "$live" ] || usage
command -v jq >/dev/null 2>&1 || { echo "missing jq: install it first" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || { echo "missing curl: install it first" >&2; exit 1; }

mkdir -p "$out"
raw="$(mktemp)"
trap 'rm -f "$raw"' EXIT

curl -fsS --max-time 30 "${live%/}/openapi.json" -o "$raw"
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
    --arg version "$(jq -r '.openapi' "$out/openapi.lock.json")" \
    --argjson paths "$(jq '.paths | length' "$out/openapi.lock.json")" \
    --argjson methods "$(jq '[.paths[] | keys[]] | length' "$out/openapi.lock.json")" \
    --argjson schemas "$(jq '.components.schemas | length' "$out/openapi.lock.json")" \
    '{service:"licenses", captured_at:$at, image_digest:$digest, lock_sha256:$sha,
      openapi:$version, paths:$paths, methods:$methods, schemas:$schemas}' \
    > "$out/manifest.json"

if [ -n "$compare" ]; then
    [ -f "$compare" ] || { echo "compare file not found: $compare" >&2; exit 1; }
    jq -S . "$compare" > "$out/.compare.json"
    if diff -q "$out/.compare.json" "$out/openapi.lock.json" >/dev/null; then
        echo "compare: byte-identical after jq -S" > "$out/compare-report.txt"
    else
        # A structural summary, never a dump: the paths and methods each side has.
        {
            echo "compare: $compare"
            echo "--- paths only in the lock ---"
            comm -23 \
                <(jq -r '.paths | keys[]' "$out/openapi.lock.json" | sort) \
                <(jq -r '.paths | keys[]' "$out/.compare.json" | sort)
            echo "--- paths only in the snapshot ---"
            comm -13 \
                <(jq -r '.paths | keys[]' "$out/openapi.lock.json" | sort) \
                <(jq -r '.paths | keys[]' "$out/.compare.json" | sort)
            echo "--- counts ---"
            jq -n \
                --argjson lock_paths "$(jq '.paths | length' "$out/openapi.lock.json")" \
                --argjson snap_paths "$(jq '.paths | length' "$out/.compare.json")" \
                --argjson lock_schemas "$(jq '.components.schemas | length' "$out/openapi.lock.json")" \
                --argjson snap_schemas "$(jq '.components.schemas | length' "$out/.compare.json")" \
                '{lock_paths:$lock_paths, snapshot_paths:$snap_paths,
                  lock_schemas:$lock_schemas, snapshot_schemas:$snap_schemas}'
        } > "$out/compare-report.txt"
    fi
    rm -f "$out/.compare.json"
    echo "wrote $out/compare-report.txt"
fi

echo "wrote $out/openapi.lock.json ($lock_sha) and $out/manifest.json"
