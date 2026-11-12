# S13 — the trace gate's second limb: a reviewed expectation (2026-11-09)

**Session:** `s13-trace-gate-behaviour` · **Tree:** `$HOME/Movies/conveyor` (shared checkout; no lane
branch of its own) · **Lease:** `00-INDEX.md` §Tree lease, opened 18:15, released with the gate below.

Builds **on** the `DiffMode::{SameSystem,Migration}` shape landed by `s13-trace-gate` (then
`record-n8n-trace-diffs`); nothing of that work was reverted. The four `trace_diff.rs` tests that pin it stay
green and stay theirs.

## Owner ruling this unit encodes

The §5.4 cross-system **payload-hash** rule is thrown out (George, 2026-11-09: *"stupid idea about this hash
stuff — think again"*). The old node returns the external system's row; the conveyor returns its own view, so
an equal hash for the same effect is impossible, and a mask list only grows until the gate cannot fail.

## What landed

| path | change |
|---|---|
| `crates/workflow/src/expect.rs` | **new** — `Expected` (fixture, workflow_key, provenance, steps, end_state), `derive`, `parse`, `to_json_pretty`, `validate`, `compare` (through the one diff path), `end_state_matches` |
| `crates/workflow/src/trace.rs` | `TraceDiffError::ExpectedInvalid` (fixture + detail); the peer's `DiffMode` untouched |
| `crates/workflow/src/lib.rs` | `pub mod expect;` |
| `crates/actions/src/workflow/trace.rs` | re-export `Expected`, `ExpectedStep`, `Provenance` for the CLI |
| `apps/server/src/admin/trace_diff.rs` | `--expect <file>` alternative to `--old`; module doc states that `end_state` is asserted by the fixture test, not the file CLI |
| `apps/server/src/main.rs` | `TraceDiff` gains `--expect` (`--old` is now `required_unless_present`), `--old-workflow` requires `--old` |
| `crates/workflow/tests/trace_expect.rs` | **new** — 6 tests + 1 ignored maintenance helper |
| `fixtures/{a,b,c}.expected.json` | **new** — derived steps + `old_node_id` provenance + authored `end_state` |
| `docs/plans/slices/S13-n8n-engine.md` | §5.4 rewritten to the two-limb gate (path + business result), scope 13 graph workflows, no hash in pass/fail |

## What was run

| # | command | result |
|---|---|---|
| 1 | `rbuild -- test -p conveyor-workflow` | every binary green — **trace_expect 6/6** (1 ignored helper), **trace_diff 11/11** (peer's), **workflow_n8n 9/9**, workflow_trace 7/7, workflow_scheduler 9/9, realtime 10/10 |
| 2 | `rbuild -- clippy -p conveyor-workflow --all-targets -- -D warnings` | exit 0 |
| 3 | `rbuild -- clippy -p conveyor-actions --lib -- -D warnings` | exit 0 |
| 4 | `rbuild -- clippy -p conveyor-server --bins -- -D warnings` | exit 0 |
| 5 | `rustfmt --edition 2024 --check` on the five touched `.rs` files | clean |
| 6 | `rbuild -- run -q -p conveyor-server -- admin trace-diff --expect fixtures/a.expected.json --new <4-step synthetic>` | `diff: zero deltas`, **exit 0** |
| 7 | same, one effect mutated (`requirements.get` → `inference.chat` at position 2) | `delta: position 2 node "brief" effect: "requirements.get" -> "inference.chat"`, **exit 1** (measured, not inferred from the 2nd line) |
| 8 | `the_committed_expectations_match_the_recordings` | each of a/b/c re-derives to the committed steps and provenance |

⚠️ One `conveyor-store` E0425 appeared during step 4's first run (a peer's in-flight write on the shared tree)
and was gone on re-run; it is **not** this unit's and nothing was changed for it.

## Reversal proofs (each was made to fail, then restored)

| claim | reversal | observed |
|---|---|---|
| a payload difference must not be a delta | *none needed* — the test asserts it, and the **same** records under `DiffMode::SameSystem` DO differ | migration: identical; same-system: delta. The zero is the rule, not a blind diff |
| a changed effect is a delta | change `agent.run` → `inference.chat` | one `Mismatch{field:"effect"}` naming both values |
| an extra / missing step fails by name | drop one step; add one | `Missing{node_id:"agent_consult"}` / `Added{node_id:"surprise"}` |
| a recorded node with no flow node is refused | derive against a flow lacking it | error names `"Agent consult"`, `agent_consult` and `fixtures/a.flow.json` |
| an expectation without an end state is invalid | derive (which leaves `end_state` empty) then validate | `ExpectedInvalid` naming the fixture and `end_state` |
| the committed expectation cannot drift | re-derive every fixture and compare steps + provenance | equality for a/b/c |

## What the gate proves — and what it does not

- **Proves:** the re-authored flow executes the same ordered `(node, effect, outcome)` path as the recording,
  and (once the fixture test calls `end_state_matches`) reaches the declared business result.
- **Does not prove:** payload-level equality across systems — by design; `input_hash`/`output_hash` remain in
  the trace for forensics and are compared only between two conveyor traces.
- The three synthetic fixtures' `end_state` is `{"terminal_effect": …}` today. Real re-authored flows
  (steps 14–15) carry business keys (card list/status, decision, message refs); an empty end state is invalid.

## Provenance note (caught by `s13-trace-gate`)

`provenance.projection_sha256` is the canonical-JSON hash of the **projected records**, not of the recorded
`.old.jsonl` bytes (those are pinned by `fixtures/SHA256SUMS`). The field was renamed from `old_sha256` for
exactly this reason: one name must not read as the other artefact.

## Open — the next session of this unit

1. `crates/actions/tests/n8n_migration.rs` (peer's) switches from the direct old-projection diff to
   `Expected::compare` + `end_state_matches` with the facts the run reads.
2. Steps 11–13's re-authoring proof runs against `--expect`; the disposition ledger records business deltas,
   not hash pairs.
