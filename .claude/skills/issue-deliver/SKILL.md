---
name: issue-deliver
description: Deliver exactly one claimed GitHub issue as one PR — local gate first, evidence in the PR, no draft, no CI probing, one merge, handoff on stop. Use after claiming an issue, when continuing its branch, or when asked to finish/merge a PR.
---

# issue-deliver

The authority is `docs/plans/03-DELIVERY.md`. One issue = one branch = exactly one PR.

## Steps

1. **Read the issue** — acceptance commands, scope, blockers, and the latest `<!-- handoff -->`
   comment. If the issue is `status:blocked`, stop and name the blocker.
2. **Work only in the issue's worktree** (`.worktrees/issue-N`). Never commit in the shared root
   checkout. Commits are free; branch pushes run no CI (the workflow triggers on `pull_request`
   and `push: main` only). Use `~/.claude/scripts/commit-with-retry.sh "msg"`; never
   `--no-verify` or `SKIP=`.
3. **Run the declared gate at the exact head**:
   - Rust: `rbuild --gate` (or the issue's own command).
   - Docs/skills: `git diff --check` + every relative Markdown link resolves.
   Paste command, result and SHA. **No PR until green.**
4. **Respect the WIP cap** before opening a PR — ≤2 open `issue/*` PRs including this one:
   ```bash
   gh pr list --state open --json headRefName \
     --jq '[.[] | select(.headRefName | startswith("issue/"))] | length'
   ```
   If full, refuse to open the PR and wait for a merge (or hand off); do not open a third.
5. **Open exactly one non-draft PR**:
   ```bash
   gh pr create --base main --head issue/N-<slug> \
     --title "#N <issue title>" --body-file -
   ```
   Body: `Closes #N`, What, Why/source, acceptance evidence (command → result → sha), Resume,
   Risks — the shape in `.github/pull_request_template.md`. Never a draft, never a second PR,
   never a push "to see CI".
6. **CI**: one run should follow (the budget is two: the PR run and the merge run). A docs-only PR
   is the ~1-minute path — checks and build skipped; its **merge run is still a full suite until
   #32**. Red → fix locally and push the same branch (a second run is legitimate). Red twice →
   `status:blocked` **and** add a plain `Blocker: #N` line to the issue body — the label contract
   requires it — then the handoff; stop pushing. Before blaming the PR, check `main`: if main is
   red on the same job, this issue is the messenger — record it, do not "fix" someone else's red.
7. **Definition-of-done gate — before merge.** Verify against the PR head sha:
   - every scenario id in the issue has a row in the PR's evidence table (command → result → sha);
   - the gate passed at that exact sha;
   - the tests named per part exist and ran; none skipped;
   - the issue's docs/migrations/lock DoD items are handled;
   - every CARRIED-BY-#9 part was re-checked on merged main (or is explicitly deferred to the
     post-train sweep, named in the PR);
   - the issue's Definition-of-done checklist is satisfied.
   A green CI check is not the DoD. If an item cannot be met, do not merge: post the handoff naming
   the gap.
8. **Merge and verify.** Prefer a merge commit. Verify the PR state is `MERGED` (not just
   "green"), then delete the branch with `git push origin --delete` or the API — never blind
   `gh pr merge --delete-branch`, which can close the PR when the base moved. The issue closes
   via `Closes #N`; if it did not, close it with the evidence.
9. **Stopping before merge?** Post the handoff comment now:
   ```markdown
   <!-- handoff -->
   **State:** HEAD <sha> · PR #<n|none> · CI <conclusion|not-run>
   **Done:** <what is on the branch, with evidence>
   **Next:** <exactly one step>
   **Blocked:** <none | issue # / decision / credential>
   **Gate:** `<exact command>` → <exit/result at this sha>
   ```

## Never

- Open a PR before the local gate is green.
- Iterate in CI / push to probe.
- Open a second PR for the same issue.
- Claim "done" from a green check: verify the merge and the shipped state.
- Merge before the Definition-of-done gate passes.
- End on a verbal status.
