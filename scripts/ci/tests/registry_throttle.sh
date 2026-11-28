#!/usr/bin/env bash
# Self-test for scripts/ci/registry-throttle.sh — no network, no runner, no clock.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
script="$root/scripts/ci/registry-throttle.sh"
[ -f "$script" ] || { echo "registry-throttle self-test: missing $script" >&2; exit 1; }

failures=0
checks=0

expect() {
  local want="$1"
  shift
  checks=$((checks + 1))
  local got
  if ! got="$(bash "$script" "$@")"; then
    echo "FAIL: '$*' exited non-zero"
    failures=$((failures + 1))
    return
  fi
  if [ "$got" != "$want" ]; then
    echo "FAIL: '$*' -> '$got', want '$want'"
    failures=$((failures + 1))
  fi
}

# A fresh open PR throttles an event; the boundary and anything older runs.
expect skip issues 1000000 1000000
expect skip issues 1000000 999000
expect skip issues 1000000 998201 1800
expect run issues 1000000 998200 1800
expect run issues 1000000 998000
# No open generated PR: run.
expect run issues 1000000 ""
# A manual dispatch always runs.
expect run workflow_dispatch 1000000 1000000
# The daily schedule is throttled like any other non-dispatch trigger.
expect skip schedule 1000000 999000

# A five-event burst against one refreshed PR yields exactly one push: the first event finds the
# PR stale and runs; the four that follow find it younger than the window and skip.
burst_runs=0
for offset in 0 60 120 180 240; do
  if [ "$offset" -eq 0 ]; then
    updated=$((1000000 - 2000))
  else
    updated=1000000
  fi
  if [ "$(bash "$script" issues $((1000000 + offset)) "$updated")" = "run" ]; then
    burst_runs=$((burst_runs + 1))
  fi
done
checks=$((checks + 1))
if [ "$burst_runs" -ne 1 ]; then
  echo "FAIL: a five-event burst produced $burst_runs pushes, want 1"
  failures=$((failures + 1))
fi

# Invalid input is a hard error, never a silent "run".
checks=$((checks + 1))
if bash "$script" issues not-an-epoch "" >/dev/null 2>&1; then
  echo "FAIL: a non-numeric now was accepted"
  failures=$((failures + 1))
fi
checks=$((checks + 1))
if bash "$script" issues 1000000 nope >/dev/null 2>&1; then
  echo "FAIL: a non-numeric updated was accepted"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "registry-throttle self-test: $failures failure(s) of $checks"
  exit 1
fi
echo "registry-throttle self-test: $checks passed"
