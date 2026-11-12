---
slice: S06d
title: Interaction connector standard — web, Telegram, MCP first
status: built
depends_on: [S02, S04, S05, S06, S06b, S06c]
spec_sections: ["§4.5", "§8 S6-α", "§11 R11"]
adds_tables: [channel_action_grant, interaction_dispatch]
adds_actions:
  [channels.kinds, channels.get, channels.list, channels.test,
   channels.policy_get, channels.policy_set, channels.policy_reset]
adds_events: []
adds_modules: [channel, channel::command, channel::connectors, interaction]
parity_ids: [PENDING-S0]
---

# S06d — Interaction connector standard

Programme project: S06d · issues none yet

## 1. Objective

Web, Telegram and MCP are **three instances of one class of user-interaction app**. This slice
builds the standard that makes the next connector cheap: one registry entry per kind (typed
config + `schemars` schema, named secrets, default grant set, `authenticate`/`parse`/`send`),
one inbound route and one command grammar, one multicast dispatcher with a first-response
close, and a tenant-admin grant matrix. Config and credentials live in Postgres and are entered
through the web UI; there are no per-connector environment variables.

## 2. What the standard is

| Piece | Where | Contract |
|---|---|---|
| Connector kind | `crates/actions/src/channel/connectors/` | A `ConnectorSpec` per kind: `config_schema` (the web form), `secret_fields` (**names** of sealed secrets), `default_actions`, `capabilities`, and three functions — `authenticate`, `parse`, `send`. The `webhook` kind is the zero-Rust on-ramp for a JSON gateway. |
| Binding | `channel_binding` | `(tenant, kind, route_key)`; `config` holds non-secret values and secret names. `channels.bind` validates the config with the kind's own schema and refuses a name the tenant does not hold. |
| Identity | `app_user(kind='service', oidc_subject='service:<kind>:<tenant>')` | One per `(tenant, kind)` for provisioned connectors; `mcp` is the licence principal; `web` is the human session. Membership role `kata_executor`; the grant set is the real bound. |
| Inbound | `POST /api/channels/{binding_id}/inbound` | Resolve binding → adapter authenticates (per-binding sealed secret) → adapter parses → **shared** command grammar → `invoke` with `Authority::Channel`. The adapter never authorizes. |
| Command grammar | `channel::command` | `approve/reject/answer`, `run/rerun/start/finish/cancel`, `close/create`, `do <action> <json>`, `help`. One implementation, every text connector. |
| Multicast | `interaction.dispatch` | A pending approval is offered to every eligible machine binding (grant allows `approvals.decide`) and human web inbox (role/policy holds the response scope). `interaction_dispatch`'s PK is the fan-out idempotency. |
| First response | `interaction.close` | A terminal approval expires every live sibling delivery to `expired` with `last_error='superseded: resolved via <channel>'` and sends one closure notice per dispatch. The action's status guard is the arbiter; this sweep is the notification half. |
| Configuration | `channels.kinds/bind/get/list/unbind/test` | The schema, the binding, the inbound URL, secret presence, and a queued real test message. |
| Authorization | `channels.policy_get/set/reset` | The tenant-admin matrix: compiled default, stored override, and the floor (a machine can never sign off evals, read held-out traces, administer the tenant (roles, invites, members, departments, channels, webhooks, secrets), control board membership, or author workflows). |

## 3. Decisions

1. **One grammar, thin adapters.** Platform payload parsing is the only per-kind code; the
   command vocabulary, identity, grants, retries and delivery state are core.
2. **Config is data.** A binding's config and secret names are rows; `channels.bind` validates
   against the kind's schema. `CONVEYOR_SECRET_KEK` remains the deployment root key.
3. **The machine floor cannot be granted.** `ActionSpec::machine_forbidden` is computed from
   declared scopes/resource, and both `channels.policy_set` and `invoke` enforce it.
4. **Eval gates are human-only.** The shared decision writer refuses a non-human or
   non-`evals:approve` decision on any approval with `eval_id IS NOT NULL`; the dispatcher never
   offers such a gate to a machine channel.
5. **The first-response loser is `expired`.** No second state machine: `pending|sent → expired`
   is the edge the data model already documents.
6. **A publisher is checked by its authority.** `procedures.publish` uses the channel grant set,
   so a channel cannot freeze a step it could not invoke.
7. **The web channel has no machine grants.** Its authorization is the human role policy.

## 4. Code map

```
migrations/0038_s06d_connector_standard.sql
crates/actions/src/channel/{mod,command}.rs
crates/actions/src/channel/connectors/{mod,telegram,webhook}.rs
crates/actions/src/control/channels.rs            kinds/bind/get/list/unbind/test/policy_*
crates/actions/src/{invoke,spec}.rs               Authority, permits_channel, machine_forbidden
crates/auth/src/{principal,authn}.rs              ChannelAuthority, Session.channel
crates/store/src/{interaction,rbac,channel,delivery}.rs
crates/api/src/routes/channels.rs                 the one inbound route
apps/server/src/worker.rs                         interaction.dispatch / interaction.close
```

## 5. Gate

```bash
rbuild -- test --workspace --no-fail-fast --locked
rbuild -- clippy --workspace --all-targets -- -D warnings
rbuild -- fmt --check
rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock
```

Result at `99ed39f` + the five-failure fix: clippy/fmt clean, lock OK, and every affected
binary re-run green (`conveyor-api --lib`, `externals_declared`, `kata_role`, `channel_inbound`,
`channel_policy`, `conveyor-server --test prune`, `interaction_dispatch`, `conveyor-store --lib`).
The first full workspace run found exactly those five failures; the rest of the suite passed.
The full suite was not repeated end-to-end after the fixes (each affected binary was).

## 6. Traps

1. **A channel kind is code, not a CHECK.** The database owns the spelling; an unknown kind must
   be refused by `channels.bind` and inert at the route, never guessed.
2. **A secret field holds a name.** Never echo a value; `channels.get` reports presence only.
3. **RLS in the migration.** `membership` is FORCE RLS; the service-identity backfill lifts
   FORCE inside the migration and restores it (the 0028 pattern).
4. **`expired` is terminal.** A delivered sibling stays delivered; the close sweep expires only
   live rows and still sends the closure notice.
5. **The dispatcher is idempotent by PK, not by a scan.** The advisory lock + the dispatch PK are
   the two guards; do not add a "has this run" column.
6. **The eval floor is a scope test.** `kata_executor` deliberately lacks `evals:approve`; a
   tenant grant cannot change that.
7. **Webhook senders retry non-2xx.** Business refusals are `200` acks; only a server-side
   failure asks for a retry.
