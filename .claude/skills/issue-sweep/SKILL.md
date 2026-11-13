---
name: issue-sweep
description: Report the whole issue queue's truth — which issues are truly unclaimed, which claims are stale, which PRs have no issue, and which merged work is still open — without changing anything. Use before claiming in a busy queue, after a merge, when asked "what is actually unclaimed", or as a periodic queue check.
---

# issue-sweep

The authority is `docs/plans/03-DELIVERY.md`. This routine is **report-only**: it never flips a label,
deletes no branch, closes no issue, and pushes nothing. A drift row is the **owning
session's** action; the sweep names it and stops.

Run it from the repo root. It costs no CI run: every command below is `gh`/`git` read-only.

## Collect

```bash
git fetch origin --prune
git ls-remote --heads origin | sed 's#refs/heads/##' | awk '{print $2}' | sort

gh issue list --state open --label unit --limit 200 \
  --json number,title,labels,closedByPullRequestsReferences

gh pr list --state all --limit 100 \
  --json number,state,isDraft,headRefName,title,body,closedAt

git log origin/main --oneline -n 200
```

For each open issue that names a blocker or a branch, the two targeted checks are:

```bash
gh issue view <N> --json state,labels,body,comments,closedByPullRequestsReferences
git ls-remote --heads origin "refs/heads/issue/<N>-*"
```

## Classify

For every open `unit` issue, exactly one class:

| Class | Test |
|---|---|
| **ready** | `status:ready`, no live `issue/<N>-*` head, no open PR whose body closes it, no merged commit naming it |
| **claimed** | `status:claimed` **and** a live branch (an `issue/<N>-*` head, or a branch named in the `## Claim` block or a comment) |
| **in review** | no status label **and** one non-draft open PR whose body carries `Closes #<N>` |
| **blocked** | `status:blocked` and a plain `Blocker: #<M>` line naming an **open** issue |
| **needs human** | `status:needs-human` — list it, do not judge it |

Then the **drift** set — a row here is a lie the next session would act on:

1. **ready-with-branch** — `status:ready` but a live branch for it exists.
2. **ready-with-pr** — `status:ready` but an open PR closes it.
3. **claimed-without-branch** — `status:claimed` but no live branch and no PR.
4. **pr-without-closes** — any open PR in this repo whose body has no `Closes #<n>` (accept
   `Closes`/`Fixes`/`Resolves`, any spacing).
5. **open-issue-with-merged-pr** — an open issue whose linked PR is `MERGED` (the `Closes` did not
   fire).
6. **branch-without-issue** — a non-`main` head that is neither `issue/<n>-*` nor recorded in any
   issue body/comment.
7. **blocked-with-closed-blocker** — `status:blocked` whose `Blocker: #<M>` is closed.
8. **orphan** — a `unit` issue with no status label, no PR, and no live branch.
9. **abandoned-claim** — `status:claimed` with a live branch, but no push to that branch and no
   issue comment for 48 hours. The claim is adoptable (see `issue-pickup`); a status comment resets
   the clock, so waiting on CI or a blocker is not abandonment.

For 9, compare the branch's last commit time (`git log -1 --format=%cI origin/<branch>`) with the
issue's last comment time (`gh issue view <N> --json comments --jq '.comments[-1].createdAt'`);
both must be older than 48 hours.

## Report

Print one table, issue number ascending:

```text
#12  ready   OK          no branch, no PR
#16  claimed OK          slice/s12-wave3/studio-ui (adopted)
#26  claimed DRIFT       claimed-without-branch (original branch lane/integration-nov10 merged in #9)
#36  —       —           PR #36 lane/spec-v2, no issue (branch-without-issue)
```

Then the counts, each with the moment measured and the command that produced it:

```text
open unit issues: 17   ready: 12   claimed: 3   blocked: 2   in review: 0   needs-human: 0
drift: 3 (claimed-without-branch 1, branch-without-issue 1, pr-without-closes 1)
```

## After the report

- **Do not fix.** No `gh issue edit`, no branch deletion, no PR edit. The sweep's whole value is
  that it can be run at any time by any session without mutating shared state.
- For each drift row, name the action and its owner: e.g. *"#26 is stale-claimed: its branch merged
  in #9 — the #26 session must adopt it with a new branch or flip it `status:ready`."*
- If a session asks you to clean the queue, that is a **new unit** with its own issue (the queue
  sweep is `conveyor#26`), not this skill.

## Never

- Flip a label, close an issue, delete a branch, or edit a PR from this skill.
- Treat a missing `status:` label as ready — classify it (in review / orphan).
- Treat a merged PR's branch as a live claim — it is merged, not claimed.
