//! The tenant role policy, platform admins, and the membership guards.
//!
//! Three facts live here, each with one reader:
//!
//! - **`tenant_role_scope`** — the tenant's overrides of the compiled role→scope table. Absent
//!   row = compiled default, so an untouched tenant stores nothing. [`policy_for`] is read once
//!   per request by `conveyor_auth::authn`, and [`set_policy_tx`] is written by `roles.policy_set`.
//! - **`platform_admin`** — the one global grant. [`is_platform_admin`] is read once per request.
//!   The table carries no `tenant_id` and no RLS: it is a platform fact, not tenant data.
//! - **Membership guards** — [`member_set_role_tx`] and [`member_remove_tx`] refuse to leave a
//!   tenant with no human admin. The check and the write share one transaction and one
//!   per-tenant advisory lock, so two admins demoting each other concurrently cannot both pass
//!   the check and leave zero.
//!
//! `kata_executor` is admin-equivalent but is a machine role: the guard counts HUMAN admins only,
//! because the grant exists to keep a person able to reach the screen that restores the tenant.

use std::collections::BTreeMap;

use conveyor_domain::TenantId;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::actions::{MemberRow, member_get};
use crate::scope::begin_scoped;

/// A membership guard refused the write.
#[derive(Debug, thiserror::Error)]
pub enum MemberStoreError {
    /// The id names no member of this tenant (RLS made it invisible).
    #[error("the member does not exist in this tenant")]
    NotFound,
    /// The `If-Match` version is stale.
    #[error("the membership moved to version {current}")]
    VersionMismatch {
        /// The version the row holds now.
        current: i64,
    },
    /// The write would leave the tenant with no human admin.
    #[error("this is the tenant's last admin; designate another admin first")]
    LastAdmin,
    /// The database refused the read or the write.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The tenant's stored policy for one role: `scope -> allowed`.
///
/// Runs in its own tenant-scoped transaction; the caller (`authn`) is not yet in a request that
/// has one, but the tenant and the selected role are already known.
pub async fn policy_for(
    db: &PgPool,
    tenant: TenantId,
    role: &str,
) -> Result<BTreeMap<String, bool>, sqlx::Error> {
    let mut tx = begin_scoped(db, tenant).await?;
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT scope, allowed FROM tenant_role_scope WHERE tenant_id = $1 AND role = $2",
    )
    .bind(tenant.as_uuid())
    .bind(role)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().collect())
}

/// The tenant's stored grant overrides for one connector kind: `action -> allowed`.
///
/// Absent row = the kind's compiled default (`actions::channel::default_actions`). Read once
/// per request for a machine credential, or per inbound message for a binding.
pub async fn channel_policy_for(
    db: &PgPool,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<BTreeMap<String, bool>, sqlx::Error> {
    let mut tx = begin_scoped(db, tenant).await?;
    let map = channel_policy_tx(&mut tx, tenant, channel_kind).await?;
    tx.commit().await?;
    Ok(map)
}

/// The same read inside the caller's transaction (the admin matrix already holds one).
pub async fn channel_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<BTreeMap<String, bool>, sqlx::Error> {
    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT action, allowed FROM channel_action_grant
          WHERE tenant_id = $1 AND channel_kind = $2",
    )
    .bind(tenant.as_uuid())
    .bind(channel_kind)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Replace one connector kind's grant overrides with `entries` (`(action, allowed)`).
///
/// Delete-then-insert in the caller's transaction, exactly like [`set_policy_tx`]: the stored
/// set is what the admin submitted, and an omitted action falls back to the compiled default.
pub async fn set_channel_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
    entries: &[(&str, bool)],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM channel_action_grant WHERE tenant_id = $1 AND channel_kind = $2")
        .bind(tenant.as_uuid())
        .bind(channel_kind)
        .execute(&mut *conn)
        .await?;
    for (action, allowed) in entries {
        sqlx::query(
            "INSERT INTO channel_action_grant (tenant_id, channel_kind, action, allowed)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(tenant.as_uuid())
        .bind(channel_kind)
        .bind(*action)
        .bind(*allowed)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Delete every stored override for one connector kind; the compiled defaults apply again.
pub async fn reset_channel_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM channel_action_grant WHERE tenant_id = $1 AND channel_kind = $2")
            .bind(tenant.as_uuid())
            .bind(channel_kind)
            .execute(&mut *conn)
            .await?;
    Ok(result.rows_affected())
}

/// Replace the tenant's policy with `entries` (`(role, scope, allowed)`; an empty slice clears
/// it).
///
/// Delete-then-insert in the caller's transaction: the stored set is exactly what the admin
/// submitted, so a scope removed from the request is removed from the table. The caller has
/// already validated the role and scope vocabularies and refused the kernel scopes.
pub async fn set_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entries: &[(&str, &str, bool)],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM tenant_role_scope WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .execute(&mut *conn)
        .await?;
    for (role, scope, allowed) in entries {
        sqlx::query(
            "INSERT INTO tenant_role_scope (tenant_id, role, scope, allowed)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(tenant.as_uuid())
        .bind(role)
        .bind(scope)
        .bind(allowed)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Delete every stored override; the compiled table applies again.
pub async fn reset_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query("DELETE FROM tenant_role_scope WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected())
}

/// `app_user.id` holds the one global grant.
pub async fn is_platform_admin(db: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM platform_admin WHERE user_id = $1)")
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// Grant the platform role. Idempotent: granting twice is one row.
pub async fn grant_platform_admin(
    db: &PgPool,
    user_id: Uuid,
    granted_by: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO platform_admin (user_id, granted_by) VALUES ($1, $2)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(granted_by)
    .execute(db)
    .await?;
    Ok(())
}

/// Revoke the platform role. Returns whether a row was removed.
pub async fn revoke_platform_admin(db: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM platform_admin WHERE user_id = $1")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// The number of HUMAN admins in this tenant (in the caller's scope).
async fn human_admin_count_tx(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*)
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.role = 'admin' AND u.kind = 'human' AND u.status = 'active'",
    )
    .fetch_one(&mut *conn)
    .await
}

/// Serialize admin-affecting membership writes for one tenant.
///
/// A count followed by an update is not atomic under READ COMMITTED: two admins demoting each
/// other would both read `2` and both proceed. The transaction-scoped advisory lock makes the
/// pair serial, and it dies at COMMIT with the transaction.
async fn lock_membership(conn: &mut PgConnection, tenant: TenantId) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("membership:{}", tenant.as_uuid()))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// True when changing `current_role` to `next_role` would remove the last human admin.
fn would_orphan(current_role: &str, next_role: &str, admins: i64) -> bool {
    current_role == "admin" && next_role != "admin" && admins <= 1
}

/// Set a member's role, refusing to remove the last human admin.
///
/// The pre-checks (member exists, version matches, role actually changes) are the same ones
/// `actions::member_set_role` made; the difference is the guard and its lock.
pub async fn member_set_role_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    role: &str,
    expected_version: Option<i64>,
) -> Result<MemberRow, MemberStoreError> {
    lock_membership(&mut *conn, tenant).await?;
    let Some(current) = member_get(&mut *conn, user_id).await? else {
        return Err(MemberStoreError::NotFound);
    };
    if let Some(expected) = expected_version
        && current.version != expected
    {
        return Err(MemberStoreError::VersionMismatch {
            current: current.version,
        });
    }
    if current.role == role {
        // Identity write: no version bump, so two identical calls return the same view.
        return Ok(current);
    }
    if would_orphan(&current.role, role, human_admin_count_tx(&mut *conn).await?) {
        return Err(MemberStoreError::LastAdmin);
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
    row.ok_or(MemberStoreError::NotFound)
}

/// Remove a member, refusing to remove the last human admin.
pub async fn member_remove_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
) -> Result<(), MemberStoreError> {
    lock_membership(&mut *conn, tenant).await?;
    let Some(current) = member_get(&mut *conn, user_id).await? else {
        return Err(MemberStoreError::NotFound);
    };
    if would_orphan(
        &current.role,
        "removed",
        human_admin_count_tx(&mut *conn).await?,
    ) {
        return Err(MemberStoreError::LastAdmin);
    }
    sqlx::query("DELETE FROM membership WHERE tenant_id = $1 AND user_id = $2")
        .bind(tenant.as_uuid())
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_admin_guard_names_the_two_promotion_directions() {
        assert!(would_orphan("admin", "member", 1));
        assert!(would_orphan("admin", "viewer", 1));
        assert!(would_orphan("admin", "removed", 1));
        assert!(!would_orphan("admin", "member", 2));
        assert!(!would_orphan("admin", "admin", 1));
        assert!(!would_orphan("member", "viewer", 1));
        assert!(!would_orphan("kata_executor", "member", 1));
    }
}
