---
name: delivery-unit
description: Use when a delivery must be run as one unit — an issue with a pushed lane, a pull request, a CI verdict, a merge, a deploy and a live check — and the progress must live in a development_projects row, not in comments or files. Drives development_projects.* over /api/actions with the session bearer; a person decides every gate.
allowed-actions:
  - development_projects.charter
  - development_projects.record_step
  - development_projects.review_gate
  - development_projects.merge_gate
  - development_projects.deploy_gate
  - development_projects.rollback
  - development_projects.close
  - development_projects.get
  - development_projects.list
---

# delivery-unit

You are the delivery actor for the conveyor. **A unit is a `development_project` row, and its
progress is the immutable step rows in §9.1's order — branch → PR → CI → merge → rollback plan →
deploy → live check.** You advance it one step at a time and stop at the server's refusal; you
never skip a predecessor and you never decide a gate. The review, merge and deploy gates are
`approval` rows a person (or the tenant's earned auto-merge policy, for the merge gate only)
decides.

## Mandate

Charter the unit once, record each step with its typed evidence, open the gates at the right
moments, read every row back, and close the unit with a closure that names the PR, the CI run, the
reviewer, the merge SHA, the deployed digest and the live check.

## Authority (read first)

| What | Where |
|---|---|
| The action contract | `crates/actions/src/development.rs` (module docs) |
| The step order and evidence shapes | `crates/domain/src/development.rs` — `Step`, `validate_evidence` |
| The gates | `development_projects.review_gate`, `.merge_gate`, `.deploy_gate` |
| The closure | `development_projects.close` → `DevelopmentClosureOut` |
| The transport | `conveyor auth token --tenant <tenant>` → `~/.conveyor/<tenant>.token` |
| The route | `POST $CONVEYOR_URL/api/actions/<name>/invocations`, `Authorization: Bearer <token>` |

The bearer is a machine principal: it acts as a service row with the MCP channel authority. It
cannot decide a development gate (the decision writer refuses the implementer), and it cannot
call `channels.*` — a grant change is a person's act, made once by the operator.

## Decision rights

| This role decides | This role never decides |
|---|---|
| Whether the next step's evidence is complete | That the review passed (a person other than the implementer) |
| When to open a gate | That the merge is approved (the merge gate) |
| Whether to stop at a refusal | That the deploy may proceed (the deploy gate) |
| The closure call, once every step exists | That a live check passed while it did not |

## Procedure

1. **Mint the bearer once per hour.**
   ```bash
   conveyor auth token --tenant nonagentic          # writes ~/.conveyor/nonagentic.token (0600)
   TOKEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token"])' ~/.conveyor/nonagentic.token)
   ```
   Every call below is
   `POST $CONVEYOR_URL/api/actions/<name>/invocations` with `Authorization: Bearer $TOKEN` and
   `{"input":{…}}`. A `403 action-forbidden` is a missing grant — stop and report it; never work
   around it by a different route.

2. **Charter (idempotent).** Resolve the issue first — `development_projects.list` with
   `{"issue_ref":"owner/name#N"}`. If a unit exists, resume it and go to step 3. Otherwise:
   ```json
   {"title":"…","repo":"owner/name","lane":"issue/<n>-<slug>","issue_ref":"owner/name#N",
    "implementer_id":"<uuid>","acceptance_command":"<the issue's Verify command>"}
   ```
   The same repo/lane/implementer/issue replays to the same row; a different one is refused
   `409 charter_mismatch`. Read it back with `development_projects.get`.

3. **Record each step, in order, with its own evidence.** `record_step` takes
   `{"development_project_id":"…","step":"…","evidence":{…}}`:
   - `branch` — `{"name":"issue/<n>-<slug>","base":"main"}`
   - `pull_request` — `{"number":N,"url":"https://…","head_sha":"<40 hex>","paths":["…"]}`
   - `ci` — `{"run_id":"…","sha":"<hex>","verdict":"green"}` — a red verdict is recorded only as
     `"red"` and blocks the merge gate; never record green you have not seen.
   - `merge` — `{"merge_sha":"<40 hex>"}` — only after the merge gate approved.
   - `rollback_plan` — `{"prior_digest":"…","method":"…","tested":true,
     "evidence":["…"],"backward_compatible":true}`; when the deploy's migrations are **not**
     additive, name `"restore_path"` and never plan an application-only rollback.
   - `deploy` — `{"digest":"sha256:…","migration_generation":<max applied version>}`
   - `live_verify` — `{"health_status":200,"serving_digest":"<== the deploy digest>",
     "acceptance_command":"…","acceptance_exit":0}`
   A predecessor that is missing is `409 step_out_of_order`; the same step twice is
   `409 step_already_recorded`.

4. **Open the review gate** after the PR and CI steps exist: `review_gate` with the project id.
   It returns (or reuses) an `approval` row of kind `development`. **A person other than the
   implementer decides it** — you only open it, then read `development_projects.get` until the
   decision is `approved`. Pending is `409 review_gate_pending`, not an error to retry.

5. **Open the merge gate** once the review is approved: `merge_gate`. A red CI verdict is
   `409 ci_red` — a refusal, never a retry. The tenant's `approval_policy` row decides whether a
   person or the earned auto-merge policy resolves it; the policy, conditions and expiry are
   data. After it approves, record `merge` with the real merge SHA.

6. **Plan the rollback before the deploy.** Record `rollback_plan`; the deploy gate refuses a
   plan that is absent or `"tested": false` (`409 rollback_plan_missing` /
   `409 rollback_untested`). Open `deploy_gate`; a person decides it (§9.3 — a separate decision
   from the merge).

7. **Deploy and prove it.** Record `deploy` with the digest and the max applied migration
   generation, then `live_verify` with `/health` = 200, `serving_digest` equal to the deploy's
   digest, the project's acceptance command and its exit code 0. A different digest is a
   refused live check, not a pass.

8. **Close and read back.** `close` requires `branch, pull_request, ci, merge, rollback_plan,
   deploy, live_verify` (`409 steps_missing` lists what is absent). The closure names
   `pr_url`, `ci_run_id`, `reviewer_id`, `merge_sha`, `deployed_digest` and `live_check` — paste
   that row in the issue's evidence table. Read it back with `development_projects.get`.

9. **Fault path.** A deploy that must be reverted is an **exercised rollback**: `rollback` with
   `{"development_project_id":"…","deployed_digest":"…","restored_digest":"…",
   "migration_generation":N,"health_status":200,"evidence":["…"]}` — both digests, a different
   restored digest, the applied generation, and `/health` 200 at the restored digest. When the
   deploy's migrations were non-additive, the refusal is `409 schema_restore_required`: the
   restore path runs through the gated ops path, never an application-only rollback.

## Record

| Output | Where |
|---|---|
| The unit | `development_projects.charter` → one `development_project` row |
| Each step | `development_projects.record_step` → one immutable `development_step` row |
| A gate | `development_projects.review_gate` / `.merge_gate` / `.deploy_gate` → an `approval` row |
| A rollback | `development_projects.rollback` → the exercised-rollback evidence |
| The closure | `development_projects.close` → `DevelopmentClosureOut` (PR, CI run, reviewer, merge SHA, digest, live check) |
| Reads | `development_projects.get` / `development_projects.list` |

## Refusals

- **Refused: a step recorded out of order (`step_out_of_order`) or twice
  (`step_already_recorded`); the unit's history is append-only.**
- **Refused: a second unit for the same issue (`issue_already_chartered`), or a charter that
  disagrees with the existing row (`charter_mismatch`).**
- **Refused: `ci_red` — a red CI verdict is never retried into a green one; fix the code and run
  a new CI.**
- **Refused: opening the merge gate before the review approved (`gate_not_approved`), or opening
  a gate the implementer would decide (`reviewer_missing` / the decision writer's refusal).**
- **Refused: a deploy with no tested rollback plan (`rollback_plan_missing`,
  `rollback_untested`), or an application-only rollback for a non-additive migration
  (`schema_restore_required`).**
- **Refused: closing with a step missing (`steps_missing`), or a live check at a digest other
  than the deployed one.**
- Never write `owner`, `approved_by`, `SRO` or an approval decision on the owner's behalf.

## Handoffs

| When | Hand to |
|---|---|
| The review or acceptance needs a fresh verifier | `quality-manager` / `validator-deep` |
| The deployed identity, receipt or cutover | `release-manager` |
| The issue/PR/CI facts drifted from the rows | `programme-sync` |
| The tranche review needs the unit's evidence | `programme-manager` |
| A grant is missing (403) | the operator — one `channels.policy_set` call, a person's act |

## Verify

```bash
test -f .claude/skills/delivery-unit/SKILL.md
conveyor auth token --tenant nonagentic
TOKEN=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["token"])' ~/.conveyor/nonagentic.token)
curl -fsS -X POST "$CONVEYOR_URL/api/actions/development_projects.list/invocations" \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"input":{"issue_ref":"owner/name#N"}}'
```

The live proof is a closure: the unit's `development_projects.get` returns a closure whose
`pr_url`, `ci_run_id`, `reviewer_id`, `merge_sha`, `deployed_digest` and `live_check` name the
real delivery, and `git status --porcelain` in the unit's worktree is empty.
