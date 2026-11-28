<!-- The reference copy of the unit issue contract. The issue form (`unit.yml`) is what the
     new-issue page offers; this file keeps the contract readable in the repository and is not a
     second chooser entry (it carries no template frontmatter on purpose, #780 P1).

     Choose exactly one state label at creation: status:ready | status:blocked | status:needs-human.
     The template deliberately does not default to ready. Contract: docs/plans/03-DELIVERY.md
     § "Issue content contract". -->

## Goal
<One sentence: the outcome that is true after this lands that is not true now. The remaining
outcome, not the plan's objective.>

## Plan / audit source
<`docs/plans/slices/S##-*.md` §… · its `00-INDEX.md` row · `docs/evidence/2026-11-10-plan-audit.md`
§W…/V…. Links only — never paste plan prose.>

## Programme
<The programme context (MSP/TOGAF). Every field names a live row or document, or the issue that
creates it (`pending #N`). A unit created on or after 2026-11-18 without this section is reported
by `scripts/ci/board.py` and is not ready to claim. See `docs/plans/03-DELIVERY.md` § "Issue
content contract".>

| Field | Value |
|---|---|
| Tranche | `programme_tranche` number the unit is authorised under (`0` for definition units), or `pending #N` |
| Brief | the project brief document id/path from P0's definition set, or `pending #122` |
| ADM phase/plateau | `Preliminary` or `A`–`H`, plus `PSA` / `HLD` / `SD` where the unit changes the architecture; otherwise `— (no architecture change)` |
| Benefits | benefit-profile id(s) in the benefits map this output is mapped to, or `enabling — no direct benefit` with one sentence of reason |

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

## User story
<Exactly one story for the unit: `As a <role>, I want <capability>, so that <value>` — all three
clauses non-blank. The parts below are its acceptance groups.>

## Parts and acceptance criteria
<One subsection per part of the plan — a Deliverable/Build-order item or an audit W/V id. One
paraphrased requirement sentence; link the source. Every part carries a Gherkin `Feature:`/`Scenario:`
block; each scenario carries a `@verify=<id>` tag and a `Verify:` line. No scenario without a
`Verify:`.>

### P1 — <part name>
`S##-*.md` §… · audit §W…
Requirement: <one sentence.>

```gherkin
Feature: <the capability this part delivers>

  @verify=P1.1
  Scenario: <the observable outcome>
    Given <state>
    When <action>
    Then <observable outcome>

  @verify=P1.2
  Scenario: <the live outcome, deployment-gated by #N>
    Given <state>
    When <action>
    Then <observable outcome>
```

Verify: `<command>` or `path::test` (exists | to create at `path`) — P1.1
Verify: `<command on host>` — P1.2 (live, deployment-gated by #N)

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
- [ ] **Independent** — <why this unit is independent: name its files, its typed dependency, if any.>
- [ ] **Negotiable** — <why this unit's outcome is fixed and what delivery still decides.>
- [ ] **Valuable** — <why this unit matters: the benefit profile or the named gap it closes.>
- [ ] **Estimable** — <why this unit can be estimated: the measured baseline and every file it touches.>
- [ ] **Small** — <why this unit is one deliverable, not a split.>
- [ ] **Testable** — <why this unit is testable: its `@verify=<id>` scenarios and the `Verify:` command for each.>
- [ ] Baseline named and the plan/audit blob shas recorded.
- [ ] Every spec/plan/code/test reference resolves at its ref; every `#N` resolves.
- [ ] Dependencies complete, typed and cited; `Blocker: #N` present iff `status:blocked`.
- [ ] One-PR scope, file-disjoint from other ready issues or explicitly sequenced.
- [ ] Scope corrections applied (no old-system shutdown/parity, no 348-flow retirement, no
      old-ledger import, no 218-item parity).

## Definition of done

The DoD is an acceptance-criteria table — **one row per scenario**, keyed by its `@verify` id,
filled at close with the command, result and SHA. A prose checklist does not satisfy it.

| Scenario | Evidence at completion | Verify: |
|---|---|---|
| `@verify=P1.1` | to fill | to fill |
| `@verify=P1.2` | to fill | to fill |

- [ ] Every scenario passes at the PR head; the table has one row per scenario.
- [ ] Repo gate green at the exact head (`preflight.yml` for the compile/lint verdict, the PR's
      `ci-public` as the final gate; docs: `git diff --check` + links).
- [ ] Tests added/updated per part; none skipped.
- [ ] Docs updated; migrations/ledger and `--check-lock` handled when touched.
- [ ] A unit that adds/removes an action **or changes an input schema** names `registry`,
      `parity_http_mcp`, `served-spec.lock.json`, `actions.json` and the design-pack index in its
      acceptance, and runs them locally before the PR.
- [ ] A unit that closes the slice's last open issue updates that slice's
      `scripts/v2-slice-artefacts.json` row to the measured shape (tables/actions/modules) and runs
      `python3 scripts/build-coverage.py --check`; a failure naming
      `a non-built project has no open unit` means the row is still curated.
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
