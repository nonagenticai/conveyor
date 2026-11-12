//! Login identity resolution: subject -> the ONE membership the caller acts in.
//!
//! CONTRACT §9: the tenant is derived from `membership`, never from a client-supplied
//! header. `app_user` is reachable without a tenant (it carries no `tenant_id` and no RLS);
//! `membership` is FORCE-RLS'd, so the lookup sets `app.current_user` transaction-locally
//! and the migration's `membership_self_read` policy exposes exactly that user's own rows.
//! Nothing else about another tenant is readable in that transaction — the policy is
//! SELECT-only, scoped to `conveyor_runtime`, and never applies to the owner.

use conveyor_domain::{Id, IdError, TenantId};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// One membership of the resolved user.
#[derive(Clone, Debug)]
pub struct MembershipRow {
    pub tenant_id: TenantId,
    pub tenant_slug: String,
    pub tenant_name: String,
    pub tenant_status: String,
    pub role: String,
}

/// A resolved principal and every membership its subject has.
#[derive(Clone, Debug)]
pub struct Identity {
    pub user_id: Id,
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
    /// `app_user.kind` — `human | agent | service` (007's CHECK). A machine credential must
    /// never resolve to a human, so the caller compares this, not the credential's word.
    pub kind: String,
    pub memberships: Vec<MembershipRow>,
}

fn decode(e: IdError) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(e))
}

/// The active `app_user` for an OIDC subject and its memberships, ordered by tenant slug.
///
/// `Ok(None)` means no such user. A user with zero or several memberships is returned as
/// found: the caller refuses it (401), because S02 has no tenant-switch path.
pub async fn resolve_identity(db: &PgPool, subject: &str) -> Result<Option<Identity>, sqlx::Error> {
    let user: Option<(uuid::Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT id, oidc_subject, display_name, coalesce(email, ''), kind
           FROM app_user
          WHERE oidc_subject = $1 AND status = 'active'",
    )
    .bind(subject)
    .fetch_optional(db)
    .await?;
    let Some((user_id, oidc_subject, display_name, email, kind)) = user else {
        return Ok(None);
    };
    let user_id = Id::from_uuid(user_id).map_err(decode)?;

    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.current_user', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    let rows: Vec<(uuid::Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT m.tenant_id, t.slug, t.name, t.status, m.role
           FROM membership m
           JOIN tenant t ON t.id = m.tenant_id
          WHERE m.user_id = app_current_user()
          ORDER BY t.slug, t.id",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let memberships = rows
        .into_iter()
        .map(|(tenant_id, slug, name, status, role)| {
            Ok(MembershipRow {
                tenant_id: TenantId::from_uuid(tenant_id).map_err(decode)?,
                tenant_slug: slug,
                tenant_name: name,
                tenant_status: status,
                role,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    Ok(Some(Identity {
        user_id,
        subject: oidc_subject,
        display_name,
        email: (!email.is_empty()).then_some(email),
        kind,
        memberships,
    }))
}

/// The display name of one `app_user`, for a document that names its actor.
///
/// `app_user` carries no `tenant_id` and no RLS (data-architecture §2.1), so this is a
/// plain read; it is a MISSING name, never an authorization decision.
pub async fn display_name_tx(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT display_name FROM app_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
}

/// The user an administrative CLI acts as inside one tenant.
///
/// The import path drives the REAL actions, so it needs an actor id and a tenant role. It
/// acts as the tenant's first active admin (falling back to the first active member) —
/// never as a fabricated user, and never above the tenant. `None` means the tenant has no
/// member to act as, which the caller reports rather than inventing one.
pub async fn tenant_actor(pool: &PgPool, tenant: TenantId) -> Result<Option<Uuid>, sqlx::Error> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT m.user_id
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND u.status = 'active'
          ORDER BY (m.role = 'admin') DESC, m.created_at, m.id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// The active `app_user` whose email is `email`, with its memberships.
///
/// This is the email-keyed half of login (owner ruling 2026-11-09): a Keycloak account may be
/// deleted and a new one created, and the new subject must still be the same person. The lookup
/// is case-insensitive and returns the SAME [`Identity`] shape as [`resolve_identity`], so the
/// caller can rebind the subject and carry on.
///
/// `Ok(None)` means no active user holds that email. It is not an authorization decision on its
/// own — the caller still requires a verified email and, failing a row, a pending invite.
pub async fn resolve_identity_by_email(
    db: &PgPool,
    email: &str,
) -> Result<Option<Identity>, sqlx::Error> {
    let user: Option<(uuid::Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT id, coalesce(oidc_subject, ''), display_name, coalesce(email, ''), kind
           FROM app_user
          WHERE lower(email) = lower($1) AND status = 'active'
          ORDER BY created_at, id
          LIMIT 1",
    )
    .bind(email)
    .fetch_optional(db)
    .await?;
    let Some((user_id, oidc_subject, display_name, email, kind)) = user else {
        return Ok(None);
    };
    let user_id = Id::from_uuid(user_id).map_err(decode)?;
    let memberships = memberships_of(db, user_id).await?;
    Ok(Some(Identity {
        user_id,
        subject: oidc_subject,
        display_name,
        email: (!email.is_empty()).then_some(email),
        kind,
        memberships,
    }))
}

/// Every membership of `user_id`, read under `membership_self_read`.
pub async fn memberships_of(db: &PgPool, user_id: Id) -> Result<Vec<MembershipRow>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.current_user', $1, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    let rows: Vec<(uuid::Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT m.tenant_id, t.slug, t.name, t.status, m.role
           FROM membership m
           JOIN tenant t ON t.id = m.tenant_id
          WHERE m.user_id = app_current_user()
          ORDER BY t.slug, t.id",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(tenant_id, slug, name, status, role)| {
            Ok(MembershipRow {
                tenant_id: TenantId::from_uuid(tenant_id).map_err(decode)?,
                tenant_slug: slug,
                tenant_name: name,
                tenant_status: status,
                role,
            })
        })
        .collect()
}

/// Rebind an existing user row to a new OIDC subject.
///
/// The person is the email, not the subject: a deleted-and-recreated Keycloak account presents a
/// new `sub` and must land on the same `app_user` row, or their memberships and history fork.
/// Returns the row count so the caller can log a real change rather than a no-op.
pub async fn rebind_subject(db: &PgPool, user_id: Uuid, subject: &str) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE app_user
            SET oidc_subject = $2, version = version + 1, updated_at = now()
          WHERE id = $1 AND (oidc_subject IS NULL OR oidc_subject <> $2)",
    )
    .bind(user_id)
    .bind(subject)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// Create the `app_user` row for an invited person, keyed by the verified subject.
///
/// Called only after a pending invite for `email` was found; there is no silent sign-up.
pub async fn create_invited_user(
    db: &PgPool,
    subject: &str,
    email: &str,
    display_name: &str,
) -> Result<Id, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind, status)
         VALUES ($1, $2, $3, 'human', 'active')
         RETURNING id",
    )
    .bind(subject)
    .bind(email)
    .bind(display_name)
    .fetch_one(db)
    .await?;
    Id::from_uuid(id).map_err(decode)
}
