#![forbid(unsafe_code)]
//! `conveyor admin feedback` — the platform operator's cross-tenant feedback surface (#842).
//!
//! `list` walks the tenants and opens one tenant-scoped transaction per tenant (the
//! `seed_programme(pool, None)` shape), so RLS stays on and no query crosses a tenant boundary.
//! `triage` resolves the tenant slug, moves one row's status, and records who decided.
//!
//! The operator is a `platform_admin` (the one global grant). There is no `Tenancy::Platform`
//! action for this: the agent surface refuses that tenancy for a tenant-bound caller, and the
//! operator's authority gates the CLI and the super-admin routes, not the tenant action route.

use conveyor_store as store;
use conveyor_store::feedback::{self, FeedbackRow, STATUSES};
use sqlx::PgPool;
use uuid::Uuid;

/// One listed row: the tenant slug, the row, and the author's email when one is recorded.
pub struct Listed {
    /// The tenant slug.
    pub slug: String,
    /// The row.
    pub row: FeedbackRow,
    /// The author's email (the operator reads people, not uuids).
    pub author_email: Option<String>,
}

/// Every tenant's rows, newest first. `tenant` and `status` filter when given.
pub async fn list(
    pool: &PgPool,
    tenant: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<Listed>, String> {
    if let Some(status) = status
        && !STATUSES.contains(&status)
    {
        return Err(format!(
            "unknown status {status:?}; expected one of {}",
            STATUSES.join(", ")
        ));
    }
    let limit = limit.clamp(1, 200);
    let rows = feedback::list_all_tx(pool, tenant, status, limit)
        .await
        .map_err(|error| format!("listing feedback: {error}"))?;
    let ids: Vec<Uuid> = rows.iter().map(|(_, row)| row.author_id).collect();
    let emails: Vec<(Uuid, String)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id, email FROM app_user WHERE id = ANY($1)")
            .bind(&ids)
            .fetch_all(pool)
            .await
            .map_err(|error| format!("reading authors: {error}"))?
    };
    let by_id: std::collections::HashMap<Uuid, String> = emails.into_iter().collect();
    Ok(rows
        .into_iter()
        .map(|(slug, row)| Listed {
            slug,
            author_email: by_id.get(&row.author_id).cloned(),
            row,
        })
        .collect())
}

/// Move one row's status and record the operator.
///
/// `status` may not be `open` (reopening drops the triage evidence, and the schema's shape check
/// ties `open` to no triage); `by` is the operator's email, resolved to the `app_user` row the
/// decision is recorded against.
pub async fn triage(
    pool: &PgPool,
    tenant: &str,
    id: Uuid,
    status: &str,
    note: Option<&str>,
    by: &str,
) -> Result<FeedbackRow, String> {
    if !STATUSES.contains(&status) || status == "open" {
        return Err(format!(
            "status must be one of {}",
            STATUSES[1..].join(", ")
        ));
    }
    let tenant_id = store::tenants::id_by_slug(pool, tenant)
        .await
        .map_err(|error| format!("resolving tenant {tenant:?}: {error}"))?
        .ok_or_else(|| format!("no tenant {tenant:?}"))?;
    let by_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM app_user WHERE lower(email) = lower($1)")
            .bind(by)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("resolving {by:?}: {error}"))?;
    let by_id = by_id.ok_or_else(|| format!("no app_user with email {by:?}"))?;
    let mut tx = store::begin_scoped(pool, tenant_id)
        .await
        .map_err(|error| format!("scoping {tenant:?}: {error}"))?;
    let row = feedback::triage_tx(&mut tx, tenant_id, id, status, note, by_id)
        .await
        .map_err(|error| format!("triaging: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("committing: {error}"))?;
    row.ok_or_else(|| format!("no feedback {id} in {tenant}"))
}
