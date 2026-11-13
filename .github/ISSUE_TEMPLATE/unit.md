---
name: Unit of work
about: One deliverable, one branch `issue/<n>-<slug>`, exactly one PR
title: ""
labels: unit
assignees: ""
---

<!-- Choose exactly one state label at creation: status:ready | status:blocked | status:needs-human.
     The template deliberately does not default to ready. Contract: docs/plans/03-DELIVERY.md
     § "Issue content contract". -->

## Goal
<One sentence: the outcome that is true after this lands that is not true now. The remaining
outcome, not the plan's objective.>

## Plan / audit source
<`docs/plans/slices/S##-*.md` §… · its `00-INDEX.md` row · `docs/evidence/2026-11-10-plan-audit.md`
§W…/V…. Links only — never paste plan prose.>

## Claim
<Written at claim time; HEAD updated on every push. Leave the em-dashes while the issue is ready.>

| Field | Value |
|---|---|
| Branch | — |
| HEAD | — |
| Session | — |
| Updated | — |
| Adopted-from | — |

## References
| Kind | Reference | Resolves at |
|---|---|---|
| Spec | `docs/SPEC.md` §… | main@<sha> |
| Plan | `docs/plans/slices/S##-*.md` §… | main@<sha> |
| Audit | `docs/evidence/2026-11-10-plan-audit.md` §W… | #9@<sha> |
| Code | `path:symbol` | main@<sha> / #9@<sha> |
| Test | `path::name` | main@<sha> / to create |
| Issue | #N | open |

<Every path resolves at the ref in the last column when the issue is created. Cite `path:symbol`;
line numbers only when re-verified at the baseline. A path this issue must create is marked
`to create`.>

## Baseline
- Code: `main@<sha>` · `#9@<sha>` (train) · measured <YYYY-MM-DD>
- Docs: plan blob `<sha>` · audit blob `<sha>` (`git rev-parse <ref>:<path>`)

## Current state (measured)
| Part | State | Evidence |
|---|---|---|
| P1 <name> | DONE / PARTIAL / ABSENT / CARRIED-BY-#9 | `<command>` → <result>, or `path:symbol@<sha>` |

<CARRIED-BY-#9 means the specific symbol/behaviour exists at the listed #9 sha; it is re-checked on
merged main before this issue closes. Never mark a part DONE from the plan's prose.>

## Parts and acceptance criteria
<One subsection per part of the plan — a Deliverable/Build-order item or an audit W/V id. One
paraphrased requirement sentence; link the source. No scenario without a `Verify:` line.>

### P1 — <part name>
`S##-*.md` §… · audit §W…
Requirement: <one sentence.>

- **P1.1** Given <state>, when <action>, then <observable outcome>.
  Verify: `<command>` or `path::test` (exists | to create at `path`)
- **P1.2** (live, deployment-gated by #N) Given …, when …, then ….
  Verify: `<command on host>`

## Dependencies
| Relation | Issue | Why (source) | State |
|---|---|---|---|
| blocked-by | #N | <plan `depends_on` / index row / quoted precondition> | open / merged |
| acceptance-blocked-by | #N | <the live scenario waits; construction does not> | open |
| blocks | #N | <what this unblocks> | open |
| related | #N | <no ordering> | — |

<Blockers are issues, never PRs: the train is #26, deployment is #18. Only a `blocked-by` sets
`status:blocked`; an `acceptance-blocked-by` never freezes construction.>

Blocker: #N — <a plain line, present iff this issue is `status:blocked`; say why.>

## Definition of ready
- [ ] Baseline named and the plan/audit blob shas recorded.
- [ ] Every spec/plan/code/test reference resolves at its ref; every `#N` resolves.
- [ ] Every part has ≥1 scenario with a `Verify:` command or test target.
- [ ] Dependencies complete, typed and cited; `Blocker: #N` present iff `status:blocked`.
- [ ] One-PR scope, file-disjoint from other ready issues or explicitly sequenced.
- [ ] Scope corrections applied (no old-system shutdown/parity, no 348-flow retirement, no
      old-ledger import, no 218-item parity).

## Definition of done
- [ ] Every scenario passes at the PR head; `Evidence at completion` has one row per scenario.
- [ ] Repo gate green at the exact head (`rbuild --gate`; docs: `git diff --check` + links).
- [ ] Tests added/updated per part; none skipped.
- [ ] Docs updated; migrations/ledger and `--check-lock` handled when touched.
- [ ] Live verification recorded where deployment is a prerequisite.
- [ ] Every CARRIED-BY-#9 part re-checked on merged main.
- [ ] One PR `Closes #N` MERGED; issue closed; branch deleted.

## Out of scope
<What this deliberately does not do. Always restate the audit's scope corrections that apply:
old-system freeze/dump/shutdown, 348-flow retirement, old-ledger import, 218-item parity,
old-service rollback — unless the plan explicitly re-admits one.>

## Deployment prerequisite
<Yes/No; if yes, the issue it waits on (#18 is the deployment gate).>

## Open questions
| # | Question | Owner | Default |
|---|---|---|---|

## Evidence at completion
| Scenario | Command / test | Result | SHA |
|---|---|---|---|
<Filled at close. The PR body carries the same table.>
