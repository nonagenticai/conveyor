#!/usr/bin/env bash
# Shadow replay — replay a recorded request corpus into a candidate service and diff the answers.
#
#   shadow.sh --corpus FILE --candidate URL [--tolerances FILE] [--only GLOB] [--report FILE]
#   shadow.sh --live URL --candidate URL
#   shadow.sh --self-test --corpus FILE
#
# The corpus is JSONL, one case per line:
#   {"id","method","path","headers":{},"request_body","status","content_type","body"}
# It is recorded from the PINNED OLD SERVICE (see contract/manifest.json); a corpus recorded from
# the candidate proves nothing.
#
# ⛔ `--live` IS NOT AVAILABLE FOR THIS SERVICE. Unlike the inference-router, the Python licenses
# service has no request-body capture: it logs path, status, duration and caller, never a body.
# This mode exits 3 and says so — a vacuous zero is not a pass, and claiming a live window this
# service cannot produce would be exactly that. The probe window belongs to the deployment (#95).
#
# Report: shadow-report.json (committed). Aggregate counts and per-case verdicts keyed by
# sha256(method path body) — no bodies, tokens or tenant ids.
#
# Exit: 0 = at least one case, unexplained=0; 1 = unexplained>0; 2 = usage or transport;
#       3 = no cases (a vacuous zero is not a pass).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
corpus=""
candidate=""
live=""
tolerances="$here/tolerances.json"
report="$here/shadow-report.json"
only=""
self_test=0

usage() {
    sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --corpus) corpus="${2:-}"; shift 2 ;;
        --candidate) candidate="${2:-}"; shift 2 ;;
        --live) live="${2:-}"; shift 2 ;;
        --tolerances) tolerances="${2:-}"; shift 2 ;;
        --report) report="${2:-}"; shift 2 ;;
        --only) only="${2:-}"; shift 2 ;;
        --self-test) self_test=1; shift ;;
        *) usage ;;
    esac
done

command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }

# shellcheck source=normalize.sh
. "$here/normalize.sh"

# A tolerance without a reason is a failure of the tolerance list itself.
if [ -f "$tolerances" ]; then
    bad="$(jq -r '[.tolerances[] | select((.reason // "") == "")] | length' "$tolerances")"
    [ "$bad" = "0" ] || { echo "FAIL: $bad tolerance row(s) carry no reason" >&2; exit 2; }
else
    tolerances=""
fi

case_tolerances() { # case_tolerances <case> -> newline-separated pointers
    [ -n "$tolerances" ] || return 0
    jq -r --arg case "$1" '.tolerances[] | select(.case == $case or .case == "*") | .pointer' "$tolerances"
}

apply_tolerances() { # apply_tolerances <case> <file>
    local case_name="$1" file="$2" ptr
    while IFS= read -r ptr; do
        [ -n "$ptr" ] || continue
        jq --arg p "$ptr" 'delpaths([$p | split("/") | .[1:] | map(select(. != "") | if test("^[0-9]+$") then tonumber else . end)])' \
            "$file" > "$file.tmp" && mv "$file.tmp" "$file"
    done < <(case_tolerances "$case_name")
}

content_kind() { # the family; boundary and charset are not contract
    printf '%s' "$1" | tr 'A-Z' 'a-z' | cut -d';' -f1 | tr -d ' '
}

case_id() { # case_id <case-json> -> a hash of the request, never its body
    local method path body
    method="$(jq -r '.method' <<<"$1")"
    path="$(jq -r '.path' <<<"$1")"
    body="$(jq -r '.request_body // ""' <<<"$1")"
    printf 'sha256:%s' "$(printf '%s\n%s\n%s' "$method" "$path" "$body" | shasum -a 256 | cut -d' ' -f1)"
}

# ── self-test ────────────────────────────────────────────────────────────────────────────────────
# Proves the COMPARATOR can fail: a mutated body must be seen as different, and the unmutated
# body must be seen as equal. A harness whose both arms return "equal" is the defect this exists
# to catch. It runs no network.
if [ "$self_test" = 1 ]; then
    [ -n "$corpus" ] || { echo "self-test needs --corpus" >&2; exit 2; }
    first="$(jq -c 'select((.body // "") != "")' "$corpus" | head -1)"
    [ -n "$first" ] || { echo "self-test: no case with a JSON body in $corpus" >&2; exit 3; }
    body="$(jq -r '.body' <<<"$first")"
    mutated="$(jq -c '. + {"__shadow_self_test_mutation": true}' <<<"$body")" \
        || { echo "self-test: corpus body is not a JSON object" >&2; exit 2; }

    clean_a="$(normalize <<<"$body")"
    clean_b="$(normalize <<<"$body")"
    if [ "$clean_a" != "$clean_b" ]; then
        echo "FAIL: the normaliser is not deterministic" >&2
        exit 1
    fi
    changed="$(normalize <<<"$mutated")"
    if [ "$clean_a" = "$changed" ]; then
        echo "FAIL: the comparator cannot see a mutated field — the harness is vacuous" >&2
        exit 1
    fi
    echo "self-test ok: identical bodies compare equal; a mutated field is unexplained"
    exit 0
fi

# ── live ────────────────────────────────────────────────────────────────────────────────────────
if [ -n "$live" ]; then
    cat >&2 <<'EOF'
NOT MEASURABLE: the licenses service exposes no request-body capture.

It logs path, status, duration and calling client — never a body — and has no `X-Capture`
surface (measured 2026-11-14 against the live service and its source). An organic live window
therefore cannot be recorded, and this harness will not pretend a zero-case run is a pass.

The probe window belongs to the deployment: #95 P3 records it from the candidate's own traffic.
EOF
    exit 3
fi

[ -n "$corpus" ] || usage
[ -n "$candidate" ] || usage
[ -f "$corpus" ] || { echo "no corpus: $corpus" >&2; exit 2; }
candidate="${candidate%/}"
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }

total=0
unexplained=0
equal=0
verdicts="$(mktemp)"

while IFS= read -r case_json; do
    [ -n "$case_json" ] || continue
    id="$(case_id "$case_json")"
    label="$(jq -r '.id // ""' <<<"$case_json")"
    [ -n "$only" ] && [[ "$label" != $only ]] && continue

    method="$(jq -r '.method' <<<"$case_json")"
    path="$(jq -r '.path' <<<"$case_json")"
    want_status="$(jq -r '.status' <<<"$case_json")"
    want_type="$(jq -r '.content_type // "application/json"' <<<"$case_json")"
    want_body="$(jq -r '.body // ""' <<<"$case_json")"

    args=(-X "$method")
    while IFS=$'\t' read -r key value; do
        [ -n "$key" ] && args+=(-H "$key: $value")
    done < <(jq -r '.headers // {} | to_entries[] | "\(.key)\t\(.value)"' <<<"$case_json")
    request_body="$(jq -r '.request_body // ""' <<<"$case_json")"
    if [ -n "$request_body" ]; then
        args+=(-H 'content-type: application/json' --data "$request_body")
    fi

    body_file="$(mktemp)"
    header_file="$(mktemp)"
    status="$(curl -sS -o "$body_file" -D "$header_file" -w '%{http_code}' "${args[@]}" "$candidate$path")" \
        || { status="000"; }
    got_type="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' "$header_file" | tr -d '\r')"
    rm -f "$header_file"

    normalize < "$body_file" > "$body_file.norm"
    printf '%s\n' "$want_body" | normalize > "$body_file.want"
    apply_tolerances "$label" "$body_file.norm"
    apply_tolerances "$label" "$body_file.want"

    reason=""
    [ "$status" = "$want_status" ] || reason="status $status != $want_status"
    if [ -z "$reason" ] && [ "$(content_kind "${got_type:-application/json}")" != "$(content_kind "$want_type")" ]; then
        reason="content-type ${got_type:-none} != $want_type"
    fi
    if [ -z "$reason" ] && ! diff -q "$body_file.norm" "$body_file.want" >/dev/null 2>&1; then
        reason="body differs"
    fi
    rm -f "$body_file" "$body_file.norm" "$body_file.want"

    total=$((total + 1))
    if [ -n "$reason" ]; then
        unexplained=$((unexplained + 1))
        jq -cn --arg id "$id" --arg reason "$reason" '{id: $id, verdict: "unexplained", reason: $reason}' >> "$verdicts"
        printf '  %-40s UNEXPLAINED  %s\n' "${label:-$id}" "$reason"
    else
        equal=$((equal + 1))
        jq -cn --arg id "$id" '{id: $id, verdict: "equal"}' >> "$verdicts"
        printf '  %-40s equal\n' "${label:-$id}"
    fi
done < "$corpus"

if [ "$total" -eq 0 ]; then
    rm -f "$verdicts"
    echo "cases=0 — a vacuous zero is not a pass" >&2
    exit 3
fi

jq -s --argjson cases "$total" --argjson equal "$equal" --argjson unexplained "$unexplained" \
    '{cases: $cases, equal: $equal, tolerated: 0, unexplained: $unexplained, verdicts: .}' \
    "$verdicts" > "$report"
rm -f "$verdicts"

echo
echo "cases=$total equal=$equal unexplained=$unexplained (report: $report)"
[ "$unexplained" -eq 0 ]
