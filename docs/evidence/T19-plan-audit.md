# T19 plan audit — independent verification record

**Scope:** issue #605 · branch `issue/605-t19-plan` · worktree `.worktrees/issue-605`
**Baseline:** `origin/main@b9a6e2933cd50633893d581c8992c06caa043ce7`, measured 2026-11-23
**Audited heads:** first audit `9f8c4a73`; re-audit `b36af6be`

Verdict: YES

## What was audited

The unit ports the meeting design onto `origin/main` as **T19** and **D66**, preserving the
in-flight T18 UI slice; writes the T9-shape implementation plan and the MSP/TOGAF/SAFe programme
artefacts; and registers T19 in the programme seed, coverage, P0, index and docs pack. No Rust,
migration, action or console code is in scope; #606 is the implementation issue and stays open.

## First audit — NO

The first independent audit returned NO on two defects:

| # | Defect | Evidence | Fix |
|---|---|---|---|
| 1 | `docs/design/scripts/check-v2.cjs` asserted the Validation page contains the literal `49`; the programme now has 50 projects, so the design-pack gate failed | `check-v2.cjs:319`; `coverage.json` total 50; CI `ci-public.yml` runs the check | the check reads `coverage.json`'s `summary.projects.total` and asserts that string; the `v2.js` mandate prose says 50 |
| 2 | issue #605's W4.3 literal `grep -q 'T0–T19'` failed because the index writes the range as `` `T0`–`T19` `` | `docs/plans/00-INDEX.md:49`; W4.3 exit 1 | the issue command now checks `T19` and `T18` presence; the index text is unchanged and still names the full range |

## Re-audit — YES

A fresh independent validator re-ran the gates at `b36af6be`:

- `check-v2.cjs` exit 0, 281 checks, "the Validation page carries the measured 50 projects".
- Non-vacuity proven by two mutants in a throwaway copy: a page rendering 49 with JSON 50, and a
  JSON total of 51 with the page at 50, both fail the assertion.
- Issue #605 W4.3 exact command exits 0; W5.2's eleven-command static gate exits 0.
- T18 row byte-identical to `origin/main`; exactly 20 T rows T0–T19; D66 present; D64/D65 remain
  the real-UI rulings; no meeting sentence calls itself T18 or D64.
- Seed: 50 projects; T19 tranche 4 with `depends_on` T1/T2/T3/P0; 7 benefits; 8 links.
- Coverage: T19 `proposed`, measured gap, 2 open units (#605/#606); #606 open and blocked only by
  #605; no false `blocked_by` on built T9/T10.
- Docs: `documents.json` 131 entries including all seven T19 pages; the only deletion is the
  generated orphan `0009-real-v2-ui.html` (its source is `0013`; the registry references it zero
  times); `outputs.js`, `output-*.html` and `standards/` are unchanged versus the baseline.
- Scope: no `crates/`, `migrations/`, `apps/`, `.rs` or `.sql` changes; no T18-lane source change.

## Non-blocking notes and disposition

| Note | Disposition |
|---|---|
| `check-v2.cjs` asserts the total over the whole page text, not the Projects cell; non-vacuous at 50, but a future single-digit total could match elsewhere | accepted for this unit; anchor to the cell if the total ever falls below two digits |
| W5.2's `git diff --check` is vacuous on a clean tree | the substantive `git diff --check origin/main...HEAD` also exits 0 |
| The issue-body edit has no git provenance | the issue is the contract; its W4.3 command is the executed gate |
| No CI run exists at the audited head | the PR's design-pack run is the final CI gate; the local W5.2 gate and the six Playwright pack checks are green |

## Reproduce

```bash
cd .worktrees/issue-605
python3 scripts/build-programme-seed.py --check
python3 scripts/check-programme-definition.py
python3 scripts/build-coverage.py --check
python3 scripts/build-v2-coverage.py --check
python3 scripts/check-output-standard.py --self-test && python3 scripts/check-output-standard.py
python3 scripts/ci/board.py --self-test
node docs/design/scripts/check-coverage.cjs --self-test && node docs/design/scripts/check-coverage.cjs
node docs/design/scripts/check-data.cjs
node docs/design/scripts/check-docs.cjs
python3 docs/design/scripts/build-docs.py --check
node docs/design/scripts/check-console.cjs
node docs/design/scripts/check-v2.cjs
node docs/design/scripts/check-studio.cjs
node docs/design/scripts/check-learning.cjs
node docs/design/scripts/check-workbench.cjs
node docs/design/scripts/check-architecture.cjs
git diff --check origin/main...HEAD
```
