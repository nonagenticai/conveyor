//! The Platform (super-admin) read model and the two writes it performs.
//!
//! ⛔ **The reads are per tenant, through `begin_scoped`.** `membership` and `app_licence`
//! are FORCE RLS: a join written as one query without `app.current_tenant` returns **zero
//! rows silently** — a tenant list that looks empty and counts that look like zero. The
//! number of tenants here is deliberately small (a handful), so one scoped transaction per
//! tenant is the correct trade against a policy that would otherwise need relaxing.
//!
//! The `tenant` and `app_user` tables have no RLS, so the create path can read them inside
//! the same scoped transaction the membership write needs.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{self, AuditEntry};
use crate::invite;
use crate::scope::begin_scoped;

/// The synced entitlement summary the list and detail show.
#[derive(Clone, Debug)]
pub struct LicenceSummary {
    /// `active | expired | revoked | unknown`.
    pub status: String,
    /// When the entitlement expires, when the service supplied it.
    pub expires_at: Option<DateTime<Utc>>,
    /// When the sync last confirmed it.
    pub checked_at: DateTime<Utc>,
}

/// One tenant row in the Platform list.
#[derive(Clone, Debug)]
pub struct TenantSummary {
    /// `tenant.id` — the same id provisioning minted.
    pub id: Uuid,
    /// The canonical slug.
    pub slug: String,
    /// The display name.
    pub name: String,
    /// `active | suspended | closed`.
    pub status: String,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// Every membership.
    pub members: i64,
    /// Memberships whose role is `admin`.
    pub admins: i64,
    /// The conveyor entitlement, when one was synced.
    pub licence: Option<LicenceSummary>,
}

/// One admin of a tenant.
#[derive(Clone, Debug)]
pub struct AdminRow {
    /// `app_user.id`.
    pub user_id: Uuid,
    /// The email (the identity key).
    pub email: Option<String>,
    /// The display name.
    pub display_name: String,
    /// The membership role (`admin` for every row here).
    pub role: String,
    /// `app_user.status`.
    pub user_status: String,
}

/// One tenant plus its admins.
#[derive(Clone, Debug)]
pub struct TenantDetail {
    /// The same row the list shows.
    pub summary: TenantSummary,
    /// Its admins, oldest identity first.
    pub admins: Vec<AdminRow>,
}

/// A tenant created (or converged) by the wizard.
#[derive(Clone, Debug)]
pub struct CreatedTenant {
    /// The tenant id (provisioning's).
    pub tenant: TenantId,
    /// The first admin's `app_user.id`.
    pub admin_user_id: Uuid,
    /// True when this call created that `app_user` row.
    pub created_user: bool,
}

/// Why a Platform write failed.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// Another tenant already holds the slug.
    #[error("a tenant with slug {0:?} already exists")]
    SlugTaken(String),
    /// The email belongs to an agent or service principal, never a person.
    #[error("the email belongs to a non-human principal")]
    EmailNotHuman,
    /// The tenant named does not exist.
    #[error("tenant not found")]
    NotFound,
    /// The database refused.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.code())
        .map(|code| code == "23505")
        .unwrap_or(false)
}

/// Every tenant, newest first, with its counts and entitlement.
pub async fn list(pool: &PgPool, app_key: &str) -> Result<Vec<TenantSummary>, PlatformError> {
    let rows: Vec<(Uuid, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, slug, name, status, created_at FROM tenant ORDER BY created_at DESC, id",
    )
    .fetch_all(pool)
    .await?;

    let mut tenants = Vec::with_capacity(rows.len());
    for (id, slug, name, status, created_at) in rows {
        let tenant = TenantId::from_uuid(id)
            .map_err(|error| PlatformError::Database(sqlx::Error::Decode(Box::new(error))))?;
        let (members, admins, licence) = scoped_counts(pool, tenant, app_key).await?;
        tenants.push(TenantSummary {
            id,
            slug,
            name,
            status,
            created_at,
            members,
            admins,
            licence,
        });
    }
    Ok(tenants)
}

/// One tenant and its admins.
pub async fn detail(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
) -> Result<Option<TenantDetail>, PlatformError> {
    let row: Option<(Uuid, String, String, String, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, slug, name, status, created_at FROM tenant WHERE id = $1")
            .bind(tenant.as_uuid())
            .fetch_optional(pool)
            .await?;
    let Some((id, slug, name, status, created_at)) = row else {
        return Ok(None);
    };
    let (members, admins, licence) = scoped_counts(pool, tenant, app_key).await?;

    let mut tx = begin_scoped(pool, tenant).await?;
    let admin_rows: Vec<(Uuid, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, m.role, u.status
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND m.role = 'admin'
          ORDER BY u.created_at, u.id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(TenantDetail {
        summary: TenantSummary {
            id,
            slug,
            name,
            status,
            created_at,
            members,
            admins,
            licence,
        },
        admins: admin_rows
            .into_iter()
            .map(
                |(user_id, email, display_name, role, user_status)| AdminRow {
                    user_id,
                    email,
                    display_name,
                    role,
                    user_status,
                },
            )
            .collect(),
    }))
}

/// Member/admin counts and the licence row, read under the tenant's RLS scope.
async fn scoped_counts(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
) -> Result<(i64, i64, Option<LicenceSummary>), PlatformError> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let (members, admins): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE role = 'admin')
           FROM membership WHERE tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    let licence: Option<(String, Option<DateTime<Utc>>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT status, expires_at, checked_at FROM app_licence
          WHERE tenant_id = $1 AND app_key = $2",
    )
    .bind(tenant.as_uuid())
    .bind(app_key)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        members,
        admins,
        licence.map(|(status, expires_at, checked_at)| LicenceSummary {
            status,
            expires_at,
            checked_at,
        }),
    ))
}

/// What the wizard supplies to [`create`].
#[derive(Clone, Copy, Debug)]
pub struct NewTenant<'a> {
    /// The tenant id provisioning minted (adopted, never generated here).
    pub tenant: TenantId,
    /// The canonical slug.
    pub slug: &'a str,
    /// The display name.
    pub name: &'a str,
    /// The first admin's email — the identity key.
    pub email: &'a str,
    /// The first admin's display name.
    pub display_name: &'a str,
    /// The super-admin's `app_user.id`.
    pub actor: Uuid,
    /// The request id, for the audit row.
    pub request_id: Option<&'a str>,
}

/// Create (or converge, on a retry) the tenant's local rows: the tenant, the first admin's
/// `app_user` and an `admin` membership.
///
/// Idempotent by `tenant` id: a retry after a later step failed converges this step instead
/// of duplicating it. The `app_user` row is keyed by the VERIFIED EMAIL the super-admin
/// typed — the same key the identity path rebinds on — so the person's first sign-in lands
/// on this row and this membership.
pub async fn create(pool: &PgPool, new: NewTenant<'_>) -> Result<CreatedTenant, PlatformError> {
    let NewTenant {
        tenant,
        slug,
        name,
        email,
        display_name,
        actor,
        request_id,
    } = new;
    let mut tx = begin_scoped(pool, tenant).await?;

    // The tenant table has no RLS; a slug held by a DIFFERENT id is a refusal, not a merge.
    if let Err(error) = sqlx::query(
        "INSERT INTO tenant (id, slug, name) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(slug)
    .bind(name)
    .execute(&mut *tx)
    .await
    {
        if is_unique_violation(&error) {
            return Err(PlatformError::SlugTaken(slug.to_owned()));
        }
        return Err(PlatformError::Database(error));
    }

    // Reuse an existing person by email; only a human may hold a tenant membership.
    let existing: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, kind, status, display_name FROM app_user
          WHERE lower(email) = lower($1)
          ORDER BY created_at, id
          LIMIT 1",
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;

    let (admin_user_id, created_user) = match existing {
        Some((id, kind, status, current_name)) => {
            if kind != "human" {
                return Err(PlatformError::EmailNotHuman);
            }
            let display_name = if current_name.trim().is_empty() {
                display_name.trim()
            } else {
                current_name.as_str()
            };
            if status != "active" || current_name.trim().is_empty() {
                sqlx::query(
                    "UPDATE app_user SET status = 'active',
                            display_name = CASE WHEN display_name = '' THEN $2 ELSE display_name END,
                            updated_at = now()
                      WHERE id = $1",
                )
                .bind(id)
                .bind(display_name)
                .execute(&mut *tx)
                .await?;
            }
            (id, false)
        }
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO app_user (oidc_subject, email, display_name, kind, status)
                 VALUES (NULL, $1, $2, 'human', 'active')
                 RETURNING id",
            )
            .bind(email)
            .bind(display_name)
            .fetch_one(&mut *tx)
            .await?;
            (id, true)
        }
    };

    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO UPDATE
             SET role = 'admin', version = membership.version + 1, updated_at = now()",
    )
    .bind(tenant.as_uuid())
    .bind(admin_user_id)
    .execute(&mut *tx)
    .await?;

    audit::record(
        &mut tx,
        tenant,
        &AuditEntry {
            actor_id: Some(actor),
            actor_kind: "user".into(),
            action: "platform.tenant_created".into(),
            status: "ok".into(),
            resource_type: Some("tenant".into()),
            resource_id: Some(tenant.as_uuid()),
            request_id: request_id.map(str::to_owned),
            details: serde_json::json!({
                "slug": slug,
                "admin_user_id": admin_user_id,
                "created_user": created_user,
            }),
        },
    )
    .await
    .map_err(|error| PlatformError::Database(sqlx::Error::Protocol(error.to_string())))?;

    tx.commit().await?;
    Ok(CreatedTenant {
        tenant,
        admin_user_id,
        created_user,
    })
}

/// Write an `admin` invite for a tenant that lost all of its admins.
///
/// The invite is redeemed by the invited person's first sign-in with that verified email
/// (the invite is the only sign-up path); it grants the `admin` membership at redemption.
pub async fn invite_admin(
    pool: &PgPool,
    tenant: TenantId,
    email: &str,
    invited_by: Uuid,
    request_id: Option<&str>,
) -> Result<Uuid, PlatformError> {
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM tenant WHERE id = $1")
        .bind(tenant.as_uuid())
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(PlatformError::NotFound);
    }

    let mut tx = begin_scoped(pool, tenant).await?;
    let row = invite::create_tx(&mut tx, tenant, email, "admin", Some(invited_by)).await?;
    audit::record(
        &mut tx,
        tenant,
        &AuditEntry {
            actor_id: Some(invited_by),
            actor_kind: "user".into(),
            action: "platform.admin_invited".into(),
            status: "ok".into(),
            resource_type: Some("invite".into()),
            resource_id: Some(row.id),
            request_id: request_id.map(str::to_owned),
            details: serde_json::json!({"role": "admin"}),
        },
    )
    .await
    .map_err(|error| PlatformError::Database(sqlx::Error::Protocol(error.to_string())))?;
    tx.commit().await?;
    Ok(row.id)
}

/// The email of a tenant's first admin, for a provisioning retry.
pub async fn first_admin_email(
    pool: &PgPool,
    tenant: TenantId,
) -> Result<Option<(Uuid, String, String)>, PlatformError> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let row: Option<(Uuid, Option<String>, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name
           FROM membership m JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1 AND m.role = 'admin'
          ORDER BY u.created_at, u.id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.and_then(|(id, email, display_name)| {
        email
            .filter(|email| !email.trim().is_empty())
            .map(|email| (id, email, display_name))
    }))
}
