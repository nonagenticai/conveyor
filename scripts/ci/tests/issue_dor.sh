#!/usr/bin/env bash
# Fixture matrix for the issue content contract (#780 P2): `board.py --contract-body` over a
# conforming body (the Markdown template's h2 shape and the issue form's h3 shape) and over each
# failure shape. No network, no git, no clock: the workflow's live half is the run on the issue.
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
| Spec | \`docs/SPEC.md\` §10.1 | main@abc1234 |

${level} Baseline
- Code: \`main@abc1234\`, measured 2026-11-26

${level} Parts and acceptance criteria

### P1 — the part
- **P1.1** Given a state, when an act, then an outcome.
  Verify: \`true\`

${level} Dependencies
| Relation | Issue | Why | State |
|---|---|---|---|
| blocks | #782 | the site reads the register | open |

${level} Definition of ready
- [x] **Independent** — one unit
- [x] **Negotiable** — the outcome is fixed
- [x] **Valuable** — mapped to a benefit
- [x] **Estimable** — the baseline is named
- [x] **Small** — one issue, one PR
- [x] **Testable** — a Verify per part

${level} Definition of done

| Acceptance scenario | Evidence at completion | Verify: |
|---|---|---|
| P1.1 | to fill | \`true\` |
EOF
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

# 6. A Verify line on every part passes.
body="$tmp/two-parts-two-verify.md"
write_body "###" "$body"
awk '{print} /^  Verify: `true`$/ && !done {
  print ""; print "### P2 — the second part"
  print "- **P2.1** Given a state, when an act, then an outcome."
  print "  Verify: `true`"; done=1
} /^\| P1\.1 \| to fill \|/ {
  print "| P2.1 | to fill | `true` |"
}' "$body" >"$body.new" && mv "$body.new" "$body"
if out=$($B --contract-body "$body" 2>&1); then
  ok "a Verify line on every part passes"
else
  bad "a Verify line on every part failed: $out"
fi

# 5. An unreadable body file is instrument failure (2), never a verdict on the issue.
rc=0
$B --contract-body "$tmp/absent.md" >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  ok "an absent body file exits 2"
else
  bad "an absent body file exited $rc (want 2)"
fi

# 6. Determinism: two runs, byte-identical.
body="$tmp/body-h.md"
write_body "###" "$body"
if a=$($B --contract-body "$body" 2>&1) && b=$($B --contract-body "$body" 2>&1) && [ "$a" = "$b" ]; then
  ok "the contract check is deterministic"
else
  bad "the contract check is not deterministic"
fi

# 7. --contract without --repo refuses (the workflow always passes one).
rc=0
$B --contract 1 >/dev/null 2>&1 || rc=$?
if [ "$rc" -eq 2 ]; then
  ok "--contract without --repo exits 2"
else
  bad "--contract without --repo exited $rc (want 2)"
fi

exit "$fail"
