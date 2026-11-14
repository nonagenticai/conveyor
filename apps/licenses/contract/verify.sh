#!/usr/bin/env bash
# Replay the captured golden set against a candidate service and count unexplained differences.
#
#   verify.sh --candidate http://HOST:PORT [--golden DIR] [--tolerances FILE] [--only GLOB]...
#
# The candidate is the Rust rewrite (or any build of it). A case passes when its status, its
# content-type family and its normalised body equal the capture. Anything else is UNEXPLAINED:
# tolerances.json is the only way to excuse a difference, and every tolerance carries a reason.
# A tolerance without a reason is a failure, not a warning — that is the rule that stops the list
# becoming "whatever the candidate happens to return".
#
# A golden set with zero cases is exit 3, not a pass: the corpus is populated in the parity
# close-out slice (the authenticated success cases), and an empty directory must never read as
# "no differences".
#
# Exit: 0 = unexplained=0 with at least one case; 1 = unexplained>0; 2 = usage/config; 3 = no cases.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
candidate=""
golden="$here/golden"
golden_explicit=0
tolerances="$here/tolerances.json"
only=()

usage() {
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --candidate) candidate="${2:-}"; shift 2 ;;
        --golden) golden="${2:-}"; golden_explicit=1; shift 2 ;;
        --tolerances) tolerances="${2:-}"; shift 2 ;;
        --only) only+=("${2:-}"); shift 2 ;;
        *) usage ;;
    esac
done

[ -n "$candidate" ] || usage
candidate="${candidate%/}"
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }
if [ ! -d "$golden" ]; then
    # A directory named on the command line that does not exist is a usage error; the DEFAULT
    # directory being absent means the corpus has not been captured yet, which is "no cases".
    if [ "$golden_explicit" = 1 ]; then
        echo "no golden dir: $golden" >&2
        exit 2
    fi
    echo "cases=0 in $golden (not captured yet) — a vacuous zero is not a pass" >&2
    exit 3
fi

# shellcheck source=normalize.sh
. "$here/normalize.sh"

if [ -f "$tolerances" ]; then
    bad="$(jq -r '[.tolerances[] | select((.reason // "") == "")] | length' "$tolerances")"
    [ "$bad" = "0" ] || { echo "FAIL: $bad tolerance row(s) carry no reason" >&2; exit 2; }
else
    tolerances=""
fi

case_tolerances() {
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

content_kind() {
    printf '%s' "$1" | tr 'A-Z' 'a-z' | cut -d';' -f1 | tr -d ' '
}

total=0
unexplained=0
failed=""

for req in "$golden"/*.request; do
    [ -e "$req" ] || continue
    name="$(basename "$req" .request)"
    if [ "${#only[@]}" -gt 0 ]; then
        selected=0
        for pattern in "${only[@]}"; do
            # shellcheck disable=SC2053
            if [[ "$name" == $pattern ]]; then
                selected=1
                break
            fi
        done
        [ "$selected" = 1 ] || continue
    fi
    total=$((total + 1))

    mapfile -t lines < "$req"
    method="${lines[0]:-GET}"
    path="${lines[1]:-/}"
    args=()
    for arg in "${lines[@]:2}"; do [ -n "$arg" ] && args+=("$arg"); done

    body="$(mktemp)"
    headers="$(mktemp)"
    status="$(curl -sS -o "$body" -D "$headers" -w '%{http_code}' -X "$method" "$candidate$path" "${args[@]+"${args[@]}"}")" || status="000"
    got_type="$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); print; exit}' "$headers" | tr -d '\r')"
    want_status="$(cat "$golden/$name.status")"
    want_type="$(cat "$golden/$name.content-type")"

    normalize < "$body" > "$body.norm"
    normalize < "$golden/$name.body" > "$body.want"
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

if [ "$total" -eq 0 ]; then
    echo "cases=0 in $golden — a vacuous zero is not a pass" >&2
    exit 3
fi

echo
echo "cases=$total unexplained=$unexplained"
[ -n "$failed" ] && echo "failed:$failed"
[ "$unexplained" -eq 0 ]
