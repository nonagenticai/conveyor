---
slice: S03
title: Domain and schema — aggregates, state machines, baseline domain migration
status: planned
depends_on: [S02]
spec_sections: ["§5.3", "§6", "§8 S3", "§11 R8/R10"]
adds_tables: [programme, project, board, list, card_type, card, run, approval, approval_decision, governed_asset, trace_link, catalogue_entry, revision, andon_state]
adds_actions: []
adds_events: []   # card.moved/card.archived moved to S05 (it ships the move/archive actions — the emitter owns the name); governed_asset.transitioned is S07's; card.claimed/run.*/approval.*/andon.* are S06's
adds_modules: [domain, store]
parity_ids: PENDING-S0
---

⚠️ **Amended 2026-11-09 (process model).** `run` becomes a **tree**: `parent_run_id uuid → run`, `step_id uuid → process_step`, `depth int NOT NULL DEFAULT 0 CHECK (depth BETWEEN 0 AND 3)`, `CHECK (parent_run_id IS NULL OR step_id IS NOT NULL)`. The card-wide `uq_run_live` is replaced by root-only indexes — `(tenant_id, card_id) WHERE parent_run_id IS NULL AND status IN ('queued','running','waiting_approval')` and the project form `(tenant_id, project_id) WHERE parent_run_id IS NULL AND card_id IS NULL AND status IN (…)` — plus `uq (parent_run_id, step_id) WHERE parent_run_id IS NOT NULL AND status IN (…)` and `uq (tenant_id, parent_run_id, step_id, attempt)` for history. The columns land in migration `0033`, not `0002`; the old index is dropped there. The DAG's `workflow` table and `run.workflow_id` are dropped by `0034`.

# S03 — Domain and schema

Programme project: S03 · issues none yet

**Gate (SPEC §8 S3):** proptest on transitions; schema review.

## 1. Objective

Fourteen aggregates as total Rust functions, plus the one migration that stores them, so no later slice invents a table: `crates/domain` holds the machines, `crates/store` the SQL, Postgres the same refusal.

## 2. Starting state

- S02 shipped `0001_platform.sql` (11 tables), `begin_scoped`, `conveyor_runtime`, `domain::{TenantId, Id, Version}`.
- `crates/store` has pool, outbox, jobs, audit, idempotency; no domain query module. Routes: health, `/api/me`, `/api/events`.

## 3. Scope

**IN** — the 14 aggregates of §2.3+§2.4; `may_move`, run/approval/asset transitions; store modules taking `TenantId` under `_tx`; `0002_domain.sql` with columns, FKs, partial uniques, indexes, `ENABLE`+`FORCE` RLS and 14 policies; the `catalogue_entry.kind` vocabulary.

**OUT** — every route and action (S04–S06); board satellites `label`/`comment`/`task*`/`custom_field*`/`board_membership`/`webhook_subscription` (S05); channels and delivery (S06); outlines, quality, project_request (S07); models, views, portfolio registers (S08); documents, repositories, secrets (S09); conversations (S10); procedures (S11); workflow (S13). `run.procedure_id` / `run.workflow_id` arrive with S11/S13's FKs, and `run.capability_id` /
`run.capability_agent_id` with S15's `0020`, never here.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `02-DATA-ARCHITECTURE §2.3/§2.4` | 14 table bodies | verbatim | anything — a deviation is a contract change |
| `agent@83b8de7` — `crates/agent-api/src/{lease,tenancy}.rs`, `crates/agent-api/migrations/0190_b8_worker_lease.sql` | conditional `UPDATE … RETURNING`, paired-null CHECK, `begin_scoped` | claim shape, tx-local `set_config` | resume targets, `ACTIVE_TENANT_HEADER` |
| `strata_rag@b9426c3` `store_providers.rs` | `put_store_providers_tx` | the `_tx` convention | its schema |
| `fr@6f7c509` `100_card_phase_lease.sql` §3.4; `card_*` satellites; `30-rm-planka` §1 `pm_*` | the closed-degradation argument; the sprawl | *a lease that grants itself when the arbiter is down is not a lease*; the concept list | the lease table, the satellites |

## 5. Deliverables

```
crates/domain/src/{id,version,catalogue,board,run,approval,governance,lib}.rs
crates/store/src/{project,board,list,card,run,approval,governance,catalogue,revision,andon}.rs
crates/store/tests/{rls_domain,claim_race,approval_race,transitions}.rs
migrations/0002_domain.sql
```

`crates/domain` deps: serde, uuid, time, thiserror — no sqlx/tokio/axum.

```rust
pub enum ListKind { Intake, Active, Waiting, Done, Archive }
pub enum RunStatus { Queued, Running, WaitingApproval, Succeeded, Failed, Cancelled }
pub enum ApprovalStatus { Pending, Approved, Rejected, Expired, Voided }
// AssetKind: Requirement | Principle | Adr
// AssetStatus: Draft | InReview | Approved | Superseded | Archived

pub struct Card {                 // no `state` field, no accessor naming one
    pub list_id: Uuid, pub card_type_id: Uuid, pub board_id: Uuid,
    pub archived_at: Option<Timestamp>, pub assignee_id: Option<Uuid>,
    pub lease_expires_at: Option<Timestamp>, pub lease_token: Option<Uuid>,
    pub claim_fence: i64, pub version: Version,
}
pub fn may_move(card: &Card, from: ListKind, to: ListKind, rules: &CardTypeRules)
    -> Result<Move, MoveError>;
impl Run           { pub fn transition(self, to: RunStatus) -> Result<Run, RunError>; }
impl Approval      { pub fn decide(self, d: Decision, required: u16) -> Result<Self, ApprovalError>;
                     pub fn expire(self, at: Timestamp) -> Result<Self, ApprovalError>; }
impl GovernedAsset { pub fn transition(self, to: AssetStatus) -> Result<Self, GovernanceError>; }
```

`adds_tables` is **14**: §2.3 lists nine, §2.4 five; §8's dependency graph enumerates the same 14 names (§2.16 gives the per-slice count), and `00-INDEX.md`'s contract-delta register carries the list. The 67 total is unaffected.

`0002_domain.sql` — bodies are §2.3/§2.4 column-for-column; `approval_decision` therefore ports `decided_by_kind` and `via` from §2.3 as well, and `domain::approval::decide` carries the decider kind; the envelope and the clauses that carry this slice's invariants:

```sql
-- text + CHECK, never CREATE TYPE: ALTER TYPE … ADD VALUE cannot share a transaction
-- with an index on the new label, and each migration file is one transaction (data-arch §1.8)
list.kind  text NOT NULL CHECK (kind IN ('intake','active','waiting','done','archive'))
run.status text NOT NULL DEFAULT 'queued'
             CHECK (status IN ('queued','running','waiting_approval','succeeded','failed','cancelled'))
CONSTRAINT card_lease_pair CHECK ((lease_expires_at IS NULL) = (lease_token IS NULL));

-- one live ROOT run per card (amended 2026-11-09)
CREATE UNIQUE INDEX uq_run_live     ON run (tenant_id, card_id)
  WHERE parent_run_id IS NULL AND status IN ('queued','running','waiting_approval');
CREATE UNIQUE INDEX uq_appr_pending ON approval (tenant_id, run_id, gate) WHERE status = 'pending';

-- all 14, no exception
ALTER TABLE card ENABLE ROW LEVEL SECURITY;
ALTER TABLE card FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON card FOR ALL
  USING (tenant_id = app_current_tenant()) WITH CHECK (tenant_id = app_current_tenant());
```

**The card-state proof.** Three executable checks: `information_schema.columns` for `card` + `column_name IN ('state','status')` returns zero rows; an `insta` snapshot of `serde_json::to_value(&Card)` pins the field set; `grep -niE '\bstate\b' migrations/0002_domain.sql` returns **0** — `andon_state` does not match on a word boundary, so the substantive proof is the `information_schema` query above, not this grep.

`archived_at` and `list.kind='archive'` are two depths. The **kind** is a place: a positionable lane the card can be dragged out of — the UI's archive drop target writes `list_id`, a `may_move` transition. The **stamp** is exclusion: every hot index is `WHERE archived_at IS NULL`, so a stamped card leaves the board scan and is reachable only from the restore screen. Invariant, proptested: `archived_at IS NOT NULL ⇒ list.kind='archive'`. The UI archives by list kind; only the sweep stamps.

**The 280 flows as one row.** `catalogue_entry.kind` has no CHECK by design; S03 fixes the vocabulary: `project_type | phase | recipe | wip_limit | gate | capability | deliverable_kind | milestone_kind | priority_level | board_template | architecture_standard`. A project type carries its own matrix — `project_type.meta.matrix` — so there is **no separate matrix kind** and no second row (data-arch §2.3, §7.2).

| column | value |
|---|---|
| `kind` | `project_type` |
| `code` | `delivery` (= `project.project_type_code`) |
| `meta` | `{"matrix":{"feature":{"intake":["active","archive"],"active":["waiting","done","archive"],"waiting":["active","archive"],"done":["archive"],"archive":["intake"]},"bug":{"intake":["active"],"active":["done","archive"],"done":["archive"]}}}` |

`may_move(feature_card, Active, Waiting, rules)` reads `matrix["feature"]["active"]`, finds `"waiting"` ⇒ `Ok`. `may_move(feature_card, Intake, Done, _)` ⇒ `Err(MoveNotAllowed)` ⇒ 409 `card-move-not-allowed`. Absent entry, card type or row ⇒ **deny**; `wip_limit` rows are templates, the instance is `list.wip_limit`.

## 6. Build order

1. Preflight: `cargo test -p conveyor-domain`; `select max(version) from _sqlx_migrations` = 1; `docker compose up -d db`; `lsof -nP -iTCP:5439 -sTCP:LISTEN`. Never 5432, never 8000.
2. `domain/src/{id,version,catalogue}.rs`: `Code`, `Version`, `CatalogueKind`, `CardTypeRules::from_entry`. `cargo clippy -p conveyor-domain --all-targets -- -D warnings`.
3. `domain/src/board.rs`: `Programme`, `Project`, `Board`, `List`, `CardType`, `Card`, `Move`, `MoveError`, `may_move`; then `Card::archive(&List, Timestamp)` / `restore` and proptest (a). `cargo test -p conveyor-domain board`.
4. `domain/src/{run,approval}.rs`: both machines, terminal-wins refusal, `decide` reaching `required_decisions`. `cargo test -p conveyor-domain run approval`.
5. `domain/src/governance.rs`: `transition`, `supersede`, `Revision`, `RevisionPolicy::is_prunable`. `cargo test -p conveyor-domain`.
6. `0002_domain.sql`: `programme, project, board, list, card_type` (§2.3 verbatim), then `card` — claim columns, paired-null CHECK, `prev_list_id`, `list_changed_at`, `tsv` generated, three partial indexes, `gin(tsv)`, then `run, approval, approval_decision` with both partial uniques. `grep -c 'CREATE TABLE' migrations/0002_domain.sql` → 9.
7. Same file: `governed_asset, trace_link, catalogue_entry, revision, andon_state`; then RLS for 14, grants to `conveyor_runtime`, `REVOKE ALL ON _sqlx_migrations`. `grep -c 'FORCE ROW LEVEL SECURITY' migrations/0002_domain.sql` → 14.
8. `store/src/migrate.rs` assertion → `[(1,"platform"),(2,"domain")]`; `cargo run -p conveyor-server -- migrate`, twice.
9. `store/src/{project,board,list,card}.rs` under `_tx`; `claim_card_tx` is the data-arch §4 `UPDATE` verbatim, returning `Option<ClaimGranted{claim_fence, version}>`. `cargo clippy -p conveyor-store --all-targets -- -D warnings`.
10. `store/src/{run,approval}.rs`: `transition_run_tx`, `insert_approval_tx` (23505 → `ApprovalConflict`), `record_decision_tx`, `expire_due_tx`. `cargo clippy -p conveyor-store --all-targets -- -D warnings`.
11. `store/src/{governance,catalogue,revision,andon}.rs`; `transition_asset_tx` writes the `revision` row with `seq = version` in the same transaction. `cargo clippy -p conveyor-store --all-targets -- -D warnings`.
12. Store tests: RLS as `conveyor_runtime` for 14; claim race (b); approval race (c).
13. Schema review by a second reader against §2.3/§2.4 line by line — 14 tables, zero deviations or a reviewed list; `cargo clippy --workspace --all-targets -- -D warnings`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cargo test -p conveyor-domain      # proptests (a) and (d), ≥256 cases
cargo test -p conveyor-store       # (b) and (c) need a database
cargo run -p conveyor-server -- migrate && cargo run -p conveyor-server -- migrate
test "$(psql "$DATABASE_MIGRATE_URL" -tAc 'select count(*) from _sqlx_migrations')" -ge 2
test -z "$(psql "$DATABASE_MIGRATE_URL" -tAc "select column_name from information_schema.columns
     where table_name='card' and column_name in ('state','status')")"     # 0 rows
test -z "$(grep -niE 'create type' migrations/0002_domain.sql || true)"      # 0
test "$(grep -c 'FORCE ROW LEVEL SECURITY' migrations/0002_domain.sql)" = 14   # 14
```

| # | property | proof |
|---|---|---|
| a | no transition leaves `list.kind` and `archived_at` contradictory | proptest over random card/list/kind sequences: `archived_at.is_some() ⇒ from_kind == Archive`; `restore` off a non-archive list is `Err` |
| b | a claim never succeeds twice without release or expiry | 32 concurrent tasks on one card: exactly one `Some`; sequential claims strictly increase `claim_fence`; a stale-fence completion writes nothing |
| c | one pending approval per (run, gate) | 16 concurrent inserts: one `Ok`, fifteen `ApprovalConflict`, pending count 1 |
| d | truncate/archive invariants | a terminal `run` rejects every transition; `revision` seq dense from 1; `is_prunable("model")` true past 50, false for `"requirement"` at any depth |

RLS: as `conveyor_runtime` scoped to A, B's rows are absent from all 14 tables and a forged `tenant_id=B` insert fails `WITH CHECK`; the **owner** sees 0 (`FORCE`), a superuser both. Migrations: fresh DB → 2 applied; second run → exit 0, `_sqlx_migrations` unchanged; no `IF NOT EXISTS` — it masks failure as a no-op.

## 8. Traps

1. **List-as-state has no second source.** A `CardState` derived from `list.kind` is a read; cached it is fr's disagreement class reborn. No `UPDATE card SET status`.
2. **`position numeric` midpoint insert.** `(a+b)/2` between neighbours, `max+1024` on append. Planka's `double precision` was the old behaviour: ~50 halvings, then the compaction job nobody wrote. A move writes `list_id, prev_list_id, list_changed_at, position` in one statement.
3. **`governed_asset` vs a fork.** `kind` is a column; a kind needing a filtered field gets a nullable column (data-arch Q2). `superseded ⇒ superseded_by_id IS NOT NULL` is an **immediate CHECK** in `0002` — PostgreSQL CHECK constraints cannot be deferred, so the one `UPDATE` writes both pointers and the reciprocal edge is domain-enforced (§3.4).
4. **`revision` unbounded for assets, 50 for models.** A job-applied policy on `entity_type`, never a trigger; unknown ⇒ keep. A wrong cap deletes history.
5. **The partial index is the guarantee, not the code.** `check-then-insert` races; 23505 → 409. `run_id IS NULL` escapes `uq_appr_pending` (NULLs distinct): gate approvals carry a run, card-level ones use `INSERT … WHERE NOT EXISTS`.
6. **`timestamptz` UTC.** Every lease test is `lease_expires_at < now()` in SQL, never a Rust clock.
7. **`FORCE` RLS.** `ENABLE` alone lets the owner read everything; prove owner 0, superuser n.
8. **`card.board_id` is denormalised** and must equal `list.board_id`; `move_card_tx`/`claim_card_tx` join `list` in the same statement.
9. **`sqlx::migrate!` path.** S02 fixed it; S03 extends the embedded-set assertion. A file outside the set is a silent no-op.
10. **R10 — porting fr's accreted model is the biggest failure mode.** The baseline is built from SPEC §6's domain model, never by transcribing fr's 224 migrations; a column that exists only because a past migration needed it does not enter this schema.
11. **R8 — tenancy is typed, not remembered.** Every store function takes `TenantId`; RLS is `FORCE`d; the isolation proof is in S02's gate and re-run here for every new table.

## 9. Decisions closed here

| id | decision | rationale |
|---|---|---|
| S3-D1 | Matrix is one row: `kind='project_type'`, `code = project_type_code`, `meta.matrix` | `kind` has no CHECK; S03 owns the vocabulary, and data-arch §2.3/§7.2 plus the S07 seed already put the matrix on the project_type row |
| S3-D2 | `archived_at ⇒ list.kind='archive'`; UI archives by list kind, sweep stamps | one proptestable invariant, not two archive notions |
| S3-D3 | Absent entry / card type / row ⇒ deny | the rewrite cannot silently allow |
| S3-D4 | `revision.seq = governed_asset.version`, both from 1 | "revision 7" means one thing everywhere |
| S3-D5 | `approval_decision` has no `version`, immutable after insert; `andon_state` is one lazily created row per tenant, pk `tenant_id`; claim TTL is a store parameter (`DEFAULT_CLAIM_TTL` 900 s) | a decision is a fact; no null tenant; fr's per-phase TTLs had no basis |

## 10. Open questions

| # | question | default |
|---|---|---|
| 1 | Claim TTL 900 s vs `agent`'s 600 s | 900 s; revisit at S06 |
| 2 | `NULLS NOT DISTINCT` on `uq_appr_pending` would close the `run_id IS NULL` hole | not here — a data-arch change S06 decides with the card-level path |
| 3 | `catalogue_entry` seed size (data-arch §10 Q6) | the worked row; full seed via `migrate --seed` from S00 counts |
