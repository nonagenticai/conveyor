#!/usr/bin/env bash
# Capture the content service's served OpenAPI document from its pinned image, and write the
# drop table that classifies every operation. Issue #163 P1, the same shape as the analysis
# capture: the document comes from the old service, never from the rewrite.
#
#   capture.sh --image REPO@sha256:DIGEST [--out DIR]
#
# The service runs nowhere in the estate, so the document is read from the image at the digest
# the estate pins — never rebuilt, never from a floating tag. The image reference is supplied at
# capture time so that no private registry or repository name is stored in this file.
#
# Why not over HTTP: the application's lifespan requires database and Redis settings, and without
# them it never reaches the point of serving `/openapi.json`. The document is therefore read from
# the same ASGI object that route serialises (`app.openapi()`), with FastAPI's JSONResponse
# separators, so the bytes are the bytes that route returns once the app is up.
#
# Nothing here is guessed: the entrypoint is `python -m src.asgi`, the module exposes `app`, and
# the provenance below (`source_revision`) is read from the image's own label.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
image=""
out="$repo/docs/evidence"

while [ $# -gt 0 ]; do
    case "$1" in
        --image) image="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

usage() { echo "usage: $0 --image REPO@sha256:DIGEST [--out DIR]" >&2; exit 2; }
[ -n "$image" ] || usage
command -v docker >/dev/null 2>&1 || { echo "missing docker: install it first" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "missing jq: install it first" >&2; exit 1; }

digest="${image##*@}"
case "$digest" in
    sha256:*) ;;
    *) echo "--image must carry an explicit sha256 digest, not a tag" >&2; usage ;;
esac

date="$(date -u +%Y-%m-%d)"
json="$out/produce-openapi-$date.json"
md="$out/produce-capture-$date.md"
revision="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image" 2>/dev/null || true)"
case "$revision" in "") revision="unknown" ;; "<no value>") revision="unknown" ;; esac

mkdir -p "$out"

# The dummy settings satisfy the settings model and point at closed ports: nothing connects.
docker run --rm \
    -e ENVIRONMENT=development \
    -e DATABASE_URL=postgresql+psycopg://capture:capture@127.0.0.1:1/capture \
    -e REDIS_URL=redis://127.0.0.1:1/0 \
    -e AUTH_ENABLED=false \
    --entrypoint python "$image" -c '
import json, sys
from src.asgi import app
sys.stdout.write(json.dumps(app.openapi(), ensure_ascii=False, separators=(",", ":")))
' > "$json"

jq -e '.openapi and .paths' "$json" >/dev/null

ops="$(mktemp)"
rows="$(mktemp)"
trap 'rm -f "$ops" "$rows"' EXIT

jq -r '[.paths | to_entries[] | .key as $p | .value | keys[]
        | select(IN("get","post","put","patch","delete"))
        | (ascii_upcase) + " " + $p] | .[]' "$json" > "$ops"

# Every operation in the document must have exactly one line in the dispositions table, and every
# line must name an operation the document has. The capture refuses rather than write a table that
# does not account for the document.
dups="$(awk -F'\t' '!/^#/ && NF && $1 != "" { print $1 }' "$here/dispositions.tsv" | sort | uniq -d)"
[ -z "$dups" ] || { echo "duplicate disposition rows: $dups" >&2; exit 1; }

while IFS= read -r op; do
    line="$(awk -F'\t' -v op="$op" '$1 == op { print $2 "\t" $3; found = 1 } END { exit !found }' \
        "$here/dispositions.tsv")" || { echo "unclassified operation: $op" >&2; exit 1; }
    printf '| `%s` | %s | %s |\n' "$op" "${line%%$'\t'*}" "${line#*$'\t'}" >> "$rows"
done < "$ops"

declared="$(awk -F'\t' '!/^#/ && NF && $1 != ""' "$here/dispositions.tsv" | wc -l | tr -d ' ')"
actual="$(wc -l < "$ops" | tr -d ' ')"
[ "$declared" -eq "$actual" ] || {
    echo "disposition table has $declared rows but the document has $actual operations" >&2; exit 1; }

count() { awk -F'\t' -v d="$1" '!/^#/ && NF && $2 == d { n++ } END { print n + 0 }' "$here/dispositions.tsv"; }
counts="$(count 'ported') ported, $(count 'dropped: transport') transport, $(count 'dropped: no consumer') no consumer, $(count 'dropped: deferred') deferred"

if command -v shasum >/dev/null 2>&1; then
    sha="$(shasum -a 256 "$json" | cut -d' ' -f1)"
else
    sha="$(sha256sum "$json" | cut -d' ' -f1)"
fi

{
    printf '# Produce contract capture — %s\n\n' "$date"
    printf 'The content service the `produce.script` family absorbs (narrative and script). Captured\n'
    printf 'from the pinned image, not from the rewrite: a document regenerated from the implementation\n'
    printf 'cannot fail for the reason the parity test exists.\n\n'
    printf '| Field | Value |\n|---|---|\n'
    printf '| service | content service (narrative, script) |\n'
    printf '| image_digest | %s |\n' "$digest"
    printf '| source_revision | %s |\n' "$revision"
    printf '| capture_date | %s |\n' "$date"
    printf '| openapi_sha256 | %s |\n' "$sha"
    printf '| openapi | %s |\n' "$(jq -r '.openapi' "$json")"
    printf '| operations | %s |\n' "$actual"
    printf '| capture_method | in-image ASGI `app.openapi()`, FastAPI JSONResponse separators |\n\n'
    printf 'The service boots its HTTP server only with a database and Redis present, so the document\n'
    printf 'was read from the same ASGI object the `/openapi.json` route serialises, inside the same\n'
    printf 'image at the same digest. The bytes are the bytes that route returns.\n\n'
    printf '## Drop table\n\n'
    printf 'One row per operation in the captured document. A disposition is `ported`,\n'
    printf '`dropped: transport`, `dropped: no consumer` or `dropped: deferred`; the test\n'
    printf '`crates/produce/tests/parity.rs::capture` fails on an operation with no row or an\n'
    printf 'illegal disposition. Decisions are recorded in `crates/produce/contract/dispositions.tsv`.\n\n'
    printf '| Operation | Disposition | Reason |\n|---|---|---|\n'
    cat "$rows"
    printf '| **Total** | **%s** | **%s** |\n' "$actual" "$counts"
} > "$md"

echo "wrote $json ($sha) and $md"
