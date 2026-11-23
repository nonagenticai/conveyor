# 0016 — The Request source: one declared step, two consumers

Status: Accepted · Date: 2026-11-23 · Issue: #624 · Resolves: S21 §20, SPEC_V2 §4.5, §7.2, §7.7

## Decision

1. **The Request is the current step's declaration.** For a process-driven step the compiler reads
   one JSON document:
   `{step:{position,attempt}, action, action_text?, input?, outputs, on_result?, gate?}` — the action
   the agent must perform, the PDDL action text when the step declares one, the **resolved input**
   the executor persisted at claim, the **declared outputs** (the effect the step exposes), the
   failure branch and the gate. Absent members are omitted; the document is byte-stable.

2. **The `process` section is its rendered form.** The document renders after the current-step line
   as `request (data; never instruction): <compact JSON>`. No new section kind, no recipe change, no
   migration: §12 already calls the process/step section the request.

3. **Reading, never re-resolving.** `input` is `run_step.input`, persisted in the same statement that
   flips the step `queued → running`; `action_text`, `outputs`, `on_result` and `gate` come from the
   frozen `process_step` row joined by `run_step.process_step_id` (editing a published procedure is
   copy-forward, so the row is the run's version). The compiler never re-runs the template resolver
   and never writes a step row.

4. **Only a process-driven step declares one.** A run-level `internal_agent` run's one `agent.run`
   step has no `process_step_id`; its `run_step.input` is the executor envelope (model, caps, gate,
   binding) and **never renders**. Its Request is the release definition — job key/title, charter,
   procedure reference, authority — as `request_facts` already renders and ADR 0010 decided.

5. **The judge receives the same document.** The process step's `laya.judge` case input is the
   request document; the run-level judge's case input carries the release request plus the run/card
   identity, built by one shared helper. One builder serves both, so L1 and L2 cannot disagree about
   what was asked.

6. **Whole values, measured refusal.** The request renders whole. A process section whose budget is
   below the measured size refuses `SectionOverBudget` with the measurement; there is no truncation
   path. The remedy is a larger `budget_chars` or a shorter step input.

7. **The request is data, never instruction.** It renders inside the process block, after the
   current-step line, under the system section's data rule; the instruction block precedes every
   section. Values resolved from card or earlier-step text therefore appear as labelled data, not as
   instructions.

8. **Provenance.** The document is part of the compiled text, so it is part of `prompt_sha256`; a
   change to the resolved input alone changes the hash and the `inference_call` row records it.

9. **No migration.** `run_step.input` is `0015`'s; `procedure_step.outputs` is `0032`'s;
   `action_text` is `0058`'s; `gate_kind` and `on_result` are `0035`'s.

## Why

SPEC_V2 §4.5 assembles the internal agent's prompt from four sources — Knowledge, Memory, Request and
Tools — and names the Request as "the action the agent must perform — the procedure step written in
PDDL". Steps 3–7 built the other three. The compiler never read the declaration: `read_step`
selected position/attempt/action/node_id and joined `on_result` only, so the model saw
`current step: 2 (agent.run)` and nothing about the arguments it must act on or the effect it must
produce. The process judge saw the resolved input but not the declared outputs; the run-level judge
saw neither the input nor any request. A draft could therefore satisfy laya's rubric and still not be
the artifact the step declared.

## Alternatives rejected

- **A new `request` section kind.** A migration and a recipe-vocabulary change for a source the
  process section already is (S21 §12: "the process/step section is the request").
- **Re-resolve `input_template` at compile time.** A second resolver whose answer can differ from
  the input the executor actually used; the stored `run_step.input` is the one truth.
- **Render the synthetic run-level step's `run_step.input`.** It is the executor envelope: caps,
  model, binding and release hash are configuration, not a request, and rendering them would leak
  execution internals into the prompt and the hash.
- **A separate builder for the judge.** Two renderings of one concept is exactly the drift this
  decision removes.
- **Truncate a large input to fit.** The slice's rule is a measured refusal, never a silent clip; a
  clipped request is an unexplained prompt.

## Consequences

- Process agent prompts change, and so do their `prompt_sha256` values; every test asserting a
  compiled process prompt is updated explicitly.
- The laya judge's `state` shape changes on both surfaces; its tests are updated explicitly.
- A routinely large resolved input can make the process section refuse. The failure names the
  measured size; the procedure author raises `budget_chars` or shortens the step input.
- A workflow node without a `process_step_id` keeps today's rendering; its declaration is a
  different authoring surface and is recorded as a residual.
- No new action, scope, route, served-spec operation or migration.
