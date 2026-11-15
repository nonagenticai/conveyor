#!/usr/bin/env bash
# Capture the small, READ-ONLY golden corpus that verify.sh replays.
#
#   capture-golden.sh --live URL --out DIR
#
# The cases are fixed in this file, never passed in. That is the read-only guarantee: the only
# requests this script can issue are (a) GET on the four public/refusal paths below and (b) the
# single side-effect-free POST /license/introspect, whose body is an unknown synthetic token.
# `assert_read_only` refuses anything else, and the capture asserts each expected status, so a
# wrong oracle (a gateway error page, a redirect, a 200 where a 401 belongs) fails the capture
# instead of becoming a "golden" nobody can explain later.
#
# ORACLE — point this at the SERVICE PROCESS, not the public ingress host. Measured 2026-11-14:
# the service's public ingress host is fronted by the gateway's istio ext-authz filter, which answers its own
# `text/plain` 401 (`Unauthorized: Bearer token required`, no WWW-Authenticate) for a gated host
# before the application sees the request. Capturing through that would record the mesh's literal
# as the service contract, and a bare candidate behind the same mesh answers identically anyway.
# From a host with cluster access, forward a loopback port to the pod and capture there:
#
#   kubectl -n licenses port-forward svc/licenses 18544:8544 &
#   capture-golden.sh --live http://127.0.0.1:18544 --out golden
#
# The application's own 401 is `application/json`
# `{"error":{"message":"Missing authentication token","status":401}}` with `WWW-Authenticate:
# Bearer` — the shape the Rust middleware reproduces.
#
# Each case writes the four files verify.sh reads:
#   <name>.request       line 1 method, line 2 path, remaining lines extra curl args (one per line)
#   <name>.status        the expected HTTP status
#   <name>.content-type  the served Content-Type header
#   <name>.body          the raw response body (verify.sh normalises both sides itself)
#
# Requires curl and jq. Exit: 0 captured; 1 a capture or assertion failed; 2 usage.
set -euo pipefail

usage() {
    sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

live=""
out=""

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --out) out="${2:-}"; shift 2 ;;
        *) usage ;;
    esac
done

[ -n "$live" ] && [ -n "$out" ] || usage
case "$live" in
    http://*|https://*) ;;
    *) echo "refusing --live that is not an http(s) URL: $live" >&2; exit 2 ;;
esac
live="${live%/}"
command -v curl >/dev/null 2>&1 || { echo "missing curl" >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo "missing jq" >&2; exit 2; }

mkdir -p "$out"

# The complete, hard-coded allowlist. Every request the script issues passes through here first.
assert_read_only() { # <method> <path>
    case "$1 $2" in
        "GET /health") return 0 ;;
        "GET /health/live") return 0 ;;
        "GET /health/ready") return 0 ;;
        "GET /licenses/active?tenant_id=00000000-0000-4000-8000-000000000000&app_key=golden-unknown-app") return 0 ;;
        "GET /licenses/user/00000000-0000-4000-8000-000000000000") return 0 ;;
        "POST /license/introspect") return 0 ;;
    esac
    echo "refusing a request outside the read-only allowlist: $1 $2" >&2
    return 1
}

captured=0

capture_case() { # <name> <expected-status> <method> <path> [curl args...]
    local name="$1" want="$2" method="$3" path="$4"
    shift 4
    local args=("$@")
    assert_read_only "$method" "$path" || exit 1

    local body headers status ctype
    body="$(mktemp)"
    headers="$(mktemp)"

    status="$(curl -sS --max-time 30 -o "$body" -D "$headers" -w '%{http_code}' \
        -X "$method" "$live$path" "${args[@]+"${args[@]}"}")" || status="000"

    if [ "$status" != "$want" ]; then
        echo "case $name: expected status $want, got $status from $live$path" >&2
        exit 1
    fi
    ctype="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' \
        "$headers" | tr -d '\r')"
    if [ -z "$ctype" ]; then
        echo "case $name: no Content-Type header" >&2
        exit 1
    fi
    if ! jq -e . "$body" >/dev/null 2>&1; then
        echo "case $name: body is not JSON — this is not the service's contract response:" >&2
        head -c 200 "$body" >&2
        echo >&2
        exit 1
    fi

    rm -f "$out/$name.request" "$out/$name.status" "$out/$name.content-type" "$out/$name.body"
    {
        printf '%s\n' "$method" "$path"
        local arg
        for arg in "${args[@]+"${args[@]}"}"; do
            [ -n "$arg" ] && printf '%s\n' "$arg"
        done
    } > "$out/$name.request"
    printf '%s\n' "$want" > "$out/$name.status"
    printf '%s\n' "$ctype" > "$out/$name.content-type"
    cp "$body" "$out/$name.body"
    rm -f "$body" "$headers"

    captured=$((captured + 1))
    printf '  %-28s %s %s\n' "$name" "$status" "$ctype"
}

# Unauthenticated health surface and refusals only. Authenticated success cases need a synthetic
# seed (tenants, licences, tokens) and are integration-test / #95 coverage; a golden set must
# never carry a production tenant, app key or token.
capture_case health 200 GET /health
capture_case health-live 200 GET /health/live
capture_case health-ready 200 GET /health/ready
capture_case introspect-unknown-token 200 POST /license/introspect \
    -H "Content-Type: application/json" \
    --data-raw '{"token":"golden-synthetic-unknown-token-0000000000000000"}'
capture_case active-unauthenticated 401 GET \
    '/licenses/active?tenant_id=00000000-0000-4000-8000-000000000000&app_key=golden-unknown-app'
capture_case user-unauthenticated 401 GET \
    '/licenses/user/00000000-0000-4000-8000-000000000000'

echo "captured $captured case(s) into $out"
