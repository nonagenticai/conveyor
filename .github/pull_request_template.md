Closes #<issue> <!-- unit work only; delete this line for a small fix -->

## What
<What changes, in one paragraph.>

## Why
<The problem or request this answers. Link the issue or report.>

## Track
- [ ] **Small fix** — docs, a typo, a link, a comment, or a test that asserts behaviour already
      present. No unit issue is required.
- [ ] **Unit work** — the `Closes` line above names the unit issue.

## Checks
- [ ] I ran the local gate for my change: `<command>` (docs-only: the design-pack checks).
- [ ] Every commit is signed off with `git commit -s`; the sign-off is mine.
- [ ] No credential, private hostname, private address or private infrastructure detail.

## Credit
Name and handle for `CONTRIBUTORS.md` (default: your GitHub handle). Write "no credit" to opt out.

<!-- Unit work: keep the evidence below. A small fix may leave it empty. -->

## Acceptance evidence
Paste the exact gate at the PR head — command, result, SHA. No PR without a green local gate.

```
$ <command>
<result>
sha=<head>
```

### Scenario evidence — one row per BDD scenario in the issue

| Scenario | Command / test | Result | SHA |
|---|---|---|---|
| P1.1 | `<command>` | pass | `<sha>` |

- [ ] Every scenario in the issue has a row above, at this SHA
- [ ] The issue's acceptance gate is green at this SHA
- [ ] Tests named per part exist and ran; none skipped
- [ ] The issue's Definition-of-done checklist is satisfied
- [ ] No second PR exists for this issue

## Resume
- Branch: `issue/<n>-<slug>`
- HEAD: `<sha>`
- Next step: <exactly one, if this PR is not merged when the session ends>

## Risks / follow-ups
<Known risks, or "none".>
