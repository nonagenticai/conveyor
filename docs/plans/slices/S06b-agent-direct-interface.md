---
slice: S06b
title: Agent direct interface — the conveyor's own MCP surface
status: planned
depends_on: [S04, S05, S06]
spec_sections: ["§4.5", "§8 S6-α", "§11 R11"]
adds_tables: []
adds_actions: [approvals.request]
adds_events: []
adds_modules: [mcp, auth]
parity_ids: PENDING-S0
---

# S06b — Agent direct interface

Programme project: S06b · issues none yet

Supersedes `S06a-mcp-server-channel.md` (SPEC D18 → **D19**, 2026-11-08). S06a's HMAC channel, text grammar and
shared secret are not built and are not carried forward.

## 1. Objective

The `agent` app operates the conveyor over the conveyor's **own** `/mcp` surface. At the end, one real agent task
creates a project, reads its progress, answers an approval and closes the project — with the decision recorded as
`decided_by_kind='agent'` — and no component called `nonagenticai_mcp_server` is in the path.

## 2. Starting state

- S04: `/mcp` (rmcp streamable HTTP) serves the same action registry as HTTP, parity-tested
  (`crates/actions/tests/parity_http_mcp.rs`).
- S05: `projects.create|get|close`, `cards.get`. S06: `runs.list`, `approvals.list|decide`, `events.list`.
- S02: JWT/dev authentication, membership resolution, identity.

## 3. Scope

**IN** — the four gaps that stop the existing `/mcp` from being consumable by `agent`: the tool key, machine
authentication, principal attribution, and an approval-request action. Also the agent-side registration contract
(implemented in `agent`, gated here).

**OUT** — the `fr` toolkit in `nonagenticai_mcp_server` (retires with fr at S14); the agent's task/elicitation
extension for *external* MCP servers (**S06c**; v1 polls until it lands); the conveyor's copilot surface (S10); every
other fr caller (S13). No HMAC channel is built.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `crates/actions/src/mcp.rs` | `tool_json`, the `_meta` block | the tool shape | — |
| `crates/auth/src/{authn,principal,licence}.rs` | `authenticate`, `Principal`, `Session`, the entitlement verdict | the fail-closed rule and the JWT-shape rule | the entitlement check's coupling to requests |
| `crates/store/src/approval.rs` | `insert_approval_tx` (nullable `run_id`), `open_gate_tx` | `insert_approval_tx` | `open_gate_tx` (requires a running run) |
| `crates/actions/src/mcp.rs`, `crates/api/src/actions.rs` | the three `actor_kind: "user"` sites | the one field | the literal |
| `agent/crates/agent-api/src/connectors/routes/mcp_import.rs` | `_meta.operation`, `inputSchema`, `outputSchema`, `readOnlyHint` | the import contract | the agent's internals |
| `agent/crates/agent-api/src/mcp_client/external.rs` | the module doc | "no tasks on an external endpoint" | the task path |
| `agent/crates/agent-api/migrations/0093_connector_instances.sql` | `connector_instance`, `connector_credential` | per-tenant credential custody | a new credential store |

Pinned: `agent` @ `83b8de7`, `nonagenticai_mcp_server` @ `dd2cabf`. Re-capture both before building.

## 5. Deliverables

```
crates/actions/src/mcp.rs                  _meta.operation; actor_kind from the principal (2 sites)
crates/auth/src/machine.rs                 licence bearer → tenant + agent principal (new)
crates/auth/src/principal.rs               Principal.kind ∈ human | agent | service
crates/auth/src/authn.rs                   resolve a machine principal before the membership check
crates/actions/src/control/approvals.rs    approvals.request (insert_approval_tx)
crates/api/src/actions.rs                  actor_kind from the principal (1 site)
crates/api/tests/mcp_agent.rs              end-to-end over /mcp
crates/actions/tests/mcp_meta.rs           every tool names its operation
docs/evidence/S06b-agent-task.md           the one real task
```

**`_meta.operation`.** The agent names an imported op from `_meta.operation` when it is a valid `object.verb`, and
otherwise from the tool name as `tool.<snake>` (`mcp_import.rs:199-211`). Every conveyor tool must carry
`"operation": spec.name` next to `x-action-name`.

**Machine authentication.** `authenticate` today accepts dev-auth or an OIDC JWT and then requires exactly one
membership (`authn.rs:41-65`). Add a licence branch: a non-JWT bearer is verified against the licences service
(`GET /licenses/active?tenant_id=&app_key=conveyor` is not it — the entitlement sync is S02's; this is
introspection of the presented token) behind a short-TTL cache, resolving `(tenant, app_key, subject)`. A
JWT-shaped token that fails JWT verification is **never** retried as a licence token. Resolve the subject to an
`app_user(kind='agent')` and a membership scoped to the tool list below.

**`approvals.request`.**

| field | type | rule |
|---|---|---|
| `project_id` | uuid | required unless `card_id` is given |
| `card_id` | uuid? | optional |
| `run_id` | uuid? | optional; must belong to the same tenant |
| `gate` | text | required, from `catalogue_entry(kind='gate')` |
| `kind` | text | required; the approval's vocabulary |
| `description`, `proposed_answer` | text | into `meta` |
| `allowed_decisions`, `required_decisions`, `expires_at` | jsonb/int/timestamptz? | optional |

Output `ApprovalOut {id, status, gate, kind, project_id, card_id, run_id, expires_at, version}`. Uses
`insert_approval_tx`; `open_gate_tx` is wrong here because it requires a `running` run.

**Licence verification is on the request path, so it is cached and fail-closed.** A verified licence is held for
the licence's TTL, capped at 60 s; an unreachable licences service refuses (`503 licence_unavailable`, no
principal) and never admits. The cache is keyed by the token's hash, never by tenant, so a revoked token cannot
ride another tenant's entry.

> **S06d supersedes the list mechanism, not the capability.** The names below became the **MCP
> channel's compiled default** in `crates/actions/src/channel.rs` (`MCP_DEFAULT_ACTIONS`); the
> tenant-admin may now add or remove them through `channels.policy_set`. The machine floor
> (`ActionSpec::machine_forbidden`) still refuses every eval sign-off, held-out read and
> administrative action, so a stored grant can never widen what this section calls the wall.

**Tool set exposed to the agent principal:** `projects.create|get|close` (S06c adds `projects.execute`),
`runs.list|create|cancel|finish`, `approvals.list|decide|request`, `boards.get`, `cards.create|get`,
`events.list`, `procedure_runs.start` and `procedures.create|get|list|update|publish|retire` (2026-11-09:
the agent platform runs project work and installs the improvements it designs; the membership role is the
synthetic tenant-admin `kata_executor`, and the allowlist remains the authorization). The list is
`AGENT_TOOLS`; workflows (S13) and knowledge (S15) authoring arrive with their slices.

## 6. Build order

1. Re-capture `agent`/`mcp_server` SHAs; record them in §4. `git -C $HOME/Movies/agent rev-parse HEAD`.
2. `_meta.operation` + `crates/actions/tests/mcp_meta.rs`; assert every registered action's tool carries it.
   `cargo test -p conveyor-actions --test mcp_meta`
3. `Principal.kind` + propagation through `Session`; compile fails until every construction site supplies it.
   `cargo test -p conveyor-auth`
4. Machine authenticator + cache; the JWT-shape rule; negative tests (JWT-shaped token, wrong tenant, expired,
   unknown app). `cargo test -p conveyor-auth machine`
5. Wire it into `authenticate` before membership resolution; one membership, scoped role.
   `cargo test -p conveyor-auth`
6. `approvals.request` + action registration + audit/invocation row. `cargo test -p conveyor-api --test approval_dual --test mcp_agent`
   ⚠️ Corrected 2026-11-09: this line said `cargo test -p conveyor-actions approvals`, which matches **0 tests** in
   this workspace — a gate line that cannot fail. The action's real coverage is `approval_dual` (dual control and
   the derived `decided_by_kind`) and `mcp_agent` (the licence-principal lifecycle).
7. Attribution: the three `actor_kind` sites read the principal; update `crates/api/tests/approval_dual.rs`.
   `cargo test -p conveyor-api --test approval_dual`
8. Tool scoping test: the agent principal sees exactly §5's tools; an admin sees more; a viewer sees fewer.
   `cargo test -p conveyor-api --test mcp_agent scopes`
9. End-to-end over `/mcp` against a local `agent` (registration: external MCP system, `base_url`, licence
   credential): create → poll → approve/reject → close. `cargo test -p conveyor-api --test mcp_agent`
10. Regenerate the served-spec lock (new action). `cargo run -q -p conveyor-server -- --print-openapi --write-lock`
11. Gate (§7) + clippy.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env
cargo test -p conveyor-auth
cargo test -p conveyor-actions --test mcp_meta
cargo test -p conveyor-api --test approval_dual
cargo test -p conveyor-api --test mcp_agent
cargo clippy --workspace --all-targets -- -D warnings
# the served document matches its lock (regenerated in step 10)
cargo run -q -p conveyor-server -- --print-openapi --check-lock
# the agent task's evidence exists
test -s docs/evidence/S06b-agent-task.md
```

| # | property | proof |
|---|---|---|
| a | tool key | every MCP tool carries `_meta.operation` = its action name |
| b | import | the agent's `mcp-import` yields `projects.create`-style keys, not `tool.*` |
| c | machine auth | a licence for tenant A cannot act for tenant B: `403`/`unknown-sender`, zero mutations |
| d | JWT rule | a JWT-shaped token that fails verification is never retried as a licence |
| e | attribution | `approval_decision.decided_by_kind='agent'`, `via` names the licence principal |
| f | escalation | `approvals.request` opens an approval with a NULL `run_id` and is decidable |
| g | end to end | one real agent task: create, progress, decide, close; evidence file contains run id, project id, one decision |
| h | no middleman | the task's evidence shows no `X-MCP-Binding` header and no `mcp.mcp.svc` host |

## 8. Traps

1. **`_meta` is the only channel** — rmcp flattens task/result payloads and drops unknown top-level fields. The
   operation key must live in `_meta`, not in a sibling object.
2. **One membership** — `authenticate` rejects 0 or >1 memberships. A licence that resolves to several tenants
   must choose the tenant from the licence, never from a header.
3. **JWT shape** — a JWT-shaped token that fails verification must not fall through to licence introspection;
   otherwise a revoked credential becomes a valid one.
4. **`open_gate_tx` needs a running run** — a standalone escalation with `run_id IS NULL` must use
   `insert_approval_tx`.
5. **`uq_appr_pending` includes `run_id`** — nullable, so multiple standalone approvals do not collide; do not
   "fix" it by adding a second index.
6. **Tool scoping is the authorization** — `/mcp` filters by role; an agent principal with `admin` would see every
   action. The membership role must be the scoped one.
7. **Do not rebuild the HMAC channel** — if a later session finds S06a text describing it, that plan is
   superseded; the seam is `/mcp`.

## 9. Decisions closed here

1. **MCP, not REST.** The wire, the import and the schemas exist on both sides; REST is a fallback only for a tool
   that cannot be expressed over MCP.
2. **The conveyor's `/mcp` is the agent interface.** `mcp_server` remains the gateway for coding, care-bus and
   builder tools, and its `fr` toolkit retires at cutover.
3. **Licence bearer, introspected.** One licence per tenant carries the tenant; an OIDC service account would hit
   the one-membership rule for every tenant it acts for.
4. **Polling is the fallback; S06c adds MCP tasks.** The agent's task extension is platform-server-only
   (`external.rs:8`), so `projects.get` + `approvals.list` inside a durable job step ships first. **S06c** implements
   SEP-2663 in the conveyor (`projects.execute`, `tasks/get|update|cancel`) and the agent's external-task client;
   the polling path stays as the fallback for a client that cannot run tasks.
5. **Typed results, not `_meta` blobs.** The fr-specific `_meta["ai.nonagentic/fr"]` shape is not reproduced.

**CONTRACT-CHANGE REQUEST:** `01-CONTRACT §8` states the two MCP surfaces "must never be conflated" and that "a
token on `/mcp` is not a substitute for the signed channel". D19 makes `/mcp` the agent interface. Proposal: one
surface, with the consumer boundary stated (agent = licence principal; other consumers = their own principal).
No slice may act on this until the contract is amended.

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Does the agent need push in v1? | No — polling is the fallback; **S06c** adds MCP tasks for the long-running project |
| 2 | Who mints the conveyor licence? | The agent's licence-minting path, extended to `app_key=conveyor` (today `BundleToken` is platform-only) |
| 3 | Is the `fr` toolkit retired now or at cutover? | At cutover, with fr |
| 4 | Does `approvals.request` need an `allowed_decisions` vocabulary? | Reuse `catalogue_entry(kind='gate')`; do not invent one |
| 5 | Should the agent see `events.list` or an SSE topic? | `events.list`; SSE is a later optimization |
