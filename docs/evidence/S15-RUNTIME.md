# S15 steps 6–13 — knowledge, memory, wiring, readiness — evidence

Measured 2026-11-09 on mini3; every Rust command through `rbuild` on m1 (CONTRACT §16).

## Harness — real `strata_rag` (not a stub)

`compose.yaml` profile `strata`: `strata-db` (pgvector pg18, :5440) + `strata-rag` (built from
`../strata_rag`, :8092). `scripts/strata/providers.toml` — dev defaults, embed/rerank →
`mlx-api-text.example.com:9131`, no chat.

```
docker compose --profile strata up -d --wait strata-db strata-rag   # exit 0
GET /health -> {"db":"ok","embed":"ok","rerank":"ok","chat":"unconfigured","laya":"unconfigured"}
POST /api/stores {kind:leaf}            -> 200 id
POST /api/stores/{id}/documents         -> 201, chunk_count 1
POST /api/query  leaf_ids=[id], metadata={capability:design} -> 1 tenant hit
```

A parentless `router` cannot be ingested into; the tenant memory store is a parentless **leaf**.

The consolidation sequence was measured against the real service: two runs of one `source_id` →
query returns 2 → `DELETE /api/sources/<source>?except_run=run-2` → `{"deleted":1,"kept":1}` →
query returns 1. ⚠️ A prune whose `except_run` carries **no** document is refused (`empty_sync_run`,
`deleted:0`); the job ingests the new digest first, which is what makes the guard pass.

## Built and proven

| command | result |
|---|---|
| `rbuild -- test -p conveyor-domain memory` | **7 passed / 0 failed** |
| `rbuild -- test -p conveyor-store --test memory` | **3 passed / 0 failed** (two-tenant RLS; same-card supersede; digest covers rows once) |
| `rbuild -- test -p conveyor-actions --test run_capability` | **3 passed / 0 failed** (no_active_agent → zero run/run_step rows; unknown_action; ready run records capability/agent ids) |
| `rbuild -- test -p conveyor-server --test memory_consolidate` | **1 passed / 0 failed** (digest header, stamps, prune `except_run`, replay no-op) |
| `rbuild -- test -p conveyor-actions --test procedure_start` | 6 passed / 0 failed |
| `rbuild -- test -p conveyor-actions --test procedure_step --test procedure_cancel` | 14 + 2 passed / 0 failed |
| `rbuild -- test -p conveyor-server --test run_retry` | 4 passed / 0 failed |
| `rbuild -- test -p conveyor-server --test prune` | 13 passed / 0 failed |
| `rbuild -- check -p conveyor-server -p conveyor-store` | exit 0 |
| `rbuild -- clippy -p conveyor-domain --all-targets -- -D warnings` | exit 0 |
| `rbuild -- clippy -p conveyor-actions -p conveyor-server --lib -- -D warnings` | exit 0 |
| `rbuild -- test -p conveyor-actions --test registry` | **8 passed / 0 failed** (EXPECTED 243, internal/external 230/13, AGENT_TOOLS 36) |
| `rbuild -- test -p conveyor-actions --test parity_http_mcp` | **4 passed / 0 failed** — every one of 243 actions on HTTP+MCP |
| `rbuild -- test -p conveyor-api --lib` | 29 passed / 0 failed (registry 243 + every action documented) |
| `rbuild -- test -p conveyor-api --test portfolio_roundtrip` | 2 passed / 0 failed (scope count 63) |
| `rbuild -- run -p conveyor-server -- --print-openapi --check-lock` | **exit 0 at 264 ops**, sha256 `9d1150b19c164aee…` |

⚠️ `--write-lock` writes on **m1**; the file must be copied back explicitly (`rsync m1:<slot>/src/served-spec.lock.json`). `rbuild --copy-back <file>` appends `/` and expects a directory — it failed for a single file.
⚠️ Workspace-wide `fmt`/`clippy` remained red on peer files during this pass (`apps/server/tests/common/mod.rs`, `eval_*`, `store/src/seed.rs`, `crates/store/tests/capability.rs`, `apps/server/src/eval_runner.rs`) and, at one point, `crates/auth/src/authn.rs` (web-ui RBAC mid-edit). None is in this lane.


Landed: `domain/src/memory.rs`, `store/src/memory.rs`, `actions/src/integrations/strata.rs`,
`actions/src/procedure/runs.rs` (resolution + `409 capability_not_ready`), `store/src/run.rs`
(terminal-transition enqueue), `apps/server/src/memory_consolidate.rs` + worker arm,
`actions/src/board/events.rs` 26 → 28 (`memory.recorded`, `knowledge.bound`),
`crates/actions/tests/run_capability.rs`, `apps/server/tests/memory_consolidate.rs`,
`crates/store/tests/memory.rs`, `compose.yaml`, `scripts/strata/**`.

## Registered

`crates/actions/src/knowledge/query.rs` (`knowledge.query`) and `crates/actions/src/memory/`
(`memory.remember`, `memory.recall`) — 243 actions · 63 scopes · 264 served-spec ops. `AGENT_TOOLS`
36: `knowledge.query` and `memory.recall` added; the writes stay off the global list and are
reachable only through a run's `allowed_actions` (CONTRACT §17).

## Open

- `capabilities.get` `{ready, blocking[]}` and the `capability_agents` caps refusal wait on the S15
  owner's step-5 handlers (not built; it keeps step 5).
- The catalogue → capability data move is unproven on a pre-existing DB (fresh-DB harness only).
