#!/usr/bin/env bash
# Capture the golden request/response set from a live lean4-prover.
#
#   capture-goldens.sh --live http://HOST:PORT [--out DIR]
#
# Every payload is synthetic: no hostname, no token, no tenant identifier. What is captured is what
# an UNLICENSED caller sees — the health probes and the compute routes' refusal — because a
# licensed 200 cannot be captured without minting a licence, and fabricating one would be a
# hand-written fixture wearing a capture's name. `apps/lean4-prover/tests/parity.rs` names the
# licensed paths in `DECLARED_UNCAPTURABLE` with that reason, and the 200 shapes are proven by the
# crate's own `tests/http_contract.rs` against a stub runner.
#
# `/metrics` is captured as a shape (its counters move), so the body is stored for reference and the
# equality assertion covers status and content type only.
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

mkdir -p "$out"

# name | METHOD | path | body
cases=(
    "health_live|GET|/health/live|"
    "health_ready|GET|/health/ready|"
    "health|GET|/health|"
    "metrics|GET|/metrics|"
    "check_unauthorised|POST|/api/check|{\"source\":\"theorem t : True := by trivial\"}"
    "elaborate_unauthorised|POST|/api/elaborate|{\"term\":\"1 + 1\"}"
    "prove_unauthorised|POST|/api/prove|{\"theorem\":\"theorem t : (1:Nat) + 1 = 2 := by norm_num\"}"
)

for case in "${cases[@]}"; do
    IFS='|' read -r name method path body <<<"$case"
    printf '%s %s' "$method" "$path" > "$out/$name.request"
    [ -n "$body" ] && printf '\n%s' "$body" >> "$out/$name.request"
    if [ -n "$body" ]; then
        curl -sS --max-time 60 -X "$method" -H 'content-type: application/json' \
            --data "$body" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    else
        curl -sS --max-time 60 -X "$method" -o "$out/$name.body" -D "$out/$name.headers" "$live$path"
    fi
    awk 'NR==1{print $2}' "$out/$name.headers" > "$out/$name.status"
    awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[ ]*/,""); sub(/\r$/,""); print; exit}' \
        "$out/$name.headers" > "$out/$name.content-type"
    rm -f "$out/$name.headers"
    echo "captured $name ($(cat "$out/$name.status"))"
done
