#!/usr/bin/env bash
# Capture golden request/response pairs from the old service.
#
#   capture-goldens.sh --live http://127.0.0.1:8899
#
# Every payload is synthetic: two studies, a handful of numbers, a monotone series. Nothing here
# comes from a tenant, a database or a private deployment, because this repository is public and a
# fixture is a fixture.
#
# Only deterministic operations are captured. `/health` reports a wall-clock timestamp and probes
# live downstreams, and the cache routes are stateful, so neither is a replay target — they are
# covered by unit tests instead and the reason is recorded in tests/parity.rs.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
live=""
while [ $# -gt 0 ]; do
    case "$1" in
        --live) live="${2:-}"; shift 2 ;;
        *) break ;;
    esac
done
[ -n "$live" ] || { echo "usage: $0 --live URL" >&2; exit 2; }

golden="$here/golden"
mkdir -p "$golden"

post() {
    local name="$1" path="$2" body="$3"
    printf '%s' "$body" > "$golden/$name.request.json"
    curl -fsS --max-time 60 -H 'content-type: application/json' \
        --data "$body" "$live$path" | jq -S . > "$golden/$name.response.json"
    echo "captured $name"
}

get() {
    local name="$1" path="$2"
    curl -fsS --max-time 10 "$live$path" | jq -S . > "$golden/$name.response.json"
    echo "captured $name"
}

post meta_analysis_fixed /api/stats/meta-analysis '{"studies":[{"effect_size":0.5,"standard_error":0.25,"sample_size":100},{"effect_size":1.0,"standard_error":0.25,"sample_size":100}],"method":"fixed","confidence_level":0.95}'
post meta_analysis_random /api/stats/meta-analysis '{"studies":[{"effect_size":0.2,"standard_error":0.1,"sample_size":40},{"effect_size":0.8,"standard_error":0.2,"sample_size":120},{"effect_size":0.5,"standard_error":0.15,"sample_size":60}],"method":"random","confidence_level":0.95}'
post confidence_interval_normal /api/stats/confidence-interval '{"values":[1.0,2.0,3.0,4.0,5.0],"confidence_level":0.95,"method":"normal"}'
post confidence_interval_bootstrap /api/stats/confidence-interval '{"values":[1.0,2.0,3.0,4.0,5.0],"confidence_level":0.95,"method":"bootstrap","n_bootstrap":10000}'
post trend_mann_kendall /api/stats/trend '{"data":[{"timestamp":0,"value":1},{"timestamp":1,"value":2},{"timestamp":2,"value":3},{"timestamp":3,"value":5},{"timestamp":4,"value":4},{"timestamp":5,"value":7},{"timestamp":6,"value":6},{"timestamp":7,"value":8}],"test":"mann_kendall"}'
post trend_linear_regression /api/stats/trend '{"data":[{"timestamp":0,"value":1},{"timestamp":1,"value":2},{"timestamp":2,"value":3},{"timestamp":3,"value":5},{"timestamp":4,"value":4},{"timestamp":5,"value":7},{"timestamp":6,"value":6},{"timestamp":7,"value":8}],"test":"linear_regression"}'
get status /status
get health_live /health/live
