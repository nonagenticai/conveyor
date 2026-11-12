-- scripts/grants-runtime.sql — re-applied by the `migrate` subcommand AFTER the last
-- migration. A table created by a later slice is otherwise invisible to conveyor_runtime.
--
-- The same block is at the end of 0001_platform.sql so a database migrated by any path is
-- correct once; this file is what keeps it correct after every later migration.
--
-- ORDER MATTERS: the schema-wide GRANT first, then the revokes. Reversed, the GRANT re-grants
-- the append-only columns and the guarantees silently disappear.
GRANT USAGE ON SCHEMA public TO conveyor_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO conveyor_runtime;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO conveyor_runtime;
REVOKE ALL ON _sqlx_migrations FROM conveyor_runtime;
REVOKE UPDATE, DELETE ON audit_event FROM conveyor_runtime;
-- `event` and `action_invocation` are immutable facts: the outbox row is written once and
-- the invocation record is history. DELETE stays on `event` — `event.prune`
-- (and `job.prune` on `job`) deletes by retention.
REVOKE UPDATE ON event FROM conveyor_runtime;
-- S13 (CONTRACT §7): the ONE write an event takes after it is appended is the relay's
-- publication cursor (`published_at`, `bus_stream_id`). Granted at COLUMN level, so the row's
-- topic/type/payload/aggregate stay immutable to the runtime role and a `SET payload = …`
-- remains a permission error. The relay's UPDATE is exactly these two columns
-- (`crates/store/src/bus.rs::mark_published_tx`); widening this list is what would re-open the
-- append-only guarantee this file exists to keep.
GRANT UPDATE (published_at, bus_stream_id) ON event TO conveyor_runtime;
REVOKE UPDATE ON action_invocation FROM conveyor_runtime;
-- No code path deletes a tenant or an app_user; `tenant` is the isolation anchor and
-- `app_user` is the identity the memberships point at. A DELETE grant here is only a way to
-- lose both.
REVOKE DELETE ON tenant, app_user FROM conveyor_runtime;

-- `message` is an append-only fact (data-arch §2.6): a message is written once per send. DELETE
-- stays — message retention prunes one once its deliveries are gone.
-- 2026-11-08: `audience_member` was dropped by migration 0011, so it is no longer named here; a
-- REVOKE against a missing relation makes the post-migration grant pass fail.
REVOKE UPDATE ON message FROM conveyor_runtime;

-- S07: `quality_evaluation` is the append-only score history (S07 §5.5, data-arch §2.7). A
-- re-score INSERTs; nothing edits a past evaluation. UPDATE and DELETE are both withheld —
-- DELETE has no retention path (the table is `ret none`), so a granted DELETE would only be a
-- way to erase the evidence for a number that was published.
REVOKE UPDATE, DELETE ON quality_evaluation FROM conveyor_runtime;

-- `approval_decision` is the recorded quorum: a decision is INSERTed once and read by the
-- status derivation, never edited or erased. (The `approval` row it belongs to is the mutable
-- one; the decision rows are the evidence.)
REVOKE UPDATE, DELETE ON approval_decision FROM conveyor_runtime;

-- `trace_link` is an edge: S07 deletes and recreates one (`trace_links.delete` keeps DELETE),
-- and nothing updates one in place.
REVOKE UPDATE ON trace_link FROM conveyor_runtime;

-- ⚠️ `revision` is deliberately NOT revoked. It looks append-only, but two code paths write it:
-- `model.prune`/`revision` retention DELETEs, and publishing supersedes the previous row with
-- `UPDATE revision SET status = 'superseded'` (`crates/store/src/model.rs:1248`). Revoking
-- either grant would make supersede or retention fail at runtime.
