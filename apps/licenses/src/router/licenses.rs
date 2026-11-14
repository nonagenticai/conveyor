//! `GET /licenses/active` — the entitlement read the conveyor and `license-authz` use.
//!
//! AUTHORIZATION is two layers, in this order, and both are the reference's:
//!
//! 1. `require_internal_or_reader` — an internal-service role or the read-only `license-reader`.
//! 2. `_enforce_own_tenant_scope` — the SOLE cross-tenant guard on this route. `tenant_id` is a
//!    caller-controlled query parameter, so the role gate authorizes nothing about *whose*
//!    entitlement is returned; a caller without a delegation role may name only its own tenant.
//! 3. The optional `realm` parameter can only NARROW: a realm owned by an app must own or be
//!    granted the tenant, else 403 `realm_owner_mismatch`.

use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{
    INTERNAL_SERVICE_ROLES, User, enforce_own_tenant_scope, require_internal_or_reader,
};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::router::tokens::TokenIntrospectResponse;
use crate::state::AppState;

/// The route's query parameters. Required fields are `Option` here so a missing one produces the
/// reference's 422 body rather than axum's 400.
#[derive(Debug, Deserialize)]
pub struct ActiveQuery {
    /// Tenant identifier
    pub tenant_id: Option<String>,
    /// App key, e.g. 'n8n-gerard', 'kanban'
    pub app_key: Option<String>,
    /// OPTIONAL seat selector: the Keycloak `sub` claim OR the local subject id.
    pub subject_id: Option<String>,
    /// OPTIONAL Keycloak realm of the end user the S2S caller resolves for.
    pub realm: Option<String>,
}

/// FastAPI's 422 body for one missing query field.
fn missing(field: &str) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": "missing",
            "loc": ["query", field],
            "msg": "Field required",
            "input": null,
        }]),
    )
}

/// `GET /licenses/active`.
#[utoipa::path(
    get,
    path = "/licenses/active",
    tag = "licenses",
    summary = "Get Active License",
    description = "Resolve the active license for (tenant_id, app_key). Response schema matches /license/introspect so callers can share a client.",
    params(
        ("tenant_id" = String, Query, description = "Tenant identifier"),
        ("app_key" = String, Query, description = "App key, e.g. 'n8n-gerard', 'kanban'"),
        ("subject_id" = Option<String>, Query, description = "OPTIONAL seat selector. Accepts EITHER the Keycloak 'sub' claim (what license-authz holds) OR the local Subject.id UUID. When it resolves to a subject holding an active license for (tenant_id, app_key), that exact license is returned; otherwise the deterministic tenant-level pick is used. Never causes a denial, and never widens the caller's tenant scope."),
        ("realm" = Option<String>, Query, description = "OPTIONAL Keycloak realm of the END USER the S2S caller (license-authz) is resolving for (migration 072, realm-per-app P0). When that realm has a `realm_owner` row, the tenant must be owned by that owner or carry an active `tenant_app_grant` to it, else 403 `realm_owner_mismatch`. A realm with no row, or no parameter, changes nothing. It can only NARROW what the caller's own authority admits — never widen it."),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TokenIntrospectResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn active(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<ActiveQuery>,
) -> Result<Json<TokenIntrospectResponse>, Refusal> {
    let Some(tenant_id) = query.tenant_id.as_deref().filter(|value| !value.is_empty()) else {
        return Err(missing("tenant_id"));
    };
    let Some(app_key) = query.app_key.as_deref().filter(|value| !value.is_empty()) else {
        return Err(missing("app_key"));
    };

    require_internal_or_reader(&user)?;
    enforce_own_tenant_scope(
        &state,
        &user,
        tenant_id,
        &INTERNAL_SERVICE_ROLES,
        "Not authorized to read another tenant's active license",
    )
    .await?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(Refusal::detail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
        ));
    };

    // Migration 072: the forwarded realm can only narrow the caller's own authority.
    if let Some(realm) = query.realm.as_deref().filter(|value| !value.is_empty()) {
        match crate::db::owners::resolve_realm_owner(pool, realm).await {
            Ok(Some(owner)) => {
                if !crate::db::owners::owner_can_access_tenant(pool, &owner, tenant_id)
                    .await
                    .unwrap_or(false)
                {
                    return Err(Refusal::detail(
                        StatusCode::FORBIDDEN,
                        crate::auth::realm_owner_mismatch_detail(),
                    ));
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, %realm, "realm_owner_resolution_failed: refused");
                return Err(Refusal::detail(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({
                        "error": "realm_owner_unresolved",
                        "message": "the forwarded token realm's owner could not be resolved",
                    }),
                ));
            }
        }
    }

    let candidates = crate::db::active::candidates(pool, tenant_id, app_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Active license lookup failed");
            Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        })?;
    if candidates.is_empty() {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            json!({
                "error": "no_active_license",
                "tenant_id": tenant_id,
                "app_key": app_key,
            }),
        ));
    }

    let (picked, subject_matched) =
        crate::db::active::select(&candidates, query.subject_id.as_deref());
    let ambiguous = candidates.len() > 1 && !subject_matched;
    if ambiguous {
        tracing::warn!(
            candidates = candidates.len(),
            %tenant_id,
            %app_key,
            license_id = %picked.license_id,
            subject_supplied = query.subject_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "ambiguous_active_license: deterministic pick"
        );
    }

    Ok(Json(TokenIntrospectResponse {
        active: true,
        tenant_id: Some(picked.tenant_id.to_string()),
        license_id: Some(picked.license_id.to_string()),
        subject_id: Some(picked.subject_id.to_string()),
        subject_keycloak_sub: None,
        subject_actor_sub: None,
        subject_kind: None,
        byo_mode: None,
        app: Some(picked.app.clone()),
        plan: Some(picked.plan.clone()),
        status: Some(picked.status.clone()),
        starts_at: Some(picked.starts_at.clone()),
        expires_at: picked.expires_at.clone(),
        token_expires_at: None,
        region_id: picked.region_id.clone(),
        metadata: picked.metadata.clone(),
        reason: None,
        roles: None,
        candidate_count: Some(candidates.len() as i64),
        ambiguous: Some(ambiguous),
        // None (not false) when no subject was asked for — "not applicable" is not "no match".
        subject_match: query
            .subject_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|_| subject_matched),
    }))
}
