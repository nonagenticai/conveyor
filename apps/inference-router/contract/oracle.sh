#!/usr/bin/env bash
# Boot the PINNED OLD inference-router image as the parity oracle.
#
#   oracle.sh [--prefix NAME] [--keep] [--down]
#             [--tenant UUID] [--policy FILE]
#             [--ledger-out FILE] [--cost-out FILE]
#             [--delegated-ledger-out FILE] [--delegated-cost-out FILE]
#             [--capture [--capture-out DIR]]
#
# The oracle is the reference implementation. Everything the Rust rewrite is measured against is
# captured HERE, never from the rewrite: `capture-goldens.sh` and `verify.sh` both take its URL.
#
# WHY THIS FILE EXISTS. The S9a capture ran from a session's shell history: the stub, the seed and
# the provider catalog were committed, but the launch was not — so a later session could not stand
# the oracle up without re-deriving the environment it needs (which services it refuses to boot
# without, which variable name is the introspection MODE, how the database reaches alembic 0016).
# This script is that launch, measured from the running containers on 2026-11-13.
#
# REPRODUCIBILITY RULES
#   * the image is pulled BY DIGEST from manifest.json, never `:latest`;
#   * every dependency is a throwaway container on a private network this script creates;
#   * the provider catalog is the committed stub with only the stub's hostname substituted (the
#     stub is named per --prefix, so two sessions can run at once);
#   * the database is EMPTY at start — the image runs `alembic upgrade head` in its own lifespan,
#     which is also the behaviour a fresh production replica has;
#   * `--down` removes everything, and a non-`--keep` run tears down on exit.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
prefix="ir-oracle"
keep=0
down=0
policy=""
ledger_out=""
cost_out=""
delegated_ledger_out=""
delegated_cost_out=""
capture=0
capture_out="$here/golden"
tenant="00000000-0000-4000-8000-0000000000a1"   # the stub's tenant (contract/stubs/oracle_stub.py)
request_model=""                                 # "" => resolved after arg parsing

usage() {
    sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) prefix="${2:-}"; shift 2 ;;
        --keep) keep=1; shift ;;
        --down) down=1; shift ;;
        --tenant) tenant="${2:-}"; shift 2 ;;
        --policy) policy="${2:-}"; shift 2 ;;
        --ledger-out) ledger_out="${2:-}"; shift 2 ;;
        --cost-out) cost_out="${2:-}"; shift 2 ;;
        --delegated-ledger-out) delegated_ledger_out="${2:-}"; shift 2 ;;
        --delegated-cost-out) delegated_cost_out="${2:-}"; shift 2 ;;
        --request-model) request_model="${2:-}"; shift 2 ;;
        --capture) capture=1; shift ;;
        --capture-out) capture_out="${2:-}"; shift 2 ;;
        *) usage ;;
    esac
done

command -v docker >/dev/null || { echo "missing docker" >&2; exit 2; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
[ -n "$prefix" ] || { echo "--prefix must not be empty" >&2; exit 2; }

# Exactly 43 characters of [A-Za-z0-9_-]: the only shape the router sends to introspection. Derived
# from the same prefix the stub uses (contract/stubs/oracle_stub.py::_tok), so the two cannot drift.
tok() { printf '%s%s' "$1" "$(printf '0%.0s' $(seq 1 $((43 - ${#1}))))"; }

# macOS ships shasum, not sha256sum.
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi; }

net="$prefix-net"
pg="$prefix-pg"
redis="$prefix-redis"
stub="$prefix-stub"
oracle="$prefix-oracle"

remove_all() {
    for c in "$oracle" "$stub" "$redis" "$pg"; do
        docker rm -f -v "$c" >/dev/null 2>&1 || true
    done
    docker network rm "$net" >/dev/null 2>&1 || true
}

if [ "$down" = 1 ]; then
    remove_all
    echo "oracle: removed (prefix $prefix)"
    exit 0
fi

if [ "$keep" != 1 ]; then
    trap 'remove_all' EXIT
fi

digest="$(jq -r '.image_digest' "$here/manifest.json")"
[ "$digest" != "null" ] && [ -n "$digest" ] || { echo "manifest.json has no image_digest" >&2; exit 2; }
# The pinned image lives in the private registry; the host is the CALLER's, never this file's.
registry="${ORACLE_REGISTRY:-}"
[ -n "$registry" ] || {
    echo "set ORACLE_REGISTRY to the private registry host:port (the pinned image is not public)" >&2
    exit 2
}
image="${registry%/}/inference-router@${digest}"

echo "==> pulling $image"
docker pull "$image" >/dev/null

remove_all
docker network create "$net" >/dev/null

echo "==> postgres:18"
# NO --tmpfs: masking /var/lib/postgresql makes the upstream entrypoint loop (measured in S9a).
docker run -d --name "$pg" --network "$net" \
    -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=oracle -e POSTGRES_DB=inference_router \
    postgres:18 >/dev/null

echo "==> redis:8.10.1-alpine (the cost bus)"
docker run -d --name "$redis" --network "$net" library/redis:8.10.1-alpine >/dev/null

echo "==> stub (providers + licences + billing reserve)"
docker run -d --name "$stub" --network "$net" \
    -v "$here/stubs:/stub:ro" \
    python:3.13-slim python /stub/oracle_stub.py 9100 >/dev/null

# The committed catalog names the stub `ir44-stub` (the containers the S9a session left behind).
# Substitute the hostname for this prefix into a temp file; the endpoint never appears in a golden
# (the /api/providers response carries name/protocol/costs only), so this cannot shift one.
providers="$(mktemp)"
sed "s/ir44-stub/$stub/g" "$here/stubs/oracle-providers.yaml" >"$providers"

echo "==> oracle ($digest)"
# INFERENCE_ROUTER_INTERNAL_SECRET MUST equal capture-goldens.sh's SECRET, or the six
# /api/internal/* cases 401.
docker run -d --name "$oracle" --network "$net" -p 127.0.0.1::8150 \
    -v "$providers:/config/providers.yaml:ro" \
    -e ENABLE_DOCS=true \
    -e COMMERCIAL_PLATFORM_TENANT_IDS=00000000-0000-4000-8000-0000000000ff \
    -e INFERENCE_ROUTER_ADAPTER_MANAGER_URL=http://127.0.0.1:9 \
    -e INFERENCE_ROUTER_OTLP_TRACES_ENDPOINT=http://127.0.0.1:9 \
    -e INFERENCE_ROUTER_DATABASE_URL="postgresql+psycopg://postgres:oracle@${pg}:5432/inference_router" \
    -e INFERENCE_ROUTER_LICENSE_INTROSPECT_URL="http://${stub}:9100/license/introspect" \
    -e INFERENCE_ROUTER_INTROSPECT_MODE=enforce \
    -e INFERENCE_ROUTER_BUDGET_GATE_MODE=enforce \
    -e INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL="http://${stub}:9100" \
    -e INFERENCE_ROUTER_POLICY_MODE=on \
    -e INFERENCE_ROUTER_INTERNAL_SECRET=stub-internal-secret \
    -e AUTH_DELEGATING_SUBJECTS=8382f0b3-c082-4f97-8d47-120ce5a18e05 \
    -e INFERENCE_ROUTER_METERING_REDIS_URL="redis://${redis}:6379/0" \
    -e INFERENCE_ROUTER_METERING_COSTS_ENABLED=true \
    -e INFERENCE_ROUTER_METERING_COSTS_STREAM=billing:costs \
    "$image" >/dev/null

echo "==> waiting for the oracle (it migrates its own empty database at boot)"
deadline=$((SECONDS + 180))
url=""
while [ $SECONDS -lt $deadline ]; do
    if [ -z "$url" ]; then
        port="$(docker port "$oracle" 8150/tcp 2>/dev/null | head -1 | sed 's/.*://')"
        [ -n "$port" ] && url="http://127.0.0.1:${port}"
    fi
    if [ -n "$url" ] && curl -fsS "$url/health" >/dev/null 2>&1; then
        break
    fi
    if [ "$(docker inspect -f '{{.State.Running}}' "$oracle" 2>/dev/null)" != "true" ]; then
        echo "oracle exited before becoming healthy:" >&2
        docker logs "$oracle" 2>&1 | tail -30 >&2
        exit 1
    fi
    sleep 2
done
[ -n "$url" ] && curl -fsS "$url/health" >/dev/null || { echo "oracle never became healthy" >&2; docker logs "$oracle" 2>&1 | tail -30 >&2; exit 1; }

psql_run() { docker exec -i "$pg" psql -U postgres -d inference_router -v ON_ERROR_STOP=1 "$@"; }

echo "oracle:  $url"
echo "psql:    docker exec -i $pg psql -U postgres -d inference_router"
echo "token:   $(tok stub-licence-token-)"
echo "delegated token: $(tok stub-licence-delegated-)"

if [ -n "$policy" ]; then
    [ -f "$policy" ] || { echo "--policy file not found: $policy" >&2; exit 2; }
    # The stored form is the canonical doc: sha256 over sorted, compact JSON — the same rule as
    # services/inference_policy.py::doc_sha256. A version row is immutable; a second run of this
    # script against the same database adds a new version rather than rewriting one.
    doc_sha="$(jq -S -c . "$policy" | sha256 | cut -d' ' -f1)"
    version="$(psql_run -Atc "SELECT COALESCE(MAX(version), 0) + 1 FROM tenant_inference_policy_version WHERE tenant_id = '$tenant'")"
    psql_run -q <<SQL
INSERT INTO tenant_inference_policy_version (tenant_id, version, doc, doc_sha256, actor_kind, actor_ref_hash)
VALUES ('$tenant', $version, '$(jq -S -c . "$policy")'::jsonb, '$doc_sha', 'oracle', 'contract-oracle');
INSERT INTO tenant_inference_policy (tenant_id, active_version)
VALUES ('$tenant', $version)
ON CONFLICT (tenant_id) DO UPDATE SET active_version = EXCLUDED.active_version, updated_at = now();
UPDATE policy_epoch SET epoch = epoch + 1 WHERE id = 1;
SQL
    echo "policy:  tenant $tenant active at version $version (sha256 $doc_sha)"
fi

if [ "$capture" = 1 ]; then
    echo "==> capturing the golden set into $capture_out"
    IR_PSQL="docker exec -i $pg psql -U postgres -d inference_router -v ON_ERROR_STOP=1" \
        "$here/capture-goldens.sh" --live "$url" --out "$capture_out"
fi

# One canonical attributed completion, so a ledger/cost dump has exactly one deterministic row to
# read. The headers and body are the `completion_attributed` golden's, byte for byte.
if [ -n "$ledger_out" ] || [ -n "$cost_out" ]; then
    # A policy names PLATFORM hops by the reserved model `local-llm` (inference_policy.PLATFORM_MODEL),
    # and `resolve()` matches a CONCRETE requested model against the hop's own model — so a policy
    # tenant asking for `stub-model` is refused 400 `outside_policy` by design. The policy capture
    # therefore asks for `local-llm`; the stub catalog aliases it to the stub's model.
    [ -n "$request_model" ] || { if [ -n "$policy" ]; then request_model=local-llm; else request_model=stub-model; fi; }
    token="$(tok stub-licence-token-)"
    curl -fsS -o /dev/null -X POST "$url/api/chat/completions" \
        -H 'content-type: application/json' \
        -H "authorization: Bearer $token" \
        -H 'X-Tenant-Id: 00000000-0000-4000-8000-0000000000a1' \
        -H 'X-Correlation-Id: 00000000-0000-4000-8000-0000000000e1' \
        -H 'X-Worker-Name: golden-worker' \
        -H 'X-AA-Trace-Id: 00000000-0000-4000-8000-0000000000f6' \
        -H 'X-AA-Card-Id: 00000000-0000-4000-8000-0000000000f7' \
        -H 'X-Project-Id: 00000000-0000-4000-8000-0000000000f5' \
        -H 'X-AA-Parent-Exec: 00000000-0000-4000-8000-0000000000f8' \
        -H 'X-Stage: golden' \
        -d "{\"model\":\"$request_model\",\"messages\":[{\"role\":\"user\",\"content\":\"attributed\"}],\"max_tokens\":8}"
fi

if [ -n "$ledger_out" ]; then
    # Only genuinely volatile columns are dropped: the identity, the clock, the measured duration,
    # and the per-request uuid. `request_sha256`/`response_sha256` STAY — they are deterministic
    # for a fixed request and are the parity target for the Rust fingerprint.
    psql_run -Atc "SELECT row_to_json(t) FROM (SELECT * FROM inference_call ORDER BY id DESC LIMIT 1) t" \
        | jq -S 'del(.id, .created_at, .latency_ms, .request_id)' >"$ledger_out"
    echo "ledger:  $ledger_out"
fi

if [ -n "$cost_out" ]; then
    # `timestamp` and `event_key` are per-event uuids/clock; every other field is deterministic
    # for a fixed request.
    # `redis-cli --raw XRANGE` emits the entry id, then field/value pairs one per line. Drop the
    # id and the three volatile fields; every other field is deterministic for a fixed request.
    docker exec "$redis" redis-cli --raw XRANGE billing:costs - + COUNT 1 \
        | awk 'NR==1 { next }
               NR%2==0 { field=$0; next }
               { if (field != "timestamp" && field != "event_key" && field != "latency_ms") print field "\n" $0 }' \
        >"$cost_out"
    echo "cost:    $cost_out"
fi

# The delegating completion, captured as the LAST request so `--delegated-ledger-out` reads its
# row. Headers and body are the `completion_delegated` golden's, byte for byte. Run this WITHOUT
# `--capture` when dumping artifacts (the capture's own completions would otherwise own the
# newest rows); the delegated cost dump uses XREVRANGE, so a stray earlier event cannot win.
if [ -n "$delegated_ledger_out" ] || [ -n "$delegated_cost_out" ]; then
    token="$(tok stub-licence-delegated-)"
    curl -fsS -o /dev/null -X POST "$url/api/chat/completions" \
        -H 'content-type: application/json' \
        -H "authorization: Bearer $token" \
        -H 'X-Effective-Tenant-Id: 00000000-0000-4000-8000-0000000000b2' \
        -H 'X-Correlation-Id: 00000000-0000-4000-8000-0000000000e2' \
        -H 'X-Worker-Name: delegated-worker' \
        -d '{"model":"stub-model","messages":[{"role":"user","content":"delegated"}],"max_tokens":8}'
fi

if [ -n "$delegated_ledger_out" ]; then
    psql_run -Atc "SELECT row_to_json(t) FROM (SELECT * FROM inference_call ORDER BY id DESC LIMIT 1) t" \
        | jq -S 'del(.id, .created_at, .latency_ms, .request_id)' >"$delegated_ledger_out"
    echo "ledger:  $delegated_ledger_out (delegated)"
fi

if [ -n "$delegated_cost_out" ]; then
    docker exec "$redis" redis-cli --raw XREVRANGE billing:costs + - COUNT 1 \
        | awk 'NR==1 { next }
               NR%2==0 { field=$0; next }
               { if (field != "timestamp" && field != "event_key" && field != "latency_ms") print field "\n" $0 }' \
        >"$delegated_cost_out"
    echo "cost:    $delegated_cost_out (delegated)"
fi

if [ "$keep" = 1 ]; then
    echo "kept:    docker rm -f $oracle $stub $redis $pg; docker network rm $net"
fi
