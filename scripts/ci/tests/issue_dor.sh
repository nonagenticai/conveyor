#!/usr/bin/env bash
# Fixture matrix for the issue content contract (#780 P2, #934): `board.py --contract-body` over a
# conforming body (the Markdown template's h2 shape and the issue form's h3 shape) and over each
# failure shape — a missing section or `Verify:` line, a missing `## User story`, a Gherkin
# scenario without `@verify`, a DoD table without the scenario's row, an INVEST row that only
# restates the template, and the forward-only story exemptions.
# No network, no git, no clock: the workflow's live half is the run on the issue.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

B="python3 scripts/ci/board.py"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail=0

ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1"; fail=1; }

# The same contract at h2 (the Markdown template) and h3 (what a GitHub issue form renders).
write_body() {
  local level="$1" out="$2"
  cat >"$out" <<EOF
${level} Goal
One outcome that is true after this lands.

${level} Plan / audit source
\`docs/plans/03-DELIVERY.md\` § "Issue content contract".

${level} Programme

| Field | Value |
|---|---|
| Tranche | 1 |
| Brief | project-brief-p0 |
| ADM phase/plateau | — (no architecture change) |
| Benefits | enabling — no direct benefit |

${level} References
| Kind | Reference | Resolves at |
|---|---|---|
| Spec | \`docs/SPEC.md\` §5.8 | main@abc1234 |

${level} Baseline
- Code: \`main@abc1234\`, measured 2026-11-28

${level} User story
As a delivery session, I want the unit contract enforced, so that a unit is ready before it is claimed.

${level} Parts and acceptance criteria

### P1 — the part
- **P1.1** Given a state, when an act, then an outcome.
  @verify=P1.1
  Scenario: the part does its job
    Given a state
    When an act
    Then an outcome
  Verify: \`true\`

${level} Dependencies
| Relation | Issue | Why | State |
|---|---|---|---|
| blocks | #782 | the site reads the register | open |

${level} Definition of ready
- [x] **Independent** — no \`blocked-by\`; one new file
- [x] **Negotiable** — the checker's shape is fixed by #934
- [x] **Valuable** — removes gap F2 in 07-BDD-DELIVERY.md
- [x] **Estimable** — four files; baseline main@a037cd90
- [x] **Small** — four files and one PR
- [x] **Testable** — P1.1 runs board.py --self-test

${level} Definition of done

| Acceptance scenario | Evidence at completion | Verify: |
|---|---|---|
| P1.1 | to fill | \`true\` |
EOF
}

# The forward-only story rule, as the live gate applies it: created on/after STORY_SINCE and not
# in STORY_EXEMPT. Tested here because the date path needs `gh` and this matrix does not.
story_required() {
  python3 - "$1" "$2" <<'PY'
import datetime as dt
import sys
sys.path.insert(0, "scripts/ci")
import issue_contract

created = dt.date.fromisoformat(sys.argv[1])
print("yes" if issue_contract.story_required(created, int(sys.argv[2])) else "no")
PY
}

# 0. A bold `**Verify:**` line is the shape most authored bodies use; it must pass.
body="$tmp/bold-verify.md"
write_body "##" "$body" && sed -i.bak 's/^  Verify:/  **Verify:**/' "$body"
if $B --contract-body "$body" >/dev/null 2>&1; then
  ok "a bold **Verify:** line passes"
else
  bad "a bold **Verify:** line was refused"
fi

# 1. Both contract shapes pass.
for level in "##" "###"; do
  body="$tmp/body-${level//#/h}.md"
  write_body "$level" "$body"
  if out=$($B --contract-body "$body" 2>&1); then
    ok "a conforming ${level} body passes"
  else
    bad "a conforming ${level} body failed: $out"
  fi
done

# 2. A body with no `Verify:` line is refused and says so.
body="$tmp/no-verify.md"
write_body "###" "$body" && sed -i.bak 's/Verify:/verify later/' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"Verify:"* ]]; then
  ok "a missing Verify line exits 1 and names it"
else
  bad "a missing Verify line exited $rc: $out"
fi

# 3. An empty Programme value is refused and names the field.
body="$tmp/empty-programme.md"
write_body "###" "$body" && sed -i.bak 's/| Tranche | 1 |/| Tranche | — |/' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"Tranche"* ]]; then
  ok "an empty Programme row exits 1 and names the field"
else
  bad "an empty Programme row exited $rc: $out"
fi

# 4. A missing section is refused and names it.
body="$tmp/no-dod.md"
write_body "###" "$body" && sed -i.bak 's/### Definition of done/### Done/' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"Definition of done"* ]]; then
  ok "a missing section exits 1 and names it"
else
  bad "a missing section exited $rc: $out"
fi

# 5. One Verify line in a multi-part body is refused: the rule is per part (#865).
body="$tmp/two-parts-one-verify.md"
write_body "###" "$body"
awk '{print} /^  Verify: `true`$/ && !done {
  print ""; print "### P2 — the second part"
  print "- **P2.1** Given a state, when an act, then an outcome."; done=1
}' "$body" >"$body.new" && mv "$body.new" "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"every part"* ]]; then
  ok "a second part without a Verify line exits 1 and names the rule"
else
  bad "a second part without a Verify line exited $rc: $out"
fi

# 6. A Verify line, a Gherkin scenario with `@verify` and a DoD row on every part passes.
body="$tmp/two-parts-two-verify.md"
write_body "###" "$body"
awk '{print} /^  Verify: `true`$/ && !done {
  print ""; print "### P2 — the second part"
  print "- **P2.1** Given a state, when an act, then an outcome."
  print "  @verify=P2.1"
  print "  Scenario: the second part does its job"
  print "    Given a state"
  print "    When an act"
  print "    Then an outcome"
  print "  Verify: `true`"; done=1
} /^\| P1\.1 \| to fill \|/ {
  print "| P2.1 | to fill | `true` |"
}' "$body" >"$body.new" && mv "$body.new" "$body"
if out=$($B --contract-body "$body" 2>&1); then
  ok "a scenario and a DoD row for every part passes"
else
  bad "a scenario and a DoD row for every part failed: $out"
fi

# 7. A missing `## User story` is refused and named.
body="$tmp/no-story.md"
write_body "###" "$body" && sed -i.bak 's/^### User story$/### Story/' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"User story"* ]]; then
  ok "a missing user story exits 1 and names it"
else
  bad "a missing user story exited $rc: $out"
fi

# 8. A scenario without its `@verify` tag is refused and names the part.
body="$tmp/no-verify-tag.md"
write_body "###" "$body" && sed -i.bak '/^  @verify=P1.1$/d' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"Gherkin scenario with @verify"* ]]; then
  ok "a scenario without @verify exits 1 and names it"
else
  bad "a scenario without @verify exited $rc: $out"
fi

# 8b. A bare `Scenario:` with no Given/When/Then steps is refused.
body="$tmp/bare-scenario.md"
write_body "###" "$body" && sed -i.bak '/^    Given /d; /^    When /d; /^    Then /d' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"has no Given/When/Then step"* ]]; then
  ok "a scenario without Given/When/Then steps exits 1"
else
  bad "a bare scenario exited $rc: $out"
fi

# 9. A DoD table without the scenario's row is refused and names the id.
body="$tmp/no-dod-row.md"
write_body "###" "$body" && sed -i.bak '/^| P1.1 | to fill |/d' "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"no row for"*"P1.1"* ]]; then
  ok "a DoD without the scenario's row exits 1 and names the id"
else
  bad "a DoD without the scenario's row exited $rc: $out"
fi

# 10. An INVEST row that restates the template's generic phrase is refused.
body="$tmp/generic-invest.md"
write_body "###" "$body" && sed -i.bak \
  "s|no \`blocked-by\`; one new file|the unit lands without another unit's files|" "$body"
rc=0
out=$($B --contract-body "$body" 2>&1) || rc=$?
if [ "$rc" -eq 1 ] && [[ "$out" == *"template's restatement"* ]]; then
  ok "a generic INVEST phrase exits 1 and names the restatement"
else
  bad "a generic INVEST phrase exited $rc: $out"
fi

# 11. Forward-only: #930 predates the rule's merge and is exempt.
if [ "$(story_required 2026-11-28 930)" = "no" ]; then
  ok "#930 is exempt from the story rule"
else
  bad "#930 was not exempt from the story rule"
fi

# 12. Forward-only: an issue created before STORY_SINCE is exempt.
if [ "$(story_required 2026-11-20 934)" = "no" ]; then
  ok "an issue created before STORY_SINCE is exempt"
else
  bad "an issue created before STORY_SINCE was not exempt"
fi

# 13. A unit created on STORY_SINCE, outside the exempt set, is covered.
if [ "$(story_required 2026-11-28 934)" = "yes" ]; then
  ok "a unit created on STORY_SINCE needs the story"
else
  bad "a unit created on STORY_SINCE escaped the story rule"
fi

# 14. An unreadable body file is instrument failure (2), never a verdict on the issue.
rc=0
$B --contract-body "$tmp/absent.md" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  ok "an absent body file exits 2"
else
  bad "an absent body file exited $rc (want 2)"
fi

# 15. Determinism: two runs, byte-identical.
body="$tmp/body-h.md"
write_body "###" "$body"
if a=$($B --contract-body "$body" 2>&1) && b=$($B --contract-body "$body" 2>&1) && [ "$a" = "$b" ]; then
  ok "the contract check is deterministic"
else
  bad "the contract check is not deterministic"
fi

# 16. --contract without --repo refuses (the workflow always passes one).
rc=0
$B --contract 1 >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  ok "--contract without --repo exits 2"
else
  bad "--contract without --repo exited $rc (want 2)"
fi

exit "$fail"
