//! The durable job queue: claim, fenced finish, and the lease-expiry reclaim.
//!
//! Data-architecture §4 and CONTRACT §7 fix three properties, and this module is where all
//! three live:
//!
//! 1. **Claim is `FOR UPDATE SKIP LOCKED`** with a fenced `lease_token` and a lease window.
//! 2. **Expired-lease reclaim.** A worker killed with `SIGKILL` leaves its row `running`;
//!    no cleanup path runs. The claim query therefore also selects
//!    `status='running' AND lease_until < now()`, so the row is reclaimed on the next poll
//!    — `attempts` increments and a NEW token is issued.
//! 3. **A stale token writes nothing.** Every finish/retry statement is guarded by
//!    `status='running' AND lease_token = $token`; the killed worker's token no longer
//!    matches, so its late write affects zero rows.
//!
//! `max_attempts` exhaustion is decided in the database, not in the handler: a running row
//! whose lease expired at or past `max_attempts` is marked `dead` before the next claim, and
//! a retry requested at the last attempt also lands `dead`.
//!
//! Every public database function takes a [`TenantId`] and opens its own [`begin_scoped`]
//! transaction; the internal `reap_exhausted` runs on the caller's already-scoped
//! connection, and `truncate` touches no database. The worker iterates tenants (the `tenant`
//! table has no RLS) instead of using a `BYPASSRLS` role that S2-D3 deliberately does not
//! create.

use std::time::Duration;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// `JOB_LEASE_SECS`'s default; the process sets it from the environment.
pub const DEFAULT_LEASE_SECS: i64 = 60;

/// `last_error` is cut to this many characters before storage.
pub const MAX_ERROR_CHARS: usize = 2000;

/// The default retry delay after a retryable handler failure.
pub const RETRY_AFTER: Duration = Duration::from_secs(30);

/// One `job` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub kind: String,
    pub dedupe_key: String,
    pub status: String,
    pub payload: Value,
    pub run_after: DateTime<Utc>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub lease_until: Option<DateTime<Utc>>,
    pub lease_token: Option<Uuid>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    /// The tenant as a typed id. A nil `tenant_id` cannot come from the schema (the column
    /// is `NOT NULL DEFAULT app_current_tenant()` and the scope refuses nil), so this is a
    /// programmer-error path, not a data path.
    pub fn tenant(&self) -> Result<TenantId, conveyor_domain::IdError> {
        TenantId::from_uuid(self.tenant_id)
    }

    /// The token a finish must present.
    pub fn lease_token(&self) -> Option<Uuid> {
        self.lease_token
    }
}

/// What a fenced finish produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finish {
    /// False when the lease was lost: nothing was written.
    pub changed: bool,
    /// The status the row now holds (`succeeded`, `pending` or `dead`), when changed.
    pub status: Option<String>,
}

const COLUMNS: &str = "id, tenant_id, kind, dedupe_key, status, payload, run_after, attempts, \
                       max_attempts, lease_until, lease_token, last_error, created_at, updated_at";

/// Mark every expired-lease row that has no attempt left as `dead`.
///
/// Runs in the caller's scoped transaction before the claim, so the claim never hands out
/// a row that must die. Returns the number of rows dead-lettered.
async fn reap_exhausted(conn: &mut sqlx::PgConnection) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE job
            SET status = 'dead',
                last_error = coalesce(last_error, 'lease expired after the maximum attempts'),
                lease_until = NULL,
                lease_token = NULL,
                updated_at = now()
          WHERE status = 'running'
            AND lease_until < now()
            AND attempts >= max_attempts",
    )
    .execute(conn)
    .await?
    .rows_affected();
    Ok(rows)
}

/// Claim one due job for `tenant`, or `None`.
///
/// The claim covers both `pending AND run_after <= now()` and the reclaim arm
/// `running AND lease_until < now()` (data-architecture §4, S02 §6 step 12). With no kind
/// filter it is the general drain, exactly as before [`claim_with`] existed.
pub async fn claim(
    pool: &PgPool,
    tenant: TenantId,
    lease_secs: i64,
) -> Result<Option<Job>, sqlx::Error> {
    claim_with(pool, tenant, lease_secs, None).await
}

/// Claim one due job, optionally restricted to `kinds`.
///
/// A worker that drains one family (`delivery.send`, `run.execute`) must not pick up an
/// unrelated kind and starve its own queue behind it; the filter is part of the same
/// `FOR UPDATE SKIP LOCKED` statement, never a post-claim check that would have to release
/// the row. `None` is the unfiltered claim and is what [`claim`] delegates with.
pub async fn claim_with(
    pool: &PgPool,
    tenant: TenantId,
    lease_secs: i64,
    kinds: Option<&[&str]>,
) -> Result<Option<Job>, sqlx::Error> {
    let lease_secs = lease_secs.max(1);
    let kinds: Option<Vec<String>> = kinds.map(|kinds| {
        kinds
            .iter()
            .map(|kind| (*kind).to_owned())
            .collect::<Vec<String>>()
    });
    let mut tx = begin_scoped(pool, tenant).await?;
    reap_exhausted(&mut tx).await?;

    let sql = format!(
        // The CTE renames `id` to `job_id`: an unqualified `RETURNING id` with `due` in the
        // FROM list is ambiguous, and PostgreSQL refuses the statement.
        "WITH due AS (
             SELECT id AS job_id FROM job
              WHERE ((status = 'pending' AND run_after <= now())
                  OR (status = 'running' AND lease_until < now()))
                AND ($2::text[] IS NULL OR kind = ANY($2))
              ORDER BY run_after, id
              FOR UPDATE SKIP LOCKED
              LIMIT 1
         )
         UPDATE job j
            SET status = 'running',
                lease_token = uuidv7(),
                lease_until = now() + make_interval(secs => $1),
                attempts = j.attempts + 1,
                updated_at = now()
           FROM due
          WHERE j.id = due.job_id
         RETURNING {COLUMNS}"
    );
    let job: Option<Job> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(lease_secs as f64)
        .bind(&kinds)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(job)
}

/// Every job of one `(kind, dedupe_key)`, newest first.
///
/// The dedupe key is unique only among **live** rows (`pending`/`running`), so a key with a
/// history has one row per attempt. The newest row is the one that decides whether the work
/// is still alive; `run.reap` uses exactly this ordering to find a dead `run.execute` job
/// whose run must be failed. Reads on the caller's scoped connection — it writes nothing.
pub async fn by_dedupe(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    dedupe_key: &str,
) -> Result<Vec<Job>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM job
          WHERE tenant_id = $1 AND kind = $2 AND dedupe_key = $3
          ORDER BY created_at DESC, id DESC"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(dedupe_key)
        .fetch_all(&mut *conn)
        .await
}

/// Extend a running job's lease, if this worker still owns it.
///
/// A handler may legitimately outlive `JOB_LEASE_SECS` (an `agent.run` step can hold a model call
/// for minutes), and a job whose lease expires is reclaimable by another worker — which would run
/// the same step twice and pay for the model call twice (audit V1, 2026-11-10). The holder renews
/// while it works; `false` means the lease was lost, so its eventual finish is fenced and writes
/// nothing.
pub async fn renew(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    lease_token: Uuid,
    lease_secs: i64,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows = sqlx::query(
        "UPDATE job
            SET lease_until = now() + make_interval(secs => $4), updated_at = now()
          WHERE id = $1 AND tenant_id = $3 AND status = 'running' AND lease_token = $2
            AND lease_until IS NOT NULL",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(tenant.as_uuid())
    .bind(lease_secs as f64)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Finish a job successfully. False (and no write) when `lease_token` is not the live one.
pub async fn finish_succeeded(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    lease_token: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows = sqlx::query(
        "UPDATE job
            SET status = 'succeeded',
                last_error = NULL,
                lease_until = NULL,
                lease_token = NULL,
                updated_at = now()
          WHERE id = $1 AND status = 'running' AND lease_token = $2",
    )
    .bind(job_id)
    .bind(lease_token)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Finish a job as permanently failed (a bug, an unknown kind, an unrecoverable input).
///
/// Fenced like every other finish: a worker that lost its lease writes nothing.
pub async fn finish_dead(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    lease_token: Uuid,
    error: &str,
) -> Result<bool, sqlx::Error> {
    let error = truncate(error);
    let mut tx = begin_scoped(pool, tenant).await?;
    let rows = sqlx::query(
        "UPDATE job
            SET status = 'dead',
                last_error = $3,
                lease_until = NULL,
                lease_token = NULL,
                updated_at = now()
          WHERE id = $1 AND status = 'running' AND lease_token = $2",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(error)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(rows == 1)
}

/// Finish a failed attempt: `pending` with a delay, or `dead` when the attempts are spent.
///
/// The decision reads the row's own `attempts`/`max_attempts` inside the statement, so two
/// racing finishers cannot both decide "one more try".
pub async fn fail(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    lease_token: Uuid,
    error: &str,
    retry_after: Duration,
) -> Result<Finish, sqlx::Error> {
    let error = truncate(error);
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE job
            SET status = CASE WHEN attempts >= max_attempts THEN 'dead' ELSE 'pending' END,
                run_after = CASE WHEN attempts >= max_attempts
                                 THEN run_after
                                 ELSE now() + make_interval(secs => $4) END,
                last_error = $3,
                lease_until = NULL,
                lease_token = NULL,
                updated_at = now()
          WHERE id = $1 AND status = 'running' AND lease_token = $2
          RETURNING status",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(error)
    .bind(retry_after.as_secs_f64())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(match row {
        Some((status,)) => Finish {
            changed: true,
            status: Some(status),
        },
        None => Finish {
            changed: false,
            status: None,
        },
    })
}

/// Enqueue a job, deduped on `(tenant, kind, dedupe_key)` while one is pending or running.
///
/// Returns the new id, or `None` when a live job with the same key already exists — the
/// second caller does not get a second worker racing the first.
pub async fn enqueue(
    pool: &PgPool,
    tenant: TenantId,
    kind: &str,
    dedupe_key: &str,
    payload: Value,
    run_after: Option<DateTime<Utc>>,
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let id = enqueue_tx(
        &mut tx,
        tenant,
        kind,
        dedupe_key,
        payload,
        run_after,
        max_attempts,
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Enqueue a job in the CALLER's scoped transaction.
///
/// S16's cadence trigger needs the ledger row and the audit job to commit together (or not at
/// all): the count is the ledger, so a job enqueued outside that transaction would name a row
/// that may roll back. Same dedupe rule as [`enqueue`].
pub async fn enqueue_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    dedupe_key: &str,
    payload: Value,
    run_after: Option<DateTime<Utc>>,
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let id: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, run_after, max_attempts)
         VALUES ($1, $2, $3, $4, coalesce($5, now()), $6)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(dedupe_key)
    .bind(payload)
    .bind(run_after)
    .bind(max_attempts)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id.map(|(id,)| id))
}

fn truncate(error: &str) -> String {
    error.chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_truncated_by_characters_not_bytes() {
        let long = "é".repeat(MAX_ERROR_CHARS + 50);
        let cut = truncate(&long);
        assert_eq!(cut.chars().count(), MAX_ERROR_CHARS);
        assert!(cut.is_char_boundary(cut.len()));
    }
}
