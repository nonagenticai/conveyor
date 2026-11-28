---
name: issue-create
description: Turn a slice-plan remainder, spec section or audit item into 1-5 GitHub issues that carry a verifiable Definition of Ready, BDD acceptance criteria, a typed dependency table and a Definition of Done. Use before starting work that has no issue, when a plan's remaining work must become a queue, or when a session is handed a TODO that is not an issue.
---

# issue-create

The authority is `docs/plans/03-DELIVERY.md` § "Issue content contract". Read it before creating
anything. The shape is `.github/ISSUE_TEMPLATE/unit.md`.

## Steps

**Before you start:** if the source is a proposed project with no P0 §4 brief or P0 §5 inception,
load `business-analyst` first — `issue-create` converts a defined plan, it does not define one. The
live register, not a plan's status label, decides whether the project is proposed.

1. **Read the source.** The slice plan `docs/plans/slices/S##-*.md` (all sections), its row in
   `docs/plans/00-INDEX.md`, and the relevant `W`/`V` section of
   `docs/evidence/2026-11-10-plan-audit.md`. Note the measured state (sha, `file:symbol`) — the
   issue body carries it, not a copy of the whole analysis.

2. **Verify references and record the baseline.** Before writing the body:
   ```bash
   git fetch origin
   git rev-parse origin/main              # the code baseline
   git cat-file -e <ref>:<path>          # every doc/code path the issue will cite
   git rev-parse <ref>:<plan> <ref>:<audit>   # blob shas for the Baseline section
   gh issue view N --json number,state    # every issue reference
   ```
   A path that does not exist yet is marked `to create`. The #9 train merged on 2026-11-11; a
   `#9@<sha>` reference in an older issue is historical, and its part is **CARRIED-BY-#9**
   (re-checked on merged main), never DONE. Record the date measured.

3. **Dedupe first — always.** Report what already exists before creating:
   ```bash
   gh issue list --state all --search "<keywords>"
   gh pr list --state all --search "<keywords>"
   git ls-remote --heads origin
   git log origin/main --grep "<keyword>" --oneline
   ```
   The #9 train merged on 2026-11-11. An item that landed with it is **closed as carried by #9**,
   not queued — verify on `origin/main`, not on the old train branch. An open `lane/*` or `slice/*`
   branch that carries the work is an in-flight adoption (step 4), not a new issue.

4. **One plan = one issue** (owner rule, until the spec is complete). A whole `S##` plan's remaining
   work is one issue, one branch, one PR — issue size is the lever: fewer PRs, fewer CI runs. Do
   **not** split one plan into work packages.
   Splitting is the exception: only when the plan contains genuinely independent deliverables that
   cannot merge together (different acceptance, different deployment gate), and then at most 5, with
   the reason in each issue. **The written split set (2026-11-11):** #16 (S12, by screen group),
   #18 (S14, docs rewrite vs deployment), #22 (S18, code vs live receipt), #23 (S19, accounting vs
   Tier-3 auditor) — the parent keeps the dependency-bearing part so inbound `blocked-by` edges stay
   valid. Never split step 1/step 2 of the same change. Prefer file-disjoint issues:
   `block-duplicate-pr.py` denies a PR that overlaps ≥2 files with a recent PR, so overlapping issues
   are sequenced with `status:blocked`.

5. **Write the contract body, not a summary.** Every section, in order:
   Goal (one sentence, the remaining outcome) · Plan/audit source (links) · References (table with
   the ref it resolves at) · Baseline (sha + date + doc blob shas) · Current state (one row per part:
   DONE / PARTIAL / ABSENT / CARRIED-BY-#9, with evidence) · User story (**exactly one per unit**:
   `As a …, I want …, so that …`, all three clauses non-blank; the value is the observable benefit
   or the named gap, never a restatement of the implementation) · Parts and acceptance criteria (one
   subsection per plan Deliverable/Build-order item or audit `W`/`V`; every `### P` part carries at
   least one Gherkin block — `Feature:`, one or more `Scenario:`s with `Given`/`When`/`Then`
   (`And`/`But`), and a `@verify=<id>` tag above each, the id carrying no space — followed by a
   mandatory `Verify:` line for each scenario with the exact command or `path::test`;
   `Scenario Outline` is expanded, each example a scenario) · Dependencies (typed: `blocked-by` /
   `acceptance-blocked-by` / `blocks` / `related`, each with its source and state; blockers are
   issues; deployment is #18) · Definition of ready (the six INVEST rows, each with **this unit's
   own evidence**, never the template's phrase: Independent — no `blocked-by`, lands without
   another unit's files; Negotiable — outcome fixed, implementation settled at delivery; Valuable
   — the `so that` names a benefit or a named gap; Estimable — the baseline and every file are
   named; Small — one issue, one branch, one PR; Testable — every scenario carries `@verify` and a
   `Verify:`) · Definition of done (a table with **one row per scenario**, keyed by its `@verify`
   id, each naming the evidence and the `Verify:` that passed at the PR head; one row per part, or
   a prose checklist, is not the DoD) · Out
   of scope (including the audit's scope corrections) · Deployment prerequisite · Open questions ·
   Evidence at completion (empty table).
   One paraphrased sentence per part; the plan and audit are linked, never pasted.

   Every body also carries the `## Programme` table exactly as
   `.github/ISSUE_TEMPLATE/unit.md`: `Tranche`, `Brief`, `ADM phase/plateau`, `Benefits`. Each
   value names a live row/document or `pending #N`; a bare `—` is empty (the template's ready
   placeholder).

6. **Self-check before create** — all must pass; fix the body, do not create yet:
   - every required heading is present, including exactly one `## User story`;
   - the story's three clauses (`As a …`, `I want …`, `so that …`) are all non-blank;
   - the four `## Programme` rows are present and non-empty, each naming a live row/document or
     `pending #N`;
   - every cited path resolves at its ref (`git cat-file -e`), every `#N` resolves;
   - every `### P` part has ≥1 `Scenario:` with `Given`/`When`/`Then` and a `@verify=<id>` (no
     space in the id) plus a `Verify:` command or test target; every id is unique;
   - the DoD table has one row per scenario key (its `@verify` id) and a `Verify:` column — a
     per-part table or a prose checklist is not the DoD;
   - the six INVEST rows are present and each names this issue's evidence, not the template's
     generic phrase (`board.py` refuses a unit created on/after 2026-11-28);
   - `python3 scripts/ci/board.py --contract-body /tmp/body.md` exits 0 — necessary, not
     sufficient: the gate checks presence and shape, while this checklist checks that every row
     and every `Verify:` carries this unit's real evidence;
   - the chosen state label matches the Dependencies table (`status:blocked` iff a `blocked-by` is
     open; an `acceptance-blocked-by` alone must not block);
   - no sentence is copied from the plan or audit (`git grep -F` a sample);
- the body carries no `scripts/ci/no-private-refs.sh` HARD pattern and no private repository slug:
  read the pattern set from that script (never retype it), and `gh repo view <slug> --json
  visibility` must be `PUBLIC` for every repository the body names. Name a private service by its
  public role (“the licensing service”), never by its repository path.

7. **Create the issue** with exactly one status label:
   ```bash
   gh issue create --title "<verb>: <deliverable>" \
     --label unit --label S## --label status:ready|blocked|needs-human \
     --body-file -
   ```
   Then run `python3 scripts/ci/board.py --repo <owner/name> --dor <new-number>`; exit 0 is the
   gate. Fix the body in place if it fails.

   **Link it into the rollup (#780 P3).** The programme parent owns the unit as a native
   sub-issue, and the tranche milestone carries the programme's own state:
   ```bash
   parent=$(gh issue list --repo <owner/name> --label programme --state open \
     --json number --jq '.[].number')
   child=$(gh api "repos/<owner/name>/issues/<new-number>" --jq .id)
   gh api -X POST "repos/<owner/name>/issues/$parent/sub_issues" -f sub_issue_id="$child"
   gh issue edit <new-number> --repo <owner/name> --milestone "T<tranche>"
   ```
   `parent` must resolve to exactly one number: an empty or multi-line value is a report, never a
   guess (the sweep flags a unit with no parent). `T<tranche>` is the `## Programme` Tranche value;
   `pending #N` or an unmapped tranche carries no milestone, and a milestone that does not exist is
   reported, never created here.
   Labels must exist first (`gh label create`); see `03-DELIVERY.md`. Never use `--add-assignee`
   (the GitHub App token cannot assign). A plan that spans two plans carries two slice labels.
   A docs-only unit (`docs/**`, any `*.md`, `.claude/**`) is budgeted as one short PR run; its merge
   run is still a full suite until #32.

8. **Record the number.** The real number comes from the `gh issue create` output; never reuse a
   number from a plan. Put it in the plan row's disposition and in the parent issue if any.

9. **Re-read the queue.** `gh issue list --state open --label unit` must show **one issue per
   unfinished `S##` plan** (or the documented ≤5 split), every blocked issue naming its blocker, and
   every part of the plan covered by an issue. If two issues for one plan cannot merge
   independently, merge them into one before reporting.

## Do not

- Create an issue for work whose branch/PR/commit already exists.
- Split one plan into per-work-package issues; one plan is one issue unless the independence
  exception is written down (max 5).
- Create a batch without checking each item against the live train.
- Pad the body with analysis that is already in the plan — link it.
- Create an issue whose acceptance is prose with no `Verify:` line.
- Create an issue with no `## User story`, a part with no Gherkin `Scenario:`/`@verify`, or a DoD
  that is one row per part instead of one row per scenario.
- Write an INVEST row as the template's generic phrase; the row names this unit's own evidence.
