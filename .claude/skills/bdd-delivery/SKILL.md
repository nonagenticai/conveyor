---
name: bdd-delivery
description: Use when this repository's delivery-item standard must be applied — one BDD user story, Gherkin acceptance with a `@verify` per scenario, the six INVEST rows with issue-specific evidence, and a Definition of Done with one row per scenario. Load it before authoring, claiming or delivering a unit issue, and whenever a card's work-package spec is written or read.
---

# bdd-delivery — the delivery-item standard

The authority is `docs/SPEC_V2.md` §5.8 (D69). The conveyor's own instance of it is
`docs/plans/03-DELIVERY.md` § "Issue content contract" and `.github/ISSUE_TEMPLATE/unit.md`: **the
unit issue is the work package**. The same standard governs a `delivery` card through its
`work_package_spec`. The global `bdd-delivery` skill defaults the standard for any development
work; this file is the canonical overlay that binds it to the conveyor's gates.

## 1. The user story — one per unit

`As a <role>, I want <capability>, so that <value>.`

- Exactly one per unit, all three clauses non-blank. A missing clause is not ready.
- The role is a live actor (a user, a job binding, a seat, an external party).
- The value is the observable benefit or the named gap the work removes — never a restatement of
  the implementation.
- The parts of the unit are not each a story; they are the scenarios that prove this one story.

## 2. The acceptance criteria are Gherkin

```gherkin
Feature: <the story's capability>

  @verify=P1.1
  Scenario: the observable outcome
    Given the state
    When the action
    Then the outcome
    And another observable fact
```

- `Feature` once, one or more `Scenario`s, `Given`/`When`/`Then` (plus `And`/`But`).
- Every scenario carries a `@verify=<id>` tag on the line above the `Scenario:`. A tag cannot
  contain a space: the id is a short space-free name (the scenario-name slug, or the test's name).
- The command stays on the issue body's `Verify:` line immediately after the block — an exact
  command or `path::test`. The tag names; the line verifies. A scenario with neither is a draft.
- `Scenario Outline` is expanded when parsed, so each example is a scenario.
- The scenario key is the `@verify` id; it is stable for the unit's life, and renaming it
  invalidates any evidence that named the old key.
- In an issue body, **each `### P` part carries its own block** — its own `Feature:` and its
  scenarios — followed by that part's `Verify:` lines, one per scenario.

```gherkin
@verify=P1.1
Scenario: a body without the story is not ready
  Given a unit body with no `## User story`
  When the claim gate reads it
  Then the claim is refused and the gap is named
```

Verify: `bash scripts/ci/tests/issue_dor.sh`

## 3. Definition of Ready — the six INVEST criteria, with evidence

A unit is pulled only when each criterion carries **the unit's own evidence**, not the template's
restatement:

| Criterion | The evidence |
|---|---|
| **Independent** | no `blocked-by`; the unit lands without another unit's files |
| **Negotiable** | the outcome is fixed; the implementation approach is settled at delivery |
| **Valuable** | the story's `so that` names a benefit or a named gap |
| **Estimable** | the baseline and every file are named |
| **Small** | one unit, one branch, one PR |
| **Testable** | every scenario carries `@verify` and a `Verify:` line |

A missing criterion is not "negotiable": it is not ready. The application refuses with
`422 work_package_dor_unmet`; the issue gate is §5.

## 4. Definition of Done — derived, one row per scenario

| Scenario | Evidence at completion | Verify: |
|---|---|---|
| `@verify=P1.1` | the command's result and the sha | the exact command |

The DoD is derived, never authored: one row per scenario key, filled with the command, its result
and the sha at the PR head. A per-part table, a prose checklist or a green CI check is not the DoD.
A waiver is a recorded approval with a reason — it is counted and never edits a scenario.

## 5. The gates in this repository

```bash
python3 scripts/ci/board.py --repo <owner/name> --contract N   # the full issue body, on edit
python3 scripts/ci/board.py --repo <owner/name> --dor N        # the claim gate
python3 scripts/ci/board.py --contract-body /tmp/body.md       # offline preview of the full contract
```

- Exit 0 is the gate; exit 1 names the missing section, INVEST criterion, story, part scenario or
  DoD row.
- What it checks: the contract sections and Programme rows; the six INVEST criteria, refusing a
  row that restates the template; one `Verify:` per part; one `## User story` in `As a …, I want
  …, so that …` form; a `Scenario:` per part with a `@verify` tag on each scenario; and a DoD row
  for every `@verify` id.
- `--apply` writes `status:not-ready` and one comment naming the gaps; it is the workflow's act,
  not a session's.
- Forward-only: an issue created before the rule (2026-11-18 for Programme, 2026-11-28 for
  INVEST/DoD and the story) is exempt and gains the shape at its next claim; #930 is exempt
  until it closes.
- The gate is necessary, not sufficient: it checks presence and shape, so whether an INVEST row
  or a `Verify:` carries real evidence — and whether every command passes at the PR head — stays
  the author's and the reviewer's check.

## 6. Where the standard runs

| Where | What |
|---|---|
| The unit issue (this repo's buildout) | one `## User story`, a `Feature:`/`Scenario:` block per `### P` part with `@verify` and a `Verify:`, the six INVEST rows, a DoD row per scenario |
| The application (D2, #935) | `work_package_spec` on the card; the feature parsed with the cucumber-rs `gherkin` crate into `{key, name, given[], when[], then[], verify}`; `cards.spec_get\|spec_put`; the run-start DoR refusal and the `card_done` scenario-evidence check |
| The browser | `media.run_gherkin` in `apps/media` — a restricted CDP executor for its documented step table, not a general parser and not a second standard (§10.5) |
| Any other stack | the project's runner (cucumber-js, pytest-bdd, …); never invent a second runner |

The refusal codes are `work_package_story_incomplete`, `work_package_scenarios_missing`,
`work_package_dor_unmet` and `work_package_evidence_unmet`.

## Completion checklist

- [ ] one user story, all three clauses non-blank;
- [ ] every `### P` part has ≥1 `Scenario:` with Given/When/Then, a `@verify=<id>` and a `Verify:` line;
- [ ] every `@verify` id is unique and maps to a DoD row and a PR evidence row;
- [ ] the six INVEST rows are present with this unit's evidence;
- [ ] the DoD table has one row per scenario key, with command → result → sha at the PR head;
- [ ] `board.py --dor` (and `--contract` after a body edit) exits 0;
- [ ] a green CI alone is not the DoD — say so rather than closing.
