# 0013 — The work package's run: the card names the binding, the terminal run records its memory

Status: Accepted · Date: 2026-11-22 · Issue: #599 · Resolves: S21 §17, SPEC_V2 §4.4, §5.1, §6.6.4,
§7.6, rework `v1-p66-1` (the binding clause)

## Decision

1. **The card names its job binding; a run inherits it.** `cards.create` and `cards.update` accept
   `job_binding_id`. The id must resolve inside the caller's tenant and be `active` — a draft,
   retired or foreign binding is `422 job_binding_unusable` and is never written. `CardOut` exposes
   the column, so the assignment is readable where the card is read. `runs.create` resolves the
   card's binding when its request omits one; an explicit binding that differs from the card's is
   `422 card_binding_mismatch`; a card with no binding keeps today's plain-run path. The retired
   binding and stale-pin checks remain in `insert_run_tx`, the one writer.

2. **`cards.duplicate` does not copy the binding.** A binding is an assignment, not content: a
   duplicated card is unassigned until someone names a job for it.

3. **A terminal root run writes one work-package memory row.** `store::run`'s terminal hook
   (`enqueue_memory_consolidation`) becomes `record_run_memory`: when a run reaches a terminal
   state, is card-scoped and is a root (`parent_run_id IS NULL`), it inserts exactly one
   `work_package_memory` row through the existing `store::memory::insert_memory_tx` — `kind=attempt`,
   `source=system`, `run_id` set, the body naming the attempt and its outcome, `meta` carrying the
   status and error class — and then enqueues the existing `memory.consolidate` job. The row and the
   terminal status commit in **one transaction**: a rolled-back finish leaves neither.

4. **A child run and a card-less run write no memory.** A child shares its parent's card and its
   outcome is part of the parent's attempt; writing one row per child would crowd the card's hot
   rows (the prompt selector reads the newest eight) with fragments of one work package. A
   card-less run has no work package.

5. **The runtime's attempt row is the second sanctioned writer, not an ad-hoc one.** CONTRACT §17
   said `memory.remember` is the only *action* that writes `work_package_memory`, with the
   consolidation job appending the digest. This decision adds one sanctioned runtime writer: the
   terminal transition's `attempt` row. It is the system's record of what the run did, written
   through the same store function; the agent's learned facts still go through `memory.remember`
   (the copilot and an `agent.run` step), and nothing else writes SQL.

6. **No migration.** `card.job_binding_id` exists (`migrations/0040_organisation.sql`),
   `work_package_memory` and its kinds exist, and the terminal transitions already exist.

7. **The budget is not this decision.** `v1-p75-1`/`v1-w4a-1` are resolved on the `#526` lane
   (commit `8f1e5770`, the invoke precheck and broker spend gate). The token ceiling
   (`ReleaseLimits.max_tokens`, recorded-only) is that gate's residual. A second budget mechanism
   in the loop would be a duplicate writer for one policy, so this unit does not add one.

## Why

Steps 3–5 built the compiled prompt, the `internal_agent` executor and the applying gate, but the
work package itself was still inert. `card.job_binding_id` existed in the schema and no Rust code
read or wrote it, so a card could not name the job that executes it and a run was bound only when
its caller happened to pass a binding — the rework finding `v1-p66-1` describes exactly this. The
executor also never called any memory writer while `memory.consolidate` was already enqueued on
every terminal transition, so the digest job consolidated nothing and the prompt's memory section
was always empty. SPEC_V2 §4.4 ("a card names a binding"), §5.1 (card → run), §6.6.4 ("memory is
written at handoff") and §7.6 (hot rows are the record; the digest is derived) already describe the
target; this unit makes the implementation match those sentences.

## Alternatives rejected

- **A new `cards.assign` action.** The finding's acceptance admits "a binding on create"; extending
  the existing create/update inputs is a smaller surface (no new action, no new scope) and still
  gives the card an assignment. A dedicated handoff verb can come with the card-driven entry unit.
- **Auto-start a run on a claim or lane move.** The spec says a handoff is a card; it does not say a
  lane move starts execution. Auto-start would change every board's semantics and is its own unit.
- **Write memory through `memory.remember` from the executor.** The action requires an invoking
  principal and a separate transaction; the run's terminal transition and its record would then be
  two commits, and a crash between them would lose the row for a run that already ended. The store
  writer inside the terminal transaction is atomic and is the same one writer.
- **Write the row only for `internal_agent` runs.** The terminal hook is per-run, not per-executor;
  a refused or failed run is also an attempt the next run on the card should see. The hook stays in
  the store, where every terminal path meets.
- **One row per child run.** See decision 4.
- **Enforce budgets here.** Duplicate of the #526 lane's resolved package.

## Consequences

- A card with a binding can no longer be "bound to" a different job by a `runs.create` caller; the
  card is the assignment. A caller that previously overrode the card's binding gets a typed
  `422 card_binding_mismatch`.
- `cards.create`/`cards.update`/`CardOut` and their served schemas gain a field, so the served-spec
  lock and the action dump move; no action or scope count changes.
- Every terminal root run of a card adds one hot row. The existing consolidation job (per card)
  drains them; the prompt selector reads the newest eight, so an old row ages out of a prompt
  before it ages out of the ledger.
- The `v1-p66-1` acceptance's first clause (a binding on create, exposed, reaching the run) is met;
  its lane-refusal clause ("moving a delivery card into a work lane without a binding is refused
  422") needs a delivery-card marker that does not exist and stays open.
- No new action, scope, route or migration.
