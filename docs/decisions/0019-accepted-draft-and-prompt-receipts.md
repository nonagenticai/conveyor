# 0019 — The accepted draft is applied and every call is receipted

Status: Accepted · Date: 2026-11-24 · Issue: #697 · Resolves: S21 §23, rework `v1-p77-1`,
`v1-p77-2`; implements SPEC_V2 §5.1, §7.1, §7.3, §7.4, §7.7

## Decision

1. **An accepted L1 gate applies its draft when — and only when — the policy says so.** The
   effective `gate.precheck` policy may declare `meta.on_accept = "apply"`; absent or `"park"` is
   the default and keeps today's behaviour exactly (the run succeeds, the draft stays unattached).
   This is the owner's decision of 2026-11-24: forward-only, opt-in, no tenant's behaviour changes
   because the unit shipped.

2. **One apply gate, three callers.** The apply is `crate::ladder::apply_draft_tx` — the same
   function `ladder.apply_draft` and the `run.resume` worker call. The executor calls it inside its
   finish transaction, with the recorded L1 decision, after `finish_step_tx` and before
   `finish_succeeded`. It never writes a decision, never re-generates, and never calls the model
   again; the decision's `meta.draft_document_id`, `prompt_sha256` and `run_id` bind it to this
   document.

3. **The policy value is validated at authoring.** `decision_policies.create|update` accept only
   `on_accept ∈ {park, apply}` (or absent) and refuse anything else `422 invalid_policy_meta`
   before any write. A stored policy that cannot be honoured is not a runtime surprise.

4. **Only a carded run applies.** A draft attaches to a card; a cardless accepted run records
   `apply_skipped: no_card` in its judge, attaches nothing and still succeeds. The work-package
   level is `card → run → step` (§5.1); a project-level run has no work package to receive the
   draft.

5. **A declared apply that cannot apply fails the run.** If `apply_draft_tx` refuses (a broken
   decision binding is a runtime inconsistency, not a user error), the run finishes `failed` with
   `draft_apply_invalid`, the same class the resume worker uses. A half-applied draft is never left
   behind.

6. **No model call without compiled text and a 64-hex hash.** An `internal_agent` that is
   `active` must name a `prompt_ref` that resolves to an active prompt in the tenant:
   `domain::capability::validate_agent_config` refuses `None` for an active agent;
   `capability_agents.create|update` answer `422 prompt_ref_required`; readiness returns
   `PromptNotConfigured`; `agent.run` refuses `409 capability_not_ready` naming the agent before
   any inference. Every recorded `inference_call` for an internal agent carries a `prompt_sha256`
   matching `^[0-9a-f]{64}$`.

7. **The recipe's character ceiling is authored with the recipe.** `prompt_recipes.create` gains
   `budget_chars: Option<u32>`; `0` is `422`; `NewPrompt` and the INSERT write it, and a new
   version carries a new ceiling. `compile` already refuses over budget with the measured size and
   never truncates — this decision makes that refusal reachable from an authored recipe.

8. **No migration.** `decision_policy.meta` is existing jsonb and `prompt.budget_chars` exists
   since migration `0031`. The unit adds no column and no table.

## Why

SPEC_V2 §7.1: "L2 has no path to a state transition. A generated output is stored as a draft
artefact; applying it is an action, and the action's gate is L1/L3." §7.3 names the gate pre-check as
`pass | review | refuse`; a pass that leaves the draft unattached is not an application, it is a
no-op that only a person's separate approval can complete. ADR 0011:77 rejected auto-apply as "a
policy choice, not this unit" and the explicit gate was kept — the owner has now chosen: park by
default, policy opts in.

SPEC_V2 §7.7: "a prompt is compiled at run time from the run's sources … with character budgets, a
measured refusal over budget, and a recorded `prompt_sha256`. Missing required source → `409
capability_not_ready` naming the pin and section, **before** a model call." Two holes remain in the
code: an active agent without a prompt passes validation, readiness and `agent.run`, so a call can
be recorded with a NULL hash; and the recipe-level ceiling has a reader but no writer. Both make a
claim in the spec unreachable rather than false.

## Alternatives rejected

- **Apply on any accepted L1 decision without a policy declaration.** It would change every
  tenant's gate behaviour at once; an accepted quality score is evidence, not a standing licence to
  attach a document.
- **A new `apply` action or a second apply path.** Three callers of one function is the invariant
  that keeps the gate honest; a second path drifts from the checks (`v1-w1c-2`'s class of defect).
- **Enforcing the prompt only at readiness.** Readiness is advisory for bound runs; the action and
  the execution path must refuse, or a run created before readiness passes can still call.
- **A default prompt when `prompt_ref` is absent.** §7.7 compiles from the run's sources; a built-in
  default would be a second compiler with no recipe, no budget and no source provenance.
- **A separate action to set the recipe budget.** The recipe is versioned by `create`; a separate
  writer would split one version's fields across two actions and two transactions.

## Consequences

- The executor's accepted branch gains one call; its judge doc already carries `decision_id` and
  gains `apply_skipped` for the cardless case.
- `capability_agents.create|update` may now refuse an active agent with no prompt. Fixtures that
  were active without one are rewritten, never relaxed; a seeded active agent without a prompt is a
  seed defect and is fixed in the seed.
- Readiness gains a blocker variant; the bound path and the unique-active-agent path share it.
- `prompt_recipes.create`'s schema moves; the served-spec lock moves and no action is added.
- `v1-p77-1` and `v1-p77-2` clear when the unit's acceptance passes.
