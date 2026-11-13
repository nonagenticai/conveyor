# 03 — Delivery: one unit = one issue = one PR

**Read this after `00-INDEX.md`, before you create a branch.** `CLAUDE.md` is the short form;
this file is the authority.

## Why

Measured 2026-11-11 on `nonagenticai/conveyor`:

- **0 issues**, 9 PRs in 3 days. #1/#2/#3 were closed as superseded; #5 (the first train)
  merged; #9 (the second train) is 92 commits / 196 files; #7/#8 are draft + conflicting and
  their head shas are ancestors of #9's branch.
- Last 200 CI runs: **54 completed — 32 cancelled, 18 failed, 3 succeeded**; 38.5 h wall,
  mean 42.8 min. Draft-PR churn and probe pushes dominate.
- 13 remote branches, 15 worktrees, 20 dirty/untracked files in the shared root checkout.

The unit of intent had no durable object. A branch is created after the decision, dies at merge,
and is invisible to the next session. The GitHub issue is the missing object: identity,
ownership, discussion, closing reference, search. The issue is the claim; the PR is the finish
line.

## The unit

**One issue = one deliverable = one branch = exactly one PR.**

- The issue exists before any branch. No issue → no branch → no work.
- A branch lives only as long as one deliverable. It is not a multi-day train.
- A slice is a session's chunk of one issue. Slices are commits on the issue branch; they never
  get their own PR.
- ⭐ **Until the spec is complete: one plan = one issue.** A whole `S##` slice plan's remaining
  work is one issue, one branch and one PR — not one issue per work package. **Issue size is the
  lever**: larger issues mean fewer PRs, fewer CI runs and fewer merge points. A plan-sized PR is
  the deliberate trade until the spec is complete.
- ⭐ **The written exception (owner decision, 2026-11-11):** the four plans that are multi-session
  by shape are split into 2–5 file-disjoint siblings each — #16 (S12, by screen group), #18 (S14,
  docs rewrite vs deployment), #22 (S18, code vs live receipt), #23 (S19, accounting vs Tier-3
  auditor). Each parent links its siblings, and the parent keeps the dependency-bearing part so
  inbound `blocked-by` edges stay valid. No other plan is split.
- Splitting is the exception, not the default. Split only when a plan contains genuinely
  independent deliverables that cannot merge together (different acceptance, different
  deployment gate), and then **at most 5**, with the reason in each issue. Never split "step 1 /
  step 2" of the same change.
- Prefer file-disjoint issues: `block-duplicate-pr.py` denies a PR that overlaps ≥2 files with a
  recent PR. Overlapping issues are sequenced with `status:blocked`, never run in parallel.

## Issue content contract — DoR, acceptance criteria, DoD

Every unit issue is a durable brief: a session that has never seen the plan must be able to decide
from the issue alone whether it is ready to claim, what "done" means, and what it waits on. The
shape is `.github/ISSUE_TEMPLATE/unit.md`; the rules are:

- **Baseline.** State `main@<sha>` — with the date measured and the blob shas of the cited plan and
  audit files. The #9 train merged on 2026-11-11; a `#9@<sha>` reference in an older issue is
  historical and its parts are re-checked on merged main. A claim without a baseline is not ready.
- **References.** Every spec/plan/audit/code/test reference is a path that resolves at the ref it
  names, or an issue number that resolves. Cite `path:symbol`; line numbers only when re-verified at
  the baseline. A path the issue must create is marked `to create`.
- **Parts.** One part per plan Deliverable/Build-order item or audit `W`/`V` id. Each part states its
  measured state — **DONE / PARTIAL / ABSENT / CARRIED-BY-#9** — with evidence, and carries at least
  one BDD scenario.
- **BDD acceptance.** `Given <state>, when <action>, then <observable outcome>` with a mandatory
  `Verify:` line: an exact command or `path::test`. A scenario without a verification is not
  acceptance; "works correctly" is not a criterion. A deployment-gated scenario says so and names its
  gate.
- **Dependencies — four relations, never one.** `blocked-by` (cannot start), `acceptance-blocked-by`
  (build now, the live scenario waits), `blocks`, `related`. Each edge cites its source (plan
  `depends_on`, the `00-INDEX.md` row, a quoted plan precondition, the audit's order). Blockers are
  **issues**, never PRs; deployment is #18. Only a `blocked-by` sets
  `status:blocked`; an `acceptance-blocked-by` never freezes construction.
- **No duplication.** At most one paraphrased sentence per part, then the link. The issue is a brief,
  not a copy of the plan.

**Definition of ready** — the claim gate: baseline and doc blob shas named · every reference resolves
· every part has a scenario with a `Verify:` · dependencies complete, typed and cited ·
`Blocker: #N` present iff `status:blocked` · one-PR scope, file-disjoint or sequenced · scope
corrections applied.

**Definition of done** — the merge gate: every scenario passes at the PR head with a row in
`Evidence at completion` · the repo gate is green at the exact head · tests added/updated per part,
none skipped · docs updated · migrations/ledger and `--check-lock` handled when touched · live
verification recorded where deployment is a prerequisite · every CARRIED-BY-#9 part re-checked on
merged main · one PR `Closes #N` MERGED, issue closed, branch deleted.

A `status:blocked` issue carries a plain line starting `Blocker: #N`; a `Blocker:` mention inside
prose or a checklist is not a blocker line. The number is an issue, never a PR.

## Labels and states

| state | label | meaning |
|---|---|---|
| ready | `status:ready` | open, no linked PR, no remote branch — safe to claim |
| claimed | `status:claimed` | branch pushed + claim comment; work in progress, no PR yet |
| in review | (native) | one non-draft PR open with `Closes #n` |
| blocked | `status:blocked` | body carries a plain `Blocker: #N` line naming an open issue; do not claim |
| needs human | `status:needs-human` | a decision/credential/permission the session cannot supply |
| done | closed | PR merged; `Closes #n` closed it; branch deleted |

Every deliverable also carries `unit` and its slice label (`S02` … `S21`; two labels when it
spans plans).

⚠️ **Assignment is not available with this repo's GitHub App token** — `gh issue edit
--add-assignee` fails with *"Assigning agents is not supported with GitHub App installation
tokens"*. Do not depend on the assignee field. The claim is: `status:claimed` + the pushed
branch + a claim comment naming the branch and HEAD. The branch is the lock; the label and
comment are the readable signal.

## Claim — four checks, then push

From any checkout, before creating a branch:

```bash
gh issue view N --json state,labels,comments,closedByPullRequestsReferences
gh pr list --state all --search "#N"          # any PR that closes it
git ls-remote --heads origin "issue/N-*"      # any live branch for it
git log origin/main --grep "#N" --oneline     # already merged
```

All four must be empty. If any is non-empty the issue is claimed: **join it**, do not create a
parallel branch.

Then check the issue's **Definition of ready** (above). If it fails, do not create a branch: comment
the exact gaps (missing headings, a reference that does not resolve, a part with no `Verify:`
scenario, a `Blocker:` line that disagrees with the label) and stop — or fix the body first if you
are its author. Claiming an unready brief is how a session re-derives the plan inside a branch.

Claiming (unclaimed case):

```bash
git fetch origin
git worktree add -b issue/N-<slug> .worktrees/issue-N origin/main
git commit --allow-empty -m "claim: conveyor#N <slug> (nonce $(uuidgen))"
git push -u origin issue/N-<slug>
git ls-remote --heads origin refs/heads/issue/N-<slug>   # MUST equal git rev-parse HEAD
gh issue edit N --add-label status:claimed --remove-label status:ready
gh issue comment N --body 'Claimed. Branch `issue/N-<slug>` at `<sha>`.'
```

- The marker commit **must carry a unique nonce**. Two sessions' "empty" marker commits can
  otherwise share parent/message/second and therefore the same SHA; the loser's push then
  succeeds as "up to date" and both believe they claimed.
- Verify the remote SHA after pushing. If it is not yours, you lost the race: fetch the branch
  and join; **never force-push**.
- Work only in `.worktrees/issue-<n>`. Never create a branch or commit in the shared root
  checkout — another session's branch is checked out there.
- If an in-flight branch predates its issue (`lane/*`, `slice/*`), record the branch in the
  issue body and adopt it. ⚠️ `block-duplicate-pr.py` refuses a PR from `slice/*`, so at ship
  time push the adopted HEAD to a new `issue/N-<slug>` ref and open the PR from that.
- If a PR already exists without an issue, create the issue for it (the full contract), record the
  branch and PR in the body, comment on the PR with the link, and add `Closes #N` to its body. The
  PR's owner keeps the work; never push to that branch. `conveyor#38` (PR #36) is the worked
  example.

## The console — a front door, not a second protocol

The operator console at `conveyor.nonagentic.ai/design` can intake an issue as a card, move a
card, comment, claim and release against the tenant's own rows. It is a **front door to the
platform**, not a second delivery protocol:

- the GitHub issue is still the unit and the pushed branch is still the claim; a card created
  from the console carries `meta.issue` and is a view of the unit, never a substitute for it;
- the console cannot open a PR, merge, or edit a label — those stay the protocol above;
- `conveyor admin intake` and the console both call `cards.create`, so the two paths cannot
  produce different rows.

## Work and gate

- Commit freely on the issue branch (commits are free; branch pushes run no CI — the workflow
  triggers on `pull_request` and `push: main` only).
- Run the repo's full local gate at the exact head before any PR:

| stack | gate |
|---|---|
| Rust | `rbuild --gate` (or the issue's declared command) |
| docs/skills only | `git diff --check` + every relative Markdown link resolves |

- Paste the command, result and SHA into the PR body. **No PR until green.**

## Enforcement

Two repo-level `PreToolUse` hooks (registered in `.claude/settings.json`) enforce the two rules
that were broken before this protocol:

- `.claude/hooks/block-shared-root-commit.py` denies `git commit` in the shared root checkout and
  names the exact worktree command. Commits in `.worktrees/issue-<n>` are allowed.
- `.claude/hooks/block-unissued-pr.py` denies `gh pr create` unless the head branch matches
  `issue/<n>-*` and the body carries `Closes #<n>` (or `Fixes`/`Resolves`).

Both **fail open** on an unparseable command: a parse gap must never block every session. The
protocol text is authoritative; the hooks are the seatbelt. Their cases live in
`.claude/hooks/tests/test-guards.py`.

## The PR — exactly one

```bash
gh pr create --base main --head issue/N-<slug> \
  --title "#N <issue title>" \
  --body-file -   # Closes #N, What, Why, evidence, Resume, risks
```

- Non-draft, never a second PR for the same issue, never a push "to see CI".
- One CI run should follow. Red → fix locally and push the same branch (a second run is
  legitimate). Red twice → `status:blocked` + handoff; stop pushing.
- Before blaming the PR, check `main`: if main is red on the same job, this issue is the
  messenger, not the cause. Record it; do not "fix" someone else's red.
- **Definition-of-done gate before merge** (above): every scenario in the issue has an evidence row
  at the PR head, the gate passed at that exact sha, the named tests ran, and every CARRIED-BY-#9
  part was re-checked on merged main. A green check is not the DoD.
- Merge when green (or hand to the train owner only for an owner-declared train). Verify the PR
  state is `MERGED` before deleting anything. Delete the branch with `git push origin --delete`
  / the API — **never blind `gh pr merge --delete-branch`**, which can close the PR when the
  base moved.
- The issue closes via `Closes #N`. If it did not, close it with the evidence.

## CI budget and WIP cap

- **2 pipeline runs per issue**: the PR run and the merge run. Every run is a verdict, never a
  probe.
- **A docs-only PR skips the checks and the build.** When every changed path is documentation
  (`docs/**`, any `*.md`, `.claude/**`), the archetype's classifier marks the change as non-code
  and both `rust-checks` and `Build, push, smoke` are skipped (gitops-production#5166). Budget
  such an issue as one short run, not a full suite — **this applies to the PR run only; the merge
  run is still a full suite until #32.** One code path — a `Cargo.toml`, a migration,
  `.gitignore`, a workflow file — puts the whole suite back.
- **At most 2 open unit PRs at a time** (the workspace suite runs on `arc-large`, whose measured
  pool max is 2). More only queues and makes sessions wait on CI. Count open PRs whose head
  matches `issue/*` — PRs do not inherit issue labels:

```bash
gh pr list --state open --json number,headRefName \
  --jq '[.[] | select(.headRefName | startswith("issue/"))] | length'
```

- `issue-deliver` checks this cap and refuses the PR when it is full. **Claims (branches) are
  unlimited; PRs are not** — `issue-pickup` never refuses a claim on this basis.

## Handoff — mandatory when stopping before merge

Post as a comment on the issue. The next session reads the latest one first.

```markdown
<!-- handoff -->
**State:** HEAD <sha> · PR #<n|none> · CI <conclusion|not-run>
**Done:** <what is on the branch, with evidence>
**Next:** <exactly one step>
**Blocked:** <none | issue # / decision / credential>
**Gate:** `<exact command>` → <exit/result at this sha>
```

Never end on a verbal status. A session that dies without a handoff forces the next one to
re-derive everything.

## Resume — a previous session's PR

1. Read the issue body, then the latest `<!-- handoff -->` comment.
2. `gh issue view N --json closedByPullRequestsReferences` → the PR; read its body and
   `gh pr checks`.
3. `git fetch origin && git log --oneline origin/issue/N-* ^origin/main`; work in a worktree.
4. Push to the **same** branch. Never create a second branch or PR for the same issue.

## Finding work

```bash
# ready queue
gh issue list --state open --label status:ready --json number,title,labels

# everything by slice
gh issue list --state open --label unit --json number,title,labels \
  --jq '.[] | "\(.number)\t\(.labels|map(.name)|join(","))\t\(.title)"'

# claimed (label, because the App token cannot assign)
gh issue list --state open --label status:claimed

# open unit PRs and the WIP count
gh pr list --state open --json number,headRefName,title
```

Default priority is issue-number order; a dependency blocker always overrides it. Do not claim a
`status:blocked` issue. Blockers are named in the body; when the blocker lands, the owner of that
issue flips it to `status:ready`.

For the whole-queue truth report — ready-with-branch, claimed-without-branch, PR-without-`Closes`,
open-issue-with-merged-PR, and the branch-to-issue map — run the `/issue-sweep` skill. It reports
and never edits; the owning session flips its own labels.

## Issue creation

Use the `/issue-create` skill. It reads the slice plan + its `00-INDEX.md` row + the relevant
`W`/`V` section of `docs/evidence/2026-11-10-plan-audit.md`, dedupes against issues, PRs,
branches and merged commits, **verifies every reference at the baseline**, then writes **one issue
per unfinished slice plan** from `.github/ISSUE_TEMPLATE/unit.md` — the full contract above, not a
summary. It self-checks before `gh issue create`: required headings, every path resolves, every part
has a `Verify:` scenario, and the chosen label matches the Dependencies table. A split is the
documented exception (max 5).

⚠️ **Historical:** while the integration train (#9) was open, every code issue was `status:blocked`
with `Blocker: #26`. #9 merged on 2026-11-11 and its files (`control/approvals.rs`,
`eval/signoffs.rs`, `workflow/agent.rs`, `workflow/node.rs`, `migrate.rs`, `served-spec.lock.json`
and migrations `0037`/`0038`) are on main. There is no blanket blocker now: a `status:blocked`
issue names its own open `Blocker: #N`, and an issue whose only wait is the live proof is
`status:ready` with `acceptance-blocked-by: #18`.

## Retired terms

`lane` and `slice` remain as vocabulary for branches and for repos that do not use an issue
queue. Here, a lane is not a unit of work: the issue is. A "train" is an owner-declared
exception, never the default, and needs its own issue and acceptance.
