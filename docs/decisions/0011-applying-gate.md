# 0011 — The applying gate: a handover parks the run, and a person's approval applies the draft

Status: Accepted · Date: 2026-11-22 · Issue: #590 · Resolves: S21 §16, SPEC_V2 §5.1, §7.1, §7.4,
rework `v1-w1c-2:T2`

## Decision

1. **A handover parks the run; it does not fail it.** The `internal_agent`'s `gate.precheck`
   handover opens a card-scoped `executor` approval in the same transaction that finishes the step:
   `running → waiting_approval` through `open_gate_with_meta_tx`, the step `succeeded` with the
   draft output and its judge record, the draft unattached. The run holds `uq_run_live` on purpose —
   it is live work waiting on a person, not a terminal failure.

2. **The approval names the draft.** `approval.meta` carries `{run_id, gate, draft_document_id,
   prompt_sha256, position: 1, attempt: 1, threshold, policy_id, laya_config_id}`. The approval
   inherits the run's `card_id` and `project_id`; no new column, no migration.

3. **The L3 decision is a fact of the approval, recorded in its transaction.** An `executor` gate
   is a person's act: the one approval writer refuses a non-human `decided_by_kind`. On approval,
   `approvals.decide` records one ladder decision — `rung=human`, `kind=gate.draft_apply`,
   `answer="approved"`, `outcome=accepted`, `approval_id`, the run's `card_id`, and `meta` naming
   the same draft identity — through the ladder's one writer. The decision and the
   `approval_decision` commit together, so an approval can never exist without its ladder row.

4. **`ladder.record` refuses a forged human rung; `ladder.apply_draft` accepts only a decision
   bound to the draft.** `record_decision_tx` requires `actor_kind="user"` for `rung=human`, and an
   approved approval with an approve `approval_decision` when one is cited. `apply_draft` requires
   the document to be a generated draft, the decision to be accepted and non-rule, its kind to be
   `gate.precheck` (L1) or `gate.draft_apply` (L3), and its `meta` to name the exact
   `draft_document_id`, with `prompt_sha256` and `run_id` matching when the document carries them.
   A generic accepted decision on the card no longer applies anything.

5. **The resume is one idempotent job.** Approval settles `waiting_approval → running` and enqueues
   `run.resume` (`{run_id, approval_id, decision_id}`, dedupe
   `run.resume:{run_id}:{approval_id}`). The worker applies the draft through the same
   `apply_draft_tx` the action uses and finishes the run `succeeded` with one `run.succeeded`. It
   never calls the model, so the draft is not re-generated or re-charged. A terminal run or a
   non-approved approval is a quiet no-op (a reclaimed job is safe); inconsistent stored state
   fails the run `draft_apply_invalid`, releasing `uq_run_live`.

6. **Rejection and expiry leave the draft unattached.** A rejection cancels the run through the
   existing generic approval path; expiry cancels a still-`waiting_approval` run with
   `approval_expired`. Neither records a ladder decision: the `approval_decision` (or the absence of
   one) is the record.

7. **No migration.** `approval.meta` and `decision.meta` are existing jsonb; `approval.kind` and
   `job.kind` are free text; the run states and the `waiting_approval → running` edge already exist
   in the domain table.

## Why

Step 4 generated the draft but could not deliver it through a human gate: `open_handover` opened the
approval in its own transaction and the finish block then called `fail_run_tx`, which force-fails any
non-terminal run. The run ended `failed` with a pending approval, and `approvals.decide` could not
move `failed → running`. At the same time, `ladder.apply_draft` admitted any accepted non-rule
decision that named the card, so a routine laya classification could apply a patch and a service
principal could record `rung=human` against a pending approval (`v1-w1c-2:T2`). SPEC_V2 §5.1 already
has the `waiting_approval → running` edge, §7.1 makes L3 a human approval, and §7.4 makes an action
with a gate the only way a generated draft becomes applicable. This unit makes the implementation
match those sentences.

## Alternatives rejected

- **Apply the draft synchronously inside `approvals.decide`.** It would make the approval action do
  the apply and finish, with no retry seam for a database fault and no place for a later non-attach
  effect. The job keeps the approval decision atomic and the application retryable.
- **Leave the run `failed` and apply through a manual `ladder.apply_draft` call.** The run would
  never return to `running`/`succeeded`, so the work package's trace would say "failed" for work a
  person accepted. The state machine has the right path; use it.
- **Record the L3 decision in the resume worker.** The worker runs as the service actor; it cannot
  honestly claim a human rung. The decision belongs to the approval transaction, where the person's
  identity and the approved `approval_decision` are present.
- **A `decision_document` link table.** The draft identity is already in both rows' jsonb and in the
  document's meta; a third store would drift. The apply check reads the existing columns.
- **Re-generate on resume.** The step already holds the draft, the inference call and the hash;
  re-running would buy the model twice and could apply a different draft than the one approved.
- **Auto-apply an L1-accepted draft.** A policy choice, not this unit; the explicit apply gate stays.

## Consequences

- A parked run is a live run: one per card (`uq_run_live`), visible in the run list as
  `waiting_approval`, and cancellable. Expiry is the existing `approval.expire` path.
- `run.resume` is a job kind like `run.execute`; it is not added to `ANDON_ALLOWED_KINDS`, so an
  andon defers the application just as it defers new work.
- `ladder.apply_draft`'s contract is narrower; a caller that previously passed any accepted card
  decision must now name the draft in the decision's `meta`. The tests that encoded the old contract
  are rewritten, not relaxed.
- No new action, scope or route. The served-spec lock moves only because the action's descriptions
  change; the registry count does not.
- Rework register package `v1-w1c-2:T2` clears when the findings ledger marks the finding resolved;
  the unit's acceptance command proves the package is gone.
