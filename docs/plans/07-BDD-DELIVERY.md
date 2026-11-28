# BDD delivery at the work package — the standard, the app, the public record and the skills

**Status:** D0 of #933. This plan is the audit and the queue; D1–D4 are created by #933.
**Programme project:** `SPEC-V2` (the V2 operating model). This is a cross-cutting plan, not a §12
slice.

## Why

The owner directed on 2026-11-28 that BDD user stories, Gherkin acceptance criteria, an INVEST
Definition of Ready and an acceptance-criteria-based Definition of Done are the standard at the
work package and lower levels, in the conveyor's own development process, in the conveyor
application, in the skills every Claude session loads, and on the public progress record.

PR #923 (issue #920) and PR #929 (issue #928) delivered the first half: the v4 standards
catalogue, the INVEST/DoD issue contract and the evidence ratchet. This plan covers what that lane
did not do or did incorrectly. It does not redo the catalogue, the ratchet or the 222-document
backfill.

## What landed

- `seed/output-standard.json` v4 / 79 categories; the four Delivery categories
  (`user-stories`, `acceptance-criteria`, `definition-of-ready`, `definition-of-done`) are
  `mandatory: true`, with real templates. The `definition-of-done` skeleton already says **one row
  per scenario in the issue**.
- `scripts/generate-standards.py` resolves the class set; scope through tranche 8; honest
  absent/partial; the register carries scope. `docs/design/data/standards.*` and the pages were
  regenerated.
- `docs/plans/03-DELIVERY.md`, `.github/ISSUE_TEMPLATE/unit.md|yml` and `scripts/ci/board.py`
  require the six INVEST names and a DoD table. `board.py` is the claim gate.
- #929 added the required/optional scorecard (`check-evidence-ratchet.py`), the required
  `checks` job and the registry trigger change.

## Findings — what is missing or incorrect

**F1 — the spec is untouched.** `docs/SPEC_V2.md` at `1ff19ecb` has zero occurrences of INVEST,
Gherkin, user story or Definition of Ready. The four templates cite `spec: §8.5` (the requirements
trace chain), not a delivery-item standard. There is no D69.

**F2 — the unit contract is not BDD/Gherkin, and its DoD is per part.**
- No user-story shape. `03-DELIVERY.md` says "Every part is a user story", but the template's
  `### P` block carries a `Requirement:` sentence, not `As a …, I want …, so that …`; `board.py`
  does not check it.
- No Gherkin. The contract says "Given…, when…, then…" in prose; there is no `Feature:` /
  `Scenario:` block, no tag, and the checker only looks for one `Verify:` per part.
- The DoD is one row per **part**, not per **scenario** (`board.py::dod_gaps` compares rows with
  the part count; `03-DELIVERY.md`; PR #923 body). The project-level `definition-of-done` template
  says one row per scenario, so the two levels contradict each other.
- INVEST is names-only (`invest_gaps`), so the template's generic explanations pass as evidence.

**F3 — the application does not use the standard.** The four categories are project documents. A
work-package card has no story, Gherkin scenarios, INVEST readiness or derived DoD; there is no
`work_package_spec`, no `cards.spec_get|spec_put`, no DoR gate at run start, and `card_done`
evidence is not matched to scenario keys.

**F4 — the public progress record has no per-unit readiness.** #929's scorecard is per project /
standard category. The linked-issues view and `issues.json` carry no story/scenario/INVEST/DoD
summary, so the owner's progress page cannot show whether a unit uses the standard.

**F5 — the skills do not mention the standard.** `issue-create/SKILL.md` still says only
`Given/When/Then` + `Verify:`; no repo skill mentions INVEST, a user story or Gherkin; there is no
global skill. A session can pass every current gate without using the standard by name.

**F6 — the unit certificate is unchanged.** `docs/spec/lean/DeliveryModel.lean` `UnitObs` carries
only `verifyParts`. This is a follow-on, not a prerequisite.

**F7 — the four templates cite §8.5.** They should cite §5.8 once it exists. Correcting them edits
`seed/output-standard.json` and regenerates `standards.*`, which the campaign lane (#930) is
publishing. It is handed to that lane's next regeneration; this plan never writes the record.

## The standard

`docs/SPEC_V2.md` §5.8 (D69) is the normative text. In short:

1. **Scope** — a work package is a card of the `delivery` card type, or a card linked from a
   `wbs_node(kind='work_package')`; meeting/coach/system cards are outside it.
2. **One spec** — `work_package_spec` on the card; the WBS node resolves through `card_id`; runs
   and steps reference scenario keys.
3. **Story** — one per work package, `As a <role>, I want <capability>, so that <value>`.
4. **Gherkin** — `Feature`/`Scenario`/`Given`/`When`/`Then`, a `@verify=<id>` tag per scenario (a
   tag has no spaces; the id names the test or the issue's `Verify:` line), `Scenario Outline`
   expanded at parse time; the authored text is stored, the parsed form is derived.
5. **INVEST DoR** — six criteria, each with the work package's own evidence; a run start refuses
   an absent or not-ready spec.
6. **Acceptance DoD** — derived, one row per scenario; the `card_done` gate's evidence must name
   every scenario key and a run/step of the card; a waiver is recorded, never an edit.
7. **Gate order** — DoR at run start → work → DoD at done; domain validator, typed 422, DB CHECK.
8. **Issue contract** — the buildout's instance: one `## User story`, Gherkin per part with a
   `Verify:`, six INVEST rows with evidence, one DoD row per scenario.
9. **Public readiness** — fixed-vocabulary booleans/counts per unit; never a body.
10. **Non-goals** — no second workflow engine, no prose DoD, no in-conveyor scenario runner, no
    second store.

## Units

### D1 — Correct the unit contract and put it in the skills (#934)
Owns `docs/plans/03-DELIVERY.md`, `.github/ISSUE_TEMPLATE/unit.md|yml`, `scripts/ci/board.py`,
the new `scripts/ci/issue_contract.py`, the repo skills (`issue-create`, `issue-pickup`,
`issue-deliver`, `delivery-unit`, new `bdd-delivery`) and the global
`~/.claude/skills/bdd-delivery/SKILL.md` plus the rule in `~/.claude/CLAUDE.md` (machine-level,
recorded as evidence, not in the PR diff).

Corrects F2:
- Add `## User story` to the template and `CONTRACT_SECTIONS`; one per unit, three clauses.
- Each `### P` part carries at least one Gherkin block with `Scenario:` and `@verify=<id>`; the
  parser counts scenarios and tags, not just parts.
- The DoD table has one row per scenario, keyed by scenario name; `dod_gaps` compares the
  scenario keys, not the part count.
- Each INVEST row names the issue's own evidence; `board.py` refuses the template's generic
  skeleton phrase.
- Forward-only: a `STORY_SINCE` date; existing units gain the shape at their next claim, and
  #930 (created before the rule) is exempt.
- The skills state the same standard; the repo `bdd-delivery` skill is the canonical overlay, and
  the global skill is the default for any development work.

Blocked-by #933. Verify: `python3 scripts/ci/board.py --self-test`; fixture bodies missing the
story, a Gherkin scenario with `@verify`, a second DoD row or an INVEST evidence line each exit 1;
a complete body exits 0; `bash scripts/ci/tests/issue_dor.sh` and `board.sh`.

### D2 — The app work-package spec (#935)
Migration `0111_work_package_spec.sql`; `crates/store/src/work_package_spec.rs`;
`crates/domain/src/work_package_spec.rs` parsing the feature with the cucumber-rs `gherkin`
crate; `cards.spec_get|spec_put`; the run-start DoR refusal in `control/runs.rs`; the scenario
evidence check in `assert_card_done_tx`; views; the served-spec lock and action data. Blocked-by
#933.

Verify: preflight; domain/action tests; a run start with an absent or not-ready spec is refused;
a card cannot enter `done` with a scenario missing from the `card_done` evidence.

### D3 — The app work-package screens (#936)
The `/card/:card_id` cockpit, the board card detail and the plan's work_package node show and edit
the story, Gherkin feature, six INVEST rows and derived DoD through `cards.spec_put`; a Playwright
spec asserts empty, error, happy and one refusal. Blocked-by D2.

### D4 — The public per-unit readiness (#937)
`scripts/fetch-issues.sh` derives a public-safe `readiness` object per unit (story, parts,
scenarios, verified, the six DoR booleans, DoD rows/evidenced) from the body through the D1
parser, never the body; `docs/design/v2.js` renders it in the project linked-issues table and the
unit detail; `check-data.cjs` asserts the exact key set and still no body. It does **not** edit
`docs/design/delivery.js`. Blocked-by D1.

## Not this lane

- The 222-document backfill (#930 and the campaign batches); no record writes here.
- The evidence ratchet and scorecard (#929); their U3 shares `docs/design/delivery.js` and
  `scripts/ci/board.py` and is not started. D1 must sequence behind their U3 if it lands first.
- The four templates' `spec: §8.5` → `§5.8` correction (F7): the campaign's next regeneration.
- The Lean certificate facts (F6): a follow-on; `UnitObs` gains `story` and `scenarios`, and
  `collect_units.py` uses the D1 parser.

## Verification

- D1: `board.py --self-test`; the four negative fixtures; `issue_dor.sh`; `board.sh`; the skill
  grep; a dry-run `issue-create`.
- D2: preflight; the domain/action tests; the two refusal tests.
- D3: preflight; the web Playwright suite.
- D4: `scripts/fetch-issues.sh`; `check-data.cjs`; `check-v2.cjs`; `build-docs.py --check`;
  `generate-standards.py --check`; `publish-standards.sh --check`.

## Execution record

| Unit | Issue | Branch / PR | State |
|---|---|---|---|
| D0 | #933 | `issue/933-bdd-delivery` | this unit |
| D1 | #934 | — | blocked-by #933 |
| D2 | #935 | — | blocked-by #933 |
| D3 | #936 | — | blocked-by #935 |
| D4 | #937 | — | blocked-by #934 |
