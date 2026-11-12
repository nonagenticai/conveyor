//! Invites: the only sign-up path.
//!
//! An invite names an email, a tenant and a role. A person with no `app_user` row is admitted
//! only while a PENDING invite names their verified email; accepting it creates the `app_user`
//! row and the membership, and marks the invite accepted in the same transaction.
//!
//! Two reads, deliberately different:
//!
//! - [`list_tx`] is the tenant's admin view, confined by `tenant_isolation` like every other
//!   tenant-owned row.
//! - [`pending_for_email`] runs BEFORE a tenant is known. It sets `app.current_email`
//!   transaction-locally and reads under `invite_self_read`, which exposes exactly that email's
//!   pending invites and nothing else.
//!
//! Emails are compared with `lower()` on both sides: the admin's casing and the identity
//! provider's casing are not required to agree.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::{begin_scoped, set_guc};

/// One `invite` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct InviteRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub role: String,
    pub status: String,
    pub invited_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub accepted_user_id: Option<Uuid>,
    pub version: i64,
}

/// A pending invite as login sees it: enough to create the membership and show where it lands.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PendingInvite {
    pub invite_id: Uuid,
    pub tenant_id: Uuid,
    pub role: String,
    pub tenant_slug: String,
    pub tenant_name: String,
}

/// Create (or re-issue) the pending invite for `email` in this tenant.
///
/// Re-inviting is the same invitation: the unique partial index on `(tenant_id, lower(email))
/// WHERE status = 'pending'` makes the second call an update of the role/inviter rather than a
/// second row.
pub async fn create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    email: &str,
    role: &str,
    invited_by: Option<Uuid>,
) -> Result<InviteRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO invite (tenant_id, email, role, invited_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, lower(email)) WHERE status = 'pending'
         DO UPDATE SET role = EXCLUDED.role,
                       invited_by = EXCLUDED.invited_by,
                       version = invite.version + 1
         RETURNING id, tenant_id, email, role, status, invited_by, created_at, accepted_at,
                   accepted_user_id, version",
    )
    .bind(tenant.as_uuid())
    .bind(email)
    .bind(role)
    .bind(invited_by)
    .fetch_one(&mut *conn)
    .await
}

/// Every invite of this tenant, pending first.
pub async fn list_tx(conn: &mut PgConnection) -> Result<Vec<InviteRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, tenant_id, email, role, status, invited_by, created_at, accepted_at,
                accepted_user_id, version
           FROM invite
          ORDER BY status = 'pending' DESC, created_at DESC, id",
    )
    .fetch_all(&mut *conn)
    .await
}

/// Revoke a pending invite. `false` means there was no pending invite with that id.
pub async fn revoke_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    invite_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE invite
            SET status = 'revoked', version = version + 1
          WHERE id = $2 AND tenant_id = $1 AND status = 'pending'",
    )
    .bind(tenant.as_uuid())
    .bind(invite_id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The pending invites naming `email`, across every tenant — login's read before a tenant exists.
///
/// `app.current_email` is set transaction-locally; `invite_self_read` is the only policy that
/// lets this query see a row, and it sees only pending invites for that exact address.
pub async fn pending_for_email(
    db: &PgPool,
    email: &str,
) -> Result<Vec<PendingInvite>, sqlx::Error> {
    let mut tx = db.begin().await?;
    set_guc(&mut tx, "app.current_email", &email.to_ascii_lowercase()).await?;
    let rows = sqlx::query_as(
        "SELECT i.id AS invite_id, i.tenant_id, i.role, t.slug AS tenant_slug,
                t.name AS tenant_name
           FROM invite i
           JOIN tenant t ON t.id = i.tenant_id
          WHERE i.status = 'pending' AND lower(i.email) = lower($1)
          ORDER BY t.slug, i.id",
    )
    .bind(email)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

/// Accept one pending invite for `user_id`; returns the role it granted.
///
/// Runs in a transaction already scoped to the invite's tenant, so `tenant_isolation` admits the
/// UPDATE. `None` means the invite was revoked or already accepted — login then simply has no new
/// membership, which is not an error.
pub async fn accept_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    invite_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE invite
            SET status = 'accepted', accepted_at = now(), accepted_user_id = $3,
                version = version + 1
          WHERE id = $2 AND tenant_id = $1 AND status = 'pending'
          RETURNING role",
    )
    .bind(tenant.as_uuid())
    .bind(invite_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await
}

/// Add (or re-role) the membership an accepted invite names.
///
/// `DO UPDATE` rather than `DO NOTHING`: a person invited back into a tenant they already belong
/// to should get the invited role, which is the admin's decision.
pub async fn grant_membership_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id)
         DO UPDATE SET role = EXCLUDED.role, version = membership.version + 1,
                       updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .bind(role)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Accept every pending invite naming this user's email and return the tenants joined.
///
/// One transaction per tenant: `begin_scoped` is per-tenant, and a failure in one tenant must not
/// roll back the others. The caller has already created the `app_user` row.
pub async fn accept_all_for(
    db: &PgPool,
    user_id: Uuid,
    email: &str,
) -> Result<Vec<TenantId>, sqlx::Error> {
    let pending = pending_for_email(db, email).await?;
    let mut joined = Vec::new();
    for invite in pending {
        let Ok(tenant) = TenantId::from_uuid(invite.tenant_id) else {
            continue;
        };
        let mut tx = begin_scoped(db, tenant).await?;
        let Some(role) = accept_tx(&mut tx, tenant, invite.invite_id, user_id).await? else {
            tx.commit().await?;
            continue;
        };
        grant_membership_tx(&mut tx, tenant, user_id, &role).await?;
        tx.commit().await?;
        joined.push(tenant);
    }
    Ok(joined)
}
