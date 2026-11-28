#!/usr/bin/env bash
# registry-throttle.sh — decide whether an issue-registry trigger is a no-op.
#
# usage: registry-throttle.sh <event_name> <now_epoch> <updated_epoch|empty> [window_seconds]
#
# Prints "skip" when the open generated PR was updated inside the window and the trigger is not
# a manual dispatch; otherwise "run". Kept out of the workflow so the decision is self-tested
# without a runner: scripts/ci/tests/registry_throttle.sh.
#
# A burst of label events used to force-push the one automation branch five times in 20 minutes,
# restarting the required suite each time and starving the shared runner pool (measured on PR
# #942, 2026-11-28). This gate makes the burst one push.
set -euo pipefail

event="${1:?usage: registry-throttle.sh <event> <now_epoch> <updated_epoch|empty> [window_seconds]}"
now="${2:?the current epoch is required}"
updated="${3:-}"
window="${4:-1800}"

case "$now" in ''|*[!0-9]*) echo "registry-throttle: now must be an epoch, got '$now'" >&2; exit 2 ;; esac
case "$window" in ''|*[!0-9]*) echo "registry-throttle: window must be seconds, got '$window'" >&2; exit 2 ;; esac
if [ -n "$updated" ]; then
  case "$updated" in *[!0-9]*) echo "registry-throttle: updated must be an epoch or empty, got '$updated'" >&2; exit 2 ;; esac
fi

# A manual dispatch is the operator's explicit "refresh now"; never throttle it.
if [ "$event" = "workflow_dispatch" ]; then
  echo run
  exit 0
fi
# No open generated PR: there is nothing whose checks a push would restart.
if [ -z "$updated" ]; then
  echo run
  exit 0
fi
# Strictly younger than the window is a skip; exactly the window is due.
if [ $((now - updated)) -lt "$window" ]; then
  echo skip
else
  echo run
fi
