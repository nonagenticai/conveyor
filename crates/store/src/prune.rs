//! Retention: one tenant at a time, batched, inside the tenant scope.
//!
//! Data-architecture fixes the rules: `event` 90 days, `job` 30 days after terminal,
//! `idempotency_key` by its own `expires_at` (§6), model revisions keep the newest 50 per
//! model (§2.4), `model_validation_run` 90 days (§5.3), `action_invocation` 30 days, `run` 180
//! days after a terminal status and `delivery` 30 days after a terminal state (§6 — the last
//! three had no producer until `0012`).
//!
//! **`approval` and `audit_event` are never pruned.** Both are the governance record; a pruned
//! run detaches from its approvals (`ON DELETE SET NULL`) rather than deleting them. Every delete runs in a
//! [`begin_scoped`] transaction, so RLS makes a cross-tenant delete impossible — there is NO
//! `BYPASSRLS` role (S2-D3) and `event.prune` must never be written as `DELETE FROM event
//! WHERE created_at < …` against the whole table, which would silently match zero rows as the
//! runtime role and read like a working prune.
//!
//! Batches keep a long transaction (and its locks) off the operational tables: the delete
//! loop commits chunk by chunk until a chunk comes back short.

use conveyor_domain::TenantId;
use sqlx::PgPool;

use crate::scope::begin_scoped;

/// `event` retention (data-architecture §6; ships with the table, never later).
pub const EVENT_RETENTION_DAYS: i64 = 90;

/// `job` retention after a terminal status.
pub const JOB_RETENTION_DAYS: i64 = 30;

/// How many revisions of each model survive `model.prune` (data-architecture §2.4).
///
/// The same number the domain's [`RevisionPolicy`](conveyor_domain::RevisionPolicy) states:
/// a model revision is recoverable working state, so the newest 50 per model are kept. A
/// governed asset's revisions are never pruned — they are the governance record.
pub const MODEL_REVISION_KEEP: i64 = 50;

/// `model_validation_run` retention (data-architecture §5.3).
pub const VALIDATION_RUN_RETENTION_DAYS: i64 = 90;

/// `action_invocation` retention (data-architecture §6). It is an operational trace, not a record.
///
/// `audit_event` is the durable history and is never pruned; this table is the latency/outcome
/// sample beside it, and 30 days is what makes it worth keeping.
pub const ACTION_INVOCATION_RETENTION_DAYS: i64 = 30;

/// `run` retention after a terminal status (data-architecture §6).
///
/// A run is kept 180 days — long enough to explain a decision — and its `run_step` rows go with
/// it: S11 declares `run_step.run_id ON DELETE CASCADE`, so this prune carries them for free.
pub const RUN_RETENTION_DAYS: i64 = 180;

/// `delivery` retention after a terminal state (data-architecture §6).
///
/// Deliveries prune before `message`: a message is deletable only once no delivery references it.
pub const DELIVERY_RETENTION_DAYS: i64 = 30;

/// Rows per delete statement.
pub const PRUNE_BATCH: i64 = 5_000;

/// Delete this tenant's events older than `days`.
///
/// The `tenant_id` predicate is redundant under RLS and deliberately kept: it makes the
/// statement's scope readable, and the policy still refuses any row another tenant owns.
pub async fn event_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM event
              WHERE seq IN (
                    SELECT seq FROM event
                     WHERE tenant_id = $1
                       AND created_at < now() - make_interval(days => $2)
                     ORDER BY seq
                     LIMIT $3)
             RETURNING seq",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's terminal jobs older than `days` (since their last state change).
///
/// `pending` and `running` rows are never pruned: they are the queue, not history.
pub async fn job_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM job
              WHERE id IN (
                    SELECT id FROM job
                     WHERE tenant_id = $1
                       AND status IN ('succeeded', 'failed', 'dead')
                       AND updated_at < now() - make_interval(days => $2)
                     ORDER BY updated_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's model revisions past the newest `keep` of each model.
///
/// The cut-off is per model, not global: `seq` is the dense per-entity sequence, so a
/// revision is a candidate when
/// `seq <= (SELECT max(seq) FROM revision … WHERE entity_id = r.entity_id) - keep`. The
/// subquery carries `tenant_id` and `entity_type = 'model'` too, so a model can never be
/// measured against another tenant's or another family's rows.
///
/// The revision named by `model.published_revision_id` is NEVER deleted, even when it is
/// older than the cut-off. A published snapshot is the governance record of a model (the
/// same way a governed asset's revisions are), and `models.compare` would lose the newest
/// thing it compares. The FK from `model.published_revision_id` is NO ACTION, so deleting it
/// would also fail the batch outright.
pub async fn model_revision_prune(
    pool: &PgPool,
    tenant: TenantId,
    keep: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM revision
              WHERE id IN (
                    SELECT r.id FROM revision r
                     WHERE r.tenant_id = $1
                       AND r.entity_type = 'model'
                       AND r.seq <= (SELECT max(newest.seq)
                                       FROM revision newest
                                      WHERE newest.tenant_id = r.tenant_id
                                        AND newest.entity_type = 'model'
                                        AND newest.entity_id = r.entity_id) - $2
                       AND NOT EXISTS (
                             SELECT 1 FROM model m
                              WHERE m.tenant_id = r.tenant_id
                                AND m.published_revision_id = r.id)
                     ORDER BY r.entity_id, r.seq
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(keep)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's validation runs older than `days`.
///
/// A run is an append-only report, so age is the only thing that may expire it. The revision
/// it named is nullable: `0010` re-created the FK as `ON DELETE SET NULL`, so a
/// `model.prune` that removes the revision leaves the run readable with `revision_id` NULL.
pub async fn validation_run_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM model_validation_run
              WHERE id IN (
                    SELECT id FROM model_validation_run
                     WHERE tenant_id = $1
                       AND created_at < now() - make_interval(days => $2)
                     ORDER BY created_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's `action_invocation` rows older than `days`.
///
/// No FK points at `action_invocation`, so nothing cascades and nothing blocks: the row is the
/// whole record. It is the simplest of the three retention rules, and the highest-volume table.
pub async fn action_invocation_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM action_invocation
              WHERE id IN (
                    SELECT id FROM action_invocation
                     WHERE tenant_id = $1
                       AND created_at < now() - make_interval(days => $2)
                     ORDER BY created_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's terminal runs older than `days`.
///
/// Three conditions, each load-bearing:
///
/// - **terminal only.** A `queued`/`running`/`waiting_approval` run is live work; `uq_run_live`
///   exists to make a second live run on one card impossible, and deleting one would unblock it.
/// - **`finished_at`, not `updated_at`.** Every terminal transition stamps `finished_at`, so it is
///   the exact clock; a `coalesce(finished_at, updated_at)` would defeat the partial index and
///   would age a run by a later, unrelated write.
/// - **no pending approval.** `uq_appr_pending` is partial on `(tenant_id, run_id, gate) WHERE
///   status = 'pending'`, and `0012` made `approval.run_id` nullable. Deleting the run would
///   detach a pending gate to NULL, where NULLs are distinct and the index stops protecting the
///   key. A run with an unanswered gate is therefore skipped, not pruned — and it stays visible.
///
/// Approvals themselves are never pruned (data-arch §6): the pruned run detaches, the human's
/// decision and its reason survive.
pub async fn run_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM run
              WHERE id IN (
                    SELECT r.id FROM run r
                     WHERE r.tenant_id = $1
                       AND r.status IN ('succeeded', 'failed', 'cancelled')
                       AND r.finished_at < now() - make_interval(days => $2)
                       AND NOT EXISTS (
                             SELECT 1 FROM approval a
                              WHERE a.tenant_id = r.tenant_id
                                AND a.run_id = r.id
                                AND a.status = 'pending')
                     ORDER BY r.finished_at, r.id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's deliveries that reached a terminal state more than `days` ago.
///
/// The clock is `updated_at`: every terminal transition stamps it, and `delivered_at` is NULL for
/// `failed` and `expired`. `pending` and `sent` are never pruned — they are the drainer's queue.
pub async fn delivery_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM delivery
              WHERE id IN (
                    SELECT id FROM delivery
                     WHERE tenant_id = $1
                       AND state IN ('delivered', 'failed', 'expired')
                       AND updated_at < now() - make_interval(days => $2)
                     ORDER BY updated_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's expired idempotency keys (24 h TTL, data-architecture §2.2).
pub async fn idempotency_sweep(
    pool: &PgPool,
    tenant: TenantId,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM idempotency_key
              WHERE (tenant_id, scope, key) IN (
                    SELECT tenant_id, scope, key FROM idempotency_key
                     WHERE tenant_id = $1 AND expires_at <= now()
                     LIMIT $2)
             RETURNING key",
        )
        .bind(tenant.as_uuid())
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// `inference_call` retention (data-architecture §6): 90 days, deliberately longer than
/// `action_invocation`'s 30 because a cache report over a quarter is the point of the table.
pub const INFERENCE_CALL_RETENTION_DAYS: i64 = 90;

/// `prompt_audit` + `prompt_audit_finding` retention (data-architecture §6): 180 days after the
/// audit's last finding is decided.
pub const PROMPT_AUDIT_RETENTION_DAYS: i64 = 180;

/// `eval_trace` retention (data-architecture §6): 90 days. The transcript is the bulk of an eval
/// run; results, cases and audits live with the eval.
pub const EVAL_TRACE_RETENTION_DAYS: i64 = 90;

/// Delete this tenant's eval traces older than `days`, batched.
///
/// A trace is the subject's transcript for one `(case, rep)` slot. Nothing references it by FK —
/// a result row is scored from the grade it recorded — so pruning a trace never orphans a result.
pub async fn eval_trace_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM eval_trace
              WHERE id IN (
                    SELECT id FROM eval_trace
                     WHERE tenant_id = $1
                       AND created_at < now() - make_interval(days => $2)
                     ORDER BY created_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's ledger rows older than `days`, batched.
pub async fn inference_call_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let deleted = sqlx::query(
            "DELETE FROM inference_call
              WHERE id IN (
                    SELECT id FROM inference_call
                     WHERE tenant_id = $1
                       AND created_at < now() - make_interval(days => $2)
                     ORDER BY created_at, id
                     LIMIT $3)
             RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?
        .len() as u64;
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}

/// Delete this tenant's audits whose last finding was decided more than `days` ago, and their
/// findings. An audit with an OPEN finding is never pruned: the finding is still a person's work.
///
/// The finding FK has no `ON DELETE CASCADE`, so the children are deleted first in the same
/// transaction — an audit is never half-pruned.
pub async fn prompt_audit_prune(
    pool: &PgPool,
    tenant: TenantId,
    days: i64,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let batch = batch.max(1);
    let mut total = 0u64;
    loop {
        let mut tx = begin_scoped(pool, tenant).await?;
        let ids: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT a.id
               FROM prompt_audit a
              WHERE a.tenant_id = $1
                AND a.status IN ('succeeded', 'failed')
                AND greatest(
                        a.updated_at,
                        coalesce((SELECT max(f.decided_at)
                                    FROM prompt_audit_finding f
                                   WHERE f.audit_id = a.id), a.updated_at)
                    ) < now() - make_interval(days => $2)
                AND NOT EXISTS (SELECT 1 FROM prompt_audit_finding f
                                 WHERE f.audit_id = a.id AND f.status = 'open')
              ORDER BY a.id
              LIMIT $3",
        )
        .bind(tenant.as_uuid())
        .bind(days as i32)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?;
        let ids: Vec<uuid::Uuid> = ids.into_iter().map(|(id,)| id).collect();
        if ids.is_empty() {
            tx.commit().await?;
            return Ok(total);
        }
        sqlx::query("DELETE FROM prompt_audit_finding WHERE tenant_id = $1 AND audit_id = ANY($2)")
            .bind(tenant.as_uuid())
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        let deleted = sqlx::query("DELETE FROM prompt_audit WHERE tenant_id = $1 AND id = ANY($2)")
            .bind(tenant.as_uuid())
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        total += deleted;
        if deleted < batch as u64 {
            return Ok(total);
        }
    }
}
