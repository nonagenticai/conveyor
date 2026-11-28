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

Do not list `status:blocked` or `status:not-ready` as claimable. Default priority is issue-number
order; a dependency
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

The body's `## Claim` block is the fifth check: a body that names a branch and a HEAD is claimed,
even when the branch list looks empty (an adopted `lane/*`/`slice/*` branch never matches
`issue/N-*`).

### Definition-of-ready gate — before any claim

An issue that fails its DoR is not claimable; a claim on an unready brief is how a session re-derives
the plan inside a branch. Read the body and check:

```bash
gh issue view N --json body --jq .body > /tmp/issue-N.md
grep -E '^## (User story|References|Baseline|Current state|Parts and acceptance criteria|Dependencies|Definition of ready|Definition of done)' /tmp/issue-N.md
grep -c 'Scenario:' /tmp/issue-N.md        # at least one per `### P` part
grep -c '@verify=' /tmp/issue-N.md         # one per scenario; the id has no spaces
grep -c 'Verify:' /tmp/issue-N.md          # one per scenario: the exact command
grep -E '^`?Blocker: #[0-9]+' /tmp/issue-N.md  # present iff the label is status:blocked
python3 scripts/ci/board.py --repo <owner/name> --dor N  # Programme + INVEST/DoD; exit 1 names the gaps
```

- a blocker line must start the line (the template's placeholder is not a blocker); its number must
  be an issue, not a PR;
- exactly one `## User story`, all three clauses (`As a …`, `I want …`, `so that …`) non-blank; a
  unit is one story, never one per part;
- every `### P` part has ≥1 Gherkin `Scenario:` (`Given`/`When`/`Then`) with a `@verify=<id>` tag
  and a `Verify:` line; a scenario missing either is a draft, not acceptance;
- the six INVEST rows are present and each names **this issue's evidence**, never the template's
  generic phrase: Independent (no `blocked-by`, lands without another unit's files), Negotiable
  (outcome fixed, implementation settled at delivery), Valuable (the `so that` names a benefit or
  a named gap), Estimable (baseline and every file named), Small (one issue, one PR), Testable
  (every scenario carries `@verify`);
- the DoD table has one row per scenario key (its `@verify` id); a per-part table or a prose
  checklist is not the DoD;
- the gate is `--dor N`: it exits 1 for a `unit` issue created on/after 2026-11-18 whose
  `## Programme` section is missing or has an empty field, and for one created on/after 2026-11-28
  whose DoR omits an INVEST criterion or restates the template, whose `## User story` is missing
  or not in `As a …, I want …, so that …` form, whose part has no `Scenario:`/`@verify`, or whose
  DoD has no row for a scenario id; #930 is exempt until it closes, and an earlier issue is
  exempt (forward-only);
- a `status:blocked` issue whose every `Blocker:` names a **closed** issue is a stale block, not a
  claimable one: run `python3 scripts/ci/board.py --repo <owner/name>` (apply mode) to release it,
  re-read the label, then claim. Never claim while the label is `status:blocked`;
- every path in References resolves (`git cat-file -e <ref>:<path>`); every `#N` resolves
  (`gh issue view N --json number,state`);
- the Dependencies table agrees with the label: `status:blocked` iff an open `blocked-by`; an
  `acceptance-blocked-by` alone is claimable (the live scenario waits; construction does not);
- any failure — a missing story, a part with no Gherkin scenario or `@verify`, a generic INVEST
  row, a DoD with no row for a scenario — → post the exact gaps as a comment and do not create a
  branch. Fix the body yourself if you are its author.

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
- Write the `## Claim` block into the body in the same operation — branch, HEAD, session, updated,
  `Adopted-from: —` — and update its HEAD on every push. The block is the claim another session can
  read without cloning.
- If the remote SHA is not yours, you lost: fetch the branch and join. Never force-push.
- Refuse to create a branch in the shared root checkout. Work only in `.worktrees/issue-N`.
- **Claims are unlimited.** One issue is one branch and exactly one PR; concurrent lanes are
  unlimited, and no repository-wide PR count gates a claim. The push is what claims the work.

### Resume a previous session's work

1. Read the issue body, then the latest `<!-- handoff -->` comment.
2. `gh issue view N --json closedByPullRequestsReferences` → the PR; read its body and
   `gh pr checks <pr>`.
3. `git fetch origin && git log --oneline origin/issue/N-* ^origin/main`; work in a worktree.
4. Push to the **same** branch. Never create a second branch or PR for the same issue.

### Adoption — an in-flight branch or an abandoned claim

- **An issue that predates its branch** (`lane/*`, `slice/*`): record that branch in the `## Claim`
  block and work there. At ship time push its HEAD to a new `issue/N-<slug>` ref and open the PR
  from that: `block-duplicate-pr.py` refuses a PR from `slice/*`.
- **A claimed issue with 48 h of silence** (no push to its branch and no comment on the issue): the
  claim is abandoned. Re-run the four checks, work on the **same** branch — never a parallel one,
  never a force-push — post a takeover comment naming the branch and HEAD, and set `Adopted-from`
  in the Claim block. No permission from the original session is needed; if it returns, it joins
  the branch.

## Never

- Create a branch before the issue exists.
- Claim a `status:blocked` or `status:not-ready` issue, or any issue that fails its Definition of
  ready. `status:not-ready` is written by the `issue-dor` workflow when the body does not meet the
  content contract; the comment on the issue names the gaps, and the label comes off by itself
  once the body is complete.
- Rename or force-push a live branch.
- End without a handoff comment if you stop before merge.
