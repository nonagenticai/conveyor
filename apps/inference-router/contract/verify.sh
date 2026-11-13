#!/usr/bin/env bash
# Replay the captured golden set against a candidate service and count unexplained differences.
#
#   verify.sh --candidate http://HOST:PORT [--golden DIR] [--tolerances FILE]
#
# The candidate is the Rust rewrite (or any build of it). A case passes when its status, its
# content-type family and its normalised body equal the capture. Anything else is UNEXPLAINED:
# the tolerance list is the only way to excuse a difference, and every tolerance carries a
# reason. A tolerance without a reason is a failure, not a warning — that is the rule that stops
# the list becoming "whatever the candidate happens to return".
#
# Exit: 0 when unexplained == 0, 1 otherwise, 2 on a usage or configuration error.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
candidate=""
golden="$here/golden"
tolerances="$here/tolerances.json"

while [ $# -gt 0 ]; do
    case "$1" in
        --candidate) candidate="${2:-}"; shift 2 ;;
        --golden) golden="${2:-}"; shift 2 ;;
        --tolerances) tolerances="${2:-}"; shift 2 ;;
        *) echo "usage: $0 --candidate URL [--golden DIR] [--tolerances FILE]" >&2; exit 2 ;;
    esac
done
[ -n "$candidate" ] || { echo "usage: $0 --candidate URL [--golden DIR] [--tolerances FILE]" >&2; exit 2; }
candidate="${candidate%/}"
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }
[ -d "$golden" ] || { echo "no golden dir: $golden" >&2; exit 2; }

# shellcheck source=normalize.sh
. "$here/normalize.sh"

# Replay mutates the ledger (every completion case writes a row), and the internal reads
# aggregate over it. Verifying therefore has to start from the SAME state the capture did, and
# walk the same order, or the reads can never match. IR_PSQL is the psql invocation that reads
# SQL on stdin, exactly as capture-goldens.sh takes it.
if [ -n "${IR_PSQL:-}" ]; then
    { echo "TRUNCATE inference_call RESTART IDENTITY;"; cat "$here/stubs/seed.sql"; } | eval "$IR_PSQL" >/dev/null
else
    echo "WARNING: IR_PSQL is unset — the internal-read cases will not match" >&2
fi

# A tolerance without a reason is a failure of the tolerance list itself.
if [ -f "$tolerances" ]; then
    bad="$(jq -r '[.tolerances[] | select((.reason // "") == "")] | length' "$tolerances")"
    [ "$bad" = "0" ] || { echo "FAIL: $bad tolerance row(s) carry no reason" >&2; exit 2; }
else
    echo "{}" >/dev/null
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

content_kind() { # collapse a content-type to its family: the boundary and charset are not contract
    printf '%s' "$1" | tr 'A-Z' 'a-z' | cut -d';' -f1 | tr -d ' '
}

total=0
unexplained=0
failed=""

for req in "$golden"/*.request; do
    [ -e "$req" ] || continue
    name="$(basename "$req" .request)"
    total=$((total + 1))

    mapfile -t lines < "$req"
    method="${lines[0]:-GET}"
    path="${lines[1]:-/}"
    # Drop empty elements: an empty curl argument is a hard error ("blank argument where
    # content is expected"), so a stray blank line must not become one.
    args=()
    for arg in "${lines[@]:2}"; do [ -n "$arg" ] && args+=("$arg"); done

    body="$(mktemp)"
    headers="$(mktemp)"
    status="$(curl -sS -o "$body" -D "$headers" -w '%{http_code}' -X "$method" "$candidate$path" "${args[@]+"${args[@]}"}")" || status="000"
    got_type="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' "$headers" | tr -d '\r')"
    want_status="$(cat "$golden/$name.status")"
    want_type="$(cat "$golden/$name.content-type")"

    if [ "$(content_kind "$want_type")" = "text/event-stream" ]; then
        # Command substitution, then printf: capture strips trailing newlines the same way, so
        # the two cannot differ by one blank line at the end of the stream.
        norm="$(normalize_sse < "$body")"
        printf '%s\n' "$norm" > "$body.norm"
    else
        normalize < "$body" > "$body.norm"
    fi
    # A tolerance removes the field from BOTH sides. Deleting it only from the candidate would
    # leave the golden's value in place and the case would still fail — which is how this
    # function was wrong the first time.
    cp "$golden/$name.body" "$body.want"
    apply_tolerances "$name" "$body.norm"
    apply_tolerances "$name" "$body.want"

    reason=""
    [ "$status" = "$want_status" ] || reason="status $status != $want_status"
    if [ -z "$reason" ] && [ "$(content_kind "$got_type")" != "$(content_kind "$want_type")" ]; then
        reason="content-type $(content_kind "$got_type") != $(content_kind "$want_type")"
    fi
    if [ -z "$reason" ] && ! diff -q "$body.norm" "$body.want" >/dev/null 2>&1; then
        reason="body differs"
        diff -u "$body.want" "$body.norm" | head -12 >&2 || true
    fi
    rm -f "$body" "$headers" "$body.norm" "$body.want"

    if [ -n "$reason" ]; then
        unexplained=$((unexplained + 1))
        failed="$failed $name"
        printf '  %-34s FAIL  %s\n' "$name" "$reason"
    else
        printf '  %-34s ok\n' "$name"
    fi
done

echo
echo "cases=$total unexplained=$unexplained"
[ -n "$failed" ] && echo "failed:$failed"
[ "$unexplained" -eq 0 ]
