#!/usr/bin/env bash
# Fixture matrix for scripts/ci/board.py, plus its command-line contract.
# No network, no git, no clock: the rule matrix lives in board.py --self-test.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

B="python3 scripts/ci/board.py"
fail=0

ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1"; fail=1; }

# 1. The rule matrix. A wrong verdict here is the whole point of the file.
if out=$($B --self-test 2>&1); then
  ok "self-test exits 0"
else
  bad "self-test exited non-zero"
  printf '%s\n' "$out"
fi

case "$out" in
  *" 0 failing"*) ok "self-test reports 0 failing" ;;
  *) bad "self-test reported failures" ;;
esac

case "$out" in
  *FAIL:*) bad "self-test printed a FAIL line" ;;
  *) ok "self-test printed no FAIL line" ;;
esac

# 2. Determinism: two runs, byte-identical. The fixtures carry a fixed clock, so a difference
#    means a rule has started depending on wall time.
if a=$($B --self-test 2>&1) && b=$($B --self-test 2>&1) && [ "$a" = "$b" ]; then
  ok "self-test is deterministic"
else
  bad "self-test is not deterministic"
fi

# 3. --repo is required outside --self-test; without it the tool must refuse, not guess a repo.
rc=0
$B >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  ok "no arguments exits 2"
else
  bad "no arguments exited $rc (want 2)"
fi

# 4. --help must not require a repo.
if $B --help >/dev/null 2>&1; then
  ok "--help exits 0"
else
  bad "--help exited non-zero"
fi

exit "$fail"
