#![forbid(unsafe_code)]
//! Queries and writes the action registry needs that S02/S03 did not own: the platform
//! read models (`tenant`, `membership` + `app_user`, `department`), the `action_invocation`
//! writer, keyset cursors, and the `action_registry` table (data-architecture §2.14).
//!
//! Every function takes the caller's connection and [`TenantId`] and runs inside the
//! transaction the invocation engine already scoped with [`crate::begin_scoped`]. A
//! function here never opens its own transaction: the mutation, `action_invocation`, the
//! `audit_event` and any outbox row must commit or roll back together (CONTRACT §6/§7).
//!
//! **Cross-tenant parents (S03's validator trap).** A parent id (`parent_id`, `user_id`,
//! a job id) is always resolved with a tenant-scoped `SELECT` first. Row-level security
//! confines that SELECT to the caller's tenant, so a forged id that exists in another
//! tenant resolves to `None` and the action returns `404` — the raw foreign id is never
//! handed to a single-column foreign key, which would accept it (RI bypasses RLS).

use base64::Engine as _;

use crate::parent::{Parent, ParentError, ensure_parent_tx};
use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// CONTRACT §5 pagination default.
pub const DEFAULT_LIMIT: i64 = 50;

/// CONTRACT §5 pagination ceiling.
pub const MAX_LIMIT: i64 = 200;

/// Clamp a requested page size into the contract's range.
pub fn clamp_limit(requested: Option<u32>) -> i64 {
    requested.map_or(DEFAULT_LIMIT, |n| i64::from(n).clamp(1, MAX_LIMIT))
}

/// The `action_invocation` outcome vocabulary (0001_platform.sql's CHECK).
pub const OUTCOMES: [&str; 4] = ["ok", "refused", "error", "timeout"];

/// One `tenant` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TenantRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub settings: Value,
    pub version: i64,
}

/// One member joined to its `app_user`.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MemberRow {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub department_id: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

/// One `department` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DepartmentRow {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    /// The industry the department belongs to (T0).
    pub industry_id: Option<Uuid>,
    /// 1-based order inside the industry (T0).
    pub ordinal: Option<i32>,
    /// The industry's primary department (T0).
    pub is_primary: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

/// A department to create (T0 adds the organisation placement).
pub struct NewDepartment<'a> {
    /// The department name (unique per tenant).
    pub name: &'a str,
    /// A parent department, when nested.
    pub parent_id: Option<Uuid>,
    /// The industry, when the department is part of one.
    pub industry_id: Option<Uuid>,
    /// 1-based order inside the industry.
    pub ordinal: Option<i32>,
    /// Whether this is the industry's primary department.
    pub is_primary: bool,
}

/// One `audit_event` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AuditRow {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub actor_kind: String,
    pub action: String,
    pub status: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

/// One `job` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub kind: String,
    pub dedupe_key: String,
    pub status: String,
    pub payload: Value,
    pub run_after: DateTime<Utc>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One `schedule` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ScheduleRowOut {
    pub id: Uuid,
    pub kind: String,
    pub cron: String,
    pub timezone: String,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub version: i64,
}

/// One `event` row in list form (`type` is a SQL keyword, hence the rename).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EventRowOut {
    pub seq: i64,
    pub id: Uuid,
    pub topic: String,
    #[sqlx(rename = "type")]
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

/// One `app_licence` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LicenceRowOut {
    pub app_key: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
}

/// One `action_registry` row (data-architecture §2.14).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RegistryRow {
    pub id: Uuid,
    pub name: String,
    pub spec_version: i32,
    pub kind: String,
    pub service: String,
    pub spec: Value,
    pub schema_hash: String,
    pub timeout_ms: i32,
    pub idempotent: bool,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<Uuid>,
}

/// A parent id that did not resolve inside the caller's tenant.
///
/// The whitelist's generic helper ([`ensure_parent_tx`]) covers the board/portfolio tables;
/// `department` (a self-parent) and `membership` (keyed by `user_id`, not `id`) need their
/// own spelling of the same tenant-scoped `EXISTS`, so the resolution still happens in the
/// store — never in a handler, and never at the foreign key.
#[derive(Debug, thiserror::Error)]
pub enum ActionParentError {
    /// The id names no row visible in this tenant.
    #[error("the referenced {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// A whitelisted parent that did not resolve.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the lookup or the write.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

// ── transaction knobs ────────────────────────────────────────────────────────────────────

/// Set `statement_timeout` for the rest of the transaction.
///
/// `set_config(..., true)` is the bind-parameter form of `SET LOCAL`, so it dies at
/// COMMIT/ROLLBACK and can never leak onto a pgbouncer server connection (the estate's
/// measured transaction-pooling hazard).
pub async fn set_statement_timeout(conn: &mut PgConnection, ms: u32) -> Result<(), sqlx::Error> {
    let value = ms.max(1).to_string();
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(value)
        .execute(conn)
        .await?;
    Ok(())
}

// ── action_invocation ────────────────────────────────────────────────────────────────────

/// The columns one invocation row carries (CONTRACT §6).
#[derive(Clone, Debug)]
pub struct InvocationRecord<'a> {
    pub actor_id: Option<Uuid>,
    pub action: &'a str,
    pub action_version: i32,
    pub input_hash: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub outcome: &'a str,
    pub duration_ms: i32,
    pub trace_id: Option<&'a str>,
}

/// Insert one `action_invocation` row into the caller's transaction.
pub async fn record_invocation(
    conn: &mut PgConnection,
    tenant: TenantId,
    record: &InvocationRecord<'_>,
) -> Result<Uuid, sqlx::Error> {
    debug_assert!(OUTCOMES.contains(&record.outcome));
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO action_invocation
             (tenant_id, actor_id, action, action_version, input_hash, idempotency_key,
              outcome, duration_ms, trace_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(record.actor_id)
    .bind(record.action)
    .bind(record.action_version)
    .bind(record.input_hash)
    .bind(record.idempotency_key)
    .bind(record.outcome)
    .bind(record.duration_ms)
    .bind(record.trace_id)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// The invocation row an `Idempotency-Key` already produced, if any.
///
/// A replay writes no new `action_invocation` row, so an engine caller that must cite the
/// invocation (the copilot's proposal execution) asks for the original by its key.
pub async fn find_invocation_by_key(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
    idempotency_key: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM action_invocation
          WHERE tenant_id = $1 AND action = $2 AND idempotency_key = $3
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(action)
    .bind(idempotency_key)
    .fetch_optional(conn)
    .await
}

/// Release an in-flight idempotency claim after a failed invocation.
///
/// Only a row that is still IN FLIGHT is deleted (`status_code IS NULL`): a completed
/// response is never removed, so a failure in another request cannot erase a replayable
/// answer. This is what makes a timed-out call retryable — the claim committed before an
/// external call must not outlive the call that never produced a response.
pub async fn release_idempotency(
    conn: &mut PgConnection,
    tenant: TenantId,
    scope: &str,
    key: &str,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "DELETE FROM idempotency_key
          WHERE tenant_id = $1 AND scope = $2 AND key = $3 AND status_code IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(scope)
    .bind(key)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(rows)
}

// ── platform read models ─────────────────────────────────────────────────────────────────

/// The caller's tenant. `tenant` carries no `tenant_id` and no RLS; the id is the one the
/// session resolved from `membership`, so the filter is the scope.
pub async fn tenant_get(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<TenantRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, slug, name, status, settings, version FROM tenant WHERE id = $1")
        .bind(tenant.as_uuid())
        .fetch_optional(conn)
        .await
}

/// Replace the tenant's settings document. A no-op write returns the current row so an
/// idempotent replay has a stable representation.
pub async fn tenant_update_settings(
    conn: &mut PgConnection,
    tenant: TenantId,
    settings: &Value,
    expected_version: Option<i64>,
) -> Result<Result<TenantRow, VersionMismatch>, sqlx::Error> {
    if let Some(expected) = expected_version {
        let current: Option<(i64,)> = sqlx::query_as("SELECT version FROM tenant WHERE id = $1")
            .bind(tenant.as_uuid())
            .fetch_optional(&mut *conn)
            .await?;
        let Some((version,)) = current else {
            return Ok(Err(VersionMismatch::NotFound));
        };
        if version != expected {
            return Ok(Err(VersionMismatch::Mismatch { current: version }));
        }
    }
    let row: Option<TenantRow> = sqlx::query_as(
        "UPDATE tenant
            SET settings = $2, version = version + 1, updated_at = now()
          WHERE id = $1 AND settings IS DISTINCT FROM $2
          RETURNING id, slug, name, status, settings, version",
    )
    .bind(tenant.as_uuid())
    .bind(settings)
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some(row) => Ok(Ok(row)),
        None => Ok(tenant_get(conn, tenant)
            .await?
            .map(Ok)
            .unwrap_or(Err(VersionMismatch::NotFound))),
    }
}

/// Why a version-guarded write did not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionMismatch {
    /// The row does not exist in this tenant.
    NotFound,
    /// The row moved; the caller's precondition is stale.
    Mismatch {
        /// The version the row holds now.
        current: i64,
    },
}

/// One page of members, ordered by `user_id` (stable, and the keyset).
pub async fn members_list(
    conn: &mut PgConnection,
    limit: i64,
    after: Option<Uuid>,
) -> Result<Vec<MemberRow>, sqlx::Error> {
    let sql = "SELECT m.user_id, u.display_name, u.email, m.role, m.department_id,
                      m.version, m.created_at
                 FROM membership m
                 JOIN app_user u ON u.id = m.user_id
                WHERE ($1::uuid IS NULL OR m.user_id > $1)
                ORDER BY m.user_id
                LIMIT $2";
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(after)
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// One member of this tenant. A user id from another tenant resolves to `None` because
/// row-level security confines the SELECT.
pub async fn member_get(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<MemberRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.user_id, u.display_name, u.email, m.role, m.department_id, m.version,
                m.created_at
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// `app_user.kind` — `human | agent | service`, or `None` when the id names no user.
///
/// `app_user` is the one global table (no RLS). Its only caller is `members.set_role`'s
/// `kata_executor` guard: the synthetic tenant-admin role is admin-equivalent and must
/// never be acquired by a human through a role change.
pub async fn app_user_kind(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT kind FROM app_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// Set a member's role, guarded by the membership version when the caller supplied one.
pub async fn member_set_role(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    role: &str,
    expected_version: Option<i64>,
) -> Result<Result<MemberRow, VersionMismatch>, ActionParentError> {
    // The user must exist (global table) AND be a member of THIS tenant. The membership
    // exists-check is the tenancy boundary a single-column FK to app_user cannot express.
    ensure_parent_tx(&mut *conn, tenant, Parent::AppUser, user_id).await?;
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM membership WHERE tenant_id = $1 AND user_id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    if !member {
        return Err(ActionParentError::NotFound("member"));
    }
    let Some(current) = member_get(&mut *conn, user_id).await? else {
        return Ok(Err(VersionMismatch::NotFound));
    };
    if let Some(expected) = expected_version
        && current.version != expected
    {
        return Ok(Err(VersionMismatch::Mismatch {
            current: current.version,
        }));
    }
    if current.role == role {
        // Identity write: no version bump, so two identical calls return the same view.
        return Ok(Ok(current));
    }
    let row: Option<MemberRow> = sqlx::query_as(
        "WITH updated AS (
             UPDATE membership
                SET role = $2, version = version + 1, updated_at = now()
              WHERE tenant_id = $3 AND user_id = $1
              RETURNING user_id, role, department_id, version, created_at
         )
         SELECT u.user_id, a.display_name, a.email, u.role, u.department_id, u.version,
                u.created_at
           FROM updated u JOIN app_user a ON a.id = u.user_id",
    )
    .bind(user_id)
    .bind(role)
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.ok_or(VersionMismatch::NotFound))
}

/// Every department of the tenant, ordered by name.
pub async fn departments_list(conn: &mut PgConnection) -> Result<Vec<DepartmentRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, name, parent_id, industry_id, ordinal, is_primary, version, created_at
           FROM department
          ORDER BY name, id",
    )
    .fetch_all(conn)
    .await
}

/// Insert a department, resolving `parent_id` and `industry_id` in this tenant first.
///
/// The checks are here, not in the handler: a caller that forgot them would hand a foreign id
/// to the FKs and the RI check (which bypasses RLS) would accept it. `None` is valid for both.
pub async fn department_create(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewDepartment<'_>,
) -> Result<DepartmentRow, ActionParentError> {
    if let Some(parent) = input.parent_id {
        // `Parent` has no `Department` variant (a department is its own parent table), so
        // this is the same tenant-scoped EXISTS the helper performs.
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM department WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(tenant.as_uuid())
        .bind(parent)
        .fetch_one(&mut *conn)
        .await?;
        if !present {
            return Err(ActionParentError::NotFound("department"));
        }
    }
    if let Some(industry) = input.industry_id {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM industry WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(tenant.as_uuid())
        .bind(industry)
        .fetch_one(&mut *conn)
        .await?;
        if !present {
            return Err(ActionParentError::NotFound("industry"));
        }
    }
    let row: DepartmentRow = sqlx::query_as(
        "INSERT INTO department (tenant_id, name, parent_id, industry_id, ordinal, is_primary)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, name, parent_id, industry_id, ordinal, is_primary, version, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(input.name)
    .bind(input.parent_id)
    .bind(input.industry_id)
    .bind(input.ordinal)
    .bind(input.is_primary)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// The principal and its memberships **inside one tenant scope** (`me.get`).
///
/// `app_user` has no RLS; `membership` is FORCE-RLS'd, so the join returns the caller's
/// membership in this tenant only. `identity::resolve_identity` cannot be reused here: it
/// opens its own transaction with `app.current_user`, not the invocation's tenant scope.
pub async fn identity_view(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<(String, String, String, Option<String>)>, sqlx::Error> {
    let user = sqlx::query_as::<_, (Uuid, String, String, Option<String>)>(
        "SELECT id, coalesce(oidc_subject, ''), display_name, email
           FROM app_user WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id, subject, display_name, email)) = user else {
        return Ok(None);
    };
    Ok(Some((id.to_string(), subject, display_name, email)))
}

/// One membership row of this tenant, joined to its tenant.
pub async fn membership_view(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<(Uuid, String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.tenant_id, t.slug, t.name, m.role
           FROM membership m JOIN tenant t ON t.id = m.tenant_id
          WHERE m.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

// ── audit / jobs / schedules / events / licence ─────────────────────────────────────────

/// One page of the audit log, newest first, keyset on `(created_at, id)`.
pub async fn audit_list(
    conn: &mut PgConnection,
    since: Option<DateTime<Utc>>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<AuditRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, actor_id, actor_kind, action, status, resource_type, resource_id,
                request_id, details, created_at
           FROM audit_event
          WHERE ($1::timestamptz IS NULL OR created_at >= $1)
            AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
          ORDER BY created_at DESC, id DESC
          LIMIT $4",
    )
    .bind(since)
    .bind(cursor.map(|c| c.0))
    .bind(cursor.map(|c| c.1))
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// One page of jobs, newest first.
pub async fn jobs_list(
    conn: &mut PgConnection,
    status: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<JobRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, kind, dedupe_key, status, payload, run_after, attempts, max_attempts,
                last_error, created_at, updated_at
           FROM job
          WHERE ($1::text IS NULL OR status = $1)
            AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
          ORDER BY created_at DESC, id DESC
          LIMIT $4",
    )
    .bind(status)
    .bind(cursor.map(|c| c.0))
    .bind(cursor.map(|c| c.1))
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// One job of this tenant (RLS makes a foreign id `None`).
pub async fn job_get(conn: &mut PgConnection, id: Uuid) -> Result<Option<JobRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, kind, dedupe_key, status, payload, run_after, attempts, max_attempts,
                last_error, created_at, updated_at
           FROM job WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Re-queue a `failed`/`dead` job. `None` means the state is not retryable.
pub async fn job_retry(conn: &mut PgConnection, id: Uuid) -> Result<Option<JobRow>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE job
            SET status = 'pending', attempts = 0, run_after = now(), last_error = NULL,
                lease_until = NULL, lease_token = NULL, updated_at = now()
          WHERE id = $1 AND status IN ('failed', 'dead')
          RETURNING id, kind, dedupe_key, status, payload, run_after, attempts,
                    max_attempts, last_error, created_at, updated_at",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Every schedule of the tenant.
pub async fn schedules_list(conn: &mut PgConnection) -> Result<Vec<ScheduleRowOut>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, kind, cron, timezone, enabled, next_run_at, last_run_at, version
           FROM schedule ORDER BY kind",
    )
    .fetch_all(conn)
    .await
}

/// Insert or replace the schedule for one kind. The caller computes `next_run_at` from the
/// same cron value it stores, so the row and its arithmetic cannot diverge.
pub async fn schedule_upsert(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    cron: &str,
    enabled: bool,
    next_run_at: DateTime<Utc>,
    payload: &Value,
) -> Result<ScheduleRowOut, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO schedule (tenant_id, kind, cron, enabled, next_run_at, payload)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, kind) DO UPDATE
             SET cron = excluded.cron,
                 enabled = excluded.enabled,
                 next_run_at = CASE WHEN schedule.cron = excluded.cron
                                    THEN schedule.next_run_at
                                    ELSE excluded.next_run_at END,
                 payload = excluded.payload,
                 version = schedule.version + 1,
                 updated_at = now()
         RETURNING id, kind, cron, timezone, enabled, next_run_at, last_run_at, version",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(cron)
    .bind(enabled)
    .bind(next_run_at)
    .bind(payload)
    .fetch_one(conn)
    .await
}

/// One page of events, newest `seq` first. `seq` is the only ordering authority.
pub async fn events_list(
    conn: &mut PgConnection,
    topics: Option<&[String]>,
    before_seq: Option<i64>,
    limit: i64,
) -> Result<Vec<EventRowOut>, sqlx::Error> {
    sqlx::query_as(
        "SELECT seq, id, topic, type, aggregate_type, aggregate_id, payload, created_at
           FROM event
          WHERE ($1::text[] IS NULL OR topic = ANY($1))
            AND ($2::bigint IS NULL OR seq < $2)
          ORDER BY seq DESC
          LIMIT $3",
    )
    .bind(topics)
    .bind(before_seq)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// The tenant's licence row for `app_key`.
pub async fn licence_get(
    conn: &mut PgConnection,
    app_key: &str,
) -> Result<Option<LicenceRowOut>, sqlx::Error> {
    sqlx::query_as(
        "SELECT app_key, status, expires_at, checked_at
           FROM app_licence WHERE app_key = $1",
    )
    .bind(app_key)
    .fetch_optional(conn)
    .await
}

// ── action_registry ─────────────────────────────────────────────────────────────────────

/// The highest **approved** spec version per name, for the tenant.
pub async fn approved_registry(conn: &mut PgConnection) -> Result<Vec<RegistryRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT DISTINCT ON (name)
                id, name, spec_version, kind, service, spec, schema_hash, timeout_ms,
                idempotent, approved_at, approved_by
           FROM action_registry
          WHERE approved_at IS NOT NULL
          ORDER BY name, spec_version DESC",
    )
    .fetch_all(conn)
    .await
}

/// Every row for one name, newest `spec_version` first.
pub async fn registry_versions(
    conn: &mut PgConnection,
    name: &str,
) -> Result<Vec<RegistryRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, name, spec_version, kind, service, spec, schema_hash, timeout_ms,
                idempotent, approved_at, approved_by
           FROM action_registry
          WHERE name = $1
          ORDER BY spec_version DESC",
    )
    .bind(name)
    .fetch_all(conn)
    .await
}

/// Write an **unapproved** spec version (an ingest never changes what serves).
pub async fn registry_insert_pending(
    conn: &mut PgConnection,
    tenant: TenantId,
    row: &PendingRegistryRow<'_>,
) -> Result<Option<RegistryRow>, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO action_registry
             (tenant_id, name, spec_version, kind, service, spec, schema_hash, timeout_ms,
              idempotent)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (tenant_id, name, spec_version) DO NOTHING
         RETURNING id, name, spec_version, kind, service, spec, schema_hash, timeout_ms,
                   idempotent, approved_at, approved_by",
    )
    .bind(tenant.as_uuid())
    .bind(row.name)
    .bind(row.spec_version)
    .bind(row.kind)
    .bind(row.service)
    .bind(&row.spec)
    .bind(row.schema_hash)
    .bind(row.timeout_ms)
    .bind(row.idempotent)
    .fetch_optional(conn)
    .await
}

/// A pending row to insert.
#[derive(Clone, Debug)]
pub struct PendingRegistryRow<'a> {
    pub name: &'a str,
    pub spec_version: i32,
    pub kind: &'a str,
    pub service: &'a str,
    pub spec: Value,
    pub schema_hash: &'a str,
    pub timeout_ms: i32,
    pub idempotent: bool,
}

/// Approve one `(name, spec_version)`; enable/disable is `approved_at` (S4-D3).
pub async fn registry_approve(
    conn: &mut PgConnection,
    name: &str,
    spec_version: i32,
    approved_by: Option<Uuid>,
) -> Result<Option<RegistryRow>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE action_registry
            SET approved_at = now(), approved_by = $3, updated_at = now()
          WHERE name = $1 AND spec_version = $2
          RETURNING id, name, spec_version, kind, service, spec, schema_hash, timeout_ms,
                    idempotent, approved_at, approved_by",
    )
    .bind(name)
    .bind(spec_version)
    .bind(approved_by)
    .fetch_optional(conn)
    .await
}

/// Withdraw approval from every other version of `name`, so exactly one serves.
pub async fn registry_revoke_others(
    conn: &mut PgConnection,
    name: &str,
    keep: i32,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE action_registry
            SET approved_at = NULL, approved_by = NULL, updated_at = now()
          WHERE name = $1 AND spec_version <> $2 AND approved_at IS NOT NULL",
    )
    .bind(name)
    .bind(keep)
    .execute(conn)
    .await?
    .rows_affected();
    Ok(rows)
}

// ── cursors ─────────────────────────────────────────────────────────────────────────────

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

/// An opaque `(sort key, id)` cursor (CONTRACT §5: never documented as stable).
pub fn encode_cursor(sort_key: &str, id: Uuid) -> String {
    b64().encode(format!("{sort_key}|{id}"))
}

/// Decode a cursor; a malformed one is a `422` at the edge, never a silent first page.
pub fn decode_cursor(cursor: &str) -> Option<(String, Uuid)> {
    let bytes = b64().decode(cursor).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (key, id) = text.rsplit_once('|')?;
    Some((key.to_owned(), Uuid::parse_str(id).ok()?))
}

/// A cursor over `(timestamptz, uuid)`.
pub fn encode_time_cursor(at: DateTime<Utc>, id: Uuid) -> String {
    encode_cursor(&at.to_rfc3339(), id)
}

/// Decode a `(timestamptz, uuid)` cursor.
pub fn decode_time_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let (key, id) = decode_cursor(cursor)?;
    Some((
        DateTime::parse_from_rfc3339(&key).ok()?.with_timezone(&Utc),
        id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_and_refuse_junk() {
        let id = Uuid::now_v7();
        let at = DateTime::parse_from_rfc3339("2026-11-08T01:02:03Z")
            .unwrap()
            .with_timezone(&Utc);
        let cursor = encode_time_cursor(at, id);
        assert_eq!(decode_time_cursor(&cursor), Some((at, id)));
        assert!(decode_cursor("not base64!").is_none());
        assert!(decode_cursor("bm90LWEtY3Vyc29y").is_none());
    }

    #[test]
    fn the_page_size_is_clamped() {
        assert_eq!(clamp_limit(None), 50);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(10)), 10);
        assert_eq!(clamp_limit(Some(1000)), 200);
    }
}
