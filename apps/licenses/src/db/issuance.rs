//! The issuance-authorization core: who may mint, revoke or administer an app's licences.
//!
//! Ported from the reference byte-identical to pinned `4e8a08d`:
//!
//! | Rust | reference |
//! |---|---|
//! | [`app_issuers`] | `models.py:5611` |
//! | [`require_known_owner`] | `main.py:618` |
//! | [`authorize_issuance`] | `main.py:824` |
//! | [`bundle_owner_may_revoke`] | `main.py:909` |
//! | [`authorize_licence_revoke`] | `main.py:949` |
//! | [`licences_for_token_hash_in_tenant`] | `models.py:8868` |
//!
//! `POST /license/revoke` is decided here and `bundle-token` reuses [`authorize_issuance`] as its
//! issuance gate, so the cross-owner bypass and the owner/grant branches are NOT optional
//! detail — every refusal below is the reference's own status and detail text.
//!
//! Three deliberate differences, none observable at a route:
//!
//! - `_ensure_owner_resolved` has no analogue. In this port `auth::attach_and_authorize` is the
//!   only place `owner_key` is attached and it already ran in the middleware; a `User` built
//!   outside that path (a test) must carry its own `owner_key`. `_ensure_owner_resolved` re-runs
//!   the `(realm, azp)` lookup, which is exactly what the middleware already did.
//! - A database fault on an authorizer's lookup is a 500 `{"detail": "Internal Server Error"}`
//!   (the reference lets the driver exception escape and Starlette answers the same way). It is
//!   logged and never admits.
//! - [`app_issuers`] answers a `Vec` ordered by `owner_key`; the reference answers a `set`. The
//!   membership test the authorizers perform is identical.
//!
//! No axum handler or extractor type appears here: the module is the DB/authorization layer, and
//! only `axum::http::StatusCode` (the `http` type `error::Refusal` itself requires) is named.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::User;
use crate::db::owners::owner_can_access_tenant;
use crate::db::tokens::hash_token;
use crate::error::Refusal;

/// The licence fields the revoke authorizers decide on.
///
/// `bundle_id` NULL means "not bundle-minted"; `is_dependency` names a shared dependency licence
/// (which, by the `ck_license_dependency_unbundled` check, always has `bundle_id` NULL).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LicenceRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub app_id: Uuid,
    pub subject_id: Uuid,
    pub status: String,
    pub is_dependency: bool,
    pub bundle_id: Option<Uuid>,
}

/// The tenant fields the authorizers decide on and audit. `archived_at` is never a gate: an
/// archived tenant is exactly the case the bundle-owner widening exists for.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct TenantRow {
    id: Uuid,
    owner_key: String,
    archived_at: Option<DateTime<Utc>>,
}

/// `models.py::DatabaseManager.app_issuers` — the owners allowed to issue `app_id`'s licences.
pub async fn app_issuers(pool: &PgPool, app_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT owner_key FROM app_issuer WHERE app_id = $1 ORDER BY owner_key")
        .bind(app_id)
        .fetch_all(pool)
        .await
}

/// `main.py::_require_known_owner` — the caller's owner, or 403 `unknown_api_client`.
///
/// Returns the owner (the reference returns it too) so every authorizer reads it from the one
/// place that guarantees it is present. Never substitutes a default owner.
pub fn require_known_owner(user: &User) -> Result<String, Refusal> {
    match user.owner_key.as_deref() {
        Some(owner) => Ok(owner.to_owned()),
        None => {
            tracing::warn!(
                realm = ?user.realm(),
                azp = ?user.azp,
                sub = %user.sub,
                "unknown_api_client: refused write"
            );
            Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                json!({
                    "error": "unknown_api_client",
                    "message": "this client is not registered as an owning app (api_client); tenant and licence writes are refused",
                }),
            ))
        }
    }
}

/// `main.py::_authorize_issuance` — the caller's owner must ISSUE the app AND own the tenant or
/// hold an ACTIVE grant on it.
///
/// An absent tenant (`None`, or an id with no row) is deliberately NOT a refusal here: the
/// reference leaves it to the route's own pre-050 404. `cross_owner` bypasses both locks, but
/// only AFTER the known-owner gate — an unknown client is never a break-glass caller.
pub async fn authorize_issuance(
    pool: &PgPool,
    user: &User,
    app_id: Uuid,
    tenant_id: Option<Uuid>,
    action: &str,
) -> Result<(), Refusal> {
    let owner = require_known_owner(user)?;
    let issuers = app_issuers(pool, app_id)
        .await
        .map_err(|error| database_fault(error, "app_issuers"))?;
    let tenant = match tenant_id {
        Some(tenant_id) => fetch_tenant(pool, tenant_id)
            .await
            .map_err(|error| database_fault(error, "get_tenant"))?,
        None => None,
    };

    let issuable = issuers.iter().any(|issuer| issuer == &owner);
    let accessible = match tenant.as_ref() {
        None => true,
        Some(tenant) => owner_can_access_tenant(pool, &owner, &tenant.id.to_string())
            .await
            .map_err(|error| database_fault(error, "owner_can_access_tenant"))?,
    };
    if issuable && accessible {
        return Ok(());
    }
    if user.cross_owner {
        audit_cross_owner(user, action, tenant.as_ref(), &format!("app_id={app_id}"));
        return Ok(());
    }
    if !issuable {
        tracing::warn!(
            action = %action,
            owner = %owner,
            app_id = %app_id,
            tenant = ?tenant_id,
            azp = ?user.azp,
            "app_not_issuable_by_caller"
        );
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "app_not_issuable_by_caller",
                "message": format!("owner '{owner}' may not issue this app's licences"),
            }),
        ));
    }
    tracing::warn!(
        action = %action,
        owner = %owner,
        tenant = ?tenant_id,
        azp = ?user.azp,
        "tenant_not_accessible"
    );
    Err(Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "tenant_not_accessible",
            "message": "the tenant is neither owned by nor granted to the calling app",
        }),
    ))
}

/// `main.py::_bundle_owner_may_revoke` (returns `bool`; [`bundle_owner_tenant`] keeps the tenant
/// the audit line needs).
///
/// REVOKE-ONLY widening of [`authorize_issuance`]: the tenant's OWNER may revoke licences its
/// bundles minted even when it does not issue their app. True only when EVERY row is
/// bundle-minted (or a dependency licence, with `allow_dependency`), on `tenant_id`, and the
/// caller's owner OWNS that tenant — a grant is not enough, and an archived tenant counts.
/// One non-bundle row refuses the whole set.
pub async fn bundle_owner_may_revoke(
    pool: &PgPool,
    user: &User,
    licences: &[LicenceRow],
    tenant_id: Option<Uuid>,
    allow_dependency: bool,
) -> Result<bool, Refusal> {
    let rows: Vec<&LicenceRow> = licences.iter().collect();
    Ok(
        bundle_owner_tenant(pool, user, &rows, tenant_id, allow_dependency)
            .await?
            .is_some(),
    )
}

/// `main.py::_authorize_licence_revoke` — [`authorize_issuance`] per app, OR — for an app the
/// caller does not issue — the [`bundle_owner_may_revoke`] widening over that app's rows.
///
/// The apps are walked in `sorted(str(app_id))` order, so which app's refusal the caller sees is
/// the reference's. The `bundle_owner_revoke` audit lines are emitted only after EVERY app has
/// passed: logging per app as it was admitted left a "revoke" record behind when a later app of
/// the same call was refused and nothing was revoked.
///
/// `allow_dependency` admits dependency rows through the owner widening; "still required" is the
/// route's own 409, not decided here.
pub async fn authorize_licence_revoke(
    pool: &PgPool,
    user: &User,
    licences: &[LicenceRow],
    tenant_id: Option<Uuid>,
    action: &str,
    allow_dependency: bool,
) -> Result<(), Refusal> {
    let owner = require_known_owner(user)?;

    // Keyed on the canonical app-id string so the walk order is Python's `sorted(by_app)`.
    let mut by_app: BTreeMap<String, (Uuid, Vec<&LicenceRow>)> = BTreeMap::new();
    for licence in licences {
        by_app
            .entry(licence.app_id.to_string())
            .or_insert_with(|| (licence.app_id, Vec::new()))
            .1
            .push(licence);
    }

    let mut admitted: Vec<(TenantRow, Vec<&LicenceRow>)> = Vec::new();
    for (app_id, rows) in by_app.values() {
        if !user.cross_owner {
            let issuers = app_issuers(pool, *app_id)
                .await
                .map_err(|error| database_fault(error, "app_issuers"))?;
            let is_issuer = issuers.iter().any(|issuer| issuer == &owner);
            if !is_issuer
                && let Some(tenant) =
                    bundle_owner_tenant(pool, user, rows, tenant_id, allow_dependency).await?
            {
                admitted.push((tenant, rows.clone()));
                continue;
            }
        }
        authorize_issuance(pool, user, *app_id, tenant_id, action).await?;
    }

    for (tenant, rows) in &admitted {
        for licence in rows {
            tracing::info!(
                action = %action,
                azp = ?user.azp,
                realm = ?user.realm(),
                owner = ?user.owner_key,
                tenant = %tenant.id,
                licence = %licence.id,
                bundle = ?licence.bundle_id,
                app_id = %licence.app_id,
                tenant_archived = tenant.archived_at.is_some(),
                dependency = licence.is_dependency,
                "bundle_owner_revoke"
            );
        }
    }
    Ok(())
}

/// `models.py::DatabaseManager.licences_for_token_hash_in_tenant` — every licence of `tenant_id`
/// carrying the token's hash, exactly the rows `revoke_token_for_tenant` would write.
///
/// `raw_token` is hashed here (`Token.hash_token`) so the caller never handles the stored form.
/// A non-UUID tenant id is an empty list, as in the reference. `DISTINCT` matters: one licence
/// with two tokens carrying the same hash must be authorized once, not once per token.
pub async fn licences_for_token_hash_in_tenant(
    pool: &PgPool,
    raw_token: &str,
    tenant_id: &str,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    let Ok(tenant_id) = Uuid::try_parse(tenant_id) else {
        return Ok(Vec::new());
    };
    sqlx::query_as(
        "SELECT DISTINCT l.id, l.tenant_id, l.app_id, l.subject_id, l.status,
                l.is_dependency, l.bundle_id
           FROM license l
           JOIN token t ON t.license_id = l.id
          WHERE t.token_hash = $1
            AND l.tenant_id = $2",
    )
    .bind(hash_token(raw_token))
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// The reference's `_bundle_owner_may_revoke`, returning the tenant so the audit line can name
/// it (and its archived state) after the whole call is authorized.
async fn bundle_owner_tenant(
    pool: &PgPool,
    user: &User,
    licences: &[&LicenceRow],
    tenant_id: Option<Uuid>,
    allow_dependency: bool,
) -> Result<Option<TenantRow>, Refusal> {
    let Some(owner) = user.owner_key.as_deref() else {
        return Ok(None);
    };
    if licences.is_empty() {
        return Ok(None);
    }
    let Some(tenant_id) = tenant_id else {
        return Ok(None);
    };
    let Some(tenant) = fetch_tenant(pool, tenant_id)
        .await
        .map_err(|error| database_fault(error, "get_tenant"))?
    else {
        return Ok(None);
    };
    if tenant.owner_key != owner {
        return Ok(None);
    }
    let admitted = licences.iter().all(|licence| {
        (licence.bundle_id.is_some() || (allow_dependency && licence.is_dependency))
            && licence.tenant_id == tenant.id
    });
    Ok(admitted.then_some(tenant))
}

async fn fetch_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Option<TenantRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, owner_key, archived_at FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
}

/// `_audit_cross_owner`: this service has no audit table, so the log line IS the record.
///
/// `pub(crate)` because the other owner-scoped authorizers (per-app settings, tenant writes)
/// emit the same record; this module is its home.
pub(crate) fn audit_cross_owner(
    user: &User,
    action: &str,
    tenant: Option<&TenantRow>,
    detail: &str,
) {
    tracing::warn!(
        action = %action,
        tenant = ?tenant.map(|tenant| tenant.id),
        tenant_owner = ?tenant.map(|tenant| tenant.owner_key.as_str()),
        caller_owner = ?user.owner_key.as_deref(),
        azp = ?user.azp,
        realm = ?user.realm(),
        sub = %user.sub,
        detail = %detail,
        at = %Utc::now().to_rfc3339(),
        "cross_owner_action"
    );
}

/// A lookup fault while authorizing. 500, never a pass — the reference's driver exception
/// escapes to the same status.
fn database_fault(error: sqlx::Error, operation: &'static str) -> Refusal {
    tracing::error!(%error, operation, "issuance authorization database fault");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}
