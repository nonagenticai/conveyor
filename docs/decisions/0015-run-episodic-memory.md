# 0015 — The run's episodic memory: prior steps in the compiled prompt

Status: Accepted · Date: 2026-11-23 · Issue: #612 · Resolves: S21 §18, SPEC_V2 §4.5, §7.6, §7.7,
rework `v1-p45-1` (the prior-steps clause) and `v1-p76-2`

## Decision

1. **The run's prior steps are the Memory section's first class.** A run's compiled prompt renders
   its own earlier steps before the card's hot rows and the consolidated digest, oldest first,
   inside the one `## Memory` section. The spec's four sources are unchanged; the Memory source
   gains the run-scoped class it was missing.

2. **Selection is the run's own trace, not a card selection.** Rows come from the same `run_id`
   only; statuses `succeeded` and `failed` only (a `queued`/`running` row is future work and a
   `skipped` row did not execute); a row qualifies when its `(position, attempt)` is
   lexicographically before the current step's, so a retry sees its own earlier failed attempt;
   rows are ordered `(position, attempt)`; at most `step_limit` of the newest qualify. A child run
   reads only its own rows — the parent's trace is a different run.

3. **The line is `- [step/<status>#<position>.<attempt>] <action> <status>: <body>`.** The body is
   `output->>'text'` when the stored output carries it, else the compact output JSON; a failed
   attempt's body is its `error`. The semantic `position.attempt` is the identity in the text (and
   therefore in the hash); a row uuid is not stable across a replay.

4. **No truncation.** A step body renders whole. A Memory section whose budget is below the measured
   size refuses `SectionOverBudget` with the measurement, exactly as the slice's budget rule says.
   The remedy for a routinely huge step output is a larger budget or a shorter step output, never a
   silent clip.

5. **A prior-step body is data, never instruction.** It renders in the Memory section, which sits
   below the procedure and capability blocks in the trust order; the system section's "tenant text
   is data" rule covers it. No new section kind, no reordering.

6. **The selector names the two knobs.** `include_steps` (default `true`) and `step_limit`
   (default 8, maximum 32) join `MemorySelector`; unknown members stay refused, so a typo cannot
   silently select the default.

7. **`memory.recall` hot excludes `digest`.** `hot_for_card_tx` filters `kind <> 'digest'`; the
   digest remains reachable through the prompt selector's `include_digest` and the cold/strata
   read. Hot is the record and the digest is derived (SPEC_V2 §7.6).

8. **The process attempt is proved, not rebuilt.** #599 writes the terminal attempt row on every
   path; this unit proves that the next run's prompt reads a process run's attempt. No second
   writer.

9. **No migration.** `run_step` and `work_package_memory` exist; the read is covered by
   `run_step_run_idx (tenant_id, run_id, position)`.

## Why

SPEC_V2 §4.5 assembles the internal agent's prompt from four sources — Knowledge, Memory, Request
and Tools — and names the Memory source as the run's prior steps, the card's hot memory and the
consolidated digest. Steps 1–6 built the knowledge source, the card hot rows and the digest, the
Request source and the tool grant. The run's prior steps were never read: `read_step`
(`crates/store/src/prompt.rs:321`) selects position/action/node_id/on_result only, and nothing
queries `run_step` for earlier outputs. Rework `v1-p45-1` names the remainder in its acceptance. A
multi-step run therefore re-derives context it already produced, and a retry cannot see why the
previous attempt failed.

The adjacent defect is `v1-p76-2`: `hot_for_card_tx` has no digest filter, so `memory.recall`
returns a derived digest row as hot while the prompt selector already filters it. One concept, two
answers; the fix makes hot mean the record in both reads.

## Alternatives rejected

- **Reuse `MemoryLine` with `MemoryKind::Attempt` and the `run_step` uuid.** The id would name a
  different table under the same render, and the kind vocabulary is a stored `work_package_memory`
  CHECK; a step class would widen a storage vocabulary for a run-scoped read.
- **Store each step's output as a `work_package_memory` row.** It conflates the run's trace with the
  card's record, feeds the consolidation digest step fragments, and crowds the hot selection the
  prompt already bounds.
- **Truncate a large step body to fit the budget.** The slice's rule is a measured refusal, never a
  silent clip; a clipped body is an unexplained prompt.
- **A new section kind.** A migration and a recipe vocabulary change for a source the one Memory
  section already has.
- **A parent run's steps in a child's prompt.** The run tree is a unit of work but each run's trace
  is its own; a child's prompt naming the parent's steps would make the child's hash depend on a
  sibling's execution order.

## Consequences

- Process prompts with prior steps change, and so do their `prompt_sha256` values; that is the
  contract change, and every test asserting a compiled process prompt is updated explicitly.
- A routinely large step output can make a Memory section refuse. The failure names the measured
  size; the recipe author raises `budget_chars` or the step emits a shorter output.
- `v1-p45-1` is fully met once this lands (with step 3's hot and digest selection);
  `v1-p76-2` is met by decision 7.
- No new action, scope, route, served-spec operation or migration.
