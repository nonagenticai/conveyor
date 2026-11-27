# S04 P4.1 — the live coding, worker and billing receipts

Measured 2026-11-27T06:21–06:24Z against the deployed engine
`ghcr.io/nonagenticai/conveyor@sha256:4cd9a9f356d69c520d245e77fb8ea8ad49fc21d8a5ec46bf6aa532285957ad7e`
(pods started 2026-11-27T03:46Z), tenant `nonagentic`.

Transport: the deployed CLI. The admin host is redacted from this public receipt; the command shape
is `ssh <admin host> 'sudo KUBECONFIG=… kubectl -n conveyor exec -i deploy/conveyor -- conveyor
admin action invoke --tenant nonagentic --name <action> --input /dev/stdin'`. Each call's input and
raw response sit beside this file.

## Verdicts

| Contract | Verdict | Command | Actor | Returned | Evidence |
|---|---|---|---|---|---|
| coding | **executed against the real coding service** | `tools.call` `coding.jobs_create` on run `01a21bab-93d2-7ea7-b0a8-e69af2e17a23`, bound to toolkit `01a1f353-cf2b-7ed5-b470-2290ef635b0e` (effect ceiling `write`) | the tenant's first active member (CLI default) | job `7c5b90bb-7fa0-4e5a-8840-e38d8bb5c6f5`, `queued` 06:23:36Z; read back through `coding.jobs_get` → `running`, started 06:23:37Z | `24-coding.json`, `25-coding-get.json` |
| worker | **executed over conveyor's durable queue** (#450 retired the Python upstream) | `workers.list` | same | 47 `(kind,status)` queue rows (for example `approval.closed.emit succeeded 20905`, `benchmark.reap succeeded 4163`) and 2 registered hosts | `10-workers.json` |
| billing | **executed over conveyor's ledger** (#46) | `billing.summary` `{window_days: 30}` | same | window 2026-10-28T06:21:42Z → 2026-11-27T06:21:42Z; `total_usd` 1.417925; 649 inference calls; 25 tool calls | `11-billing.json` |

## The coding path

The external action itself (`coding_jobs.create`) is refused by the admin CLI with "not an internal
action" (`12-coding-direct.stderr`). That is the design: an external action is dispatched by the
broker, so the real contract is `tools.call`, which resolves the run's binding, pins the published
toolkit version, validates the input, owns the deadline and ledgers the call.

The run and bind sequence, all live:

- card `01a21bab-7756-766b-a55e-4d8059825076` (`20-card.json`);
- run `01a21bab-93d2-7ea7-b0a8-e69af2e17a23` created `queued` (`21-run.json`), then moved to
  `running` at 06:22:41Z (`26-run-get.json`) — `runs.start` afterwards refused `409 run is running`
  (`22-run-start-attempt.stderr`);
- toolkit `coding` published at v1, id `01a1f353-cf2b-7ed5-b470-2290ef635b0e` (`23-toolkits.json`);
- binding `01a21bac-591d-7b80-a7de-ad394a1c55e3`, run-scoped, effect ceiling `write`, enabled
  (`23-bind.json`).

## What is not claimed

- No coding **result** is claimed: the job was `running` at its read-back. The receipt proves the
  call executed through the real contract; the job's own output is the coding service's record.
- The worker and billing rows are reads over conveyor state, not external calls. That is the
  contract after #450 and #46, and this receipt says so rather than implying an upstream answered.
- Nothing here re-runs T14's six toolkit calls; this is #3's P4.1 only.
