#!/usr/bin/env bash
# Fixture matrix for scripts/build-walk-coverage.py, plus its command-line contract.
# No server, database, network or clock: the census is the frozen comparator.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

B="python3 scripts/build-walk-coverage.py"
fail=0

ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1"; fail=1; }

# 1. The rule matrix. The four failure modes (a claim without a registry row, an action claim
#    without the action sheet, a claimed spec that is missing, an expired exemption) live here;
#    a regression is a FAIL line and a non-zero exit.
if out=$($B --self-test 2>&1); then
  ok "self-test exits 0"
else
  bad "self-test exited non-zero"
  printf '%s\n' "$out"
fi
case "$out" in
  *"0 failing"*) ok "self-test reports 0 failing" ;;
  *) bad "self-test reported failures" ;;
esac
case "$out" in
  *FAIL:*) bad "self-test printed a FAIL line" ;;
  *) ok "self-test printed no FAIL line" ;;
esac

# 2. Determinism: two runs, byte-identical. A difference means the instrument depends on
#    wall time or iteration order.
if a=$($B --print 2>&1) && b=$($B --print 2>&1) && [ "$a" = "$b" ]; then
  ok "output is deterministic"
else
  bad "output is not deterministic"
fi

# 3. The frozen census reproduces the measured set: 56 routes, 576 attributes + 26 manual = 602.
if printf '%s' "$a" | python3 -c '
import json, sys
d = json.load(sys.stdin)
c = d["counts"]
assert c["routes"] == 56, c
assert c["actions"] == 602, c
assert c["attrs"] == 576, c
assert c["manual"] == 26, c
assert len(d["routes"]) == 56, len(d["routes"])
assert len(d["actions"]) == 602, len(d["actions"])
'; then
  ok "counts are 56 routes / 602 actions (576 attrs + 26 manual)"
else
  bad "counts are not 56 / 602 (576 + 26)"
fi

# 4. The committed record passes claims integrity. At W0 every row is absent by design;
#    --gate=P6 is the strict completeness gate and is exercised by the self-test.
if $B --check >/dev/null 2>&1; then
  ok "--check exits 0 (claims integrity)"
else
  bad "--check exited non-zero"
fi

exit "$fail"
