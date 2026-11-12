# S06b — agent task evidence (local)

**Date:** 2026-11-09T04:38Z · **Session:** `conveyor-agent-mcp-interface` · **Tree:** `$HOME/Movies/conveyor`
(no `.git`; the file set below is the whole change) · **Owner:** georgedekker

This is the **local** evidence `docs/plans/slices/S14-cutover.md:27` (P2) names. It proves the conveyor-side
contract over the real `/mcp` HTTP surface with a licence principal. It does **not** run the `agent` app — its plan
records "no code written" and this build has no conveyor registration — and it does **not** execute the agent's
`mcp-import`. Both, and the live task against a deployed conveyor, are S14's gate (`S14-cutover.md:33-34`).

## Files

| path | change |
|---|---|
| `crates/api/tests/mcp_agent.rs` | **new** — the two proofs below |
| `crates/actions/src/control/approvals.rs` | `approvals.request` now assigns the `meta` it builds (`description`, `proposed_answer`, `allowed_decisions`) into `NewApproval`; before this they were discarded, violating S06b §5's table |
| `docs/evidence/S06b-agent-task.md` | this file |

Untouched: every S11 path (`crates/store/src/run.rs`, `procedure*`, `crates/actions/src/procedure/`,
`apps/server/src/worker.rs`, `migrations/0015–0017`), `crates/actions/tests/support/mod.rs`,
`crates/api/tests/machine_login.rs`, migrations, `served-spec.lock.json`.

## What was run

The licence is a stub `POST /license/introspect` (the contract in `crates/auth/src/machine.rs:130,154-171`, the
same shape `machine_login.rs` uses). Sessions are real HTTP against a bound loopback server and a real Postgres
(`support::Pg::fresh`).

| # | command | result |
|---|---|---|
| 1 | `cargo test -p conveyor-auth` | **35 passed**, 0 failed |
| 2 | `cargo test -p conveyor-actions --test mcp_meta` | **1 passed** (`every_tool_carries_its_operation`) |
| 3 | `cargo test -p conveyor-actions approvals` | **0 tests matched** — see "A vacuous gate line" |
| 4 | `cargo test -p conveyor-api --test approval_dual` | **3 passed** |
| 5 | `cargo test -p conveyor-api --test machine_login` | **6 passed** |
| 6 | `cargo test -p conveyor-api --test mcp_agent scopes` | **1 passed** (`scopes_are_the_agent_allowlist_and_admin_and_viewer_bracket_it`) |
| 7 | `cargo test -p conveyor-api --test mcp_agent` | **2 passed** |
| 8 | `cargo clippy --workspace --all-targets -- -D warnings` | **green** (0 warnings) |
| 9 | `cargo run -q -p conveyor-server -- --print-openapi --check-lock` | **exit 0** — the served document still matches its lock |

Line 3 is not evidence; line 4–7 are. The approval action path is covered by `approval_dual` (dual control,
`decided_by_kind` derived from the actor), `machine_login` (agent request/decide over MCP) and `mcp_agent` (the
lifecycle, below).

### The receipt (verbatim, from `--nocapture`)

```
S06B-RECEIPT {"agent_tools":9,"approval_id":"01a1be97-4249-764a-8c44-4909e7bd5124","approval_meta":{"allowed_decisions":["approve","reject"],"description":"Which of the two designs should this follow?","proposed_answer":"the first"},"base_url":"http://127.0.0.1:64914","decided_by_kind":"agent","decision":"approve","project_id":"01a1be97-4232-7b70-ab79-62267ed638fa","run_id":null,"run_id_reason":"no run-creating action is on the agent allowlist (AGENT_TOOLS has runs.list only); S14 P2 owns the agent task's run","via":{"channel":"mcp"}}
```

### Mutation control (the assertion bites)

With `meta: Value::Object(meta)` reverted to an empty map, test 7 fails at the meta assertion and prints `{}`:
`assertion left == right failed: S06b §5: the description goes into approval.meta; {}`. Restored, it passes. The
assertion is therefore load-bearing, not decorative.

## Properties (§7)

| # | property | verdict | evidence |
|---|---|---|---|
| a | tool key: every tool carries `_meta.operation` = its action name | **PROVEN** | `mcp_meta` (1/1); `mcp_agent` reads `_meta.operation` from `tools/list` for every one of the agent's 9 tools and fails on a tool that lacks it. Note: the key rides the **tool definition**, not the call result (the result carries `structuredContent`); that is the shape the agent's importer reads (`mcp_import.rs:199-211`). |
| b | import: the agent's `mcp-import` yields `projects.create`, not `tool.*` | **NOT PROVEN** | the agent's importer is not executed here. (a) proves the conveyor-side precondition: the operation key is an `object.verb`, never absent. Live import remains S14 §7. |
| c | machine auth: a licence for tenant A cannot act for tenant B | **PROVEN** | `machine_login::the_licence_names_the_tenant_even_when_the_principal_also_belongs_to_another` |
| d | JWT rule: a JWT-shaped token is never retried as a licence | **PROVEN** | `machine_login::a_jwt_shaped_token_is_never_retried_as_a_licence` (stub hits = 0) |
| e | attribution: `decided_by_kind='agent'`, `via` names the licence principal | **PROVEN** | `mcp_agent` SQL: `decided_by_kind='agent'`, `via={"channel":"mcp"}` |
| f | escalation: `approvals.request` opens an approval with NULL `run_id` and is decidable | **PROVEN** | `mcp_agent`: pending with `run_id: null`, then `approvals.decide` succeeds |
| g | end to end: create, progress, decide, close; evidence contains run id, project id, one decision | **PARTIAL** | create → `projects.get` + `events.list` → request → decide → `projects.close` → `projects.get` shows `closed`, all over `/mcp` as a licence principal. **`run_id` is `null`**: no run-creating action is on the agent allowlist (`AGENT_TOOLS` has only `runs.list`; `runs.create` needs a card; `cards.create` is outside the allowlist). See "Open". |
| h | no middleman: no `X-MCP-Binding`, no `mcp.mcp.svc` | **PARTIAL** | the local path is `http://127.0.0.1:<ephemeral>` and `support::Mcp` sends exactly `authorization`, `accept`, `content-type`, `mcp-session-id` (`support/mod.rs:643-683`) — no binding header. The deployed path is S14's live gate. |

## Licence

Measured 2026-11-09:

- `conveyor` is a **bundle** key in licences, not a registered app: `seed_apps.py` has no conveyor row, and
  `POST /internal/tenants/{id}/bundle-token?app=conveyor` refuses (`app_not_in_bundle` / `no_app_licence`).
  Registering the app + issuer + grant is a write to a shared service and is not in this unit.
- `origin` carries `lane/agent-bundle-conveyor`, tip `72e67b0 "lane: start"` (2026-11-09 05:08:48 +0100). The owner
  reports the licences update for the agent's direct connection is landing; **at the time of this evidence the
  lane carries no code**, so a real `app_key=conveyor` mint was neither available nor attempted.
- This proof does not depend on it: the server's own introspection path is exercised end to end against a stub.

## Open

1. **`run_id` (S14 P2 vs the agent allowlist).** `S14-cutover.md:27` requires a `run_id` in this evidence. The
   agent cannot create one: `runs.create` needs a card, `cards.create` is outside `AGENT_TOOLS`, and `runs.list`
   is all the allowlist has. Either P2's `run_id` means the agent platform's task run (live, S14), or S06b needs a
   run-creating action — an owner/spec decision. Recorded as `null`; not fabricated.
2. **The vacuous gate line.** `S06b-...md:132` runs `cargo test -p conveyor-actions approvals`, which matches 0
   tests in this tree. Either point it at `crates/api/tests/approval_dual.rs` + `mcp_agent` or delete it; a line
   that cannot fail is not a gate.
3. **The agent app.** Properties (b) and (h)'s deployed half, and the live task, need `agent`'s MCP client and
   its conveyor registration — S14 §7.
