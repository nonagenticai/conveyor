#![forbid(unsafe_code)]
//! Feedback about the platform, submitted from inside a tenant's UI (issue #842).
//!
//! One tenant-scoped row per submission. The tenant's own page lists its rows; the platform
//! operator's cross-tenant read is the admin CLI, which walks tenants and scopes each one in
//! turn — the same [`crate::begin_scoped`] path `seed_programme(pool, None)` uses. There is
//! deliberately no `Tenancy::Platform` action: the agent surface refuses that tenancy for a
//! tenant-bound caller, and the operator's authority is the global `platform_admin` grant, which
//! gates the CLI and the super-admin routes, not the tenant action route.
//!
//! Every function takes the caller's already-scoped transaction (`_tx`, CONTRACT §7) unless it is
//! [`list_all_tx`], which opens one scoped transaction per tenant.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// The kind vocabulary, in CHECK order.
pub const KINDS: [&str; 3] = ["bug", "feature", "other"];

/// The status vocabulary, in CHECK order. Only the admin triage moves a row off `open`.
pub const STATUSES: [&str; 5] = ["open", "triaged", "accepted", "declined", "duplicate"];

/// The columns every read returns, in one spelling.
const COLUMNS: &str = "id, author_id, kind, title, body, route, status, triaged_by, triaged_at, \
                       note, version, created_at, updated_at";

/// One `feedback` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct FeedbackRow {
    /// The row id.
    pub id: Uuid,
    /// The `app_user` who submitted it.
    pub author_id: Uuid,
    /// `bug | feature | other`.
    pub kind: String,
    /// The one-line summary.
    pub title: String,
    /// The report itself.
    pub body: String,
    /// The app route the submission came from, when the client sent one.
    pub route: Option<String>,
    /// `open | triaged | accepted | declined | duplicate`.
    pub status: String,
    /// The operator who triaged it; NULL while `open`.
    pub triaged_by: Option<Uuid>,
    /// When the operator triaged it; NULL while `open`.
    pub triaged_at: Option<DateTime<Utc>>,
    /// The operator's note, when one was recorded.
    pub note: Option<String>,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// The fields a create supplies. The author is the calling session's actor.
#[derive(Clone, Debug)]
pub struct NewFeedback<'a> {
    /// The submitting `app_user`.
    pub author_id: Uuid,
    /// `bug | feature | other`.
    pub kind: &'a str,
    /// The one-line summary.
    pub title: &'a str,
    /// The report itself.
    pub body: &'a str,
    /// The app route the submission came from, when the client sent one.
    pub route: Option<&'a str>,
}

/// Insert one row in the caller's tenant-scoped transaction.
pub async fn insert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewFeedback<'_>,
) -> Result<FeedbackRow, sqlx::Error> {
    let sql = format!(
        "INSERT INTO feedback (tenant_id, author_id, kind, title, body, route)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.author_id)
        .bind(new.kind)
        .bind(new.title)
        .bind(new.body)
        .bind(new.route)
        .fetch_one(&mut *conn)
        .await
}

/// One row of the caller's tenant, or `None` when it names no row here.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM feedback WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// The caller's tenant's rows, newest first. `status` filters when given.
pub async fn list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<FeedbackRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM feedback
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
          ORDER BY created_at DESC
          LIMIT $3"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await
}

/// Move one row's status in the caller's tenant-scoped transaction.
///
/// The operator who triaged it is recorded with the time; the row's version bumps. `None` means
/// the id names no row in this tenant — the caller's 404, never a silent no-op.
pub async fn triage_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    status: &str,
    note: Option<&str>,
    by: Uuid,
) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let sql = format!(
        "UPDATE feedback
            SET status = $3, note = $4, triaged_by = $5, triaged_at = now(),
                updated_at = now(), version = version + 1
          WHERE tenant_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(status)
        .bind(note)
        .bind(by)
        .fetch_optional(&mut *conn)
        .await
}

/// Every tenant's rows, newest first, as `(tenant slug, row)`.
///
/// The operator's read. It resolves the tenant list on the platform pool and opens one
/// tenant-scoped transaction per tenant, so RLS stays on and no query crosses a tenant boundary:
/// this is the `seed_programme(pool, None)` shape. `tenant_slug` and `status` filter when given.
pub async fn list_all_tx(
    pool: &PgPool,
    tenant_slug: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<(String, FeedbackRow)>, sqlx::Error> {
    let tenants: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, slug FROM tenant
          WHERE ($1::text IS NULL OR slug = $1)
          ORDER BY slug",
    )
    .bind(tenant_slug)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (id, slug) in tenants {
        let tenant =
            TenantId::from_uuid(id).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        let mut tx = crate::begin_scoped(pool, tenant).await?;
        let rows = list_tx(&mut tx, tenant, status, limit).await?;
        tx.commit().await?;
        out.extend(rows.into_iter().map(|row| (slug.clone(), row)));
    }
    out.sort_by_key(|(_, row)| std::cmp::Reverse(row.created_at));
    Ok(out)
}
