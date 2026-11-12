# 02 — DATA ARCHITECTURE (binding)

**Authority:** this file fixes every schema decision for `conveyor`. S03 implements it. **No slice may add a
table that is not in §2, and no slice may change a column, key or index defined here** without a
`CONTRACT-CHANGE REQUEST` to `01-CONTRACT.md`.
**Written:** 2026-11-07, before the slice plans. Sources: `docs/SPEC.md` §4/§5.3/§6/§7, `01-CONTRACT.md`,
`02-source-map/50-schema-inventory.md` (the total source lists), `02-source-map/{10,20,30,40}`.

---

## 1. Principles

1. One database `conveyor`, one runtime user `conveyor_runtime` (`NOSUPERUSER NOBYPASSRLS`), one migration role
   that owns the schema. One baseline + forward-only migrations, never edited after their slice closes.
2. `tenant_id uuid NOT NULL` on every tenant-owned table, with `ENABLE ROW LEVEL SECURITY` and a `FOR ALL`
   policy `tenant_isolation` carrying **both** `USING` and `WITH CHECK`
   (`tenant_id = current_setting('app.current_tenant', true)::uuid`). Scope is set per transaction with
   `set_config(…, true)`; a session-level `SET` is forbidden (PgBouncer transaction pooling).
3. PKs are uuid v7. **No external id is ever a PK or an FK target** — no `planka_card_id`, no
   `execution_id` as a key.
4. Every mutable aggregate has `version bigint NOT NULL DEFAULT 1`. **Edits** require `If-Match` and a
   zero-row `UPDATE` is `412`, never a blind write; **state transitions are status-guarded, not
   version-guarded** — the transitions named in CONTRACT §3 take no `If-Match` and no `version` input, and a
   zero-row update is `409` with the current state.
5. `timestamptz` everywhere, UTC, `now()` defaults. Never `timestamp` without a zone.
6. **The list is the state.** A card's lifecycle is `list_id` + `archived_at`. There is no `card.state`.
7. Flattening per SPEC §4: one `revision`, one `catalogue_entry`, one `outline_node`, one `comment`, one
   `delivery`, one identity. A derived value is a query, not a column — the two exceptions are `tsv`
   (a generated column that exists only to be indexed) and `card.board_id` (denormalised to carry the board index).
8. **No Postgres enum types.** Every vocabulary is `text` + `CHECK`. Measured reason: `ALTER TYPE … ADD VALUE`
   cannot be used in the same transaction as an index on the new label (`fr/backend/migrations/153_u3_approval_kinds.sql`
   documents the failure), and the migration runner wraps each file in one transaction.
9. JSONB holds only values read whole with their parent. Anything in a `WHERE`, `ORDER BY`, `JOIN` or `UNIQUE`
   is a column. §5 is the exhaustive list.
10. `event` is written in the same transaction as the mutation and is the only fan-out source. `audit_event`
    is append-only by `GRANT` (`SELECT, INSERT` only). Neither is optional per slice.
11. Names: lowercase singular tables, `app_user` (never `user`), `position` (never `order`).

## 2. Entity catalogue

Shorthand: `nn` = not null, `d=` default, `uq` = unique, `ix` = index, `rls` = row-level security,
`ret` = retention. All uuid PKs `DEFAULT uuidv7()`. All tables carry `created_at timestamptz nn d=now()`;
mutable aggregates also carry `updated_at` and `version`, and those columns are not repeated per table below.

### 2.1 Identity and tenancy — S02 (4)

```
tenant · S02 · the root aggregate; one row per customer, the only object with no tenant_id
  id uuid pk · slug text nn · name text nn · status text nn d='active' check(status in ('active','suspended','closed'))
  settings jsonb nn d='{}'
  uq (slug) · ix (status) · rls NO · ret none

app_user · S02 · one row per human or agent principal; identity is OIDC sub, never a realm claim
  id uuid pk · oidc_subject text null (null for agents) · email text null · display_name text nn d=''
  kind text nn d='human' check(kind in ('human','agent','service')) · status text nn d='active'
  uq (oidc_subject) where oidc_subject is not null · ix (email) · rls NO (no tenant_id; reached via membership) · ret none

membership · S02 · the ONE role store per tenant; absorbs fr user_roles + agent_members + department_members + member_knowledge_areas
  id uuid pk · tenant_id uuid nn · user_id uuid nn · role text nn d='member'
  check(role in ('owner','admin','member','viewer','kata_executor')) · department_id uuid null · knowledge_areas text[] nn d='{}'
  meta jsonb nn d='{}'
  fk tenant_id→tenant, user_id→app_user, department_id→department
  uq (tenant_id, user_id) · ix (tenant_id, role) · ix (user_id) · rls YES · ret none

department · S02 · tenant org unit; absorbs fr departments
  id uuid pk · tenant_id uuid nn · name text nn · parent_id uuid null check(parent_id <> id)
  fk parent_id→department
  uq (tenant_id, name) · rls YES · ret none
```

No separate agent table: an agent is an `app_user` row with `kind='agent'` and a null `oidc_subject`,
reached per tenant through a `membership` row whose `meta.product` names the product.

### 2.2 Platform — S02 (7)

The seven tables of SPEC §6. Nothing else may be called a platform table.

```
event · S02 · the transactional outbox and the ONLY fan-out source (contract §7 shape, verbatim)
  seq bigserial pk · id uuid nn d=uuidv7() · tenant_id uuid nn · topic text nn · type text nn
  aggregate_type text nn · aggregate_id uuid nn · payload jsonb nn d='{}'
  correlation_id uuid null · causation_id uuid null · created_at timestamptz nn d=now()
  bus_stream_id text null · published_at timestamptz null   -- 0035: the Redis relay's cursor (the bus is a nudge)
  uq (id) · ix (tenant_id, seq) · (topic, seq) · (aggregate_type, aggregate_id, seq) · (published_at) where published_at is null
  rls YES · ret 90 days, pruned by job; partition in 09 not before ~50M rows

job · S02 · durable work queue; replaces all fourteen fr setInterval workers
  id uuid pk · tenant_id uuid nn · kind text nn · dedupe_key text nn · status text nn d='pending'
  check(status in ('pending','running','succeeded','failed','dead'))
  payload jsonb nn d='{}' · run_after timestamptz nn d=now() · attempts int nn d=0 · max_attempts int nn d=5
  lease_until timestamptz null · lease_token uuid null · last_error text null
  uq (tenant_id, kind, dedupe_key) where status in ('pending','running')
  ix (tenant_id, run_after, id) where status='pending' · (lease_until) where status='running'
  rls YES · ret 30 days after terminal

schedule · S02 · materialises jobs under one advisory lock; replaces scheduled_triggers
  id uuid pk · tenant_id uuid nn · kind text nn · cron text nn · timezone text nn d='UTC'
  payload jsonb nn d='{}' · enabled bool nn d=true · next_run_at timestamptz nn · last_run_at timestamptz null
  uq (tenant_id, kind) · ix (next_run_at) where enabled · rls YES · ret none

audit_event · S02 · append-only; GRANT SELECT, INSERT only, never UPDATE/DELETE
  id uuid pk · tenant_id uuid nn · actor_id uuid null · actor_kind text nn d='user'
  action text nn check(action ~ '^[a-z0-9_]+\.[a-z0-9_]+$') · status text nn check(status in ('ok','refused','error'))
  resource_type text null · resource_id uuid null · request_id text null
  details jsonb nn d='{}' check(length(details::text) <= 8192)
  ix (tenant_id, created_at desc, id) · (tenant_id, resource_type, resource_id, created_at desc)
  rls YES · ret UNBOUNDED (see §6)

action_invocation · S02 · one row per registry invocation, written in the mutation transaction
  id uuid pk · tenant_id uuid nn · actor_id uuid null · action text nn · action_version int nn d=1
  input_hash text nn · idempotency_key text null · outcome text nn check(outcome in ('ok','refused','error','timeout'))
  duration_ms int nn d=0 · trace_id text null
  ix (tenant_id, action, created_at desc) · (tenant_id, trace_id) · rls YES · ret 30 days

idempotency_key · S02 · POST replay store; absorbs action_idempotency + router_dedupe
  tenant_id uuid nn · scope text nn · key text nn · request_hash text nn
  status_code int null · response jsonb null · expires_at timestamptz nn
  pk (tenant_id, scope, key) · ix (expires_at) · rls YES · ret 24 h TTL, swept by job

app_licence · S02 · the synced app-level entitlement; one row per (tenant, app_key)
  tenant_id uuid nn · app_key text nn d='conveyor' · status text nn check(status in ('active','expired','revoked','unknown'))
  expires_at timestamptz null · checked_at timestamptz nn · raw jsonb nn d='{}'
  pk (tenant_id, app_key) · rls YES · ret none
```

### 2.3 Board spine — S03 (9)

The aggregates the domain crate models and proptests. Nothing here is a satellite.

```
programme · S03 · portfolio container; ships here because `project` (S03) is its first consumer
  id uuid pk · tenant_id uuid nn · name text nn · status text nn d='active' · owner_user_id uuid null
  fk owner_user_id→app_user · uq (tenant_id, name) · rls YES · ret none

project · S03 · the ONE project entity; absorbs fr projects + Planka project + RM pm_projects
  id uuid pk · tenant_id uuid nn · key text nn · name text nn · description text nn d=''
  status text nn d='active' check(status in ('draft','active','paused','closed','archived'))
  programme_id uuid null · project_type_code text null · phase_code text null
  owner_user_id uuid null · repo_url text null · meta jsonb nn d='{}'
  fk programme_id→programme, owner_user_id→app_user
  uq (tenant_id, key) · ix (tenant_id, status, updated_at desc, id) · (tenant_id, programme_id)
  rls YES · ret none (closed projects are history, not garbage)

board · S03 · one board per project today, but the FK is the contract, not a 1:1 assumption
  id uuid pk · tenant_id uuid nn · project_id uuid nn · name text nn · position numeric nn d=0
  settings jsonb nn d='{}'   -- default_card_type_code, default_view, display flags
  fk project_id→project · uq (tenant_id, project_id, name) · ix (tenant_id, project_id, position)
  rls YES · ret none

list · S03 · the card STATE; `kind` is the only lifecycle vocabulary in the system
  id uuid pk · tenant_id uuid nn · board_id uuid nn · name text nn · position numeric nn
  kind text nn check(kind in ('intake','active','waiting','done','archive'))
  color text null · wip_limit int null check(wip_limit is null or wip_limit > 0)
  fk board_id→board · uq (tenant_id, board_id, id) · ix (tenant_id, board_id, position, id)
  rls YES · ret none

card_type · S03 · a card's type; the join target of the conveyor matrix. Deliberate exception to "one catalogue" — see §10 Q1
  id uuid pk · tenant_id uuid nn · code text nn · label text nn · definition jsonb nn d='{}'
  is_active bool nn d=true
  uq (tenant_id, code) · ix (tenant_id, is_active, code) · rls YES · ret none

card · S03 · the work item; claim columns replace fr card_phase_lease per contract §4
  id uuid pk · tenant_id uuid nn · board_id uuid nn · list_id uuid nn · card_type_id uuid nn
  parent_card_id uuid null · name text nn · description text nn d='' · position numeric nn
  due_at timestamptz null · due_done_at timestamptz null
  stopwatch_started_at timestamptz null · stopwatch_seconds int nn d=0   -- read-only stopwatch
  archived_at timestamptz null · assignee_id uuid null
  lease_expires_at timestamptz null · lease_token uuid null · claim_fence bigint nn d=0
  prev_list_id uuid null · list_changed_at timestamptz nn d=now() · meta jsonb nn d='{}' · created_by uuid null
  tsv tsvector generated always as (to_tsvector('english', name || ' ' || description)) stored
  fk list_id→list, board_id→board, card_type_id→card_type, parent_card_id→card(id), assignee_id→app_user
  uq (tenant_id, board_id, id) · uq (tenant_id, list_id, id)
  ix (tenant_id, board_id, list_id, position, id) where archived_at is null
  ix (tenant_id, board_id, position, id) where archived_at is null
  ix (tenant_id, assignee_id, lease_expires_at) where assignee_id is not null
  ix gin(tsv) · ix using gin (name gin_trgm_ops) · rls YES · ret archived cards kept; no partition

run · S03 · one execution of a card through a process; a run is a TREE (2026-11-09)
  id uuid pk · tenant_id uuid nn · card_id uuid null · project_id uuid nn
  status text nn d='queued' check(status in ('queued','running','waiting_approval','succeeded','failed','cancelled'))
  attempt int nn d=1 · error_class text null · error_message text null
  process_id uuid null    -- the process; S11's `procedure_id`, renamed by 0035 (FK in the same file)
  inputs jsonb nn d='{}'  -- column added by S11's migration: the run's input document
  parent_run_id uuid null · step_id uuid null       -- the tree: a child run names its parent and its step
  depth int nn d=0                                   -- 0 = root; CHECK (depth between 0 and 3)
  correlation_id uuid null · external_ref text null · started_at timestamptz null · finished_at timestamptz null
  fk card_id→card, project_id→project, parent_run_id→run, step_id→process_step
  check(parent_run_id is null or step_id is not null)
  uq (tenant_id, card_id) where parent_run_id is null and status in ('queued','running','waiting_approval')  -- one live ROOT run per card
  uq (tenant_id, project_id) where parent_run_id is null and card_id is null and status in (…)             -- one live root per project
  uq (parent_run_id, step_id) where parent_run_id is not null and status in (…)                          -- one live child per (parent, step)
  uq (tenant_id, parent_run_id, step_id, attempt) where parent_run_id is not null                        -- one row per attempt
  ix (tenant_id, status, created_at, id) where status='queued' · (tenant_id, created_at desc, id) · (tenant_id, card_id, created_at desc)
  rls YES · ret terminal runs 180 days, then pruned

⚠️ **Amended 2026-11-09, corrected as built 2026-11-10.** `run.procedure_id` is renamed `run.process_id` and
`run_step.process_step_id` likewise, by migration `0035_process_tree.sql`. **`run.workflow_id` and the `workflow`
table are RETAINED** (owner ruling 2026-11-09): the workflow engine is the tool-sequence executor a process `call`
step can name, and there is no drop migration. The old card-wide `uq_run_live` is replaced by the two root-only
indexes above — which is what makes a child run (the coach, a `cap.*` call) possible on a card that already has a
live parent.

approval · S03 · the gate. At most ONE pending per (run, gate); dual control is required_decisions, not a second gate
  id uuid pk · tenant_id uuid nn · run_id uuid null · card_id uuid null · project_id uuid null
  gate text nn · kind text nn · status text nn d='pending' check(status in ('pending','approved','rejected','expired','voided'))
  required_decisions int nn d=1 check(required_decisions >= 1)
  requested_by uuid null · expires_at timestamptz null · callback_url text null
  correlation_id uuid null · meta jsonb nn d='{}'
  fk run_id→run, card_id→card, project_id→project, requested_by→app_user
  uq (tenant_id, run_id, gate) where status='pending'
  ix (tenant_id, status, expires_at) where status='pending' · (tenant_id, card_id, created_at desc)
  rls YES · ret 1 year

approval_decision · S03 · one row per approver per approval; absorbs fr approval_decisions
  id uuid pk · tenant_id uuid nn · approval_id uuid nn · approver_id uuid nn
  decision text nn check(decision in ('approve','reject')) · note text nn d='' · item_decisions jsonb null
  decided_by_kind text nn d='human' check(decided_by_kind in ('human','triage_agent','rule','agent')) · via jsonb null   -- fr 211/219: an agent decision must be distinguishable from a human one (SPEC D18)
  fk approval_id→approval, approver_id→app_user
  uq (approval_id, approver_id) · ix (tenant_id, approver_id, created_at desc) · rls YES · ret 1 year
```

### 2.4 Governance core — S03 (5)

S03 ships these because two later slices need them and a table is assigned to its earliest consumer.

```
governed_asset · S03 · ONE lifecycle for requirements, principles and ADRs; absorbs RM requirements,
                 principles, decisions. Kind-specific fields live in `meta`
  id uuid pk · tenant_id uuid nn · kind text nn check(kind in ('requirement','principle','adr'))
  key text nn · title text nn · body text nn d=''
  status text nn d='draft' check(status in ('draft','in_review','approved','superseded','archived'))
  version bigint nn d=1 · supersedes_id uuid null
  superseded_by_id uuid null check(superseded_by_id is null or superseded_by_id <> id)
  project_id uuid null · owner_user_id uuid null · phase_code text null
  effort numeric null · reach numeric null · impact numeric null · confidence numeric null
  acceptance jsonb nn d='[]' · links jsonb nn d='{}' · lens jsonb nn d='{}' · meta jsonb nn d='{}'
  tsv tsvector generated always as (to_tsvector('english', key || ' ' || title || ' ' || body)) stored
  fk project_id→project, owner_user_id→app_user, supersedes_id→governed_asset(id),
     superseded_by_id→governed_asset(id)
  check(status <> 'superseded' or superseded_by_id is not null)
  uq (tenant_id, kind, key) · ix (tenant_id, kind, status, updated_at desc, id) · (tenant_id, project_id)
  ix gin(tsv) · ix using gin (title gin_trgm_ops) · rls YES · ret none (superseded rows are the history)

trace_link · S03 · the only many-to-many between governed assets, cards and model elements
  id uuid pk · tenant_id uuid nn · from_type text nn · from_id uuid nn · to_type text nn · to_id uuid nn
  kind text nn d='traces' check(kind in ('traces','realises','derives','satisfies','conflicts'))
  fk none (polymorphic; the domain function validates both ends in the same transaction)
  uq (tenant_id, from_type, from_id, to_type, to_id, kind)
  ix (tenant_id, to_type, to_id) · (tenant_id, from_type, from_id) · rls YES · ret none

catalogue_entry · S03 · the ONE vocabulary table; absorbs RM's project/spec phases, capabilities, project
                   types, recipes, wip limits, priority levels, deliverable/milestone kinds, gates, board
                   templates, architecture standards
  id uuid pk · tenant_id uuid nn · kind text nn · code text nn · label text nn
  meta jsonb nn d='{}' · sort int nn d=0 · is_active bool nn d=true
  uq (tenant_id, kind, code) · ix (tenant_id, kind, sort, code) · ix using gin (label gin_trgm_ops)
  rls YES · ret none

revision · S03 · the ONE history table; absorbs entity_version_history, modeling_model_revisions,
           modeling_publications, pm_project_brief_versions, card_version_stamps
  id uuid pk · tenant_id uuid nn · entity_type text nn · entity_id uuid nn
  seq int nn · label text nn d='' · content jsonb nn d='{}' · content_hash text nn
  author_id uuid null · status text nn d='draft' check(status in ('draft','published','superseded'))
  fk author_id→app_user
  uq (tenant_id, entity_type, entity_id, seq) · ix (tenant_id, entity_type, entity_id, seq desc)
  rls YES · ret none for governed assets; model revisions pruned to the last 50 per model

andon_state · S03 · one row per tenant; the estate-wide stop
  tenant_id uuid pk · engaged bool nn d=false · engaged_by uuid null · engaged_at timestamptz null
  released_at timestamptz null · reason text nn d=''
  fk engaged_by→app_user · ix (tenant_id) where engaged · rls YES · ret none
```

### 2.5 Board satellites — S05 (9)

```
board_membership · S05 · replaces Planka board_membership + project_manager + board_subscription
  id uuid pk · tenant_id uuid nn · board_id uuid nn · user_id uuid nn
  role text nn d='member' check(role in ('owner','admin','member','viewer')) · can_comment bool nn d=true
  watching bool nn d=true
  fk board_id→board, user_id→app_user · uq (tenant_id, board_id, user_id)
  ix (user_id) · rls YES · ret none

label · S05 · per-board label
  id uuid pk · tenant_id uuid nn · board_id uuid nn · name text nn · color text nn
  position numeric nn d=0
  fk board_id→board · uq (tenant_id, board_id, name) · ix (tenant_id, board_id, position) · rls YES · ret none

card_label · S05 · join
  tenant_id uuid nn · card_id uuid nn · label_id uuid nn
  fk card_id→card, label_id→label · pk (card_id, label_id) · ix (label_id, card_id) · rls YES · ret none

comment · S05 · the ONE comment store (replaces fr comments + Planka comment)
  id uuid pk · tenant_id uuid nn · card_id uuid nn · author_id uuid null · body text nn
  edited_at timestamptz null
  fk card_id→card, author_id→app_user · ix (tenant_id, card_id, created_at, id) · rls YES · ret none

task_list · S05 · checklist container; replaces Planka task_list
  id uuid pk · tenant_id uuid nn · card_id uuid nn · name text nn · position numeric nn d=0
  hide_completed bool nn d=false · show_on_front bool nn d=false
  fk card_id→card · ix (tenant_id, card_id, position) · rls YES · ret none

task · S05 · checklist item
  id uuid pk · tenant_id uuid nn · task_list_id uuid nn · name text nn · done bool nn d=false
  position numeric nn d=0
  fk task_list_id→task_list · ix (tenant_id, task_list_id, position) · rls YES · ret none

custom_field · S05 · group flattened into two columns; replaces Planka custom_field_group + base_custom_field_group
  id uuid pk · tenant_id uuid nn · board_id uuid nn · group_name text nn d='' · group_position numeric nn d=0
  name text nn · kind text nn d='text' check(kind in ('text','number','date','select','checkbox'))
  options jsonb nn d='[]' · position numeric nn d=0
  fk board_id→board · uq (tenant_id, board_id, name) · ix (tenant_id, board_id, group_position, position)
  rls YES · ret none

custom_field_value · S05 · one value per (card, field)
  tenant_id uuid nn · card_id uuid nn · custom_field_id uuid nn · content jsonb nn d='null'
  fk card_id→card, custom_field_id→custom_field · pk (card_id, custom_field_id)
  ix (tenant_id, custom_field_id) · rls YES · ret dies with the card

webhook_subscription · S05 · replaces Planka webhook; semantics kept, instance-admin scoping dropped
  id uuid pk · tenant_id uuid nn · board_id uuid null · url text nn
  secret_ciphertext bytea null · events text[] nn d='{}' · excluded_events text[] nn d='{}' · active bool nn d=true
  fk board_id→board · ix (tenant_id, active) · rls YES · ret none
```

### 2.6 Channel and delivery — S06 (6)

```
channel_binding · S06 · per-tenant channel config; absorbs fr channels + telegram_forum_topics
  id uuid pk · tenant_id uuid nn · channel_kind text nn check(channel_kind in ('web','telegram','mcp'))
  route_key text nn d=''   -- e.g. project key or 'default'
  config jsonb nn d='{}'   -- chat id, topic id, bot ref; secrets by reference only
  active bool nn d=true
  uq (tenant_id, channel_kind, route_key) · rls YES · ret none

channel_preference · S06 · absorbs fr notification_prefs + user_channel_prefs
  id uuid pk · tenant_id uuid nn · user_id uuid nn · channel_kind text nn
  enabled bool nn d=true · quiet_hours jsonb nn d='{}'
  fk user_id→app_user · uq (tenant_id, user_id, channel_kind) · rls YES · ret none

message · S06 · the outbound content, once per send; delivery fans out from it
  id uuid pk · tenant_id uuid nn · channel_kind text nn · sender_id uuid null
  kind text nn d='text' · body text nn · meta jsonb nn d='{}' · correlation_id uuid null
  fk sender_id→app_user · ix (tenant_id, correlation_id) · rls YES · ret 90 days, but only once no delivery references it (deliveries prune first — §6)

delivery · S06 · one row per (message, channel, recipient); the state machine of contract §8
  id uuid pk · tenant_id uuid nn · message_id uuid nn · channel_kind text nn · recipient_ref text nn
  state text nn d='pending' check(state in ('pending','sent','delivered','failed','expired'))
  attempts int nn d=0 · last_error text null · sent_at/delivered_at/expires_at timestamptz null
  fk message_id→message · uq (message_id, channel_kind, recipient_ref)
  ix (tenant_id, state, expires_at) where state in ('pending','sent') · rls YES · ret 30 days after terminal

-- audience, audience_member · S06 · DROPPED 2026-11-08 by migration 0011.
-- S00 day 2 measured zero usage: no `streams` node operation in any of the 348 flows, fr.streams 0 rows,
-- asset_stream_membership 0, and fr has no /api/streams route at all. `messages.send` takes an explicit
-- recipient list; there is no multicast audience.
```

### 2.7 Requirements, catalogues, revision — S07 (3 after the S00 verdicts)

```
-- outline, outline_node · S07 · DROPPED 2026-11-08 by migration 0011.
-- S00 day 2 measured zero usage: RM toc_trees/toc_nodes/toc_settings 0 rows, fr project_outlines 0 rows,
-- no `toc` operation in any of the 348 flows, fr /toc/* = 15 unused + 1 rare. `project_request` never
-- referenced them, so project intake is unaffected. Re-adding is a new migration, not an edit here.

quality_evaluation · S07 · score history; absorbs RM quality_evaluations
  id uuid pk · tenant_id uuid nn · entity_type text nn · entity_id uuid nn
  score numeric null · criteria jsonb nn d='{}' · evaluator_kind text nn d='heuristic'
  ix (tenant_id, entity_type, entity_id, created_at desc) · rls YES · ret none

principle_conflict · S07 · persisted verdicts; absorbs RM principle_conflicts
  id uuid pk · tenant_id uuid nn · principle_a_id uuid nn · principle_b_id uuid nn
  verdict text nn · dismissed_by uuid null · dismissed_at timestamptz null
  fk principle_a_id→governed_asset, principle_b_id→governed_asset, dismissed_by→app_user
  uq (tenant_id, principle_a_id, principle_b_id) · rls YES · ret none

project_request · S07 · the CP→conveyor bridge intake; absorbs fr project_requests
  id uuid pk · tenant_id uuid nn · external_ref text nn · external_origin text nn
  payload jsonb nn d='{}' · status text nn d='draft' check(status in ('draft','approved','rejected','promoted'))
  project_id uuid null · repository_url text null   -- text, not an FK: repositories ship in S09
  fk project_id→project · uq (tenant_id, external_ref) · ix (tenant_id, status, created_at, id) · rls YES · ret none
```

### 2.8 Model, architecture, UML — S08 (7)

ArchiMate and UML share one shape; `model.language` is the discriminator. Absorbs RM's `architecture_*`,
`modeling_*` and `uml_*` families.

```
model · S08 · one model header; language decides the ruleset, not the schema
  id uuid pk · tenant_id uuid nn · project_id uuid null
  language text nn check(language in ('archimate','uml')) · language_version text nn
  title text nn · description text nn d='' · head_revision int nn d=0 · published_revision_id uuid null
  meta jsonb nn d='{}'
  fk project_id→project, published_revision_id→revision · ix (tenant_id, project_id) · rls YES · ret none

model_element · S08 · absorbs architecture_elements + uml_elements
  id uuid pk · tenant_id uuid nn · model_id uuid nn · kind text nn · name text nn · documentation text nn d=''
  parent_element_id uuid null · meta jsonb nn d='{}'
  fk model_id→model, parent_element_id→model_element · ix (tenant_id, model_id, kind) · rls YES · ret none

model_relationship · S08 · absorbs architecture_relationships + uml_relationships + uml_interactions
  id uuid pk · tenant_id uuid nn · model_id uuid nn · kind text nn
  source_id uuid nn · target_id uuid nn · meta jsonb nn d='{}'
  check(source_id <> target_id)
  fk model_id→model · ix (tenant_id, model_id, kind) · (source_id) · (target_id) · rls YES · ret none

model_view · S08 · absorbs architecture_views + uml_diagrams
  id uuid pk · tenant_id uuid nn · model_id uuid nn
  kind text nn d='diagram' check(kind in ('diagram','archimate_view','uml_diagram'))
  name text nn · meta jsonb nn d='{}'
  fk model_id→model · ix (tenant_id, model_id) · rls YES · ret none

view_object · S08 · absorbs architecture_view_objects + uml_diagram_objects
  id uuid pk · tenant_id uuid nn · view_id uuid nn · element_id uuid null · parent_object_id uuid null
  kind text nn d='element' · bounds jsonb nn d='{}' · style jsonb nn d='{}' · label text null
  position numeric nn d=0
  fk view_id→model_view, element_id→model_element, parent_object_id→view_object
  ix (tenant_id, view_id, position, id) · rls YES · ret none

view_connection · S08 · absorbs architecture_view_connections + uml_diagram_connections
  id uuid pk · tenant_id uuid nn · view_id uuid nn · relationship_id uuid null
  source_object_id uuid nn · target_object_id uuid nn · bendpoints jsonb nn d='[]' · style jsonb nn d='{}'
  fk view_id→model_view, relationship_id→model_relationship
  ix (tenant_id, view_id) · rls YES · ret none

model_validation_run · S08 · absorbs modeling_validation_runs; findings folded into jsonb
  id uuid pk · tenant_id uuid nn · model_id uuid nn · revision_id uuid null
  ruleset text nn · ruleset_version text nn · status text nn d='ok' check(status in ('ok','findings','error'))
  findings jsonb nn d='[]' · content_hash text nn
  fk model_id→model, revision_id→revision · ix (tenant_id, model_id, created_at desc) · rls YES · ret 90 days
```

### 2.9 Portfolio — S08 (1 after the S00 verdicts)

```
project_stakeholder · S08 · absorbs fr project_stakeholders
  id uuid pk · tenant_id uuid nn · project_id uuid nn · user_id uuid null · name text nn d=''
  email text null · role text nn d=''
  fk project_id→project, user_id→app_user · ix (tenant_id, project_id) · rls YES · ret none

-- project_capability, deliverable, register_item · S08 · DROPPED 2026-11-08 by migration 0011.
-- S00 day 2 measured zero usage: project_deliverables/project_milestones 0, pm_project_capabilities 0,
-- pm_resources/pm_risks/pm_benefits 0, no deliverable/capability operation and no register route in any
-- of the 348 flows. `project_stakeholder` STAYS on 21 live rows.
```

### 2.10 Documents, repositories, secrets — S09 (4)

```
document · S09 · metadata only; bytes go to the object store, never to the row (absorbs fr documents +
           project_reports — a report is a document of kind 'report')
  id uuid pk · tenant_id uuid nn · project_id uuid null
  kind text nn d='document' check(kind in ('document','report','export')) · title text nn
  status text nn d='active' check(status in ('active','rejected','failed','deleted'))
  object_key text null · content_hash text null · external_ref text null · meta jsonb nn d='{}'
  fk project_id→project · ix (tenant_id, project_id, kind, updated_at desc) · rls YES · ret none

secret · S09 · encrypted at rest; absorbs fr user_secrets — declared FIRST so `repository`'s FK resolves in 0013
  id uuid pk · tenant_id uuid nn · name text nn · ciphertext bytea nn · key_id text nn
  scope text nn d='tenant' · version bigint nn d=1   -- rotation bumps version, never overwrites
  uq (tenant_id, name) · rls YES · ret none

repository · S09 · the repo registry; absorbs fr repositories + github_app_installations
  id uuid pk · tenant_id uuid nn · provider text nn d='github' · url text nn · default_branch text nn d='main'
  installation_ref text null · credential_secret_id uuid null
  fk credential_secret_id→secret · uq (tenant_id, url) · rls YES · ret none

project_repository · S09 · absorbs fr project_repositories
  tenant_id uuid nn · project_id uuid nn · repository_id uuid nn · role text nn d='primary'
  fk project_id→project, repository_id→repository · pk (project_id, repository_id) · rls YES · ret none
```

### 2.11 Copilot and conversation — S10 (2)

```
conversation · S10 · absorbs fr agent_sessions
  id uuid pk · tenant_id uuid nn · user_id uuid null · project_id uuid null
  title text nn d='' · status text nn d='active' check(status in ('active','archived'))
  fk user_id→app_user, project_id→project · ix (tenant_id, user_id, updated_at desc) · rls YES · ret 90 days

conversation_turn · S10 · one row per message; tool calls land in action_invocation, not here
  id uuid pk · tenant_id uuid nn · conversation_id uuid nn · seq int nn
  role text nn check(role in ('user','assistant','system')) · body text nn
  meta jsonb nn d='{}' · trace_id text null
  fk conversation_id→conversation · uq (conversation_id, seq) · rls YES · ret with the conversation
```

**Index added by S10's migration** (not a table): `uq_approval_copilot_turn` — a partial unique index on
S03's `approval` keyed by the copilot turn that proposed the action, so one turn cannot raise two pending
approvals. It belongs to S10's migration because S10 is its only writer; S03 stays unaware of copilot.

**Indexes added by S07's migration** (not tables): `CREATE EXTENSION pg_trgm` plus the trigram indexes on
`card (name)`, `governed_asset (title)` and `catalogue_entry (label)` (§4). They belong to S07 because the
extension ships there (S02 §5 decision 5); S03's baseline creates only the `gin(tsv)` indexes.

### 2.12 Process, step and procedure (SOP) — S11 (5)

⚠️ **Amended 2026-11-09 (the process model).** S11's `procedure` is **renamed `process`** and `procedure_step` →
`process_step` by migration `0035_process_tree.sql`; a **new `procedure` table is the SOP** attached to a step (definition of ready,
definition of done, acceptance criteria, inspection). `process_map` is the user-facing DAG. The word *procedure*
now means the SOP, never the sequence.

```
process · S11 (renamed by 0035) · the ordered sequence; tenant-authored or the default catalogue
  id uuid pk · tenant_id uuid nn · key text nn · title text nn · description text nn d=''
  status text nn d='draft' check(status in ('draft','published','retired')) · version bigint nn d=1
  capability_id uuid null · card_type_id uuid null · list_kind text null · project_type_id uuid null
  outputs jsonb nn d='[]'          -- the USER-FACING deliverables a person sees when the process completes
  fk capability_id→capability, card_type_id→card_type, project_type_id→project_type
  uq (tenant_id, key, version) · uq (tenant_id, key) where status='published'
  ix (tenant_id, status, key) · rls YES · ret none

process_step · S11 (renamed by 0035) · one ordered step; the four kinds are agent | computed | gate | call
  id uuid pk · tenant_id uuid nn · process_id uuid nn · position int nn
  kind text nn d='computed' check(kind in ('agent','computed','gate','call'))
  actor text nn d='tool_agent' check(actor in ('process_agent','tool_agent','system','human'))
  capability_id uuid null · process_agent_id uuid null · procedure_id uuid null
  child_process_key text null · action text null · input_template jsonb nn d='{}'
  gate_kind text null · on_result text nn d='fail' check(on_result in ('fail','continue','retry'))
  max_attempts int nn d=1 check(max_attempts >= 1)
  fk process_id→process, capability_id→capability, process_agent_id→capability_agent, procedure_id→procedure
  uq (process_id, position) · rls YES · ret none

procedure · S11 (NEW: the SOP) · the per-step standard operating procedure, versioned and reusable
  id uuid pk · tenant_id uuid nn · key text nn · title text nn · description text nn d=''
  status text nn d='draft' check(status in ('draft','published','retired')) · version bigint nn d=1
  definition_of_ready jsonb nn d='{}'     -- inputs and preconditions
  definition_of_done jsonb nn d='{}'      -- outputs and checks
  acceptance_criteria jsonb nn d='[]'     -- what the judge evaluates
  inspection jsonb nn d='{}'              -- {kind: laya|eval|checker, threshold?, max_rejects?}
  outputs jsonb nn d='[]'                 -- the step's deliverables
  uq (tenant_id, key, version) · uq (tenant_id, key) where status='published'
  ix (tenant_id, status, key) · rls YES · ret none

process_map · S11 · the USER-FACING DAG, per project type; declarative (it never executes)
  id uuid pk · tenant_id uuid nn · key text nn · title text nn · status text nn d='draft'
  check(status in ('draft','published','retired')) · version bigint nn d=1
  project_type_id uuid nn · definition jsonb nn d='{"nodes":[],"edges":[]}'
  fk project_type_id→project_type · uq (tenant_id, key, version) · rls YES · ret none

run_step · S11 · step-level execution; the trace S13's diff gate reads
  id uuid pk · tenant_id uuid nn · run_id uuid nn · process_step_id uuid null · position int nn
  attempt int nn d=1 check(attempt >= 1)           -- one row per attempt; a retry appends
  action text nn · status text nn d='queued' check(status in ('queued','running','succeeded','failed','skipped'))
  input jsonb nn d='{}' · output jsonb nn d='{}' · error text null
  judge jsonb null                                  -- the laya/eval verdict (2026-11-09)
  started_at/finished_at timestamptz null
  fk run_id→run, process_step_id→process_step · uq (run_id, position, attempt)
  ix (tenant_id, run_id, position) · rls YES · ret with the run
```

### 2.13 Workflow engine — RETAINED (S13, 2026-11-09)

✅ **The engine is retained (owner ruling 2026-11-09).** `crates/workflow`, the `workflow` table,
`workflows.*`/`workflow_runs.*` and the `workflow.node`/`workflow.start` job kinds stay: the workflow engine is the
**tool-sequence executor**, and a process `call` step resolves `child_process_key` **process-first,
workflow-second**. It is never a top-level executor — the process is (see §2.12). There is **no drop migration**:
`run.workflow_id` and the `workflow` table are not removed. S13's migrations are `0035_process_tree.sql` (the
renames, the SOP, `process_map`, the run tree and the bus cursor) and `0036_s19_output_grade.sql`; the earlier
`0033`/`0034` numbers belong to the train (`0033_d23_eval_caller`, `0034_s18_kata_arm`) and keep them.

```
workflow · S13 · RETAINED — the tool-sequence executor (owner ruling 2026-11-09; no drop migration)
  id uuid pk · tenant_id uuid nn · key text nn · title text nn · status text nn d='draft'
  check(status in ('draft','published','retired')) · version bigint nn d=1
  definition jsonb nn d='{"nodes":[],"edges":[]}' · published_revision_id uuid null
  fk published_revision_id→revision · uq (tenant_id, key, version) · rls YES · ret none
```

### 2.14 Action registry — S04 (1)

```
action_registry · S04 · rows exist ONLY for kind='external' (internal actions are compile-time); a schema
                    refresh that removes or retypes a field needs an approved row
  id uuid pk · tenant_id uuid nn · name text nn · spec_version int nn d=1
  kind text nn d='external' check(kind = 'external') · service text nn   -- 'workflow' retained with the engine (2026-11-09)
  spec jsonb nn d='{}' · schema_hash text nn · timeout_ms int nn d=30000 · idempotent bool nn d=false
  approved_at timestamptz null · approved_by uuid null
  fk approved_by→app_user · uq (tenant_id, name, spec_version)
  ix (tenant_id, service, name) · rls YES · ret none
```

### 2.15 Capability, knowledge and memory — S15 (5)

Design: `docs/KNOWLEDGE-MEMORY.md` (SPEC D20).

```
capability · S15 · the aggregate; replaces catalogue_entry(kind='capability'), whose 17 rows migrate here
  id uuid pk · tenant_id uuid nn · code text nn · label text nn · description text nn d=''
  kind text nn d='tenant' check(kind in ('system','tenant'))   -- 0035: PM/coach/kata are system, always present
  status text nn d='draft' check(status in ('draft','active','retired')) · meta jsonb nn d='{}'
  uq (tenant_id, code) · ix (tenant_id, status) · rls YES · ret none

capability_agent · S15 · the configured executor of one capability; the ROLE splits process from tool (2026-11-09)
  id uuid pk · tenant_id uuid nn · capability_id uuid nn · key text nn
  status text nn d='draft' check(status in ('draft','active','retired'))
  role text nn d='process_agent' check(role in ('process_agent','tool_agent'))   -- 0035
  process_key text null   -- the process/card type a process agent owns (0035)
  toolset text null       -- the toolset a tool agent executes (0035)
  model_ref text nn · prompt_ref text null · params jsonb nn d='{}'
  allowed_actions text[] nn d='{}' · max_iterations int nn d=1 check(max_iterations >= 1)
  timeout_secs int nn d=600 check(timeout_secs > 0)
  check((role='process_agent' and process_key is not null and toolset is null)
        or (role='tool_agent' and toolset is not null and process_key is null))
  fk capability_id→capability · uq (tenant_id, key) · ix (tenant_id, capability_id, status) · rls YES · ret none

capability_knowledge · S15 · the domain-knowledge binding; pack_key/pack_version pin a strata pack
  id uuid pk · tenant_id uuid nn · capability_id uuid nn · pack_key text nn · pack_version int nn
  min_score numeric null · required boolean nn d=true
  fk capability_id→capability · uq (tenant_id, capability_id, pack_key, pack_version) · rls YES · ret none

capability_relation · S15 · the ontology edges between capabilities
  tenant_id uuid nn · from_capability_id uuid nn · to_capability_id uuid nn
  kind text nn check(kind in ('requires','enables','concern'))
  fk from_capability_id→capability, to_capability_id→capability
  pk (from_capability_id, to_capability_id, kind) · check(from_capability_id <> to_capability_id) · rls YES · ret none

work_package_memory · S15 · hot memory; the record, consolidated into strata and never the reverse
  id uuid pk · tenant_id uuid nn · project_id uuid nn · card_id uuid nn
  run_id uuid null · capability_id uuid null
  kind text nn check(kind in ('fact','attempt','decision','blocker','digest'))
  source text nn check(source in ('agent','human','system'))
  body text nn · supersedes_id uuid null · consolidated_at timestamptz null · meta jsonb nn d='{}'
  fk project_id→project, card_id→card, run_id→run, capability_id→capability, supersedes_id→work_package_memory
  ix (tenant_id, card_id, created_at) · ix (tenant_id) where consolidated_at is null
  rls YES · ret none
```

### 2.16 Count

**88 tables** across 18 slice groups (S02 11 · S03 14 · S04 1 · S05 9 · S06 **4** · S07 **3** · S08 **8** · S09 4 ·
S10 2 · S11 3 · **S12 3** · S13 1 · S15 5 · **S16 3** · **S17 4** · **S19 10** · **S20 1** · **S21 2**) —
**re-counted from `migrations/` on 2026-11-09: 95 `CREATE TABLE` − 7 `DROP TABLE` = 88**. The earlier re-count
(2026-11-08, after `0011`) said 83 and omitted `0028`'s three RBAC tables (`tenant_role_scope`, `invite`,
`platform_admin`) and `0030`'s two prompt tables (`prompt`, `prompt_section`); corrected here, not carried forward. SPEC §10's target was "~60"; 67 was the parity baseline, and S15's five capability/memory tables are a
**contract change taken with D20** (2026-11-08). **S16's three tables are the second such change, taken with D23**
(2026-11-09): `inference_call`, `prompt_audit`, `prompt_audit_finding`, specified in §2.17. They are the first
tables in this catalogue whose subject is the conveyor's *own* model use rather than a domain aggregate, and
`inference_call` is the only one written by the action engine rather than by a handler. **S17's four tables are
the third such change, taken with D24** (2026-11-09): `benchmark_suite`, `benchmark_instance`, `benchmark_run`,
`benchmark_case`, specified in §2.18 — the absorbed `conveyor-bench` harness, whose rows are the measurement
record rather than a domain aggregate. **S19's ten tables are the fourth such change, taken with D25** (2026-11-09,
§2.19): `eval`, `eval_version`, `eval_case`, `eval_metric`, `eval_run`, `eval_case_result`, `eval_attempt_error`,
`eval_trace`, `eval_audit`, `eval_audit_finding` — the `build-eval` method's own record, the second family whose
subject is the conveyor's model use rather than a domain aggregate, and the first whose rows are *intentionally*
comparable across time only within one version. **S20's one table is the fifth, taken with D26** (§2.20):
`eval_climb`, the loop state the `eval-hillclimb` method keeps in `_state.json`. ⚠️ **S19 and S17 are two
instruments, not two copies of one:** the benchmark measures a pinned SWE-bench subset with a parity-ported
verdict ladder and a k8s sandbox; an eval measures any registered action with a per-metric grader. They share the
`job` runner, the outbox, the `run`/`inference_call` ledger and the SSE stream, and **no table** — a consolidation
is a later decision taken after both are built, with its own plan. The seven over the parity target were
`audience`, `audience_member`, `conversation`, `conversation_turn`, `workflow`, `action_registry` and
`register_item`; **five of them are gone** (`audience`, `audience_member` and `register_item` by `0011`; the two
are the S00 verdicts), so the count now sits **below** the parity baseline. §9's other six PENDING-S0 tables were
adjudicated KEEP by S00 day 2. No further table is permitted without a contract change.

⚠️ **Amended 2026-11-09, corrected as built 2026-11-10 (the process model).** The count stays **88 on disk**; migration `0035_process_tree.sql` adds two tables (the `procedure` SOP and `process_map`) and renames `procedure`/`procedure_step` to `process`/`process_step`; `0036_s19_output_grade.sql` is S13's second migration. **`workflow` is retained** (owner ruling 2026-11-09), so nothing is dropped and the earlier "89 once both land" prediction no longer applies. The row above counts the 88 measured on 2026-11-09 — re-count from `migrations/` after `0035` applies rather than quoting it.

### 2.17 Inference ledger and prompt audit — S16 (3)

⚠️ Numbered after §2.16 on purpose: four documents cite "§2.16" as the table ceiling, and renumbering them to gain
a tidy reading order would churn history for nothing. The count in §2.16 already includes these three.

All three are `ENABLE`+`FORCE` ROW LEVEL SECURITY with one `tenant_isolation` policy each, exactly as §2.11's pair.
**`inference_call` is written by the engine, not by a handler** — inside `record_bookkeeping`'s transaction
(CONTRACT §18) — so no action can write a usage row for a call that was not made.

```sql
CREATE TABLE inference_call (
    id                          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id                   uuid NOT NULL DEFAULT app_current_tenant(),
    action_invocation_id        uuid,
    trace_id                    text,
    correlation_id              text,
    caller_kind                 text NOT NULL
                                CHECK (caller_kind IN ('copilot','process_step','workflow_step','agent_run','other')),
    run_id                      uuid,
    run_step_id                 uuid,
    conversation_turn_id        uuid,
    prompt_ref                  text,
    prompt_version              text,
    prompt_sha256               text,
    model                       text NOT NULL,
    served_model                text,
    prompt_tokens               int  NOT NULL DEFAULT 0 CHECK (prompt_tokens >= 0),
    completion_tokens           int  NOT NULL DEFAULT 0 CHECK (completion_tokens >= 0),
    total_tokens                int  NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    cache_creation_input_tokens int  NOT NULL DEFAULT 0 CHECK (cache_creation_input_tokens >= 0),
    cache_read_input_tokens     int  NOT NULL DEFAULT 0 CHECK (cache_read_input_tokens >= 0),
    cache_status                text NOT NULL DEFAULT 'unknown'
                                CHECK (cache_status IN ('hit','write','miss','unknown')),
    latency_ms                  int  NOT NULL DEFAULT 0 CHECK (latency_ms >= 0),
    outcome                     text NOT NULL DEFAULT 'ok'
                                CHECK (outcome IN ('ok','refused','error','timeout')),
    created_at                  timestamptz NOT NULL DEFAULT now(),
    -- "whose call was this" must always be answerable (CONTRACT §18)
    CONSTRAINT inference_call_join_key CHECK (
        caller_kind = 'other'
        OR run_id IS NOT NULL OR run_step_id IS NOT NULL OR conversation_turn_id IS NOT NULL),
    -- the cache definition lives in the database too, so a second writer cannot invent a fifth meaning.
    -- §3's "no CHECK duplicates the function" is about TRANSITIONS; this is a derived-value invariant.
    CONSTRAINT inference_call_cache_status CHECK (
        cache_status = 'unknown'
        OR (cache_status = 'hit'   AND cache_read_input_tokens > 0)
        OR (cache_status = 'write' AND cache_read_input_tokens = 0 AND cache_creation_input_tokens > 0)
        OR (cache_status = 'miss'  AND cache_read_input_tokens = 0 AND cache_creation_input_tokens = 0))
);
CREATE INDEX inference_call_tenant_created_idx ON inference_call (tenant_id, created_at DESC, id);
CREATE INDEX inference_call_cache_idx          ON inference_call (tenant_id, cache_status, created_at DESC);
CREATE INDEX inference_call_run_idx            ON inference_call (tenant_id, run_id) WHERE run_id IS NOT NULL;
CREATE INDEX inference_call_prompt_idx         ON inference_call (tenant_id, prompt_sha256)
    WHERE prompt_sha256 IS NOT NULL;

CREATE TABLE prompt_audit (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    scope         jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(scope::text) <= 8192),
    target_model  text NOT NULL,
    prompt_ref    text NOT NULL,                  -- 'prompt_audit@1'
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','succeeded','failed')),
    finding_count int  NOT NULL DEFAULT 0 CHECK (finding_count >= 0),
    error         text,
    requested_by  uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX prompt_audit_tenant_created_idx ON prompt_audit (tenant_id, created_at DESC, id);

CREATE TABLE prompt_audit_finding (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    audit_id     uuid NOT NULL,
    location     text NOT NULL,                   -- 'process_step:<uuid>' | 'action:<name>' | 'copilot:SYSTEM_PROMPT'
    evidence     text NOT NULL,
    pattern      text NOT NULL CHECK (pattern IN ('1a','1b','1c','1d','1e','1f','2','3','4')),
    why_obsolete text NOT NULL,
    confidence   text NOT NULL CHECK (confidence IN ('high','medium','low')),
    action       text NOT NULL CHECK (action IN ('remove','rewrite','move','replace_with_api_feature','add','flag')),
    replacement  text,
    status       text NOT NULL DEFAULT 'open' CHECK (status IN ('open','accepted','rejected','applied')),
    decided_by   uuid,
    decided_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_audit_finding_audit_fk
        FOREIGN KEY (audit_id) REFERENCES prompt_audit (id),
    -- the skill's Step 6: 'rewrite' and 'add' always carry their replacement text
    CONSTRAINT prompt_audit_finding_replacement
        CHECK (action NOT IN ('rewrite','add') OR (replacement IS NOT NULL AND replacement <> '')),
    -- a decided finding names its decider (CONTRACT §18: every transition is a person's)
    CONSTRAINT prompt_audit_finding_decider
        CHECK (status = 'open' OR decided_by IS NOT NULL)
);
CREATE INDEX prompt_audit_finding_audit_idx ON prompt_audit_finding (tenant_id, audit_id, confidence, id);
```

**Retention:** `inference_call` is an operational trace, pruned at 90 days by `inference_call.prune` (§6 — the
longer window than `action_invocation`'s 30 is deliberate: a cache report over a quarter is the point of the
table). `prompt_audit`/`prompt_audit_finding` follow the procedures they audit — a finding is retained while its
`evidence` still resolves, and the audit's own `prompt_ref` version is what makes an old finding re-readable.

**The audit cadence has no table.** *After N executions* is `tenant.settings.prompt_audit`
`{enabled, after_executions, surfaces, target_model}`, set through the existing `tenants.update_settings`. The
trigger is an **event at capture time** (owner 2026-11-09): the engine counts the tenant's `inference_call` rows
since its last `prompt_audit.created_at` and, at the threshold, inserts the queued audit and its job in the same
transaction. The counter is therefore the ledger itself — there is no `last_counted` column that can drift, which
is the failure mode a separate counter would have. The count excludes `prompt_ref = 'prompt_audit@1'`, so an audit
cannot re-trigger the cadence it is servicing; `prompt_audit_auto_live_idx` makes two calls crossing the threshold
together insert exactly one event-created audit. `target_model` is required for an event-created audit; an enabled
policy without one is not armed.

**`run_id` / `run_step_id` / `conversation_turn_id` carry no FK, on purpose.** A foreign key's RI check runs as the
table owner and **bypasses RLS**, so it would accept another tenant's id — the same trap `departments.create`
records. The engine resolves each id in the caller's tenant before it writes, and the `inference_call_join_key`
CHECK is what keeps "whose call was this" answerable without one. `prompt_audit_finding.audit_id` *is* an FK
(parent and child share the tenant by construction) and is still resolved in-tenant at insert.

### 2.18 Benchmark harness — S17 (4)

⚠️ Numbered after §2.17 for the same reason §2.17 follows §2.16: the count in §2.16 already includes these four,
and four documents cite the sections by number.

All four are `ENABLE`+`FORCE` ROW LEVEL SECURITY with one `tenant_isolation` policy each. **`benchmark_run` and
`benchmark_case` are written only by the engine and the run driver**, never by an action handler, so they carry no
`version` column — the S16 `prompt_audit` exception, stated here rather than left implicit. `resolved_rate` is not
a column: it is `n_resolved / n_graded`, derived by the read models.

```sql
CREATE TABLE benchmark_suite (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    slug              text NOT NULL,
    dataset           text NOT NULL,              -- e.g. SWE-bench/SWE-bench_Verified
    dataset_revision  text NOT NULL,              -- the HF revision the seed was cut from
    seed_sha256       text NOT NULL,              -- hash of seed/benchmark/<slug>.json
    image_lock_sha256 text NOT NULL,              -- hash of data/images-<slug>.lock.json
    swebench_version  text NOT NULL,              -- the semantics the grader reproduces
    parser_version    text NOT NULL,              -- the parser set's own version
    baseline_run_id   uuid,                       -- set by benchmark_runs.promote; no FK (see below)
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_suite_slug UNIQUE (tenant_id, slug)
);

CREATE TABLE benchmark_instance (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    suite_id          uuid NOT NULL REFERENCES benchmark_suite (id),
    instance_id       text NOT NULL,              -- e.g. django__django-16493
    repo              text NOT NULL,
    base_commit       text NOT NULL,
    image             text NOT NULL,              -- registry path @ sha256 digest
    problem_statement text NOT NULL,
    oracle_files      text[] NOT NULL DEFAULT '{}',  -- the paths the gold patch touches
    gold_patch        text NOT NULL,
    test_patch        text NOT NULL,
    fail_to_pass      text[] NOT NULL,
    pass_to_pass      text[] NOT NULL,
    eval_script       text NOT NULL,              -- precomputed, deterministic for the pinned dataset
    parser            text NOT NULL,              -- the repo's parser key
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_instance_key UNIQUE (suite_id, instance_id)
);
CREATE INDEX benchmark_instance_suite_idx ON benchmark_instance (tenant_id, suite_id);

CREATE TABLE benchmark_run (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    suite_id            uuid NOT NULL REFERENCES benchmark_suite (id),
    arm                 text NOT NULL
                        CHECK (arm IN ('gold','empty','baseline','conveyor')),
    subject_kind        text NOT NULL DEFAULT 'suite'
                        CHECK (subject_kind IN ('suite','capability_agent','procedure')),
    capability_agent_id uuid,                    -- forward reference, no FK until S15 lands
    procedure_id        uuid,                    -- forward reference, no FK until S11's table is the target
    label               text NOT NULL,           -- the predictions label, e.g. conveyor-oracle-files+failing-test
    model               text,
    status              text NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued','running','succeeded','failed','cancelled')),
    n                   int NOT NULL DEFAULT 0 CHECK (n >= 0),
    n_graded            int NOT NULL DEFAULT 0 CHECK (n_graded >= 0),
    n_resolved          int NOT NULL DEFAULT 0 CHECK (n_resolved >= 0),
    n_no_attempt        int NOT NULL DEFAULT 0 CHECK (n_no_attempt >= 0),
    counts              jsonb NOT NULL DEFAULT '{}',   -- the full status histogram, read whole
    control_ok          boolean,                 -- NULL for non-control arms
    cancel_requested_at timestamptz,
    last_polled_at      timestamptz,             -- the driver's heartbeat
    started_at          timestamptz,
    finished_at         timestamptz,
    error               text,
    environment         jsonb NOT NULL DEFAULT '{}',
    options             jsonb NOT NULL DEFAULT '{}',
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_run_subject CHECK (
        (subject_kind = 'suite' AND capability_agent_id IS NULL AND procedure_id IS NULL)
        OR (subject_kind = 'capability_agent' AND capability_agent_id IS NOT NULL)
        OR (subject_kind = 'procedure' AND procedure_id IS NOT NULL)),
    CONSTRAINT benchmark_run_arm_subject CHECK (arm <> 'conveyor' OR subject_kind <> 'suite')
);
CREATE INDEX benchmark_run_tenant_created_idx ON benchmark_run (tenant_id, created_at DESC, id);
CREATE INDEX benchmark_run_suite_idx          ON benchmark_run (tenant_id, suite_id, created_at DESC);

CREATE TABLE benchmark_case (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    benchmark_run_id  uuid NOT NULL REFERENCES benchmark_run (id),
    instance_id       text NOT NULL,
    produce_status    text NOT NULL DEFAULT 'pending'
                      CHECK (produce_status IN ('pending','ok','fail','no_attempt')),
    attempts          int NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    patch             text,
    patch_sha256      text,
    verdict           text NOT NULL DEFAULT 'pending'
                      CHECK (verdict IN ('pending','resolved','unresolved','apply_failed','eval_timeout','infra_error')),
    seconds           numeric CHECK (seconds IS NULL OR seconds >= 0),
    sandbox_ref       text,                       -- the k8s Job name
    log_document_id   uuid,                       -- S09 document; no FK (see below)
    detail            jsonb NOT NULL DEFAULT '{}',
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_case_key UNIQUE (benchmark_run_id, instance_id),
    CONSTRAINT benchmark_case_patch_cap CHECK (patch IS NULL OR octet_length(patch) <= 262144)
);
CREATE INDEX benchmark_case_run_idx     ON benchmark_case (tenant_id, benchmark_run_id, verdict, instance_id);
```

**`baseline_run_id`, `capability_agent_id`, `procedure_id` and `log_document_id` carry no FK**, for the reason
§2.17 records: a foreign key's RI check runs as the table owner and bypasses RLS. Each is resolved inside the
caller's tenant before it is written. The `benchmark_run_subject` CHECK is what keeps "which executor did this run
measure" answerable without one. **`suite_id` and `benchmark_run_id` *are* FKs** — parent and child share the
tenant by construction — the same shape as `prompt_audit_finding.audit_id`.

**The `patch` cap is 256 KiB** — the `revision.content` precedent — enforced in the domain *and* by the CHECK;
an over-cap patch is a typed 422 naming the instance, never a truncation. The assembled sandbox ConfigMap
(`patch.diff` + `eval.sh` + `run.sh`) is capped at 1 MiB at run time, the k8s limit.

### 2.19 Eval builder — S19 (10)

All ten are `ENABLE`+`FORCE` ROW LEVEL SECURITY with one `tenant_isolation` policy each, exactly as §2.17's set.
The aggregate is `eval` (the flow) → `eval_version` (a frozen instrument) → `eval_case`/`eval_metric`, with
`eval_run` → `eval_case_result`/`eval_attempt_error`/`eval_trace` on the measurement side and
`eval_audit` → `eval_audit_finding` beside it. **`eval_version` is what makes two numbers comparable**: a run
names a version, and a comparison across versions, graders or splits is refused (CONTRACT §20).

**One ALTER precedes the ten** — the sign-offs are `approval` rows (§20), and `uq_appr_pending` cannot cover them
because `run_id` is NULL there and Postgres treats NULLs as distinct. This is the CCR recorded in `00-INDEX.md`'s
decision register; it is additive and nullable, so no existing row, query or index changes:

```sql
ALTER TABLE approval ADD COLUMN eval_id uuid;
CREATE UNIQUE INDEX uq_appr_eval_pending
    ON approval (tenant_id, eval_id, gate) WHERE status = 'pending' AND eval_id IS NOT NULL;
```

⚠️ **The index alone is not the guarantee until the column exists.** Measured 2026-11-09 on a throwaway
Postgres 18: with the ALTER omitted, two pending `eval_inputs` gates on one eval are **both accepted**, and a
probe that checks "the second insert failed" reports a pass for the wrong reason — the error is
`column "eval_id" does not exist`, not a unique violation. A gate line must assert the constraint *name*.

```sql
CREATE TABLE eval (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    name          text NOT NULL CHECK (name <> ''),
    purpose       text NOT NULL DEFAULT '',
    subject_kind  text NOT NULL
                  CHECK (subject_kind IN ('completions','copilot','procedure','external','agent_run')),
    entry_action  text NOT NULL,
    model_ref     text,
    prompt_ref    text,
    inputs_schema jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(inputs_schema::text) <= 8192),
    trace_source  text NOT NULL DEFAULT 'synthesized'
                  CHECK (trace_source IN ('synthesized','conversation_turn','run_step')),
    -- 'draft' | 'running' | 'climbing' | 'retired'. There is deliberately NO 'ready':
    -- readiness is computed from the two approvals at start time, so no column can
    -- disagree with the sign-off rows it summarises (the S16 counter lesson).
    status        text NOT NULL DEFAULT 'draft'
                  CHECK (status IN ('draft','running','climbing','retired')),
    current_version_id uuid,
    created_by    uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX eval_tenant_idx ON eval (tenant_id, created_at DESC, id);

CREATE TABLE eval_version (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id         uuid NOT NULL,
    version         int  NOT NULL CHECK (version >= 1),
    digest          text NOT NULL,                 -- sha256 over the frozen instrument
    -- NO case_count / train_count: a maintained counter drifts from the rows it counts
    -- (the §2.17 lesson). Counts are read-time, and the split lock is the real fact.
    split_sha256    text,
    split_locked_at timestamptz,
    prices          jsonb NOT NULL DEFAULT '{}'::jsonb,
    harness_paths   text[] NOT NULL DEFAULT '{}',
    created_by      uuid,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_version_eval_fk FOREIGN KEY (eval_id) REFERENCES eval (id),
    CONSTRAINT eval_version_unique UNIQUE (tenant_id, eval_id, version),
    -- a locked split names when it was drawn; readiness is the predicate, not this table
    CONSTRAINT eval_version_split_pair CHECK ((split_sha256 IS NULL) = (split_locked_at IS NULL))
);
CREATE INDEX eval_version_eval_idx ON eval_version (tenant_id, eval_id, version DESC);

CREATE TABLE eval_case (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    version_id      uuid NOT NULL,
    ordinal         int  NOT NULL CHECK (ordinal >= 0),
    input           text NOT NULL CHECK (input <> ''),
    tags            text[] NOT NULL DEFAULT '{}',       -- tags[0] is the grouping key
    expected        jsonb,                              -- WRITE-ONLY: no view returns it
    attachments     jsonb NOT NULL DEFAULT '[]'::jsonb,
    source          text NOT NULL DEFAULT 'handwritten'
                    CHECK (source IN ('production','ticket','handwritten','synthesized')),
    gold_provenance text NOT NULL DEFAULT 'human_written'
                    CHECK (gold_provenance IN ('human_written','human_verified','model')),
    gold_model      text,
    split           text NOT NULL DEFAULT 'none' CHECK (split IN ('train','test','none')),
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_case_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_case_ordinal_unique UNIQUE (tenant_id, version_id, ordinal),
    -- model-derived gold names its model; a golden set that cannot name its source cannot be trusted
    CONSTRAINT eval_case_gold_model CHECK (gold_provenance <> 'model' OR gold_model IS NOT NULL),
    CONSTRAINT eval_case_attachments_cap CHECK (length(attachments::text) <= 16384)
);
CREATE INDEX eval_case_version_idx ON eval_case (tenant_id, version_id, ordinal);
CREATE INDEX eval_case_split_idx   ON eval_case (tenant_id, version_id, split);

CREATE TABLE eval_metric (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    version_id  uuid NOT NULL,
    key         text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_]*$'),   -- the grade dict key
    role        text NOT NULL CHECK (role IN ('quality','perf')),
    kind        text NOT NULL CHECK (kind IN ('binary','judge','float')),
    better      text NOT NULL DEFAULT 'higher' CHECK (better IN ('higher','lower')),
    -- the full viewer's legend truncates; qualifiers belong in the rubric, not the label
    label       text NOT NULL CHECK (char_length(label) <= 14),
    unit        text,
    grader_kind text NOT NULL CHECK (grader_kind IN ('programmatic','pairwise','rubric','human','none')),
    grader      jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(grader::text) <= 8192),
    rubric      text,
    ordinal     int  NOT NULL CHECK (ordinal >= 0),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_metric_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_metric_key_unique UNIQUE (tenant_id, version_id, key),
    -- a perf field is measured, never graded
    CONSTRAINT eval_metric_perf_ungraded CHECK (role = 'quality' OR grader_kind = 'none'),
    -- a rubric or pairwise grader always has its rubric text
    CONSTRAINT eval_metric_rubric_required
        CHECK (grader_kind NOT IN ('rubric','pairwise') OR (rubric IS NOT NULL AND rubric <> ''))
);
CREATE INDEX eval_metric_version_idx ON eval_metric (tenant_id, version_id, ordinal);

CREATE TABLE eval_run (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id       uuid NOT NULL,
    version_id    uuid NOT NULL,
    -- the report builder silently ignores anything else; here it is impossible
    variant       text NOT NULL CHECK (variant = 'baseline' OR variant ~ '^v[0-9]+$'),
    round         int  NOT NULL DEFAULT 0 CHECK (round >= 0),
    mode          text NOT NULL DEFAULT 'full' CHECK (mode IN ('pilot','full')),
    reps          int  NOT NULL DEFAULT 1 CHECK (reps >= 1),
    case_count    int  NOT NULL DEFAULT 0 CHECK (case_count >= 0),
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','partial','complete','void','cancelled')),
    change_md     text,
    change_patch  text,
    summary       jsonb NOT NULL DEFAULT '{}'::jsonb,   -- {description,target,suspicious}
    narrative_md  text,
    environment   jsonb NOT NULL DEFAULT '{}'::jsonb,   -- repo sha, model ids, stubs, flags
    noise_floor   numeric,
    cost_ceiling_cents int CHECK (cost_ceiling_cents >= 0),
    last_polled_at timestamptz,
    started_at    timestamptz,
    finished_at   timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_run_eval_fk    FOREIGN KEY (eval_id)    REFERENCES eval (id),
    CONSTRAINT eval_run_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_run_variant_unique UNIQUE (tenant_id, eval_id, variant),
    CONSTRAINT eval_run_patch_cap CHECK (change_patch IS NULL OR length(change_patch) <= 262144)
);
CREATE INDEX eval_run_eval_idx   ON eval_run (tenant_id, eval_id, round DESC);
CREATE INDEX eval_run_status_idx ON eval_run (tenant_id, status) WHERE status IN ('queued','running','partial');

CREATE TABLE eval_case_result (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id          uuid NOT NULL,
    case_id              uuid NOT NULL,
    rep                  int  NOT NULL CHECK (rep >= 0),
    status               text NOT NULL CHECK (status IN ('ok','truncated','pending_label')),
    stop_reason          text,
    grade                jsonb NOT NULL,                -- {metric_id: number}; never a bare scalar here
    explanation          jsonb NOT NULL DEFAULT '{}'::jsonb,
    model                text NOT NULL,                 -- from the RESPONSE, not from config
    usage                jsonb NOT NULL DEFAULT '{}'::jsonb,
    latency_ms           int  NOT NULL DEFAULT 0 CHECK (latency_ms >= 0),
    tool_calls           int  NOT NULL DEFAULT 0 CHECK (tool_calls >= 0),
    action_invocation_id uuid,
    subject_run_id       uuid,                          -- the `run` this case produced, if any
    judge_model          text,
    judge_usage          jsonb,
    human_score          numeric,                       -- the judge-calibration label
    created_at           timestamptz NOT NULL DEFAULT now(),
    -- ONE row per slot, and a failed attempt never writes here (see eval_attempt_error)
    CONSTRAINT eval_case_result_slot UNIQUE (tenant_id, eval_run_id, case_id, rep),
    -- the method's rule: with more than one quality metric the grade MUST be the dict form,
    -- or every metric column renders as a dash
    CONSTRAINT eval_case_result_grade_object CHECK (jsonb_typeof(grade) = 'object'),
    CONSTRAINT eval_case_result_label CHECK (status <> 'pending_label' OR human_score IS NULL)
);
CREATE INDEX eval_case_result_run_idx  ON eval_case_result (tenant_id, eval_run_id, case_id, rep);
CREATE INDEX eval_case_result_slow_idx ON eval_case_result (tenant_id, eval_run_id, latency_ms DESC);

CREATE TABLE eval_attempt_error (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id   uuid NOT NULL,
    case_id       uuid NOT NULL,
    rep           int  NOT NULL CHECK (rep >= 0),
    attempt       int  NOT NULL CHECK (attempt >= 1),
    failure_class text NOT NULL
                  CHECK (failure_class IN ('refusal','harness','serving','timeout',
                                           'served_model_mismatch','grader')),
    retry_count   int  NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    model         text,
    usage         jsonb,                                -- billed-but-failed spend is still spend
    message       text NOT NULL,
    deadline_at   timestamptz,                          -- the per-case ceiling that fired, if any
    created_at    timestamptz NOT NULL DEFAULT now(),
    -- APPEND-ONLY across resumes: one row per failed attempt. A later success writes
    -- eval_case_result and leaves these in place, superseded but never deleted.
    CONSTRAINT eval_attempt_unique UNIQUE (tenant_id, eval_run_id, case_id, rep, attempt)
);
CREATE INDEX eval_attempt_error_run_idx ON eval_attempt_error (tenant_id, eval_run_id, failure_class);

CREATE TABLE eval_trace (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id uuid NOT NULL,
    case_id     uuid NOT NULL,
    rep         int  NOT NULL CHECK (rep >= 0),
    turns       jsonb NOT NULL,                        -- [{role,content,thinking?,name?,attachments?}]
    sha256      text NOT NULL,
    bytes       int  NOT NULL CHECK (bytes >= 0 AND bytes <= 1048576),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_trace_slot UNIQUE (tenant_id, eval_run_id, case_id, rep),
    CONSTRAINT eval_trace_turns_array CHECK (jsonb_typeof(turns) = 'array'),
    -- a role outside the method's five is a writer bug, not data
    CONSTRAINT eval_trace_roles CHECK (NOT jsonb_path_exists(turns, '$[*] ? (@.role != "system" &&
        @.role != "user" && @.role != "assistant" && @.role != "tool_call" && @.role != "tool_result")'))
);

CREATE TABLE eval_audit (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id       uuid NOT NULL,
    version_id    uuid NOT NULL,
    target_run_id uuid,
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','succeeded','failed')),
    oracle_score  numeric,                              -- the must-pass control, expected ~1.0
    null_score    numeric,                              -- the must-fail control, expected ~0.0
    noise_floor   numeric,
    headroom      numeric,
    summary       jsonb NOT NULL DEFAULT '{}'::jsonb,
    error         text,
    requested_by  uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_audit_eval_fk FOREIGN KEY (eval_id) REFERENCES eval (id)
);
CREATE INDEX eval_audit_eval_idx ON eval_audit (tenant_id, eval_id, created_at DESC);

CREATE TABLE eval_audit_finding (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    audit_id    uuid NOT NULL,
    section     text NOT NULL CHECK (section IN ('1','2','3','4','5','6')),
    item        text NOT NULL,                          -- the checklist key, e.g. 'no_answer_is_not_negative'
    severity    text NOT NULL CHECK (severity IN ('misleading','noise','generality')),
    evidence    text NOT NULL CHECK (evidence <> ''),
    suggestion  text NOT NULL CHECK (suggestion <> ''),
    status      text NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','accepted','fixed','dismissed')),
    decided_by  uuid,
    decided_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_audit_finding_audit_fk FOREIGN KEY (audit_id) REFERENCES eval_audit (id),
    CONSTRAINT eval_audit_finding_decider CHECK (status = 'open' OR decided_by IS NOT NULL)
);
CREATE INDEX eval_audit_finding_audit_idx ON eval_audit_finding (tenant_id, audit_id, severity, id);
```

**`eval_run_id`, `case_id` and `subject_run_id` carry no FK**, for the §2.17 reason: an RI check runs as the
table owner and bypasses RLS, so it would accept another tenant's id. `eval_version.eval_id`,
`eval_case.version_id`, `eval_metric.version_id`, `eval_run.eval_id`/`version_id` and
`eval_audit_finding.audit_id` *are* FKs — parent and child share the tenant by construction, the same shape as
`prompt_audit_finding.audit_id`. **`subject_run_id` never points at a `benchmark_run`**: S19 has its own run
notion and overloading one column with two ids would make the cost join ambiguous.

**`expected` is write-only.** It is read by the grader and by nothing else — no `CaseView`, no export of a case
list, no agent-reachable action. The subject-input builder is constructed from `input` alone, so a handler cannot
pass the answer key by accident.

### 2.20 Eval hillclimb — S20 (1)

One table, `ENABLE`+`FORCE` RLS with a `tenant_isolation` policy. It is the method's `_state.json`: the loop's
goal, stopping condition, split reference and best-so-far. **Rounds are `eval_run` rows** (`variant='v<N>'`,
`round`, `change_md`, `change_patch`, `summary.suspicious`, `narrative_md`) — S19 ships those columns and S20
ships only the loop.

```sql
CREATE TABLE eval_climb (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id            uuid NOT NULL,
    version_id         uuid NOT NULL,
    -- {target, direction, hold[]}; the search order for a cost goal is state here too,
    -- so a resumed session does not restart the staircase at the top
    goal               jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(goal::text) <= 8192),
    direction          text NOT NULL DEFAULT 'higher' CHECK (direction IN ('higher','lower')),
    reps               int  NOT NULL DEFAULT 1 CHECK (reps >= 1),
    stopping           text NOT NULL DEFAULT 'plateau' CHECK (stopping IN ('plateau','rounds','manual')),
    plateau_k          int  NOT NULL DEFAULT 3 CHECK (plateau_k >= 3),  -- two flat rounds is too few
    max_rounds         int  CHECK (max_rounds >= 1),
    delta              numeric NOT NULL CHECK (delta > 0),              -- set above the noise floor
    off_limits         text[] NOT NULL DEFAULT '{}',
    approve_each_round boolean NOT NULL DEFAULT true,
    cost_ceiling_cents int CHECK (cost_ceiling_cents >= 0),
    current_round      int  NOT NULL DEFAULT 0 CHECK (current_round >= 0),
    best_round         int,
    best_test_score    numeric,
    -- the final report, written by `eval_climbs.stop`. Added by migration 0029: the one column
    -- the loop state cannot derive, and the reason this table's DDL and its migration differ.
    narrative_md       text,
    status             text NOT NULL DEFAULT 'planned'
                       CHECK (status IN ('planned','running','plateaued','stopped')),
    created_by         uuid,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_climb_eval_fk    FOREIGN KEY (eval_id)    REFERENCES eval (id),
    CONSTRAINT eval_climb_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_climb_rounds CHECK (stopping <> 'rounds' OR max_rounds IS NOT NULL)
);
CREATE INDEX eval_climb_eval_idx ON eval_climb (tenant_id, eval_id, created_at DESC);
```

**There is no `spend` column.** Cumulative spend is `sum` over the climb's `eval_case_result.usage` at read time,
plus the billed `usage` on `eval_attempt_error` rows — a maintained counter is the one thing that can drift from
the rows it counts, and the method says so.

**Rounds are runs, and a re-grade is a round's sibling.** A round is an `eval_run` row (`variant='v<N>'`,
`round`, `change_md`, `change_patch`, `summary.decision`), so the climb needs no state table of its own.
`eval_runs.regrade` adds no table either: it writes another `eval_run` whose `summary.regraded_from` names the
source, and its slots are ordinary `eval_case_result` rows. The one thing it adds to `eval_trace` is the
captured `output` on the assistant turn — a field inside the existing `turns` JSONB, which is why no migration
accompanies it.

### 2.21 Prompt catalogue — D27 (2)

Two tables, `ENABLE`+`FORCE` RLS with a `tenant_isolation` policy. A prompt is a **recipe**, not a stored
text: `prompt` names a versioned key and `prompt_section` is its ordered layout. The compiled text is never
stored — `inference_call.prompt_ref`/`prompt_version`/`prompt_sha256` name the recipe and hash every call,
and a second copy would drift from the sources it was built from.

```sql
CREATE TABLE prompt (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    key         text NOT NULL,
    version     int  NOT NULL CHECK (version >= 1),
    status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active','retired')),
    title       text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_key_not_blank   CHECK (btrim(key) <> ''),
    CONSTRAINT prompt_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT prompt_tenant_key_version_key UNIQUE (tenant_id, key, version)
);
-- At most one active version per key; retired rows stay addressable by exact pin.
CREATE UNIQUE INDEX uq_prompt_active ON prompt (tenant_id, key) WHERE status = 'active';

CREATE TABLE prompt_section (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    prompt_id   uuid NOT NULL,
    position    int  NOT NULL CHECK (position >= 1),
    source_kind text NOT NULL
                CHECK (source_kind IN ('system','procedure','capability','project_card','knowledge')),
    required    boolean NOT NULL DEFAULT false,
    template    text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_section_prompt_fk
        FOREIGN KEY (prompt_id) REFERENCES prompt (id) ON DELETE CASCADE,
    CONSTRAINT prompt_section_position_key UNIQUE (prompt_id, position),
    CONSTRAINT prompt_section_system_has_text
        CHECK (source_kind <> 'system' OR btrim(template) <> '')
);
```

**`system` is the only literal kind.** The other four are rendered from the run's facts — the process (stored as a
process) and its step, the capability/agent row, the project and card, and the capability's bound packs. A
`required` section with no source is a readiness refusal, not an empty section. The built-in recipes
(`copilot_system@1`, `prompt_audit@1`, `agent@1`) are seeded per tenant by `seed::prompts_for_tenant`; an
existing `(key, version)` is never rewritten by a seed.

## 3. State machines and enforcement

Every transition is a pure function in `crates/domain`; no DB trigger, no per-flow rule, no CHECK that
duplicates the function. The CHECK constraints below guard *vocabulary*, never *legality*.

### 3.1 Card lifecycle

| | |
|---|---|
| States | not a column. `list.kind ∈ {intake, active, waiting, done, archive}` plus `card.archived_at` |
| Transitions | any move allowed by `domain::board::may_move(card_type_id, from_kind, to_kind)`; setting `archived_at` is the only other lifecycle act |
| Enforced | `domain::board::may_move` — one function, called by the API, by actions, and by the procedure runner |
| Stored | `card.list_id`, `card.prev_list_id`, `card.list_changed_at`, `card.archived_at`, `card.version` |
| Invariant | `UPDATE card SET list_id=$new, prev_list_id=list_id, list_changed_at=now(), version=version+1 WHERE id=$1 AND tenant_id=$2 AND version=$3` — 0 rows ⇒ 412; a move can never be applied to a card that changed since it was read |

`may_move` reads the matrix from `catalogue_entry(kind='project_type').meta.matrix` for the project's
`project_type_code`; the 280 generated card-type×list flows become this one lookup.

### 3.2 Run

| | |
|---|---|
| States | `queued → running → waiting_approval → running → succeeded \| failed \| cancelled` |
| Transitions | `queued→running`, `queued→cancelled`, `running→waiting_approval`, `running→succeeded`, `running→failed`, `running→cancelled`, `waiting_approval→running`, `waiting_approval→cancelled`; `succeeded`/`failed`/`cancelled` are terminal |
| Enforced | `domain::run::transition(from, to) -> Result<Run, RunError>`; the store layer refuses a status the function rejects |
| Stored | `run.status`, `started_at`, `finished_at`, `attempt`, `error_class`, `error_message` |
| Invariant | **a run is a tree (2026-11-09).** Partial unique `(tenant_id, card_id) WHERE parent_run_id IS NULL AND status IN ('queued','running','waiting_approval')` — **one live ROOT run per card**; the project form is `(tenant_id, project_id) WHERE parent_run_id IS NULL AND card_id IS NULL AND status IN (…)`; one live child per `(parent_run_id, step_id)` and one row per `(parent_run_id, step_id, attempt)`. `CHECK (parent_run_id IS NULL OR step_id IS NOT NULL)` and `CHECK (depth BETWEEN 0 AND 3)`. Cancel cascades from a parent to its children; a child's failure does not re-fire the parent's `err.card`. Enforced by the DB even if two API pods race |

### 3.3 Approval

| | |
|---|---|
| States | `pending → approved \| rejected \| expired \| voided` (`voided` is a withdrawn gate) |
| Transitions | `pending→approved` when `count(approval_decision where decision='approve') >= required_decisions`; `pending→rejected` on any reject; `pending→expired` by the expiry job at `expires_at`; `pending→voided` by `runs.cancel` — the gate is withdrawn and will never be answered |
| Enforced | `domain::approval::decide`; expiry is a `job` kind (`approval.expire`), not a timer |
| Stored | `approval.status`, `required_decisions`, `expires_at`; `approval_decision(approver_id, decision, note, decided_by_kind, via)` |
| Invariant | partial unique `(tenant_id, run_id, gate) WHERE status='pending'` — **at most one pending per (run, gate)**; unique `(approval_id, approver_id)` — one person cannot satisfy dual control twice |

### 3.4 Requirement (and every `governed_asset`)

| | |
|---|---|
| States | `draft → in_review → approved → superseded`; `in_review → draft` when changes are requested; `archived` is an operator act from any non-superseded state |
| Transitions | `domain::governance::transition(kind, from, to)`; `approved` is required before a `superseded_by` may be set |
| Stored | `governed_asset.status`, `version` (bigint), `supersedes_id`, `superseded_by_id`, `approved_by/at` in `meta` |
| Invariant | every transition writes a `revision` row with `seq = version` in the same transaction. The FK on `superseded_by_id` keeps the chain from dangling; `check (superseded_by_id is null or superseded_by_id <> id)` forbids a self-cycle; `check (status <> 'superseded' or superseded_by_id is not null)` makes `superseded ⇒ superseded_by_id` a DB guarantee — PostgreSQL CHECK constraints are immediate, so both pointers are set in the one `UPDATE`. The reciprocal edge (`supersedes_id`/`superseded_by_id` agree) is domain-enforced by `governed_asset.supersede`, **not** a DEFERRED constraint |

### 3.5 Procedure

| | |
|---|---|
| States | `draft → published → retired`. `published` is immutable: an edit forks `version+1` in `draft` |
| Enforced | `domain::process::publish`; publishing validates that every `process_step.action` exists in the registry and that referenced gates are in `catalogue_entry(kind='gate')` |
| Stored | `procedure.status`, `procedure.version`, `process_step` rows (never mutated after publish, only copied forward) |
| Invariant | a `run.procedure_id` must point at a `published` procedure (FK + CHECK via the domain; the partial unique index `uq (tenant_id, key) WHERE status='published'` guarantees at most one live version) |

### 3.6 Delivery

| | |
|---|---|
| States | `pending → sent → delivered \| failed \| expired` |
| Enforced | `domain::channel::deliver`; the drainer is a `job` kind (`delivery.send`), at-least-once, idempotent on `delivery.id` |
| Stored | `delivery.state`, `attempts`, `last_error`, `sent_at`, `delivered_at` |
| Invariant | unique `(message_id, channel_kind, recipient_ref)` — a retried send cannot create a second row, so "delivered once" is a key, not a convention |

### 3.7 Prompt audit (S16)

| | |
|---|---|
| States | `prompt_audit`: `queued → running → succeeded \| failed`. `prompt_audit_finding`: `open → accepted \| rejected → applied` |
| Enforced | `domain::prompt_audit`; the run is one `prompt_audit.run` job, whose inventory and signal scan are code and whose judgement is one bounded `completions.create`. Only `prompt_findings.decide` moves a finding out of `open`, and the CHECK in §2.17 requires a `decided_by` for every such move |
| Stored | `prompt_audit.status`, `finding_count`, `prompt_ref`; `prompt_audit_finding.status`, `decided_by`, `decided_at`, `replacement` |
| Invariant | **`applied` is never reachable from the audit.** No action in this slice holds `procedures:write`; an applied finding records that some later `procedures.update` changed the surface, and the finding keeps its `evidence` as the before-image. A finding cannot point at a surface the audit did not inventory — `location` is written by the scanner from the same list `scope` names |

### 3.8 Benchmark runs (S17)

| | |
|---|---|
| States | `benchmark_run`: `queued → running → succeeded \| failed \| cancelled` (`cancel_requested_at` is a stamp, not a state). `benchmark_case`: `produce_status pending → ok \| fail \| no_attempt`; `verdict pending → resolved \| unresolved \| apply_failed \| eval_timeout \| infra_error` |
| Enforced | `domain::benchmark`: the arm/verdict vocabulary; the verdict ladder of CONTRACT §19 (apply marker → eval marker → timeout marker → parsed statuses → infra/end-marker guard); the 256 KiB patch cap; the McNemar pairing. The single driver `benchmark.run.poll` is the only writer of a run's progress; `benchmark.reap` fails a run whose `last_polled_at` is stale |
| Stored | `benchmark_run.status`, the four counters, `counts`, `control_ok`, `last_polled_at`, `finished_at`, `error`; `benchmark_case.produce_status`, `verdict`, `patch`, `sandbox_ref`, `log_document_id`, `detail` |
| Invariant | **A case is final only once.** The verdict is written by the driver in the same transaction that records the log document id; a re-poll of a finished case is a no-op, and a sandbox Job name is deterministic so a retried create is a reattach. A run is `succeeded` only when every case is final; `control_ok=false` never becomes `failed` — the data exists and its interpretation is refused downstream |

### 3.9 Eval (S19)

| | |
|---|---|
| States | `eval`: `draft → running ⇄ climbing → retired`. **No `ready` state** — readiness is computed at `eval_runs.start` from the two approvals. `eval_run`: `queued → running → partial ⇄ running → complete \| void`, plus `cancelled`; a pilot is `mode='pilot'` on the `baseline` variant and is superseded by the full pass, never deleted. `eval_audit`: `queued → running → succeeded \| failed`; `eval_case_result`: `pending_label → ok` after a human label |
| Enforced | `domain::eval`: the subject-kind vocabulary; the readiness predicate (`blocking[]` names each missing gate, refusal `409 eval_not_ready`); the variant grammar; the retry policy. **One writer per slot**: `eval.case.run` is the only writer of `eval_case_result`/`eval_trace`, and only the attempt that produced a scorable output writes it. `eval.run.tick` finalises; `eval.reap` fails a run whose `last_polled_at` is stale — never one whose cases are merely silent (the S17 lesson) |
| Stored | `eval.current_version_id`; `eval_version.digest`/`split_sha256`/`split_locked_at`; `eval_run.status`/`round`/`noise_floor`/`last_polled_at`; `eval_case_result.grade`/`model`/`usage`/`status`; `eval_attempt_error.failure_class`/`attempt` |
| Invariant | **A sign-off is bound to an artifact, not to a moment.** The approval carries `meta.artifact_sha256`; an edit to the version's cases or metrics changes the digest and the gate reads `void` at the next start. A sign-off whose digest no longer matches is never "still valid because nobody re-asked". |
| Invariant | **A failed attempt cannot occupy the slot.** `eval_attempt_error` is append-only and never shares the `(eval_run, case, rep)` uniqueness of `eval_case_result`, so resume re-runs exactly what produced no output and never re-uses a row whose score and trace came from different calls. |
| Invariant | **`grade` is an object, always** (CHECK `jsonb_typeof = 'object'`), because a scalar beside two declared metrics renders as a dash in every column and would be read as a zero. |

### 3.10 Eval climb (S20)

| | |
|---|---|
| States | `eval_climb`: `planned → running → plateaued \| stopped`. A round is not a state of the climb: it is an `eval_run` whose `round` is `current_round + 1` |
| Enforced | `domain::eval::climb`: the keep/revert rule (train up + test flat ⇒ revert; both up ⇒ keep; train down + test up ⇒ keep only if it repeats; tie ⇒ the later round); the stopping predicate at a `delta` above the noise floor; the split is read-only (a climb cannot redraw it); a comparison across versions, graders or splits is refused; after two rounds that do not clear the band the next round **must** carry categorisation buckets and a content diff is refused |
| Stored | `eval_climb.goal`/`stopping`/`plateau_k`/`delta`/`current_round`/`best_round`/`best_test_score`; per round, `eval_run.change_md`/`change_patch`/`summary.suspicious`/`narrative_md` |
| Invariant | **The held-out wall is access, not convention.** `eval_climbs.round` records the trace ids it handed to the analyzer; a round citing a `test`-split id is void, and reading a test trace requires `evals:approve` — which an `agent` principal does not hold. |

## 4. Concurrency and access paths

**Version / If-Match.** Every mutating statement is
`UPDATE <t> SET …, version = version + 1, updated_at = now() WHERE id = $1 AND tenant_id = $2 AND version = $3`
with `RETURNING *`. Zero rows is unambiguously either 404 or 412 — the handler re-reads once to distinguish,
and returns the current representation in the problem body. Creation returns `201` + `ETag: "<version>"`.

**The card claim is one conditional UPDATE — no lease table.**

```sql
UPDATE card
   SET assignee_id = $3, lease_expires_at = now() + $4::interval,
       lease_token = uuidv7(), claim_fence = claim_fence + 1,
       version = version + 1, updated_at = now()
 WHERE tenant_id = $1 AND id = $2 AND version = $5
   AND (lease_expires_at IS NULL OR lease_expires_at < now() OR assignee_id = $3)
RETURNING claim_fence, version;
```

Zero rows = not granted. There is **no fallback arm and no `granted: true` on a DB error** — the
closed-degradation argument in `fr/backend/migrations/100_card_phase_lease.sql` §3.4 is carried verbatim as a
principle: *a lease that grants itself when the arbiter is down is not a lease*. `claim_fence` is monotonic
and never reset (the card row survives release), which is what lets a holder that slept past its TTL
discover the card moved beneath it — the ABA window fr's `card_phase_lease` existed to close. Waiters are
not a table: a blocked claimant enqueues `job(kind='card.claim')`, deduped on `(tenant_id, 'card.claim', card_id)`.

**Job claim** — `FOR UPDATE SKIP LOCKED`, fenced:

```sql
UPDATE job SET status='running', lease_token=uuidv7(), lease_until=now()+interval '60 s',
               attempts=attempts+1
 WHERE id = (SELECT id FROM job
              WHERE tenant_id=$1 AND status='pending' AND run_after <= now()
              ORDER BY run_after, id
              FOR UPDATE SKIP LOCKED LIMIT 1)
RETURNING *;
```

A handler must present `lease_token` on completion; a stale token writes nothing. `schedule` materialisation
runs under `pg_advisory_xact_lock(hashtext('conveyor:schedule'))` so two pods cannot double-materialise.

**SSE replay.** `pg_notify('conveyor_event', seq::text)` fires in the mutation transaction. A subscriber
reads `SELECT … FROM event WHERE topic = ANY($topics) AND seq > $last ORDER BY seq LIMIT 500`, using the
`Last-Event-ID` header. Replay is bounded by that `LIMIT 500`; a client whose cursor is older than the retained window (or ahead of it) gets `resync`, never a silent skip; a client
further behind than the retention window is told to refetch, not silently skipped. The `event` seq is the
only ordering authority — never a timestamp.

**Cursor pagination.** Cursors are base64 of `(sort_key, id)`; they are opaque and never documented as stable.

| endpoint | sort key | cursor | index it must hit |
|---|---|---|---|
| `GET /api/boards/{id}/cards` | list, position, id | `(list_id, position, id)` | `card (tenant_id, board_id, list_id, position, id) WHERE archived_at IS NULL` |
| `GET /api/cards/{id}/comments` | created_at, id | `(created_at, id)` | `comment (tenant_id, card_id, created_at, id)` |
| `GET /api/runs` | created_at desc, id | `(created_at, id)` | `run (tenant_id, created_at desc, id)` / `(tenant_id, card_id, created_at desc)` |
| `GET /api/requirements` | updated_at desc, id | `(updated_at, id)` | `governed_asset (tenant_id, kind, status, updated_at desc, id)` |
| `GET /api/audit` | created_at desc, id | `(created_at, id)` | `audit_event (tenant_id, created_at desc, id)` |
| `GET /api/events` | seq | `seq` | `event (topic, seq)` |

**The three access paths that must stay fast.**

- *Board, 1,000 cards.* One partial index on `(tenant_id, board_id, list_id, position, id)` serves the whole
  board as an index scan grouped by list; the API never sorts in Rust. Archived cards are excluded by the
  predicate, so board load cost is independent of history depth.
- *Run queue.* `run (tenant_id, status, created_at, id) WHERE status='queued'` — a queue that is empty in
  steady state, so the index stays tiny and the claim is an index-only scan.
- *Search.* `gin(tsv)` on `card` and `governed_asset`, plus `pg_trgm` trigram indexes on `card.name`,
  `governed_asset.title` and `catalogue_entry.label` for fuzzy prefix. S07's `0007` migration creates
  `CREATE EXTENSION pg_trgm` and the three trigram indexes (S02 §5 decision 5); S03 ships only the FTS indexes.
  FTS first, pgvector only after the S07 eval set proves an FTS baseline (R2).
  ⛔ **These indexes do nothing unless two operators are LEAKPROOF** — measured 2026-11-08: under FORCE RLS the
  planner will not use a non-leakproof qual, and `ts_match_vq` (`@@`) and `word_similarity_op` (`<%`) both ship
  `proleakproof = false`. The `ALTER FUNCTION`s are a superuser step and live in `scripts/db-roles.sql`; see
  `01-CONTRACT.md` §2. Without it the search runs at O(rows per tenant) and reports no error.

**Board position.** Planka's float `position` is replaced by `numeric` (exact, unconstrained scale) with a
**midpoint insert**: a card between neighbours `a` and `b` gets `(a+b)/2`; a card appended to a list gets
`max+1024`. There is no renumber job and no precision loss, because `numeric` is exact — a float implementation
degrades after ~50 halvings and then needs the compaction job that nobody writes. Moves that touch more than
one card (multi-select drag) take `pg_advisory_xact_lock(hashtext('board:'||board_id))` and renumber the
affected lists inside one transaction.

## 5. JSONB policy

**Allowed in jsonb** — read whole with the parent, never filtered, sorted, joined or uniquely constrained:

| column | why |
|---|---|
| `tenant.settings` | per-tenant display/feature settings **and the prompt-audit cadence** (`prompt_audit.{enabled, after_executions, surfaces}`, §2.17); read whole with the tenant |
| `membership.meta` | agent `product` and role metadata; read with the membership row |
| `event.payload` | carries the event's own shape; consumers parse by `type`, never query into it |
| `job.payload` | handler-specific work input |
| `schedule.payload` | handler-specific config for the jobs it materialises |
| `audit_event.details` | scrubbed at write, capped at 8 KiB, never queried |
| `idempotency_key.response` | the replayed status and body of one POST, returned verbatim |
| `app_licence.raw` | the upstream licence payload; `status`/`expires_at` are the columns |
| `project.meta` | per-project blobs with no query path; filters use the columns |
| `board.settings` | default card type, default view, display flags; loaded with the board |
| `card_type.definition` | inputs/outputs/acceptance/rules read as one object at run time |
| `card.meta` | fr fold targets (`card_contracts`, `card_plan_pins`); never filtered |
| `run.inputs` | the run's input document, read whole at start |
| `approval.meta` | gate context; only `turn_id` is indexed, and that is the exception below |
| `approval_decision.item_decisions` | per-item decisions on a multi-item gate; read with the decision |
| `approval_decision.via` | channel/agent provenance blob (D18) |
| `governed_asset.acceptance / links / lens / meta` | authored blobs; search uses `tsv`, never these |
| `catalogue_entry.meta` | matrix, phase order, recipes and WIP limits; loaded once per project type and cached |
| `revision.content` | an immutable snapshot; the only query is "give me content by id" |
| `custom_field.options` | select-field option list, read with the field |
| `custom_field_value.content` | the value for one (card, field); no query path |
| `channel_binding.config` | chat/topic/bot references; secrets by reference only |
| `channel_preference.quiet_hours` | per-user quiet window, read when a send is considered |
| `prompt_audit.scope` | which surfaces the audit covered (tenant/procedure ids, `action_registry`, `copilot_prompt`); read with the audit row, never filtered — §2.17 |
| `message.meta` | per-send content metadata |
| `quality_evaluation.criteria` | per-evaluation criterion scores, read with the row |
| `project_request.payload` | the CP intake document; `status` and `external_ref` are columns |
| `model.meta` | model-level vendor extensions; `language` and `title` are columns |
| `model_element.meta` | element vendor extensions |
| `model_relationship.meta` | relationship vendor extensions |
| `model_view.meta` | view-level layout/vendor extensions |
| `view_object.bounds / style` | diagram geometry |
| `view_connection.bendpoints / style` | connection geometry and styling |
| `model_validation_run.findings` | a report, read whole |
| `project_capability.meta` | capability-ledger extras; the key is `capability_code` |
| `deliverable.meta` | deliverable details; `kind_code` and `due_at` are columns |
| `register_item.meta` | register-specific fields; `kind` and `status` are columns |
| `document.meta` | document metadata; bytes and object keys are columns |
| `conversation_turn.meta` | per-turn tool/citation payloads; the turn key is `seq` |
| `benchmark_run.counts` | the grader's full status histogram, read whole with the run; the four counters are columns because they are compared and ordered — §2.18 |
| `benchmark_run.environment` | the captured environment (versions, hashes, subject tenant, provider), read with the run |
| `benchmark_run.options` | the start options (`concurrency`, `only`, `with_failing_test`), read with the run |
| `benchmark_case.detail` | the verdict's reason and produce provenance, read with the case |
| `process_step.input_template` | the step's input template, expanded at run time |
| `run_step.input / output` | per-attempt invocation blobs |
| `workflow.definition` | **RETAINED (owner ruling 2026-11-09)** — the workflow engine is the tool-sequence executor; a process is still the ordered `process_step` rows |
| `action_registry.spec` | the external action's schema document, read whole at invoke |
| `eval_version.prices` (§2.19) | the token price table the report derives cost from; read whole with the version, so a price change is a new version and never re-prices history |
| `eval_metric.grader` (§2.19) | the grader's config document (pattern, schema, judge model, normalisation); read whole with the metric |
| `eval_case.expected` (§2.19) | the answer key or the per-case rubric, read only by the grader; **write-only by design** — no query filters it and no view returns it |
| `eval_case.attachments` / `meta` (§2.19) | artifact refs and provenance sidecar, rendered with the case, never filtered; `tags`/`source`/`split`/`gold_provenance` are the columns |
| `eval_case_result.grade` / `explanation` / `usage` (§2.19) | the per-metric scores keyed by metric id, the judge's reasoning, and the response's token block — read whole with the row |
| `eval_trace.turns` (§2.19) | the transcript; the viewer and the analyzer read the whole list, and `tool_calls`/`latency_ms`/`status` are the columns anything queries |
| `eval_run.summary` / `environment` (§2.19) | `{description,target,suspicious}` and the captured fingerprint; read with the run |
| `eval_audit.summary` (§2.19) | the audit's per-section detail, read whole with the audit |
| `eval_climb.goal` (§2.20) | the target/hold pair and the cost-search position; read whole with the climb |

⚠️ **The report's arithmetic is Rust, not SQL.** Per-metric means, the confidence interval, the noise floor
(`1/sqrt(n·reps)`), the per-tag breakdown and the derived cost are all computed over the fetched rows, so
`eval_case_result.grade` is never a `GROUP BY` target and `eval_case.tags` never a join key. That is what admits
them under this policy; if a future read wants a SQL aggregate over a grade, it becomes a column first.

**Never in jsonb** — these must be columns: `tenant_id`, any id, any status/kind vocabulary, `list_id`,
`position`, any timestamp used for ordering or retention, `version`, anything with a FK, anything in a
`UNIQUE`, and anything the UI sorts or filters on (name, title, key, email, due_at, score). The test is
mechanical: *if it appears in a `WHERE`, `ORDER BY`, `JOIN`, `GROUP BY` or `UNIQUE`, it is a column.*

**One documented exception:** `uq_approval_copilot_turn` (§2.11) is a *partial unique index over an expression* —
`approval.meta->>'turn_id'` — not a table constraint. It is allowed because it enforces a concurrency invariant
(one copilot turn cannot raise two pending approvals) and the key is written only by S10. Any second such index
needs the same justification recorded here; "it is only an index" is not one.

Size caps, enforced by CHECK where the value is written by us and by a domain guard where it is not:
`event.payload` ≤ 64 KiB, `revision.content` ≤ 256 KiB (larger artifacts go to the object store and the row
keeps the key), `audit_event.details` ≤ 8 KiB. A payload that exceeds the cap is a modelling error, not a
reason to raise the cap.

## 6. Retention, growth and size

Volumes are per tenant per day at the planning target (one active project, ~200 card transitions/day).

| table | rows/day/tenant | rule | index cost |
|---|---|---|---|
| `event` | ~1,500 (≈7 per card transition, plus run/approval events) | **prune at 90 days** by job, batched; monthly RANGE partition on `created_at` only if the table passes ~50M rows | 3 secondary indexes; each ~60 B/row — the cheapest of the high-volume tables |
| `audit_event` | ~150 | **UNBOUNDED. Never pruned.** Append-only by GRANT; this is the one table whose growth is deliberate | 2 indexes, append-only inserts, no bloat from updates |
| `action_invocation` | ~600 | prune at 30 days; it is an operational trace, not a record | 3 indexes (one added by `0012` for the age scan) |
| `run` | ~200 | keep terminal runs 180 days, then prune with their `run_step` rows; a run with a **pending** approval is skipped, not pruned | 3 indexes + 1 partial unique (`0012` adds the terminal-age index) |
| `delivery` | ~400 | delete 30 days after terminal state; pending/sent rows are never pruned — so a `message` is deletable only after its last delivery is gone (deliveries prune first) | 2 indexes, 2 partial (`0012` adds the terminal-age index) |
| `revision` | ~250 | model revisions keep the last 50 per model; governed-asset revisions are unbounded (they are the governance record) | 1 composite |
| `job` | ~2,000 (transient) | 30 days after terminal; the queue table should be near-empty, and steady-state growth is a defect | 2 partial |
| `inference_call` | ~400 (one per model call: a copilot turn is 1, an `agent.run` up to its 20-turn cap) | **prune at 90 days** — deliberately longer than `action_invocation`'s 30, because a cache report over a quarter is the point of the table | 4 indexes, two partial |
| `prompt_audit` + `prompt_audit_finding` | one audit per surface change; tens of findings | **180 days after the audit's last finding is decided**; the audit's `prompt_ref` version is what keeps an old finding re-readable | 3 indexes |
| `benchmark_suite` + `benchmark_instance` | seed-time only: one suite is ~50 instances, written once | **UNBOUNDED. Never pruned.** Immutable seed data; a suite's identity *is* its hashes | 2 indexes |
| `benchmark_run` + `benchmark_case` | a run is ~50 cases, started by a person or a kata iteration | **UNBOUNDED. Never pruned.** A run is the number's provenance — deleting it deletes the evidence behind a published score; the case log is an S09 document and follows that table's retention | 4 indexes |
| `eval_trace` | N cases × R reps × one row each; an eval pass is tens to hundreds of rows, and a round adds another | **prune at 90 days** — it is the bulk of an eval and the only table here that grows per round; a trace older than a quarter is not re-read | 2 indexes |
| `eval_case_result` + `eval_attempt_error` | the same rows as `eval_trace`, plus the failures | **UNBOUNDED. Never pruned.** This is the evidence behind a number a person may act on; a trace can be discarded, a score cannot | 4 indexes |
| `eval` + `eval_version` + `eval_case` + `eval_metric` | written when an instrument is authored, not per run | **UNBOUNDED. Never pruned.** A version is the instrument; pruning a case set makes every past run unreproducible | 5 indexes |
| `eval_run` + `eval_audit` + `eval_audit_finding` | one run or audit per pass | **UNBOUNDED. Never pruned.** The round history is the climb's record and the audit is why a number was believed | 4 indexes |

**A stated rule is not a producer.** This paragraph used to claim "everything else is bounded by a stated rule",
which was false: a rule in this table does nothing until a job enforces it, and five of them had no job.
Corrected 2026-11-08 (S08 Part B2) — the table above states the *intent*; this list states the *producer*:

| rule | producer | state |
|---|---|---|
| `event` 90 days | `event.prune` (S02) | shipped |
| `job` 30 days after terminal | `job.prune` (S02) | shipped |
| `idempotency_key` by `expires_at` | `idempotency.sweep` (S02) | shipped |
| `revision`: model revisions keep the last 50 per model | **`model.prune`** (S08 B2) | shipped 2026-11-08 |
| `model_validation_run` 90 days | **`validation.prune`** (S08 B2) | shipped 2026-11-08 |
| `action_invocation` 30 days | **`action_invocation.prune`** (`0012`) | shipped 2026-11-08 |
| `run` 180 days (with its `run_step` rows) | **`run.prune`** (`0012`) | shipped 2026-11-08 |
| `delivery` 30 days after terminal (deliveries before messages) | **`delivery.prune`** (`0012`) | shipped 2026-11-08 |
| `inference_call` 90 days | **`inference_call.prune`** (S16) | planned — the slice is not built |
| `prompt_audit` + findings 180 days | **`prompt_audit.prune`** (S16) | planned — the slice is not built |
| benchmark suites, instances, runs and cases | **none — UNBOUNDED by design** (the `audit_event` shape: seed data and the evidence behind a score) | n/a |
| `eval_trace` 90 days | **`eval.prune`** (S19) | planned — the slice is not built |
| eval instruments, results, runs and audits | **none — UNBOUNDED by design** (the evidence behind a number a person may act on) | n/a |

**Every rule above now has a producer** — with three exceptions, named honestly rather than implied: S16's two
pruners are written into its slice plan (§6 there) but the slice is not built, so those two rows read `planned`;
the benchmark tables are **UNBOUNDED by design** (the `audit_event` shape — a run is the evidence behind a score),
so they have no producer and need none. A rule whose producer is `planned` enforces nothing today, and the table
says so.

Two consequences are part of the design, not accidents:

- **`approval` is never pruned.** `0012` re-created `approval.run_id` as `ON DELETE SET NULL`, so a pruned run
  detaches and the human decision — with `approval_decision.note`, which exists nowhere else — survives. A
  `run_step` row goes the other way: **S11 must declare `run_step.run_id ON DELETE CASCADE`**, and `run.prune`
  then carries the steps for free.
- A run whose gate is still `pending` is **skipped**: `uq_appr_pending` is partial on
  `(tenant_id, run_id, gate) WHERE status = 'pending'`, and a detached NULL `run_id` would defeat it.

**Explicitly unbounded:** `audit_event`, `approval`, `revision` for governed assets. `model.prune` never deletes the revision named by `model.published_revision_id` — a
published snapshot is the governance record of a model in the same way a governed asset's revision is, and a
retention rule that could delete it would make `models.compare` fail on the newest thing it compares.

## 7. Source mapping — TOTAL

Every table enumerated in `02-source-map/50-schema-inventory.md` appears exactly once.
Dispositions: **port** (same concept, new shape) · **fold** (columns move onto another table) · **drop** ·
**replaced-by-platform**.

> Count note, measured 2026-11-07 so nobody re-derives it: **fr 86 domain tables** (88 base tables in `public`,
> 90 relations including two `pg_stat_statements*` views), **RM 57 domain tables** (58 base tables including
> `alembic_version`), **Planka 33 domain tables** (35 base tables including `migration` and `migration_lock`).
> Reproduce: `psql -Atc "select count(*) from pg_tables where schemaname='public'"` → 88 / 58 / 35; relations:
> `psql -Atc "select relkind, count(*) from pg_class c join pg_namespace n on n.oid=c.relnamespace where
> n.nspname='public' and c.relkind in ('r','p','v','m') group by relkind"` → `r|88, v|2` for fr. **`pm_projects`
> is mapped** — §2.3's `project` folds it — and was missing only from the inventory list; it is now a row in §7.2
> and in `50-schema-inventory.md`.

### 7.1 fr — 86 enumerated + bookkeeping

| source table | target table(s) | disposition | reason (≤12 words) |
|---|---|---|---|
| action_idempotency | idempotency_key | replaced-by-platform | one POST replay store, contract §5 |
| agent_members | membership | fold | agent is `app_user.kind='agent'` (null `oidc_subject`); `product` in `membership.meta`; `agent:<tenant_id>` → `channel_binding(mcp)` |
| agent_runtime_sessions | — | drop | runtime state belongs to the `agent` service |
| agent_sessions | conversation | port | copilot thread survives, S10 |
| analytics_rollup_meta | — | drop | rollups derive from `event` |
| analytics_rollups | — | drop | derived; no second aggregate store |
| andon_state | andon_state | port | one row per tenant, audited |
| andon_state_history | audit_event | fold | a history row is an audit fact |
| approval_callback_urls | approval.callback_url | fold | one URL per approval |
| approval_decisions | approval_decision | port | dual control needs per-approver rows; decided_by_kind/via carried, not dropped |
| approvals | approval | port | one pending per (run, gate) |
| asset_assignments | — | drop | KAG out; strata_ingest/strata_rag replaces |
| asset_bindings | — | drop | KAG out; strata replaces |
| asset_stream_membership | — | drop | KAG out; strata replaces |
| budget_alert_receipts | — | drop | billing stays external (D12) |
| budget_topups | — | drop | billing stays external (D12) |
| capability_audit | action_invocation | replaced-by-platform | every invocation is already recorded |
| card_asset_binding | — | drop | KAG out; strata replaces |
| card_bridge_links | — | drop | bridge retired; PENDING-S0 |
| card_computed_checks | run | fold | a check outcome is run data |
| card_contracts | card.meta | fold | no query path; PENDING-S0 |
| card_done_overrides | audit_event | fold | an override is an audit fact |
| card_drafts | — | drop | 0 rows; PENDING-S0 |
| card_executed_activities | action_invocation | fold | an activity IS an invocation |
| card_phase_lease | card | fold | the claim is card columns (contract §4) |
| card_phase_lease_waiter | job | fold | a waiter is a queued claim job |
| card_plan_events | event | fold | already an event stream |
| card_plan_pins | card.meta | fold | one flag, never queried |
| card_scenario_evidence | — | drop | PENDING-S0 |
| card_seeker_links | — | drop | PENDING-S0 |
| card_version_stamps | revision | fold | a version stamp is a revision reference |
| channels | channel_binding | port | per-tenant channel configuration |
| coding_analysis_runs | — | drop | coding fleet not absorbed (D14) |
| comments | comment | port | one comment store |
| contract_dependencies | card.meta | fold | PENDING-S0, no query path |
| conveyor_flow_runs | run | port | status vocabulary remapped, §7.4 |
| decompositions | card | fold | `parent_card_id` plus an event |
| department_members | membership | fold | `department_id` column |
| departments | department | port | tenant org unit |
| documents | document | port | metadata row; bytes in object store |
| engagement_deliveries | delivery | fold | one delivery state machine |
| execution_signoff_blocks | approval | fold | a block is a pending gate |
| family_bundles | — | drop | PENDING-S0 |
| family_decisions | — | drop | PENDING-S0 |
| finding_suppression_events | — | drop | findings live in coding-security |
| finding_suppressions | — | drop | findings live in coding-security |
| github_app_installations | repository | fold | `installation_ref` column |
| inapp_messages | message | fold | channel `web` message |
| interface_validations | — | drop | PENDING-S0 |
| katas | — | drop | kata domain outside this spec |
| member_knowledge_areas | membership | fold | `knowledge_areas text[]` |
| n8n_execution_mapping | run | fold | `correlation_id`/`external_ref` on run |
| notification_log | delivery | fold | delivery IS the log |
| notification_prefs | channel_preference | fold | one preference row per channel |
| notifications | message | fold | content and delivery split |
| priority_change_events | event | fold | already an event stream |
| project_asset_copy_intents | — | drop | KAG out; strata replaces |
| project_conveyor_failures | run | fold | `error_class`/`error_message` on run |
| project_conveyor_start_attempts | run | fold | `attempt` counter on run |
| project_deliverables | — | **drop** | S00 day 2: 0 rows, no deliverable operation in 348 flows |
| project_metrics | — | drop | derived from run/event |
| project_milestones | — | **drop** | S00 day 2: 0 rows |
| project_outlines | — | **drop** | S00 day 2: 0 rows; no `toc` operation in 348 flows |
| project_reports | document | fold | a report is a document |
| project_repositories | project_repository | port | project↔repository join |
| project_request_repositories | project_request | fold | `repository_url` column on the request row |
| project_requests | project_request | port | intake state machine |
| project_stakeholders | project_stakeholder | port | small, listed, per project |
| projects | project | port | one project entity |
| provisioning_audit | audit_event | replaced-by-platform | audit is structural, not a feature |
| repositories | repository | port | the repo registry |
| required_approvers | approval | fold | `required_decisions` + gate meta |
| router_dedupe | idempotency_key | replaced-by-platform | one dedupe store |
| scheduled_trigger_runs | job | fold | a trigger run is a job row |
| scheduled_triggers | schedule | replaced-by-platform | durable schedule table |
| signup_approvals | — | drop | signup flow not carried |
| streams | — | **drop** | S00 day 2: 0 rows, no `/api/streams` route, no `streams` node operation |
| suggested_schedules | — | drop | PENDING-S0 |
| super_admin_tenant_actions | audit_event | replaced-by-platform | impersonation is audited |
| telegram_forum_topics | channel_binding | fold | `route_key` + `config` |
| tenant_improvement_projects | — | drop | PENDING-S0 |
| user_channel_prefs | channel_preference | fold | one preference row per channel |
| user_roles | membership | replaced-by-platform | one role store per tenant |
| user_secrets | secret | port | encrypted secret store |
| wip_slot | run | fold | WIP occupancy is active runs |
| workspaces | tenant | replaced-by-platform | tenant is the only root |
| *(bookkeeping: `_migrations`, `_migration_effects`)* | — | drop | replaces the 224-migration history with one baseline |

### 7.2 requirements_management — 57

| source table | target table(s) | disposition | reason (≤12 words) |
|---|---|---|---|
| architecture_element_realisation_bindings | trace_link | fold | it is a typed trace |
| architecture_elements | model_element | fold | one element table |
| architecture_relationships | model_relationship | fold | one relationship table |
| architecture_standards | catalogue_entry | fold | kind='architecture_standard' |
| architecture_view_connections | view_connection | fold | one view-connection table |
| architecture_view_objects | view_object | fold | one view-object table |
| architecture_views | model_view | fold | one view table |
| decisions | governed_asset | fold | kind='adr'; identical lifecycle |
| entity_version_history | revision | replaced-by-platform | one revision table |
| idempotency_keys | idempotency_key | replaced-by-platform | one key store |
| modeling_change_sets | — | drop | drafts are transactional; no set objects |
| modeling_model_revisions | revision | fold | one revision table |
| modeling_models | model | port | model header row |
| modeling_publications | model | fold | `published_revision_id` |
| modeling_reviews | approval | fold | a review is a decision |
| modeling_trace_links | trace_link | fold | one trace table |
| modeling_validation_findings | model_validation_run | fold | findings jsonb on the run |
| modeling_validation_runs | model_validation_run | port | one validation-run row |
| pm_benefits | — | **drop** | S00 day 2: 0 rows, no register route |
| pm_board_templates | catalogue_entry | fold | kind='board_template' |
| pm_capabilities | capability | port | S15: the aggregate, not a catalogue kind (D20) |
| pm_card_ledger | — | drop | keyed by Planka card id (SPEC §4) |
| pm_card_types | card_type | port | the card-type definition |
| pm_coach_actions | — | drop | coach retired; PENDING-S0 |
| pm_deliverable_kinds | catalogue_entry | fold | kind='deliverable_kind' |
| pm_gates | catalogue_entry | fold | kind='gate' |
| pm_milestone_kinds | catalogue_entry | fold | kind='milestone_kind' |
| pm_priority_levels | catalogue_entry | fold | kind='priority_level' |
| pm_programmes | programme | port | portfolio container |
| pm_project_brief_versions | revision | fold | brief revisions are revisions |
| pm_project_capabilities | — | **drop** | S00 day 2: 0 rows, no capability operation |
| pm_project_capability_events | event | fold | already an event stream |
| pm_project_phase_events | event | fold | already an event stream |
| pm_project_phases | catalogue_entry | fold | kind='phase' |
| pm_project_type_card_types | catalogue_entry | fold | the matrix inside project_type.meta |
| pm_project_type_phases | catalogue_entry | fold | phase order inside project_type.meta |
| pm_project_types | catalogue_entry | fold | kind='project_type' + meta.matrix |
| pm_projects | project | fold | columns land on the one project entity |
| pm_recipe_items | catalogue_entry | fold | inside project_type.meta.recipes |
| pm_recipes | catalogue_entry | fold | inside project_type.meta |
| pm_resources | — | **drop** | S00 day 2: 0 rows, no register route |
| pm_risks | — | **drop** | S00 day 2: 0 rows, no register route |
| pm_wip_limits | catalogue_entry | fold | kind='wip_limit' |
| principle_conflicts | principle_conflict | port | persisted verdicts, read + dismiss |
| principles | governed_asset | fold | kind='principle'; identical lifecycle |
| quality_evaluations | quality_evaluation | port | score history |
| requirements | governed_asset | fold | kind='requirement'; one lifecycle |
| tenant_id_crosswalk | — | drop | historical remap ledger |
| toc_nodes | outline_node | fold | one outline-node table |
| toc_settings | outline | fold | settings on the outline |
| toc_trees | outline | fold | one outline |
| uml_diagram_connections | view_connection | fold | one view-connection table |
| uml_diagram_objects | view_object | fold | one view-object table |
| uml_diagrams | model_view | fold | kind='uml_diagram' |
| uml_elements | model_element | fold | one element table |
| uml_interactions | model_relationship | fold | kind='message', seq in meta |
| uml_relationships | model_relationship | fold | one relationship table |

### 7.3 Planka (`kanban`) — 33 enumerated

| source table | target table(s) | disposition | reason (≤12 words) |
|---|---|---|---|
| _conveyor_reset_20260922 | — | drop | a reset marker, not a feature |
| action | event | replaced-by-platform | activity is the event stream |
| attachment | — | drop | SPEC §2: attachments out |
| background_image | — | drop | not in the used-feature list |
| base_custom_field_group | custom_field | fold | `group_name` column |
| board | board | port | board entity |
| board_membership | board_membership | port | role + `can_comment` |
| board_subscription | board_membership | fold | `watching` flag on membership |
| card | card | port | the card entity |
| card_label | card_label | port | join table |
| card_membership | — | drop | SPEC §2: card-level memberships out |
| card_subscription | — | drop | no watcher store; fan-out is `delivery` |
| comment | comment | port | one comment store |
| config | — | drop | instance config, no consumer |
| custom_field | custom_field | port | fields; groups flattened |
| custom_field_group | custom_field | fold | `group_name`, `group_position` |
| custom_field_value | custom_field_value | port | one value per card per field |
| identity_provider_user | app_user | replaced-by-platform | OIDC identity is the only identity |
| internal_config | — | drop | instance config, no consumer |
| label | label | port | board label |
| list | list | port | `kind` + `position` |
| notification | message | fold | channel `web` message |
| notification_service | channel_binding | fold | a channel config row |
| project | project | port | the project entity |
| project_favorite | — | drop | PENDING-S0; not in used-feature list |
| project_manager | board_membership | fold | owner is a membership role |
| session | — | drop | auth sessions are Keycloak/OIDC |
| storage_usage | — | drop | attachment storage is gone |
| task | task | port | checklist item |
| task_list | task_list | port | checklist container |
| uploaded_file | — | drop | SPEC §2: attachments out |
| user_account | app_user | replaced-by-platform | one identity table |
| webhook | webhook_subscription | port | payload and semantics kept |

### 7.4 Vocabulary remap (`conveyor_flow_runs` → `run`)

`running → running` · `waiting → waiting_approval` · `success → succeeded` · `error → failed` ·
`canceled → cancelled` · `crashed → failed` (with `error_class='crashed'`). The old table's terminal-wins
upsert rule is carried as: a `run` in a terminal state rejects any further transition.

## 8. Data dependency graph

Slice order is `00-INDEX.md`'s execution order: S02 → S03 → S04 → S05 → S06 → S06b → S07 → S08 → S09 → S10 →
S11 → S12 → S13 → S14 → S15 → S16 → S17 → S18. S16 sits last in the **table**, not in readiness: its only
dependencies are S02, S03, S10 and S11, all built, so it may start as soon as its own plan is accepted. S17's
only unmet dependency is a decision about the grading node; S18 waits for S13 and S15 by construction.

| slice | ships | depends on |
|---|---|---|
| S02 | tenant, app_user, membership, department, event, job, schedule, audit_event, action_invocation, idempotency_key, app_licence | — |
| S03 | programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state | S02 (tenant, app_user, event) |
| S04 | action_registry | S03 (tenant only) |
| S05 | board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription | S03 (board, list, card), S02 (app_user) |
| S06 | channel_binding, channel_preference, message, delivery | S02 (app_user), S03 (run, approval) |
| S06b | — (no tables; `approvals.request` writes S03's `approval`) | S04, S05, S06 |
| S07 | quality_evaluation, principle_conflict, project_request | S03 (governed_asset, catalogue_entry, project) |
| S08 | project_stakeholder, model, model_element, model_relationship, model_view, view_object, view_connection, model_validation_run | S03 (project, revision) |
| S09 | document, repository, project_repository, secret | S03 (project) |
| S10 | conversation, conversation_turn | S03 (approval), S02 (app_user) |
| S11 | process, process_step, procedure (SOP), process_map, run_step | S03 (run, card_type), S04 (action names) |
| S13 | **— no new tables** (the process step columns and the run tree land on S11's `process_step`/`run`/`run_step` in `0035`; the `workflow` table it used to ship is **RETAINED**, owner ruling 2026-11-09) | S03 (run), S11 (procedure, process_step, run_step), S04 (action names) |
| S15 | capability, capability_agent, capability_knowledge, capability_relation, work_package_memory | S03 (card_type, run, card), S04 (action names), S06 (run outcomes), S11 (procedure) |
| S16 | inference_call, prompt_audit, prompt_audit_finding | S02 (tenant), S03 (run), S04 (action descriptions it audits), S10 (conversation_turn, `completions.create`), S11 (run_step, procedure) |
| S17 | benchmark_suite, benchmark_instance, benchmark_run, benchmark_case | S02 (tenant), S03 (tenant only), S04 (action names), S09 (the case log document) |
| S18 | — (no tables) | S13 (`agent.run`), S15 (`capability_agent`), S17 (runs, compare) |

The graph is a DAG in slice order; **every FK points from a higher slice to a lower one, with five deliberate
forward references, all into S03's `run`**, each added by the slice that owns the target and never
pre-created by S03:

| column | added by | shape |
|---|---|---|
| `run.procedure_id` | S11 (`0015`) | `uuid NULL` → `procedure` |
| `run.inputs` | S11 (`0015`) | `jsonb NOT NULL DEFAULT '{}'` — the run's input document, not a reference |
| `run.workflow_id` | S13 (`0019`) | `uuid NULL` → `workflow` — **RETAINED (owner ruling 2026-11-09): the column and the `workflow` table stay**; there is no drop migration |
| `run.capability_id` | S15 (`0020`) | `uuid NULL` → `capability` |
| `run.capability_agent_id` | S15 (`0020`) | `uuid NULL` → `capability_agent` |

**Cross-slice column additions that are not FK targets into `run`:** S13 adds `run_step.node_id`, `input_hash`,
`output_hash` and `duration_ms` to S11's `run_step` (the table is S11's `0015`; the four columns arrive in S13's
`0019`). S15 adds `card_type.capability_id` (into S03) and `procedure.capability_id` (into S11's `procedure`,
`0020`). S11's `attempt` column and its `uq (run_id, position, attempt)` are unchanged by S13. Neither slice
pre-creates the other's table.

**S17's cross-slice columns are deliberately not FKs.** `benchmark_run.capability_agent_id` (S15) and
`procedure_id` (S11) name another slice's aggregate, but each is resolved in-tenant at write and carries no FK —
the §2.17 RLS reason. `benchmark_case.log_document_id` (S09) is the same shape. The `benchmark_run_subject` CHECK
is what makes them answerable without the FK.

**Tables needed by two slices are assigned to the earlier one:** `catalogue_entry` (S06, S07, S08 → S03),
`revision` (S07, S08, S09 → S03), `trace_link` (S07, S08 → S03), `approval` (S06, S10 → S03),
`message`/`delivery` (S06, S10 → S06), `membership` (every slice → S02). No table is defined twice.

## 9. PENDING-S0 — ADJUDICATED 2026-11-08

S00 day 2 measured every table below against the 348-flow node census, the 24 h Loki sample and the source
row counts (`parity/evidence/02-day2-method.txt` §9 "PENDING-S0 — the 13 tables 02-DATA-ARCHITECTURE §9 assigns to S0"). The section is kept, with its outcome, because the
rule it states is the one that governed the build.

| table | verdict | evidence |
|---|---|---|
| audience, audience_member | **DROP** — migration `0011` | no `streams` node operation in 348 flows; fr.streams 0 rows; asset_stream_membership 0; no `/api/streams` route |
| outline, outline_node | **DROP** — migration `0011` | RM toc_trees/toc_nodes/toc_settings 0; fr project_outlines 0; no `toc` operation; fr /toc/* 15 unused + 1 rare |
| deliverable | **DROP** — migration `0011` | project_deliverables 0, project_milestones 0, no deliverable operation |
| project_capability | **DROP** — migration `0011` | pm_project_capabilities 0, no capability operation |
| register_item | **DROP** — migration `0011` | pm_resources / pm_risks / pm_benefits all 0, no register route |
| project_stakeholder | **KEEP** | fr project_stakeholders 21 rows |
| custom_field, custom_field_value | **KEEP** | Planka custom_field 1815 rows, custom_field_value 70 (§9's old premise "measured 0" was stale) |
| webhook_subscription | **KEEP** | Planka webhook 3 live rows — these are what trigger the conveyor |
| conversation, conversation_turn | **KEEP** | S10 (copilot) is planned, not dropped |

Dropping a PENDING-S0 table is a **migration**, not a silent omission — five were dropped by `0011`, and the
evidence class is recorded next to each verdict in `parity/evidence/02-day2-method.txt`. Reversing one is a new
migration from the old database, which stays read-only until S14.

## 10. Open data questions

| # | Question | Default if unanswered |
|---|---|---|
| 1 | **`card_type` is a table while 14 RM catalogue tables become `catalogue_entry`.** Is that exception right? | **Yes.** `Card *—1 CardType` is an entity in SPEC §6, it is the FK target of the conveyor matrix, and it carries executable definition (`inputs`, `outputs`, `rules`). The *vocabularies* collapse; the aggregate does not. |
| 2 | **`governed_asset` folds requirements, principles and ADRs into one table with a `kind` discriminator.** | **Yes.** Identical lifecycle, identical revision/quality/trace machinery. If a kind later needs a column that is filtered on, add the column and leave the others NULL — do not fork the table. |
| 3 | **`position numeric` with midpoint insert, no renumber job.** | **Yes.** Planka used a float; numeric is exact and never degrades. Revisit only if a board with >10k cards in one list measures a problem. |
| 4 | **`event` prune at 90 days ships with `event` in S02.** | **Yes.** An outbox with no reaper is unbounded growth; the retention job is part of the platform slice, not a later chore. |
| 5 | **model revisions keep the last 50; governed-asset revisions are unbounded.** | **Yes.** A model revision is recoverable working state; a governed-asset revision is the governance record and is never pruned. |
| 6 | **Seed data for `catalogue_entry`** (project types, phases, card types, gates) is data, not schema. | Seed it from the live RM counts (`pm_card_types=293`, `pm_project_types=26`, `pm_project_type_card_types=282`) in a `migrate --seed` step; the numbers are `pg_stat_user_tables` estimates and must be re-counted before seeding. |
