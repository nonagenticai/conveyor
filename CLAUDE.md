# conveyor — session entry contract

Read, in order: `docs/SPEC.md` → `docs/plans/00-INDEX.md` → `docs/plans/03-DELIVERY.md` → the
`S##` slice plan you are building.

**Work is claimed by a GitHub issue, never by conversation or by a branch you create first.**

- Before `git checkout -b` / `git switch -c` / `git worktree add -b`, run the `issue-pickup`
  skill or perform its four checks. No issue → no branch → no work.
- An issue body follows `03-DELIVERY.md`'s **issue content contract**: Definition of Ready, BDD
  acceptance per part, typed dependencies, Definition of Done. `issue-pickup` refuses an unready
  issue; `issue-deliver` refuses to merge before the DoD.
- One issue = one branch `issue/<n>-<slug>` = **exactly one PR** (`Closes #<n>`). Slices are
  commits on that branch; they never get their own PR.
- The PR opens only when the issue's acceptance command is green locally. No draft PRs, no
  push/draft to probe CI. The CI budget is **2 runs per issue**: the PR and its merge.
- Work in `.worktrees/issue-<n>`; never commit in this shared root checkout.
- Two repo-level guards enforce the two rules (`.claude/settings.json`): a `git commit` in the
  shared root is denied, and `gh pr create` is denied unless the head is `issue/<n>-*` and the body
  carries `Closes #<n>`. Both fail open on an unparseable command; the text above is authoritative.
- Stopping before merge? Write the `<!-- handoff -->` comment from `03-DELIVERY.md`. Never end
  on a verbal status.

## Roles — load the skill for the work

| Work | Skill |
|---|---|
| Tranche review, programme register, baselines, #302/#303 | `programme-manager` |
| Architecture contracts, ADM phase/plateau, compliance, waiver, #395 | `programme-architect` |
| Benefit profiles, baselines, transitions, realization | `benefits-manager` |
| Project brief and analysis/architecture inception | `business-analyst` |
| Standards fixpoint, closure evidence, independent acceptance | `quality-manager` |
| Release receipt, deployed identity, cutover, live probe | `release-manager` |
| Queue truth, claim, issue body, PR mechanics | `issue-pickup`, `issue-sweep`, `issue-create`, `issue-deliver` |

**Separation of duties.** The role that builds a gate's subject never certifies it: the builder
hands a live or end-to-end part to a fresh verifier; the architect's own contract is approved by
the Board; the BCM's realization is accepted by the SRO. No session writes `owner`, `approved_by`,
`SRO` or an approval row on the owner's behalf — it prepares the evidence and the recommendation,
and the human signs. A question is an approval, and its decision records who decided.

Current queue: `gh issue list --label unit --state open`. `status:blocked` issues are not
claimable. This file overrides the global `LANES` line for this repo: the lane's one-PR rule is
kept, but the unit is the issue and the branch carries its number.
