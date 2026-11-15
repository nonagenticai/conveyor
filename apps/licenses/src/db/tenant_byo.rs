//! The tenant BYO-mode operations and their authorizers.
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`resolve_tenant_selector`] | `models.py:5820` |
//! | [`lookup_tenant_posture`] | `models.py:8976` |
//! | [`set_tenant_byo_mode`] | `models.py:5073` |
//! | [`get_tenant_app_byo_mode`] | `models.py:6248` |
//! | [`set_tenant_app_byo_mode`] | `models.py:6111` |
//! | [`clear_tenant_app_byo_mode`] | `models.py:6305` |
//! | [`resolve_for_caller`] | `main.py:645` (`_resolve_tenant_for_caller`) |
//! | [`resolve_owned_tenant`] | `main.py:670` (`_resolve_owned_tenant`) |
//! | [`resolve_tenant_for_app_setting`] | `main.py:710` |
//! | [`authorize_app_admin`] | `main.py:874` (`_authorize_app_admin`) |
//!
//! Three rules are the whole point of this module and none of them is obvious:
//!
//! - the tenant-WIDE write is owner-only (`tenant_owner_only`), while a per-APP write is
//!   allowed for the tenant's owner when it ISSUES the app or when its own bundle brought
//!   the app onto the tenant ([`tenant_has_bundle_licence`]), and for a grantee only when it
//!   issues the app AND holds an active grant;
//! - clear/unprovision actions admit a REVOKED bundle licence, so offboarding can still clean
//!   up after revoking, while a set/provision does not;
//! - a tenant-wide BYO read with no `tenants` row is `byo_required`, and a read that faulted
//!   twice is the same string with `resolution: "fault"` — a fabrication the route must be
//!   able to tell apart from a measurement.

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::User;
use crate::db::issuance::{app_issuers, authorize_issuance, require_known_owner};
use crate::error::Refusal;

/// `_APP_SETTING_CLEAR_ACTIONS`: remove-only per-app actions that a revoked bundle licence
/// still admits (offboarding cleanup).
pub const APP_SETTING_CLEAR_ACTIONS: [&str; 2] =
    ["tenant_app_byo_mode_clear", "tenant_offering_unprovision"];

/// `RESOLUTION_ROW` / `RESOLUTION_MISSING_ROW` / `RESOLUTION_FAULT` (`models.py:80`).
pub const RESOLUTION_ROW: &str = "row";
pub const RESOLUTION_MISSING_ROW: &str = "missing_row";
pub const RESOLUTION_FAULT: &str = "fault";

/// One `tenants` row in the shape the routes publish.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
    pub keycloak_realm: Option<String>,
    pub byo_mode: Option<String>,
    pub kind: Option<String>,
    pub cost_centre: Option<String>,
    pub budget_currency: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub owner_key: Option<String>,
    pub external_ref: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// The three outcomes of [`resolve_tenant_selector`], each with its own status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorOutcome {
    Found,
    NotFound,
    /// The string can never name a tenant — decided BEFORE any query (400).
    Malformed,
}

/// A resolved selector and which of the three outcomes happened.
#[derive(Debug)]
pub struct TenantLookup {
    pub tenant: Option<TenantRecord>,
    pub outcome: SelectorOutcome,
}

/// The outcome of an owner-scoped resolution.
#[derive(Debug)]
pub enum ScopedLookup {
    Found(Box<TenantRecord>),
    NotFound,
    /// A bare name matched LIVE tenants of several owners (cross-owner only).
    Ambiguous,
}

/// A resolved per-app posture read.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfferingRow {
    pub app_key: String,
    pub app_name: String,
    pub byo_mode: String,
    pub offering_code: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// The upsert result, carrying what the write OVERWROTE (`prior_*`) so it is undoable.
#[derive(Debug, Clone)]
pub struct AppByoWrite {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub app_id: Uuid,
    pub app_key: String,
    pub byo_mode: String,
    pub offering_code: Option<String>,
    pub prior_byo_mode: Option<String>,
    pub prior_offering_code: Option<String>,
}

/// The BYO posture plus WHERE it came from (`row`, `missing_row`, `fault`).
#[derive(Debug, Clone)]
pub struct TenantPosture {
    pub byo_mode: String,
    pub resolution: String,
}

/// The database's own definition of a tenant NAME (`ck_tenants_name_format`):
/// `^[a-z0-9][a-z0-9-]*$`, full match.
fn is_tenant_name_shape(selector: &str) -> bool {
    let mut chars = selector.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// One tenant by id — archived tenants INCLUDED, exactly as the reference's `get_tenant` does.
pub async fn get_tenant(pool: &PgPool, id: Uuid) -> Result<Option<TenantRecord>, sqlx::Error> {
    sqlx::query_as::<_, TenantRecord>(
        "SELECT id, name, keycloak_realm, byo_mode, kind, cost_centre, budget_currency,
                archived_at, owner_key, external_ref, created_at
           FROM tenants WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// A tenant NAME resolves to its LIVE row only (migration 053), and with `owner` set to that
/// owner's row alone. Without one, a name matching several owners is refused, never picked.
async fn tenant_by_name(
    pool: &PgPool,
    name: &str,
    owner: Option<&str>,
) -> Result<Option<TenantRecord>, sqlx::Error> {
    let rows = sqlx::query_as::<_, TenantRecord>(
        "SELECT id, name, keycloak_realm, byo_mode, kind, cost_centre, budget_currency,
                archived_at, owner_key, external_ref, created_at
           FROM tenants
          WHERE name = $1 AND archived_at IS NULL
            AND ($2::text IS NULL OR owner_key = $2)
          ORDER BY id LIMIT 2",
    )
    .bind(name)
    .bind(owner)
    .fetch_all(pool)
    .await?;
    if rows.len() > 1 {
        tracing::warn!(
            name = %name,
            "tenant_name_ambiguous: refusing to pick one (address the tenant by id)"
        );
        return Ok(None);
    }
    Ok(rows.into_iter().next())
}

/// `models.py::DatabaseManager.resolve_tenant_selector` — and WHICH of the three outcomes
/// happened, because the route maps them to three different statuses.
pub async fn resolve_tenant_selector(
    pool: &PgPool,
    selector: &str,
) -> Result<TenantLookup, sqlx::Error> {
    let tenant_uuid = Uuid::try_parse(selector).ok();
    if tenant_uuid.is_none() && !is_tenant_name_shape(selector) {
        return Ok(TenantLookup {
            tenant: None,
            outcome: SelectorOutcome::Malformed,
        });
    }
    let tenant = match tenant_uuid {
        Some(id) => get_tenant(pool, id).await?,
        None => tenant_by_name(pool, selector, None).await?,
    };
    let outcome = if tenant.is_some() {
        SelectorOutcome::Found
    } else {
        SelectorOutcome::NotFound
    };
    Ok(TenantLookup { tenant, outcome })
}

/// `models.py::DatabaseManager.get_tenant_scoped` as a three-way outcome.
///
/// By UUID: the tenant when the caller owns it or holds an active grant (`allow_grant` is
/// always on for these routes); otherwise absent. By NAME: only within the caller's own
/// owner, live rows only. `cross_owner` reaches any tenant by id, and by name only when the
/// name is unambiguous.
pub async fn resolve_tenant_scoped(
    pool: &PgPool,
    selector: &str,
    owner: Option<&str>,
    cross_owner: bool,
) -> Result<ScopedLookup, sqlx::Error> {
    if let Ok(id) = Uuid::try_parse(selector) {
        let Some(tenant) = get_tenant(pool, id).await? else {
            return Ok(ScopedLookup::NotFound);
        };
        if cross_owner {
            return Ok(ScopedLookup::Found(Box::new(tenant)));
        }
        let Some(owner) = owner else {
            return Ok(ScopedLookup::NotFound);
        };
        if tenant.owner_key.as_deref() == Some(owner) {
            return Ok(ScopedLookup::Found(Box::new(tenant)));
        }
        let granted = crate::db::owners::owner_can_access_tenant(pool, owner, &id.to_string())
            .await
            .unwrap_or(false);
        return Ok(if granted {
            ScopedLookup::Found(Box::new(tenant))
        } else {
            ScopedLookup::NotFound
        });
    }

    if cross_owner {
        let mut rows = sqlx::query_as::<_, TenantRecord>(
            "SELECT id, name, keycloak_realm, byo_mode, kind, cost_centre, budget_currency,
                    archived_at, owner_key, external_ref, created_at
               FROM tenants
              WHERE name = $1 AND archived_at IS NULL ORDER BY id LIMIT 2",
        )
        .bind(selector)
        .fetch_all(pool)
        .await?;
        return Ok(match rows.len() {
            0 => ScopedLookup::NotFound,
            1 => ScopedLookup::Found(Box::new(rows.remove(0))),
            _ => ScopedLookup::Ambiguous,
        });
    }
    let Some(owner) = owner else {
        return Ok(ScopedLookup::NotFound);
    };
    Ok(match tenant_by_name(pool, selector, Some(owner)).await? {
        Some(tenant) => ScopedLookup::Found(Box::new(tenant)),
        None => ScopedLookup::NotFound,
    })
}

/// `_resolve_tenant_for_caller` (READ resolution): own or granted tenants, a foreign tenant
/// indistinguishable from an absent one (404). `cross_owner` reads anything, audited.
pub async fn resolve_for_caller(
    pool: &PgPool,
    user: &User,
    selector: &str,
    action: &str,
) -> Result<TenantRecord, Refusal> {
    let tenant =
        match resolve_tenant_scoped(pool, selector, user.owner_key.as_deref(), user.cross_owner)
            .await
            .map_err(|error| database_fault(error, "get_tenant_scoped"))?
        {
            ScopedLookup::Found(tenant) => *tenant,
            ScopedLookup::NotFound => return Err(tenant_not_found()),
            ScopedLookup::Ambiguous => return Err(tenant_name_ambiguous(selector)),
        };
    if user.cross_owner && tenant.owner_key.as_deref() != user.owner_key.as_deref() {
        audit_cross_owner(user, action, Some(&tenant), "");
    }
    Ok(tenant)
}

/// `_resolve_owned_tenant`: owner-only WRITE resolution for TENANT-WIDE settings.
///
/// A foreign tenant the caller holds no grant on is 404; a GRANT is not ownership, so a
/// grantee that resolves the row gets 403 `tenant_owner_only`. `cross_owner` bypasses,
/// audited.
pub async fn resolve_owned_tenant(
    pool: &PgPool,
    user: &User,
    selector: &str,
    action: &str,
) -> Result<TenantRecord, Refusal> {
    let owner = require_known_owner(user)?;
    let tenant = match resolve_tenant_scoped(pool, selector, Some(&owner), user.cross_owner)
        .await
        .map_err(|error| database_fault(error, "get_tenant_scoped"))?
    {
        ScopedLookup::Found(tenant) => *tenant,
        ScopedLookup::NotFound => return Err(tenant_not_found()),
        ScopedLookup::Ambiguous => return Err(tenant_name_ambiguous(selector)),
    };
    if tenant.owner_key.as_deref() != Some(owner.as_str()) {
        if user.cross_owner {
            audit_cross_owner(user, action, Some(&tenant), "");
            return Ok(tenant);
        }
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "tenant_owner_only",
                "message": "only the tenant's owning app may do this; a grant does not",
            }),
        ));
    }
    Ok(tenant)
}

/// `_resolve_tenant_for_app_setting`: WRITE resolution for a per-(tenant, app) setting.
///
/// The tenant's OWNER may set it for an app it issues, or for one its own bundle brought onto
/// the tenant — an ACTIVE bundle licence to set, any status to clear. A GRANTEE may set it
/// only for an app it issues on a tenant it holds an active grant on, the same rule as minting
/// that app's licence ([`authorize_issuance`]). An unknown app on the caller's own tenant is
/// admitted here so the route keeps answering its own 404 (PUT) / false (DELETE).
pub async fn resolve_tenant_for_app_setting(
    pool: &PgPool,
    user: &User,
    selector: &str,
    app_key: &str,
    action: &str,
) -> Result<TenantRecord, Refusal> {
    let owner = require_known_owner(user)?;
    let parsed = Uuid::try_parse(selector).ok();
    let tenant = match parsed {
        // A UUID selector for a non-cross-owner caller resolves GLOBALLY here; the owner /
        // grantee branches below decide, so a foreign tenant is a 403 from the authorizer.
        Some(id) if !user.cross_owner => get_tenant(pool, id)
            .await
            .map_err(|error| database_fault(error, "get_tenant"))?,
        _ => match resolve_tenant_scoped(pool, selector, Some(&owner), user.cross_owner)
            .await
            .map_err(|error| database_fault(error, "get_tenant_scoped"))?
        {
            ScopedLookup::Found(tenant) => Some(*tenant),
            ScopedLookup::NotFound => None,
            ScopedLookup::Ambiguous => return Err(tenant_name_ambiguous(selector)),
        },
    };
    let Some(tenant) = tenant else {
        return Err(tenant_not_found());
    };
    let app = app_by_key(pool, app_key)
        .await
        .map_err(|error| database_fault(error, "get_app_by_key"))?;

    if tenant.owner_key.as_deref() == Some(owner.as_str()) {
        let Some((app_id, _)) = app else {
            // Unknown app on the caller's own tenant: no write can happen. The route answers
            // its own 404 (PUT) and its idempotent false (DELETE).
            return Ok(tenant);
        };
        let issuers = app_issuers(pool, app_id)
            .await
            .map_err(|error| database_fault(error, "app_issuers"))?;
        if issuers.iter().any(|issuer| issuer == &owner) {
            return Ok(tenant);
        }
        let active_only = !APP_SETTING_CLEAR_ACTIONS.contains(&action);
        if tenant_has_bundle_licence(pool, tenant.id, app_id, active_only)
            .await
            .map_err(|error| database_fault(error, "tenant_has_bundle_licence"))?
        {
            return Ok(tenant);
        }
        // Owning the tenant is not owning the app.
        authorize_app_admin(pool, user, app_id, action, &tenant).await?;
        return Ok(tenant);
    }
    if user.cross_owner {
        audit_cross_owner(user, action, Some(&tenant), &format!("app_key={app_key}"));
        return Ok(tenant);
    }
    let Some((app_id, _)) = app else {
        // Refused the same way for an app that does not exist, so a non-owner cannot
        // enumerate the catalogue through this route.
        return Err(app_not_issuable(&owner));
    };
    authorize_issuance(pool, user, app_id, Some(tenant.id), action).await?;
    Ok(tenant)
}

/// `_authorize_app_admin`: the caller's owner must ISSUE the app. `cross_owner` bypasses.
async fn authorize_app_admin(
    pool: &PgPool,
    user: &User,
    app_id: Uuid,
    action: &str,
    tenant: &TenantRecord,
) -> Result<(), Refusal> {
    let owner = require_known_owner(user)?;
    let issuers = app_issuers(pool, app_id)
        .await
        .map_err(|error| database_fault(error, "app_issuers"))?;
    if issuers.iter().any(|issuer| issuer == &owner) {
        return Ok(());
    }
    if user.cross_owner {
        audit_cross_owner(user, action, Some(tenant), &format!("app_id={app_id}"));
        return Ok(());
    }
    tracing::warn!(
        action = %action,
        owner = %owner,
        app_id = %app_id,
        azp = ?user.azp,
        "app_not_administrable_by_caller"
    );
    Err(app_not_administrable(&owner))
}

/// The app row a write needs: its id and its canonical key.
pub async fn app_by_key(pool: &PgPool, key: &str) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as("SELECT id, key FROM app WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
}

/// `tenant_has_bundle_licence`: a licence for `app_id` on `tenant_id` that a bundle minted
/// (`bundle_id` set) — any status, or only an effectively active one (the three-part rule).
pub async fn tenant_has_bundle_licence(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    active_only: bool,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM license l
              WHERE l.tenant_id = $1
                AND l.app_id = $2
                AND l.bundle_id IS NOT NULL
                AND (NOT $4::boolean OR (
                      ((l.deleted_at IS NULL AND l.status = 'active')
                        OR (l.deleted_at IS NOT NULL
                            AND l.deleted_at > $3
                            AND l.deleted_from_status = 'active'))
                      AND (l.starts_at IS NULL OR l.starts_at <= $3)
                      AND (l.expires_at IS NULL OR l.expires_at > $3)
                ))
         )",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(Utc::now())
    .bind(active_only)
    .fetch_one(pool)
    .await
}

/// `models.py::DatabaseManager.set_tenant_byo_mode` — update one tenant's posture and return
/// the row exactly as `Tenant.to_dict` publishes it.
pub async fn set_tenant_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    byo_mode: &str,
) -> Result<Option<TenantRecord>, sqlx::Error> {
    sqlx::query_as::<_, TenantRecord>(
        "UPDATE tenants SET byo_mode = $2 WHERE id = $1
         RETURNING id, name, keycloak_realm, byo_mode, kind, cost_centre, budget_currency,
                   archived_at, owner_key, external_ref, created_at",
    )
    .bind(tenant_id)
    .bind(byo_mode)
    .fetch_optional(pool)
    .await
}

/// `models.py::DatabaseManager.get_tenant_app_byo_mode` — the override row for one
/// `(tenant, app)`, or `None` when there is none (or the app does not exist).
pub async fn get_tenant_app_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
) -> Result<Option<OfferingRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.key AS app_key, a.name AS app_name, r.byo_mode, r.offering_code,
                r.created_at, r.updated_at
           FROM tenant_app_byo_mode r
           JOIN app a ON a.id = r.app_id
          WHERE r.tenant_id = $1 AND a.key = $2",
    )
    .bind(tenant_id)
    .bind(app_key)
    .fetch_optional(pool)
    .await
}

/// `models.py::DatabaseManager.set_tenant_app_byo_mode` — upsert the override.
///
/// The prior values are read INSIDE the same transaction as the write, so the undoable pair
/// cannot be raced by a concurrent writer between a read and a write. `offering_code` is
/// ALWAYS written (as NULL): a direct posture write did not come from an offering.
pub async fn set_tenant_app_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
    byo_mode: &str,
) -> Result<Option<AppByoWrite>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let tenant: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((tenant_id, tenant_name)) = tenant else {
        return Ok(None);
    };
    let app: Option<(Uuid, String)> = sqlx::query_as("SELECT id, key FROM app WHERE key = $1")
        .bind(app_key)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((app_id, app_key)) = app else {
        return Ok(None);
    };
    let prior: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT byo_mode, offering_code FROM tenant_app_byo_mode
          WHERE tenant_id = $1 AND app_id = $2",
    )
    .bind(tenant_id)
    .bind(app_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (prior_byo_mode, prior_offering_code) = match prior {
        Some((mode, code)) => (Some(mode), code),
        None => (None, None),
    };
    sqlx::query(
        "INSERT INTO tenant_app_byo_mode
             (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
         VALUES ($1, $2, $3, NULL, now(), now())
         ON CONFLICT (tenant_id, app_id) DO UPDATE
             SET byo_mode = EXCLUDED.byo_mode,
                 offering_code = NULL,
                 updated_at = now()",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(byo_mode)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(AppByoWrite {
        tenant_id,
        tenant_name,
        app_id,
        app_key,
        byo_mode: byo_mode.to_owned(),
        offering_code: None,
        prior_byo_mode,
        prior_offering_code,
    }))
}

/// `models.py::DatabaseManager.clear_tenant_app_byo_mode` — true iff a row was deleted.
pub async fn clear_tenant_app_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM tenant_app_byo_mode r
           USING app a
          WHERE r.app_id = a.id AND r.tenant_id = $1 AND a.key = $2",
    )
    .bind(tenant_id)
    .bind(app_key)
    .execute(pool)
    .await?;
    Ok(deleted.rows_affected() > 0)
}

/// `models.py::DatabaseManager._lookup_tenant_posture` — resolve a tenant's posture with the
/// reference's one bounded retry, and say where the answer came from.
///
/// A missing row and a double fault both fail CLOSED to `byo_required`; only `resolution`
/// tells them apart, which is what lets a caller that bills refuse to freeze a fabrication.
pub async fn lookup_tenant_posture(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: Option<&str>,
) -> TenantPosture {
    match posture_once(pool, tenant_id, app_key).await {
        Ok(posture) => posture,
        Err(first) => match posture_once(pool, tenant_id, app_key).await {
            Ok(posture) => posture,
            Err(retry) => {
                tracing::warn!(
                    tenant = %tenant_id,
                    app = ?app_key,
                    first = %first,
                    retry = %retry,
                    "byo_mode lookup failed on both the first attempt and a retry; \
                     failing CLOSED to byo_required with resolution=fault"
                );
                TenantPosture {
                    byo_mode: "byo_required".to_owned(),
                    resolution: RESOLUTION_FAULT.to_owned(),
                }
            }
        },
    }
}

/// One posture read. Raises on any database fault — the retry decision is the caller's.
async fn posture_once(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: Option<&str>,
) -> Result<TenantPosture, sqlx::Error> {
    let Some(tenant) = get_tenant(pool, tenant_id).await? else {
        return Ok(TenantPosture {
            byo_mode: "byo_required".to_owned(),
            resolution: RESOLUTION_MISSING_ROW.to_owned(),
        });
    };
    if let Some(app_key) = app_key {
        let override_mode: Option<String> = sqlx::query_scalar(
            "SELECT r.byo_mode
               FROM tenant_app_byo_mode r
               JOIN app a ON a.id = r.app_id
              WHERE r.tenant_id = $1 AND a.key = $2",
        )
        .bind(tenant.id)
        .bind(app_key)
        .fetch_optional(pool)
        .await?;
        if let Some(mode) = override_mode.filter(|mode| !mode.is_empty()) {
            return Ok(TenantPosture {
                byo_mode: mode,
                resolution: RESOLUTION_ROW.to_owned(),
            });
        }
    }
    Ok(TenantPosture {
        byo_mode: tenant
            .byo_mode
            .filter(|mode| !mode.is_empty())
            .unwrap_or_else(|| "byo_required".to_owned()),
        resolution: RESOLUTION_ROW.to_owned(),
    })
}

// ── Refusals and audit ──────────────────────────────────────────────────────

/// `_audit_cross_owner`: this service has no audit table, so the log line IS the record.
fn audit_cross_owner(user: &User, action: &str, tenant: Option<&TenantRecord>, detail: &str) {
    tracing::warn!(
        action = %action,
        tenant = ?tenant.map(|tenant| tenant.id),
        tenant_owner = ?tenant.and_then(|tenant| tenant.owner_key.as_deref()),
        caller_owner = ?user.owner_key.as_deref(),
        azp = ?user.azp,
        realm = ?user.realm(),
        sub = %user.sub,
        detail = %detail,
        at = %Utc::now().to_rfc3339(),
        "cross_owner_action"
    );
}

/// `_tenant_not_found()`.
fn tenant_not_found() -> Refusal {
    Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found")
}

/// `_resolve_tenant_for_caller`'s `TenantNameAmbiguousError` arm.
fn tenant_name_ambiguous(selector: &str) -> Refusal {
    Refusal::detail(
        StatusCode::CONFLICT,
        json!({
            "error": "tenant_name_ambiguous",
            "name": selector,
            "message": "this tenant name is used by several owning apps; use the id",
        }),
    )
}

/// A `_authorize_app_admin` refusal.
fn app_not_administrable(owner: &str) -> Refusal {
    Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "app_not_administrable_by_caller",
            "message": format!("owner '{owner}' does not issue this app and may not administer it"),
        }),
    )
}

/// `_resolve_tenant_for_app_setting`'s non-owner / unknown-app refusal.
fn app_not_issuable(owner: &str) -> Refusal {
    Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "app_not_issuable_by_caller",
            "message": format!("owner '{owner}' may not issue this app's licences"),
        }),
    )
}

/// A lookup fault while authorizing: 500, never a pass.
fn database_fault(error: sqlx::Error, operation: &'static str) -> Refusal {
    tracing::error!(%error, operation, "tenant byo-mode database fault");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}
