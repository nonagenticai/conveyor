---
slice: S05
title: Board — projects, boards, lists, cards, labels, comments, tasks, custom fields, webhooks, SSE
status: planned
depends_on: [S03, S04]
spec_sections: ["§2", "§5.3", "§6", "§7", "§8 S5"]
adds_tables: [board_membership, label, card_label, comment, task_list, task, custom_field, custom_field_value, webhook_subscription]
adds_actions: [projects.create, projects.get, projects.close, boards.get, boards.set_member, lists.create, lists.update, lists.move, cards.create, cards.get, cards.move, cards.update, cards.archive, cards.unarchive, cards.duplicate, cards.comment, comments.update, comments.delete, task_lists.create, task_lists.update, task_lists.delete, tasks.create, tasks.update, tasks.move, tasks.delete, custom_fields.create, custom_fields.update, custom_fields.delete, custom_fields.set_value, custom_fields.clear_value, labels.create, labels.update, labels.delete, cards.add_label, cards.remove_label, webhooks.create, webhooks.update, webhooks.delete, webhooks.test]
adds_events: [card.moved, card.archived, card.created, card.updated, card.unarchived, comment.created, comment.updated, comment.deleted, task_list.changed, task.changed, custom_field.changed, custom_field_value.changed, label.changed, board_member.changed, webhook.changed]
adds_modules: [board]
parity_ids: PENDING-S0
---

# S05 — Board

Programme project: S05 · issues none yet

**Gate (SPEC §8 S5):** replay a board export; two-client race; 1k-card load.

## 1. Objective

`project → board → list → card` plus satellites, on one action registry, one comment store, one membership model, one outbox. The 280-flow board↔project lookup becomes a foreign key.

## 2. Starting state

- S03: `project`, `board`, `list`, `card`, `card_type`, `catalogue_entry` (incl. the project_type row's `meta.matrix`), `domain::board::may_move`, `card.version`.
- S04: `ActionSpec` derive, registry, `POST /api/actions/{name}/invocations`, OpenAPI + MCP projection.
- S02: `event`, `job`, `audit_event`, `/api/events` SSE with `Last-Event-ID` replay.

## 3. Scope

**IN** — the 9 tables of §2.5; 39 actions; read projections; SSE `board:{id}`/`card:{id}`; the webhook emitter; the export/import CLI.

**OUT** — attachments and card-level memberships (SPEC §2: no table, route or action); claims, assignee and stopwatch writes (S06, contract §4); webhook `delivery` rows (a job); the UI (S12).

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `kanban/docs/openapi.json` (PLANKA **2.0.0-rc.4**, **67 paths**) | `Card`,`List`,`Board`,`Label`,`Comment`,`Task*`,`CustomField*`,`BoardMembership`,`Webhook` | field shapes | external ids as keys |
| live `kanban` schema (33 tables, `50-schema-inventory.md`) | columns, FKs, indexes | constraints | row data (test only) |
| `30-rm-planka-mcp-n8n.md` §2 | identity correction; `{event,data,prevData,user}` | event names; comma-separated `events` | instance-admin scoping |
| `20-fr.md` `cards`,`kanban`,`decompositions`,`projects` | fr's board surface | comment/member consolidation | the card-state split |
| `40-live-state.md` | **4 boards, 12 lists, 5 cards, 3 webhooks, 0 tasks** | the test-data warning | usage inference |

## 5. Deliverables

```
migrations/0004_board.sql · crates/store/src/board/{list,card,comment,task,custom_field,label,webhook}.rs
crates/api/src/board.rs · crates/actions/src/board/*.rs · apps/server/src/export.rs
crates/api/tests/{position,race_move,webhook_shape,sse_board}.rs
tests/fixtures/board/{v1-recording,v1-1000}.json · tests/load/board.js
```

`tests/load/board.js` (k6): 20 rps for 60 s on `GET /api/boards/{id}/cards?limit=200` with `authorization: Bearer ${__ENV.DEV_AUTH_TOKEN}` (CONTRACT §10), threshold p95 ≤ 120 ms.

**Tables** — §2.5 verbatim, nothing added or retyped: `board_membership`, `label`, `card_label`, `comment`, `task_list`, `task`, `custom_field`, `custom_field_value`, `webhook_subscription`.

**Custom fields.** Planka is group → field → value. The group is **dropped as an entity**: `custom_field.group_name` + `group_position` carry it. It is board data, not vocabulary — no `catalogue_entry` row. `kind ∈ text|number|date|select|checkbox`, validated in the domain.

**Comments and memberships.** One `comment` table. Board membership only: one `role ∈ owner|admin|member|viewer` with `can_comment`, `watching`; Planka's `editor|viewer` maps `editor→member`. `project_manager`/`board_subscription` fold into that row. A card's assignee is `card.assignee_id`; no card-membership table.

**Actions** (all `POST /api/actions/{name}/invocations`):

| actions | input → output |
|---|---|
| `projects.create` `.get` `.close` | `ProjectCreateIn{title,project_type_code?,description?,problem_statement?,desired_outcome?,repo_url?,programme_id?,key?}` → `ProjectOut`; absent `key` is server-derived (slug of `title`, uppercased, collision-suffixed); mcp maps `project_type`→`project_type_code`, `repository_url`→`repo_url`, `id`→`result.project_id`; `IdIn` → `ProjectView`; `ProjectCloseIn{id,reason?}` → `ProjectView` |
| `boards.get` `.set_member` | `IdIn` → `BoardView{board,lists+card_count,labels,fields,members}` (no cards); `BoardMemberIn` → `BoardMemberOut` |
| `lists.create` `.update` `.move` | `ListCreateIn`, `ListUpdateIn`, `ListMoveIn` → `ListOut` |
| `cards.create` `.get` `.move` `.update` `.archive` `.unarchive` `.duplicate` | `CardCreateIn{board_id,list_id,name,description?,card_type_code,due_at?,parent_card_id?}` → `CardOut`; `IdIn` → `CardView`; `CardsMoveIn{id,list_id,after_id?}` → `CardOut`; `CardUpdateIn{id,name?,description?,due_at?,due_done?}` → `CardOut`; `IdIn` → 204 (archive); `IdIn` → `CardOut` (unarchive); `CardDuplicateIn{id,list_id?,name?}` → `CardOut` |
| `cards.comment` `comments.update` `.delete` | `CardsCommentIn{id,body}` → `CommentOut`; `CommentUpdateIn{id,body}` → `CommentOut`; `IdIn` → 204 |
| `task_lists.*` `tasks.*` | `TaskListIn` → `TaskListOut`; `TaskIn`, `TaskUpdateIn` → `TaskOut` |
| `custom_fields.*` | `CustomFieldCreateIn{board_id,name,kind,group_name?,options?}`, `CustomFieldUpdateIn{id,…}` → `CustomFieldOut`; `CustomFieldValueIn{card_id,custom_field_id,content}` → `CustomFieldValueOut` |
| `labels.*` `cards.add_label` `.remove_label` | `LabelIn` → `LabelOut`; `CardLabelIn` → `CardOut` |
| `webhooks.*` | `WebhookCreateIn` → `WebhookOut` (secret returned once); `.test` → `WebhookDeliveryOut` |

**Read projections:** `GET /api/projects[/{id}]`, `/api/boards/{id}`, `/api/boards/{id}/cards?cursor=&limit=` (max 200), `/api/boards/{id}/webhooks`, `/api/cards/{id}[/comments]`.

**UI surface declared for S12** (declaration only, no UI code): `board` and `card cockpit` — `boards.get` + `GET /api/boards/{id}/cards`; `cards.get` (+ `/api/cards/{id}/comments`, tasks and field values inside `CardView`); project list over `GET /api/projects`; webhook settings over `GET /api/boards/{id}/webhooks`. All subscribe to `board:{id}`.

**SSE.** Every mutation writes its `event` row in-tx with `topic='board:<board_id>'`; card events carry `aggregate_type='card'`, `aggregate_id=card_id`. `card:{id}` is **resolved** to topic `board:<board>` plus `AND aggregate_id = <card_id>`. No second row, no payload filtering.

S05 ships `cards.move`/`cards.archive`, so `card.moved`/`card.archived` are S05's (the emitter owns the name); it adds these 13:

| type | emitter | payload keys |
|---|---|---|
| `card.created` | `cards.create`, `cards.duplicate` | card_id, list_id, position |
| `card.updated` | `cards.update`, `cards.add_label`/`remove_label` | card_id, changed[] |
| `card.unarchived` | `cards.unarchive` | card_id, list_id |
| `comment.created` | `cards.comment` | comment_id, card_id |
| `comment.updated` | `comments.update` | comment_id, card_id |
| `comment.deleted` | `comments.delete` | comment_id, card_id |
| `task_list.changed` | `task_lists.*` | task_list_id, card_id, op |
| `task.changed` | `tasks.*` | task_id, task_list_id, op |
| `custom_field.changed` | `custom_fields.create\|update\|delete` | custom_field_id, board_id, op |
| `custom_field_value.changed` | `custom_fields.set_value\|clear_value` | card_id, custom_field_id |
| `label.changed` | `labels.*` | label_id, board_id, op |
| `board_member.changed` | `boards.set_member` | board_id, user_id, role |
| `webhook.changed` | `webhooks.*` | webhook_id, board_id, op |

**Webhook emitter.** Body `{event, data, prevData, user}`; `data`/`prevData` are the `*Out` representation, `user{id,name}` the actor. **`event` is the internal register type** (`card.created`, `card.updated`, `card.moved`, `card.archived`, `card.unarchived`, `comment.created|updated|deleted`, `task_list.changed`, `task.changed`, `custom_field.changed`, `custom_field_value.changed`, `label.changed`, `board_member.changed`, `webhook.changed`) and `data`/`prevData` use the `*Out` field names (`list_id`, not `listId`). Planka's camelCase vocabulary (`cardCreate`/`cardUpdate`/`cardData.listId`) was `UNVERIFIED` (fork not local) and is **not** emitted; if S14 finds an external consumer that requires it, the mapping is a cutover adapter, not a second emitter. `events[]`/`excluded_events[]` select the subscription; one `job(kind='webhook.deliver', dedupe_key=<event>:<subscription>)` per subscription, drained with `Authorization: Bearer <secret>`, 15 s timeout, 3 attempts.

**The 280 flows, as one query.** `cards.move` loads the card, both lists and `project.project_type_code`, then:

```sql
SELECT e.meta -> 'matrix' -> $card_type_code ->> $from_kind
  FROM catalogue_entry e
 WHERE e.tenant_id=$1 AND e.kind='project_type' AND e.code=$project_type_code AND e.is_active
```

The value is the allowed to-kind array (S03's `<card_type> → <from_kind> → [<to_kind>]`). Absent row, type, from-kind, or a kind not in the array ⇒ `409 card-move-not-allowed`. The lookup lives in `domain::board::may_move`, cached per `(tenant, project_type_code)`.

**The board export** (`board-export/v1`). Planka holds five `[agent-contract-test]` cards, so a dump cannot be evidence. The export is a **defined, key-referenced format**:

```json
{"format":"board-export/v1","source":"fixture|planka","project":{"key":"…"},
 "boards":[{"board":{…},"lists":[{"key":"…","kind":"intake","position":1024}],"labels":[],"custom_fields":[],"members":[],
  "cards":[{"key":"…","list_key":"…","position":1024,"name":"…","archived_at":null,"labels":[],
            "comments":[{"key":"…","author":"…","body":"…"}],
            "task_lists":[{"name":"…","tasks":[{"name":"…","done":false}]}],
            "custom_field_values":{}}]}]}
```

Two committed fixtures, emitted by the CLI: `v1-recording.json` (1 board, 5 lists, 20 cards, 30 comments, 40 tasks, 6 fields, openapi shapes) by `conveyor admin fixture board --cards 20 --lists 5 --comments 30 --tasks 40 --fields 6 --seed 7 --out tests/fixtures/board/v1-recording.json`, and `v1-1000.json` (1,000 cards / 10 lists) by `conveyor admin fixture board --cards 1000 --lists 10 --seed 42 --out tests/fixtures/board/v1-1000.json`. Replay uses the **real actions**, not SQL: `conveyor admin import board`; `conveyor admin export board` re-exports; `conveyor admin diff board <a> <b>` prints deltas, non-zero exit on any. Four subcommands: `fixture|import|export|diff`.

## 6. Build order

1. Preflight: `cargo test --workspace`; `lsof -nP -iTCP:8930 -sTCP:LISTEN` empty; `docker compose up -d db`.
2. `0004_board.sql`: 9 tables, FKs, §2.5 indexes, `ENABLE`+`FORCE` RLS, policies. `conveyor-server -- migrate`, twice.
3. `store/src/board/`: `_tx` functions, `tenant_id` first. `cargo test -p conveyor-store`.
4. Positions: midpoint, append `max+1024`, board advisory lock. `cargo test -p conveyor-api --test position`.
5. Matrix loader + cache + `may_move`. `cargo test -p conveyor-domain matrix`.
6. `projects.*`, `boards.get` (4) — `projects.create` also creates the board and five lists from `catalogue_entry(kind='board_template')`.
7. `lists.*` (3) — a `kind` edit that would strand a live card is 409. `cargo test -p conveyor-domain list_kind`.
8. `cards.create|get|update|archive|unarchive|duplicate` (6): `If-Match`, `201+ETag`, 412 body = current card.
9. Read projections + cursors; `EXPLAIN` each on a 1,000-card board, assert the index.
10. `cards.move` + `card.moved` + 409. `cargo test -p conveyor-api --test race_move`.
11. Comments (3) + `GET /api/cards/{id}/comments`. `cargo test -p conveyor-actions comments`.
12. Tasks: `task_lists.*`, `tasks.*` (7). `cargo test -p conveyor-actions tasks`.
13. Custom fields (5), kind validation, upsert `(card_id, custom_field_id)`. `cargo test -p conveyor-domain custom_field`.
14. Labels (3) + `cards.add_label|remove_label` + `boards.set_member`. `cargo test -p conveyor-actions labels`.
15. SSE: topic parse, `card:{id}` resolution, 200/topic replay, 15 s keepalive. `cargo test -p conveyor-api --test sse_board`.
16. Webhook CRUD + `webhook.deliver`. `cargo test -p conveyor-api --test webhook_shape`.
17. `admin fixture|import|export|diff board`, as §5. `cargo run -p conveyor-server -- admin diff board --help`
18. Gate (§7); `cargo clippy --workspace --all-targets -- -D warnings`; lock refresh.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
command -v k6 || brew install k6   # k6 is not installed (CONTRACT §16)
docker compose up -d db && cargo run -p conveyor-server -- migrate --dev-seed
CONVEYOR_DEV_AUTH=1 cargo run -p conveyor-server -- serve &
trap 'kill %1 2>/dev/null || true' EXIT
API=http://127.0.0.1:8930
F=tests/fixtures/board/v1-1000.json
T=$(psql "$DATABASE_URL" -tAc "select id from tenant where slug='dev-a'")   # pinned to dev-a: `dev:alice` is its principal (`tenant` has no RLS)
B=$(cargo run -q -p conveyor-server -- admin import board --file $F --tenant $T | jq -r .board_id)
cargo run -p conveyor-server -- admin export board --board $B > /tmp/out.json
cargo run -p conveyor-server -- admin diff board $F /tmp/out.json
k6 run -e BASE_URL=$API -e BOARD_ID=$B -e DEV_AUTH_TOKEN=dev:alice tests/load/board.js   # the script sends `authorization: Bearer ${__ENV.DEV_AUTH_TOKEN}` (CONTRACT §10)
cargo test -p conveyor-api --test position --test webhook_shape --test race_move --test sse_board
test -z "$(psql "$DATABASE_URL" -tAc "select tablename from pg_tables where schemaname='public' and tablename in ('card_membership','attachment')")"   # property (f): neither table exists (catalog read, no RLS)
```

| # | property | proof |
|---|---|---|
| a | replay fidelity | 1,000 cards/10 lists: every card, list, position, label, comment, task and field value matches; `admin diff` prints zero deltas |
| b | one winner | two clients move one card with the same `If-Match`: one 200, one **412 with the current representation**; `version` +1 |
| c | 1k load | `GET /api/boards/{id}/cards?limit=200` p95 **≤ 120 ms** at 20 rps; 1,000 cards in 5 pages **≤ 500 ms**, no sort in Rust |
| d | webhook shape | `{event,data,prevData,user}` exactly; `event` is the register type (`card.moved`), `data.list_id` the new lane and `prevData.list_id` the old. Planka's `cardUpdate`/`listId` spelling is a deferred cutover adapter (S14), not the emitter's vocabulary |
| e | SSE | a second client on `board:{id}` gets `card.moved` **≤ 200 ms** after the mover's 200, live and on replay |
| f | no second store | `\dt` shows one `comment`, one `board_membership`; no `card_membership`, no `attachment` |

## 8. Traps

1. **The Planka fork is not on this machine.** Port from the openapi (2.0.0-rc.4, 67 paths) + live `kanban` schema only. The per-event `data` envelope, `List.type` transitions and position seeding are `UNVERIFIED` — measure, never guess.
2. **`position numeric` midpoints.** `(a+b)/2`, append `max+1024`, never float; floats degrade after ~50 halvings and need the renumber job nobody writes.
3. **`list.kind` ≠ Planka's `List.type`.** Measured enum `active|closed|archive|trash` has no `intake`/`waiting`/`done`. The export carries **our** `kind`; import maps `closed|trash → archive` and infers nothing. `Card.isClosed` → `archived_at`.
4. **Attachments and card memberships are OUT** (SPEC §2); `Card.coverAttachmentId` has no target column. **One role enum, not three** — no `project_manager`, `board_subscription` or `user_roles` on this path.
5. **A webhook fires on the event, never a second write path.** A mutation that forgets its `event` row loses webhook and SSE — assert `event_count = 1` in every action test.
6. **Cursor pagination, never OFFSET** — concurrent drags skip and duplicate rows.
7. **Event-name drift.** The names are `card.moved`/`card.archived`, not `cards.moved`; S05 owns them because it ships the actions. `commentsTotal` is a query, not a column.
8. **Duplicate copies labels, tasks and field values — never comments** (a copied thread is false history). **The live Planka DB is five test cards**; no gate reads it as evidence.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S5-D1 | `board-export/v1`, key references, recorded + generated fixtures | uuids make a diff meaningless; live DB is test data |
| S5-D2 | zero REST write routes; every write is an action | one write path makes audit, idempotency and webhooks unskippable |
| S5-D3 | `custom_field_group` dropped into two columns | the group is a label, never joined |
| S5-D4 | `card:{id}` = board topic + `aggregate_id` filter | one outbox row, no fan-out duplication |
| S5-D5 | `projects.create` creates the default board and lists | no `boards.create` exists, and a board-less project is unusable |
| S5-D6 | `cards.create` accepts any non-archive list | birth is not a transition; the matrix has no from-state |
| S5-D7 | webhooks drained by a job from `event` | a synchronous POST couples the mutation to a third party |
| S5-D8 | `assignee_id`, `stopwatch_*` never written here | contract §4: the claim is S06's conditional UPDATE |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Full Planka event vocabulary or the proved subset? | subset; rest `UNVERIFIED` until measured |
| 2 | Label colour free text or the 20 named colours? | free text, seed the 20 |
| 3 | Multi-select: `ids[]` batch or N calls? | N calls until S12 measures a need |
| 4 | Export archived cards? | no — archives are history and would make every diff permanent |
