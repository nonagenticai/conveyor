# S18-loop-0096cc0da — the improvement loop after #638, measured on the deployed revision

Issue #12. A fresh live run at the deployed revision carrying #638 (`7c72c12c6`):

| item | value |
|---|---|
| `CONVEYOR_GIT_SHA` | `0096cc0da…` (both `deploy/conveyor` and `deploy/conveyor-worker`, read with `printenv`) |
| image | `sha256:f13dea55…0171c` |
| suite | `verified50-v1` · `01a1d039-0d04-7568-98ab-457e1d84fab0` |
| subset | the same 12 django instances as the 2026-11-23 run, `concurrency=6`, `with_failing_test=true` |
| model | `local-llm` (the subject's pinned model) |

## The two runs

| run | arm | label | id | window (UTC) | n | graded | no_attempt | resolved | apply_failed |
|---|---|---|---|---|---|---|---|---|---|
| baseline (direct model) | `baseline` | `baseline-oracle-files+failing-test` | `01a21f36-4d4c-7def-8dcd-ed91664e8356` | 22:52–23:00 | 12 | 2 | 6 | 1 | 4 |
| pipeline | `conveyor` | `conveyor-oracle-files+failing-test` | `01a21f25-a0bd-78c0-896d-d2afdd50f194` | 22:34–22:50 | 12 | 2 | 7 | 1 | 3 |

The arms are comparable under the contract's rule (oracle `files+test` and prompt `bug_fix` on
both; the pipeline is what is compared). The comparison below is the action the improvement job
invokes, called directly:

```json
{"comparable": true, "both": 0, "neither": 0, "a_only": 0, "b_only": 1,
 "discordant": 1, "missing_in_one": 2, "p_value": 1.0}
```

`a` = the pipeline run, `b` = the direct run. **p = 1.0**: under the loop's rule
(`p < 0.05` and at least six same-direction discordant pairs) the verdict is `inconclusive` by
construction. One instance is graded in both runs, one in the direct run only, two in the
pipeline run only.

## Per case

| instance | direct arm | pipeline arm |
|---|---|---|
| `django__django-10880` | `apply_failed` | `no_attempt` |
| `django__django-11149` | `no_attempt` | `no_attempt` |
| `django__django-11163` | `apply_failed` | `apply_failed` |
| `django__django-11451` | `apply_failed` | `resolved` |
| `django__django-11490` | `no_attempt` | `no_attempt` |
| `django__django-11848` | `unresolved` | `no_attempt` |
| `django__django-11880` | `resolved` | `unresolved` |
| `django__django-13112` | `no_attempt` | `no_attempt` |
| `django__django-13158` | `no_attempt` | `no_attempt` |
| `django__django-13401` | `no_attempt` | `apply_failed` |
| `django__django-13512` | `apply_failed` | `apply_failed` |
| `django__django-14500` | `no_attempt` | `no_attempt` |

## Why the denominator is two

- **No patch produced.** 6 direct cases and 7 pipeline cases are `no_attempt`. In the pipeline
  arm the worker recorded `the agent produced no patch` for one and, for three,
  `idempotency-conflict: a request with this Idempotency-Key is still running` — a retry of
  `agent.run` colliding with the previous attempt's in-flight call and being recorded as a
  terminal refusal. The direct arm's six are one-call refusals (`attempts=1`).
- **Patch not applying.** Of the six patches the direct arm produced, four failed to apply in the
  sandbox (`apply_failed`); of the pipeline's five, three did. `apply_failed` is withheld from the
  graded denominator, like `no_attempt`.
- The significance rule needs six same-direction discordant pairs. At a paired intersection of
  1 in 12, a 50-instance run projects to roughly four pairs — still below the bar.

## What #638 did, verified live

The baseline arm no longer fails: it ran, produced patches on six cases and had them graded
(4 `apply_failed`, 1 `unresolved`, 1 `resolved`). The grading fix is live. The remaining cap is
the producer yield — production and patch applicability — not the grading path.

## Conclusion

#12's acceptance — "a fresh live improvement run at a deployed revision carrying #638, with a
decisive verdict (not `inconclusive`)" — is not met by this run, and cannot be met on the
deployed revision: the verdict machinery needs six paired cases and the instrument yields one.
No candidate configuration changes this, because the paired denominator is capped by the smaller
arm's graded set. The producer's yield is the prerequisite, filed as #900.

## Artefacts

- `snapshot`/`facts` are not applicable here: this is a live benchmark run, not a unit-ledger
  render. The run ids above are the durable handles; `benchmark_runs.get`, `benchmark_cases.list`
  and `benchmark_runs.compare` re-read them.
