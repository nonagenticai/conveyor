#!/usr/bin/env bash
# Capture the golden set from a running inference-router.
#
#   capture-goldens.sh --live http://HOST:PORT --out DIR
#
# Run it against the PINNED OLD IMAGE (see contract/manifest.json). The output is the oracle:
# every later slice is measured against it, and `verify.sh` replays the same cases against a
# candidate. This script never talks to the Rust rewrite — a golden set regenerated from the
# implementation under test proves nothing.
#
# Determinism: responses are normalised before they are written (see normalize below). The
# normaliser only ever REMOVES information that cannot be stable across runs — a clock, a
# generated id, a duration. It never rewrites a value into the one the candidate happens to
# produce; that is what tolerances.json is for, and every tolerance carries a reason.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
out="$here/golden"

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) echo "usage: $0 --live URL [--out DIR]" >&2; exit 2 ;;
    esac
done
[ -n "$live" ] || { echo "usage: $0 --live URL [--out DIR]" >&2; exit 2; }
live="${live%/}"
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }

mkdir -p "$out"

# Deterministic ledger state. The internal reads aggregate over inference_call, so a capture
# that starts from whatever the previous run left behind is irreproducible. IR_PSQL is a psql
# invocation that reads SQL on stdin, e.g.
#   IR_PSQL="docker exec -i ir44-oracle-pg psql -U postgres -d inference_router -v ON_ERROR_STOP=1"
# Without it the capture still runs, but the internal-read goldens are not reproducible and the
# script says so.
if [ -n "${IR_PSQL:-}" ]; then
    { echo "TRUNCATE inference_call RESTART IDENTITY;"; cat "$here/stubs/seed.sql"; } | eval "$IR_PSQL" >/dev/null
    echo "ledger reset and seeded"
else
    echo "WARNING: IR_PSQL is unset — internal-read goldens are not reproducible" >&2
fi

# Exactly 43 characters of [A-Za-z0-9_-]: the only shape the router sends to introspection.
# Derived from the same prefixes the stub uses, so the two cannot drift.
tok() { printf '%s%s' "$1" "$(printf '0%.0s' $(seq 1 $((43 - ${#1}))))"; }
TOKEN="$(tok stub-licence-token-)"
BAD_TOKEN="$(tok stub-licence-inactive-)"
OVER_TOKEN="$(tok stub-licence-over-)"
NOBUDGET_TOKEN="$(tok stub-licence-nobudget-)"
# A licence held by a platform service on the fleet delegation allowlist, used by the one case
# that names a different tenant (AUTH_DELEGATING_SUBJECTS is set by oracle.sh).
DELEGATED_TOKEN="$(tok stub-licence-delegated-)"
DELEGATED_TENANT="00000000-0000-4000-8000-0000000000b2"
TENANT="00000000-0000-4000-8000-0000000000a1"
TENANT_OVER="00000000-0000-4000-8000-0000000000ba"
TENANT_NOBUDGET="00000000-0000-4000-8000-0000000000cb"
SECRET="stub-internal-secret"

# normalize: the one shared normaliser — see normalize.sh for its contract.
# shellcheck source=normalize.sh
. "$here/normalize.sh"

record() { # record <name> <status> <content-type> <body-file>
    local name="$1" status="$2" ctype="$3" bodyfile="$4" norm
    if printf '%s' "$ctype" | grep -q 'text/event-stream'; then
        norm="$(normalize_sse < "$bodyfile")"
    else
        norm="$(normalize < "$bodyfile")"
    fi
    printf '%s\n' "$norm" > "$out/$name.body"
    printf '%s\n' "$status" > "$out/$name.status"
    printf '%s\n' "$ctype" > "$out/$name.content-type"
    printf '  %-34s %s\n' "$name" "$status"
}

request() { # request <name> <method> <path> [curl args...]
    local name="$1" method="$2" path="$3"; shift 3
    local body status ctype tmp
    # The request is recorded WITH the golden (method, path, then the curl args one per line,
    # none of which contains a newline). verify.sh replays from this file, so the case list
    # exists once and a golden can never be checked against a request it was not captured from.
    # `printf '%s\n'` with NO arguments still prints one blank line, which would become an empty
    # curl argument ("blank argument where content is expected"), so the args are conditional.
    { printf '%s\n%s\n' "$method" "$path"; [ "$#" -eq 0 ] || printf '%s\n' "$@"; } > "$out/$name.request"
    tmp="$(mktemp)"
    status="$(curl -sS -o "$tmp" -D /tmp/ir-hdr.$$ -w '%{http_code}' -X "$method" "$live$path" "$@")" || status="000"
    ctype="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' /tmp/ir-hdr.$$ | tr -d '\r')"
    rm -f /tmp/ir-hdr.$$
    record "$name" "$status" "${ctype:-application/json}" "$tmp"
    rm -f "$tmp"
}

auth=(-H "authorization: Bearer $TOKEN")
json=(-H 'content-type: application/json')

echo "capturing from $live"

# --- health ------------------------------------------------------------------------------
request root            GET /
request health          GET /health
request liveness        GET /health/live
request readiness       GET /health/ready

# --- catalogue ---------------------------------------------------------------------------
request models_unauth          GET /api/models
request providers_unauth       GET /api/providers
request provider_models_unauth GET /api/providers/stub-openai/models
request whoami                 GET /api/whoami "${auth[@]}"
request models          GET /api/models "${auth[@]}"
request providers       GET /api/providers "${auth[@]}"
request provider_models GET /api/providers/stub-openai/models "${auth[@]}"

# --- completions -------------------------------------------------------------------------
request completion_nonstream POST /api/chat/completions "${json[@]}" "${auth[@]}" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"hi"}],"max_tokens":8}'

request completion_attributed POST /api/chat/completions "${json[@]}" "${auth[@]}" \
  -H "X-Tenant-Id: $TENANT" \
  -H 'X-Correlation-Id: 00000000-0000-4000-8000-0000000000e1' \
  -H 'X-Worker-Name: golden-worker' \
  -H 'X-AA-Trace-Id: 00000000-0000-4000-8000-0000000000f6' \
  -H 'X-AA-Card-Id: 00000000-0000-4000-8000-0000000000f7' \
  -H 'X-Project-Id: 00000000-0000-4000-8000-0000000000f5' \
  -H 'X-AA-Parent-Exec: 00000000-0000-4000-8000-0000000000f8' \
  -H 'X-Stage: golden' \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"attributed"}],"max_tokens":8}'

request completion_stream POST /api/chat/completions "${json[@]}" "${auth[@]}" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"stream"}],"max_tokens":8,"stream":true}'

# The tenant a request is FOR is the delegated one: the licence server vouches for the
# allow-listed subject, and `X-Effective-Tenant-Id` names another tenant. Its ledger row is the
# parity target for `delegated_by` and the effective `tenant_id`.
request completion_delegated POST /api/chat/completions "${json[@]}" \
  -H "authorization: Bearer $DELEGATED_TOKEN" \
  -H "X-Effective-Tenant-Id: $DELEGATED_TENANT" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"delegated"}],"max_tokens":8}'

request completion_bad_token POST /api/chat/completions "${json[@]}" \
  -H "authorization: Bearer $BAD_TOKEN" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"hi"}]}'

request completion_unauthenticated POST /api/chat/completions "${json[@]}" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"hi"}]}'

request completion_validation POST /api/chat/completions "${json[@]}" "${auth[@]}" \
  -d '{"model":"stub-model"}'

request completion_over_budget POST /api/chat/completions "${json[@]}" \
  -H "authorization: Bearer $OVER_TOKEN" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"hi"}],"max_tokens":8}'

request completion_no_active_budget POST /api/chat/completions "${json[@]}" \
  -H "authorization: Bearer $NOBUDGET_TOKEN" \
  -d '{"model":"stub-model","messages":[{"role":"user","content":"hi"}],"max_tokens":8}'

# --- internal reads ----------------------------------------------------------------------
internal=(-H "X-Internal-Secret: $SECRET")
# Filtered by correlation, not "the two most recent": the streamed ledger row is written by a
# background task, so its commit can land after the next request's row. A recency-ordered golden
# is therefore a race, not a contract (measured: capture and verify disagreed on which row was
# newest while every request hash was stable).
request internal_calls            GET '/api/internal/inference-calls?correlation_id=00000000-0000-4000-8000-0000000000e1' "${internal[@]}"
request internal_summary          GET /api/internal/inference-calls/summary "${internal[@]}"
request internal_pinning_summary  GET /api/internal/inference-calls/pinning-summary "${internal[@]}"
request internal_repeat_groups    GET /api/internal/inference-calls/repeat-groups "${internal[@]}"
request internal_timeline         GET /api/internal/served-models/timeline "${internal[@]}"
request internal_capture          GET '/api/internal/inference-calls/00000000-0000-4000-8000-0000000000aa/capture' "${internal[@]}"
request internal_calls_no_secret  GET '/api/internal/inference-calls?limit=1'

# --- replay / canary / admin (unauthenticated: the parity is the refusal) ------------------
request replay_list   GET '/api/replay/invocation-log?limit=1'
request canary_get    GET /api/canary/prompt/golden-worker
request admin_catalog GET /api/admin/catalog

echo "wrote $(ls -1 "$out"/*.status 2>/dev/null | wc -l | tr -d ' ') cases to $out"
