#!/usr/bin/env bash
# Shadow replay — record a request window once, replay it into the pinned old image and the
# Rust candidate behind the SAME stub provider, and diff the two answers.
#
#   shadow.sh --live URL --candidate URL [--since ISO] [--until ISO] [--limit N]
#   shadow.sh --corpus FILE --candidate URL [--oracle URL]
#   shadow.sh --from-golden DIR --candidate URL [--oracle URL]
#   shadow.sh --self-test --corpus FILE --candidate URL
#
# `--live` fetches the window from a running inference-router. That path needs three things the
# S9b measurement proved are all required: the deployment's INFERENCE_CAPTURE_ALLOWED_CALLERS
# must name our X-App-Id, our requests must carry `X-Capture: full`, and the caller must send
# X-App-Id. Bodies of STREAMED requests are never captured, so a live window yields non-stream
# cases only. The corpus itself is written to a temporary directory and never committed: it can
# contain live prompts.
#
# The replay target is NOT the live service. Both sides are rebuilt behind the deterministic
# stub (`oracle.sh`), so the diff measures the implementations, not provider sampling.
#
# Report: shadow-report.json (committed). It carries aggregate counts and per-case verdicts,
# each case identified by sha256(request) — no body, prompt, tenant or call id.
#
# Exit: 0 = replayed at least one case with unexplained=0; 1 = unexplained>0; 2 = usage or a
# transport failure; 3 = no cases (a vacuous zero is not a pass).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

live=""
corpus=""
from_golden=""
candidate=""
oracle=""
since=""
until=""
limit=100
report="$here/shadow-report.json"
tolerances="$here/tolerances.json"
secret="${INFERENCE_ROUTER_INTERNAL_SECRET:-}"
app_id="${CAPTURE_APP_ID:-}"
prefix="${SHADOW_PREFIX:-ir-shadow}"
keep=0
self_test=0

usage() {
    sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        --corpus) corpus="${2:-}"; shift 2 ;;
        --from-golden) from_golden="${2:-}"; shift 2 ;;
        --candidate) candidate="${2:-}"; shift 2 ;;
        --oracle) oracle="${2:-}"; shift 2 ;;
        --since) since="${2:-}"; shift 2 ;;
        --until) until="${2:-}"; shift 2 ;;
        --limit) limit="${2:-}"; shift 2 ;;
        --report) report="${2:-}"; shift 2 ;;
        --tolerances) tolerances="${2:-}"; shift 2 ;;
        --prefix) prefix="${2:-}"; shift 2 ;;
        --keep) keep=1; shift ;;
        --self-test) self_test=1; shift ;;
        -h|--help) usage ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done

[ -n "$candidate" ] || { echo "--candidate is required" >&2; exit 2; }
candidate="${candidate%/}"
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 2; }
command -v shasum >/dev/null || { echo "missing shasum" >&2; exit 2; }

# --- deterministic stub credentials -------------------------------------------------------
# The same 43-character shape the live router sends to introspection; the stub understands it.
tok() { printf '%s%s' "$1" "$(printf '0%.0s' $(seq 1 $((43 - ${#1}))))"; }
STUB_TOKEN="$(tok stub-licence-token-)"
STUB_MODELS="stub-model alias-model local-llm"

work="$(mktemp -d "${TMPDIR:-/tmp}/shadow.XXXXXX")"
cleanup() {
    if [ "$keep" != 1 ] && [ -n "$booted_oracle" ]; then
        bash "$here/oracle.sh" --prefix "$prefix" --down >/dev/null 2>&1 || true
    fi
    rm -rf "$work"
}
booted_oracle=""
trap cleanup EXIT

# --- corpus sources ------------------------------------------------------------------------

# from_golden DIR: build a corpus from golden completion requests (the recorded old-service
# answers are not needed; the replay target produces its own).
build_from_golden() {
    local dir="$1" out="$2" found=0 f method path body
    for f in "$dir"/*.request; do
        [ -f "$f" ] || continue
        method="$(sed -n '1p' "$f")"
        path="$(sed -n '2p' "$f")"
        [ "$method" = "POST" ] || continue
        [ "$path" = "/api/chat/completions" ] || continue
        body="$(awk 'f{print; exit} /^-d$/{f=1}' "$f")"
        [ -n "$body" ] || continue
        jq -cn --arg name "$(basename "$f" .request)" --argjson request "$body" \
            '{name:$name, request:$request}' >> "$out"
        found=$((found + 1))
    done
    echo "$found"
}

# fetch_live URL OUT: list the window and pull every captured body.
fetch_live() {
    local base="$1" out="$2" page ids id row fetched=0 skipped=0
    [ -n "$secret" ] || { echo "--live needs INFERENCE_ROUTER_INTERNAL_SECRET" >&2; exit 2; }
    [ -n "$app_id" ] || { echo "--live needs CAPTURE_APP_ID (an allowlisted X-App-Id)" >&2; exit 2; }
    local query=(--get --data-urlencode "limit=$limit")
    [ -n "$since" ] && query+=(--data-urlencode "since=$since")
    [ -n "$until" ] && query+=(--data-urlencode "until=$until")
    page="$(curl -sS --max-time 30 -H "X-Internal-Secret: $secret" \
        "${query[@]}" "$base/api/internal/inference-calls")" || {
        echo "fetch: list request failed" >&2; exit 2; }
    ids="$(printf '%s' "$page" | jq -r '.rows[]? | select(.captured == true) | .id')"
    for id in $ids; do
        row="$(curl -sS --max-time 30 -H "X-Internal-Secret: $secret" -H "X-App-Id: $app_id" \
            "$base/api/internal/inference-calls/$id/capture")" || {
            echo "fetch: capture $id failed" >&2; exit 2; }
        case "$(printf '%s' "$row" | jq -r 'if .detail then "detail" else "body" end')" in
            detail)
                skipped=$((skipped + 1))
                continue
                ;;
        esac
        if printf '%s' "$row" | jq -e '.request | type == "object"' >/dev/null 2>&1; then
            printf '%s' "$row" | jq -c '{request:.request, response:.response,
                request_sha256_verified:(.request_sha256_verified // null)}' >> "$out"
            fetched=$((fetched + 1))
        else
            skipped=$((skipped + 1))
        fi
    done
    echo "fetch: captured=$fetched skipped=$skipped" >&2
    echo "$fetched"
}

mkdir -p "$work"
if [ -n "$live" ]; then
    corpus="$work/corpus.jsonl"
    fetched="$(fetch_live "${live%/}" "$corpus")"
    [ "$fetched" -gt 0 ] || {
        echo "shadow: no captured bodies in the window; the live service stores a body only" >&2
        echo "        when the caller sends X-Capture: full AND its X-App-Id is allowlisted," >&2
        echo "        and streamed rows never capture. Arm both, then re-record." >&2
        exit 3
    }
elif [ -n "$from_golden" ]; then
    corpus="$work/corpus.jsonl"
    found="$(build_from_golden "$from_golden" "$corpus")"
    [ "$found" -gt 0 ] || { echo "shadow: no completion cases under $from_golden" >&2; exit 3; }
elif [ -n "$corpus" ]; then
    [ -f "$corpus" ] || { echo "shadow: no such corpus: $corpus" >&2; exit 2; }
else
    echo "shadow: one of --live, --from-golden or --corpus is required" >&2
    exit 2
fi

cases="$(wc -l < "$corpus" | tr -d ' ')"
[ "$cases" -gt 0 ] || { echo "shadow: corpus is empty" >&2; exit 3; }
corpus_sha="$(shasum -a 256 "$corpus" | awk '{print $1}')"

# --- replay ---------------------------------------------------------------------------------

# boot_oracle: start the pinned image behind the stub and print its URL. The caller may pass
# --oracle to reuse a stack it already owns.
if [ -z "$oracle" ]; then
    command -v docker >/dev/null || { echo "missing docker (run with a login shell)" >&2; exit 2; }
    out="$(bash "$here/oracle.sh" --prefix "$prefix" --keep 2>&1)" || {
        printf '%s\n' "$out" >&2; exit 2; }
    oracle="$(printf '%s\n' "$out" | awk '/^oracle: /{print $2; exit}')"
    [ -n "$oracle" ] || { printf '%s\n' "$out" >&2; echo "shadow: could not read the oracle URL" >&2; exit 2; }
    booted_oracle=1
fi
oracle="${oracle%/}"

# completion URL: the recorded requests are completions; the path is the captured contract's.
completion="$oracle/api/chat/completions"
candidate_completion="$candidate/api/chat/completions"

# rewrite the requested model onto a stub alias; the live catalog's names do not exist here.
rewrite_model() {
    local body="$1" model
    model="$(printf '%s' "$body" | jq -r '.model // empty')"
    case " $STUB_MODELS " in
        *" $model "*) printf '%s' "$body" ;;
        *) printf '%s' "$body" | jq -c '.model = "stub-model"' ;;
    esac
}

call() { # call <url> <body> <outfile> <statusfile> <ctypefile>
    local url="$1" body="$2" out="$3" status="$4" ctype="$5"
    curl -sS --max-time 180 -D "$work/headers" -o "$out" -w '%{http_code}' \
        -H 'content-type: application/json' -H "authorization: Bearer $STUB_TOKEN" \
        -d "$body" "$url" > "$status" || { echo "shadow: replay failed: $url" >&2; exit 2; }
    tr -d '\r' < "$work/headers" | awk 'tolower($1)=="content-type:"{print $2; exit}' > "$ctype"
}

# shellcheck source=normalize.sh
. "$here/normalize.sh"

# body_differing <json-a> <json-b> <tolerance-pointers-json>: print the differing pointers,
# one per line. Values are never printed — prompts stay out of the report.
body_differing() {
    local a="$1" b="$2" drop="$3"
    jq -n --argjson a "$a" --argjson b "$b" --argjson drop "$drop" '
      def dropall($o; $ps):
        $o | delpaths([$ps[] | split("/") | .[1:] | map(if test("^[0-9]+$") then tonumber else . end)]);
      def diff($x; $y; $p):
        if $x == $y then []
        elif ($x|type) != ($y|type) then [$p]
        elif ($x|type) == "object" then
          ((($x|keys) + ($y|keys)) | unique) as $ks
          | [$ks[] as $k | diff($x[$k]; $y[$k]; ($p + "/" + $k))] | add // []
        elif ($x|type) == "array" then
          [range(0; ([($x|length), ($y|length)] | max)) as $i
            | diff($x[$i]; $y[$i]; ($p + "/" + ($i|tostring)))] | add // []
        else [$p] end;
      diff(dropall($a; $drop); dropall($b; $drop); "")
    ' | jq -r '.[]?' 2>/dev/null || true
}

tolerances_for() { # tolerances_for <case-id>: array of pointers
    [ -f "$tolerances" ] || { printf '[]'; return; }
    jq -c --arg case "$1" '[.tolerances[]? | select(.case == $case) | .pointer]' "$tolerances" 2>/dev/null || printf '[]'
}

rows="$work/rows.jsonl"
: > "$rows"
equal=0; tolerated=0; unexplained=0; rewrites=0; index=0

while IFS= read -r line; do
    index=$((index + 1))
    request="$(printf '%s' "$line" | jq -c '.request')"
    rewritten="$(rewrite_model "$request")"
    [ "$rewritten" = "$request" ] || rewrites=$((rewrites + 1))
    case_id="$(printf '%s' "$rewritten" | shasum -a 256 | awk '{print $1}')"

    call "$completion" "$rewritten" "$work/o.body" "$work/o.status" "$work/o.ctype"
    call "$candidate_completion" "$rewritten" "$work/c.body" "$work/c.status" "$work/c.ctype"

    verdict="equal"; pointers=""
    if [ "$(cat "$work/o.status")" != "$(cat "$work/c.status")" ]; then
        verdict="unexplained"; pointers="/status"
    else
        octype="$(cut -d';' -f1 < "$work/o.ctype")"; cctype="$(cut -d';' -f1 < "$work/c.ctype")"
        if [ "$octype" != "$cctype" ]; then verdict="unexplained"; pointers="/content-type"; fi
        if printf '%s' "$octype" | grep -q 'text/event-stream'; then
            norm_o="$(normalize_sse < "$work/o.body")"
            norm_c="$(normalize_sse < "$work/c.body")"
            if [ "$norm_o" != "$norm_c" ]; then verdict="unexplained"; pointers="$pointers/sse"; fi
        elif jq -e . "$work/o.body" >/dev/null 2>&1 && jq -e . "$work/c.body" >/dev/null 2>&1; then
            drop="$(tolerances_for "$case_id")"
            diffs="$(body_differing "$(normalize < "$work/o.body")" "$(normalize < "$work/c.body")" "${drop:-[]}")"
            if [ -n "$diffs" ]; then
                if [ "$(printf '%s' "${drop:-[]}" | jq 'length')" -gt 0 ]; then
                    verdict="tolerated"
                else
                    verdict="unexplained"
                fi
                pointers="$(printf '%s\n' "$diffs" | paste -sd, -)"
            fi
        elif ! cmp -s "$work/o.body" "$work/c.body"; then
            verdict="unexplained"; pointers="/body"
        fi
    fi

    case "$verdict" in
        equal) equal=$((equal + 1)) ;;
        tolerated) tolerated=$((tolerated + 1)) ;;
        *) unexplained=$((unexplained + 1)) ;;
    esac
    jq -cn --arg case "$case_id" --arg verdict "$verdict" --arg pointers "$pointers" \
        '{case:$case, verdict:$verdict, differing:($pointers | split(",") | map(select(length>0)))}' >> "$rows"
done < "$corpus"

# --self-test: the comparator must catch a one-field mutation. Re-diff case 1 with a synthetic
# field added to the candidate's body and require it to be flagged.
if [ "$self_test" = 1 ]; then
    first="$(head -1 "$corpus")"
    request="$(printf '%s' "$first" | jq -c '.request')"
    rewritten="$(rewrite_model "$request")"
    call "$completion" "$rewritten" "$work/so.body" "$work/so.status" "$work/so.ctype"
    call "$candidate_completion" "$rewritten" "$work/sc.body" "$work/sc.status" "$work/sc.ctype"
    if jq -e . "$work/sc.body" >/dev/null 2>&1; then
        mutated="$(jq -c '. + {"__shadow_selftest__": true}' "$work/sc.body")"
        diffs="$(body_differing "$(normalize < "$work/so.body")" "$(printf '%s' "$mutated" | normalize)" '[]')"
        if [ -n "$diffs" ]; then
            echo "self-test: OK — a one-field mutation was flagged ($diffs)"
        else
            echo "self-test: FAILED — the mutation was not flagged" >&2
            exit 1
        fi
    else
        echo "self-test: SKIPPED — the first case is not JSON" >&2
    fi
fi

# --- report ---------------------------------------------------------------------------------

jq -n \
    --arg source "$([ -n "$live" ] && echo live || echo corpus)" \
    --arg since "$since" --arg until "$until" \
    --arg corpus_sha256 "$corpus_sha" \
    --argjson cases "$cases" --argjson equal "$equal" --argjson tolerated "$tolerated" \
    --argjson unexplained "$unexplained" --argjson rewrites "$rewrites" \
    --slurpfile rows "$rows" \
    '{source:{kind:$source, window:{since:(if $since=="" then null else $since end),
        until:(if $until=="" then null else $until end)}, corpus_sha256:$corpus_sha256},
      model_rewrites:$rewrites, cases:$cases, equal:$equal, tolerated:$tolerated,
      unexplained:$unexplained, rows:$rows}' > "$report"

echo "shadow: cases=$cases equal=$equal tolerated=$tolerated unexplained=$unexplained"
echo "shadow: report $report"
[ "$unexplained" -eq 0 ] || exit 1
