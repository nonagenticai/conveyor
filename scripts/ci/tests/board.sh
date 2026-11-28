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

# 1b. The new contract fixtures actually ran: a matrix that silently skips a case is not a gate.
case "$out" in
  *"contract — a generic INVEST restatement"*) ok "the INVEST restatement fixture ran" ;;
  *) bad "the INVEST restatement fixture did not run" ;;
esac

case "$out" in
  *"contract — a missing user story"*) ok "the missing-story fixture ran" ;;
  *) bad "the missing-story fixture did not run" ;;
esac

case "$out" in
  *"contract — a scenario with no @verify tag"*) ok "the missing-@verify fixture ran" ;;
  *) bad "the missing-@verify fixture did not run" ;;
esac

case "$out" in
  *"contract — a DoD without the scenario's row"*) ok "the missing-DoD-row fixture ran" ;;
  *) bad "the missing-DoD-row fixture did not run" ;;
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

# 5. The pure parser lives in issue_contract.py; board.py must delegate, never keep a second
#    copy, or the proof collector and the claim gate would read a body differently.
if python3 - <<'PY'
import sys
sys.path.insert(0, "scripts/ci")
import board
import issue_contract

assert board.contract_gaps.__module__ == "issue_contract"
assert board.section_lines is issue_contract.section_lines
assert board.story_gaps is issue_contract.story_gaps
assert board.scenario_gaps is issue_contract.scenario_gaps
assert board.dod_gaps is issue_contract.dod_gaps
PY
then
  ok "board delegates the contract parser to issue_contract"
else
  bad "board does not delegate the contract parser"
fi

# 6. The story gate is forward-only: #930 predates the rule's merge, an old issue is exempt, and
#    a unit created on STORY_SINCE outside the exempt set is covered.
if python3 - <<'PY'
import datetime as dt
import sys
sys.path.insert(0, "scripts/ci")
import board

assert board.story_required(dt.date(2026, 11, 28), 930) is False
assert board.story_required(dt.date(2026, 11, 20), 934) is False
assert board.story_required(dt.date(2026, 11, 28), 934) is True
PY
then
  ok "the story gate is forward-only and exempts #930"
else
  bad "the story gate exemption is wrong"
fi

exit "$fail"
