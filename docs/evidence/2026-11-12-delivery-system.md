# Delivery system — queue truth, adoption and the four-plan split

**Date:** 2026-11-12. **Unit:** conveyor#26 (resumed). **Protocol:** `docs/plans/03-DELIVERY.md`
(PR #11, hardened by #39/PR #40).

Every number below is a reading with its time and the command that produced it. Do not quote one
without its row.

## What changed

1. **In-flight adoption.** PR #36 (`lane/spec-v2`) had no issue and no closing reference. Issue
   **#38** now records the branch, baseline and delivery contract; PR #36's body carries the
   closing reference (restored after the PR owner's rewrite dropped it).
2. **Merged branches deleted.** Nine `lane/*` / `slice/*` heads whose tips are ancestors of
   `origin/main` were deleted by API. Local worktrees keep working; the map is below.
3. **The four multi-session plans split** into file-disjoint siblings, each with the full contract
   (DoR, BDD + `Verify:`, DoD, typed dependencies). The parent keeps the dependency-bearing part.
4. **Inbound edges re-pointed** where the split moved the target: #28 → #44, #24 → #43, #21 → #41,
   #20 → #43.

The protocol hardening that this unit consumes was **#39/PR #40**, merged at `80dcf49`
(2026-11-12T07:04:19Z): skills conform to `03-DELIVERY.md`, the `/issue-sweep` report-only routine,
and the two repo guards (shared-root commit, unissued PR). Its PR run `43170039391` is **success**;
`rust-checks` ran 20 minutes because `.claude/hooks/*.py` is code, not documentation. Its merge run
`43172563380` was **in progress** at the final reading below.

## Before / after

| Measure | Before | After | Query |
|---|---|---|---|
| Open `unit` issues | 19 at 2026-11-11T11:56:08Z | 24 at 2026-11-12T07:09:24Z | `gh issue list --state open --label unit --limit 100 --json number \| jq length` |
| Ready / claimed / blocked / in review | 12 / 3 / 2 / 1 (+1 claimed this unit) | 17 / 3 / 3 / 1 | `gh issue list --state open --label unit --json labels` |
| Non-`main` heads | 13 at 2026-11-11T11:56:08Z | 4 at 2026-11-12T07:09:24Z | `git ls-remote --heads origin` |
| Heads that are merged ancestors | 9 | 0 | per-head `git merge-base --is-ancestor` |
| Open PRs without a closing reference | 1 (PR #36) | 0 | `gh pr list --state open --json body` + the `Closes` regex |
| Branches outside the model | 10 `lane/*` / `slice/*` work heads | 2 live claims (`lane/spec-v2`, `slice/s12-wave3/studio-ui`) | `git ls-remote --heads origin` |

The before reading is the sweep snapshot taken during #39's build; the after reading is the final
one taken for this doc. The issue count moves 19 → 24 by: −1 (#39 closed by its merge), +6 split
siblings.

## Branch cleanup (9 deleted, all ancestors of `origin/main`)

| Branch | Tip | Deleting it |
|---|---|---|
| `lane/bootstrap` | `f18cb03` | worktree `.worktrees/bootstrap` keeps working locally |
| `lane/docs-process-model` | `8bb6099` | — |
| `lane/integration-nov10` | `917f18b` | worktree `.worktrees/integration-nov10` |
| `lane/s12-wave3` | `c88a2da` | worktree `.worktrees/render-determinism` |
| `lane/s13-process-engine` | `b72d775` | worktree `.worktrees/s13-finish/conveyor` |
| `lane/s18-kata-improvement` | `d61f739` | worktree `.worktrees/s18-kata` |
| `lane/s19-eval-build-c` | `7d74c8c` | worktree `.worktrees/s19-eval-c` |
| `lane/s20-eval-climb` | `deba81d` | no local worktree at deletion time |
| `slice/train-2026-11-09/s06d` | `8a1817d` | worktree `.worktrees/s06d` |

Deletion command, per branch: `gh api -X DELETE repos/nonagenticai/conveyor/git/refs/heads/<b>` →
HTTP 204.

**Left in place:** `lane/spec-v2` (live claim of #38/PR #36) and `slice/s12-wave3/studio-ui` (live
claim of #16). `lane/train-2026-11-09` duplicates PR #36's one-file change; it is deleted after
PR #36 merges (worktree `.worktrees` root is parked on it — do not commit there).

## Split map

| Parent | Keeps | Sibling | Takes |
|---|---|---|---|
| #16 S12 | P1 OIDC, P2 board/cockpit | **#41** | P3 measurement UI |
| | | **#42** | P4.1–P4.2 eval-report + eval-signoff |
| | | **#43** | P4.3–P4.4 hillclimb + inference-usage |
| #18 S14 | P2–P6 deployment, acceptance, receipt | **#44** | P1 the S14 rewrite (docs-only) |
| #22 S18 | P1–P5 producers + improvement job | **#45** | P6 the live loop proof (`blocked-by #18`) |
| #23 S19 | P1–P5 accounting model | **#46** | P6 the Tier-3 auditor |

Why the parent keeps that part: every inbound `acceptance-blocked-by #18` edge names the deployment
gate, and every consumer of the accounting points at the model — moving those would silently
re-point live-proof dependencies.

## Adoption record (PR #36 / issue #38)

- Issue #38 created 2026-11-11 with the 13-section contract, baseline `main@66ff7de`, and the
  branch `lane/spec-v2@76d26eb`.
- PR #36's body first received the closing reference, then the PR owner rewrote the body and
  dropped it; it was restored at the top and the owner was asked to keep it (PR comment).
- At 2026-11-12T07:09:24Z: #38 `OPEN`, PR #36 `OPEN`, and the body's `Closes` regex is true.

## Final queue reading (2026-11-12T07:09:24Z)

- 24 open `unit` issues: 17 `status:ready`, 3 `status:claimed` (#16, #26, #27), 3
  `status:blocked` (#23, #32, #45), 1 in review (#38, no status label).
- 5 remote heads: `issue/26-queue-truth`, `lane/spec-v2`, `lane/train-2026-11-09`, `main`,
  `slice/s12-wave3/studio-ui`.
- 1 open PR: #36 (`lane/spec-v2`), with a closing reference.

Commands: `gh issue list --state open --label unit --json number,labels`,
`git ls-remote --heads origin`, `gh pr list --state open --json number,headRefName,body`.
