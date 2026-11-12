---
name: issue-pickup
description: Find truly unclaimed work, claim one GitHub issue, or resume a previous session's issue/PR without duplicating it. Use when starting work, receiving a TODO or handoff, before creating any branch, or when asked "what should I work on".
---

# issue-pickup

The authority is `docs/plans/03-DELIVERY.md`. The pushed branch is the lock; the label and
comment are the readable signal. **This repo's GitHub App token cannot assign issues**, so never
use `--add-assignee`.

## No argument — print the board

```bash
gh issue list --state open --label status:ready --json number,title,labels
gh issue list --state open --label status:claimed --json number,title,labels
gh pr list --state open --json number,headRefName,title
```

Do not list `status:blocked` as claimable. Default priority is issue-number order; a dependency
blocker overrides it. If the ready queue is empty, say so and stop — do not invent work.

For the full truth table (ready-with-branch, claimed-without-branch, PR-without-`Closes`,
open-issue-with-merged-PR, branch map), run the `/issue-sweep` skill. It is report-only; a label
the sweep flags is the owning session's to fix.

## With issue N — four checks

```bash
gh issue view N --json state,labels,comments,closedByPullRequestsReferences
gh pr list --state all --search "#N"
git ls-remote --heads origin "issue/N-*"
git log origin/main --grep "#N" --oneline
```

All four empty → claim. Any non-empty → **resume**, never create a parallel branch. The issue's
`state` must be `OPEN`: a closed issue is finished — do not claim it; find the next unit.

### Definition-of-ready gate — before any claim

An issue that fails its DoR is not claimable; a claim on an unready brief is how a session re-derives
the plan inside a branch. Read the body and check:

```bash
gh issue view N --json body --jq .body > /tmp/issue-N.md
grep -E '^## (References|Baseline|Current state|Parts and acceptance criteria|Dependencies|Definition of ready|Definition of done)' /tmp/issue-N.md
grep -c 'Verify:' /tmp/issue-N.md          # at least one per `### P` part
grep -E '^`?Blocker: #[0-9]+' /tmp/issue-N.md  # present iff the label is status:blocked
```

- a blocker line must start the line (the template's placeholder is not a blocker); its number must
  be an issue, not a PR;

- every path in References resolves (`git cat-file -e <ref>:<path>`); every `#N` resolves
  (`gh issue view N --json number,state`);
- the Dependencies table agrees with the label: `status:blocked` iff an open `blocked-by`; an
  `acceptance-blocked-by` alone is claimable (the live scenario waits; construction does not);
- any failure → post the exact gaps as a comment and do not create a branch. Fix the body yourself
  if you are its author.

### Claim

```bash
git fetch origin
git worktree add -b issue/N-<slug> .worktrees/issue-N origin/main
git commit --allow-empty -m "claim: conveyor#N <slug> (nonce $(uuidgen))"
git push -u origin issue/N-<slug>
git ls-remote --heads origin refs/heads/issue/N-<slug>   # MUST equal git rev-parse HEAD
gh issue edit N --add-label status:claimed --remove-label status:ready
gh issue comment N --body 'Claimed. Branch `issue/N-<slug>` at `<sha>`.'
```

- The nonce is load-bearing: identical marker commits can share a SHA and make the loser's push
  look successful.
- If the remote SHA is not yours, you lost: fetch the branch and join. Never force-push.
- Refuse to create a branch in the shared root checkout. Work only in `.worktrees/issue-N`.
- **Claims are unlimited.** The WIP cap is a PR cap, not a claim cap: `issue-pickup` never refuses
  a claim on this basis. `issue-deliver` refuses the PR when 2 or more open `issue/*` PRs already
  exist — the third is not opened.

### Resume a previous session's work

1. Read the issue body, then the latest `<!-- handoff -->` comment.
2. `gh issue view N --json closedByPullRequestsReferences` → the PR; read its body and
   `gh pr checks <pr>`.
3. `git fetch origin && git log --oneline origin/issue/N-* ^origin/main`; work in a worktree.
4. Push to the **same** branch. Never create a second branch or PR for the same issue.

### Adopted in-flight branch

If the issue predates its branch (`lane/*`, `slice/*`), record that branch in the issue body and
work there. At ship time push its HEAD to a new `issue/N-<slug>` ref and open the PR from that:
`block-duplicate-pr.py` refuses a PR from `slice/*`.

## Never

- Create a branch before the issue exists.
- Claim a `status:blocked` issue, or any issue that fails its Definition of ready.
- Rename or force-push a live branch.
- End without a handoff comment if you stop before merge.
