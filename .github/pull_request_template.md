Closes #<issue>

## What
<The deliverable, in one paragraph.>

## Why / source
<Plan/audit section this implements. Link, do not copy.>

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
- [ ] Every CARRIED-BY-#9 part re-checked on merged main, or deferred in writing
- [ ] No second PR exists for this issue
- [ ] The WIP cap is respected (≤2 open `issue/*` PRs, including this one)

## Resume
- Branch: `issue/<n>-<slug>`
- HEAD: `<sha>`
- Next step: <exactly one, if this PR is not merged when the session ends>

## Risks / follow-ups
<Known risks, or "none".>
