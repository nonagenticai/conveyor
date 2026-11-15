//! The five tenant BYO-mode operations.
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Route | Reference |
//! |---|---|
//! | `GET /tenants/{name_or_id}/byo-mode` | `main.py::get_tenant_byo_mode` |
//! | `PATCH /tenants/{name_or_id}/byo-mode` | `main.py::set_tenant_byo_mode` |
//! | `GET /tenants/{name_or_id}/apps/{app_key}/byo-mode` | `main.py::get_tenant_app_byo_mode` |
//! | `PUT /tenants/{name_or_id}/apps/{app_key}/byo-mode` | `main.py::set_tenant_app_byo_mode` |
//! | `DELETE /tenants/{name_or_id}/apps/{app_key}/byo-mode` | `main.py::clear_tenant_app_byo_mode` |
//!
//! ⛔ The tenant-wide GET is AUTH-EXEMPT FOR UUID SELECTORS ONLY — `worker_management` calls
//! it on the trusted network with no token and FAILS OPEN on a 401. The exemption itself lives
//! in `auth::authenticate` (`_BYO_MODE_OPEN_ROUTES`); the handler here must therefore work
//! without a `User`, and the write variants plus the per-app paths keep their role gates.
//!
//! A NAME selector is NOT exempt: it takes the normal auth path, so a tokenless name lookup
//! is the middleware's 401 (and the reference's `byo_mode_open_route_name_lookup` warning),
//! while an authenticated name lookup resolves as before.

use axum::Extension;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::db::tenant_byo::{
    self, RESOLUTION_FAULT, RESOLUTION_ROW, SelectorOutcome, TenantRecord,
};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `OWNER_WRITE_ROLES`.
const OWNER_WRITE_ROLES: [&str; 2] = ["license-admin", "license-manager"];
/// `BYO_MODES`.
const BYO_MODES: [&str; 3] = ["local_allowed", "byo_required", "platform_only"];

/// `GET /tenants/{name_or_id}/byo-mode` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TenantByoModeQuery {
    /// Resolve the PER-PRODUCT posture for this app; empty/absent is the tenant-wide value.
    #[param(nullable = true)]
    pub app: Option<String>,
    /// `fail_closed` (default) answers 200 with the fabricated `byo_required` and
    /// `resolution: "fault"`; `error` answers 503 `byo_mode_unresolved` instead.
    #[param(
        value_type = String,
        required = false,
        default = "fail_closed",
        pattern = "^(error|fail_closed)$"
    )]
    pub on_unresolved: Option<String>,
}

/// `TenantByoModeUpdate`: `{"byo_mode": "…"}` — `byo_mode` is required.
#[derive(Debug, ToSchema)]
pub struct TenantByoModeUpdate {
    /// 'local_allowed', 'byo_required', or 'platform_only'.
    #[schema(required = true)]
    pub byo_mode: String,
}

/// `TenantByoModeResponse`: the resolved posture, optionally scoped to one product.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantByoModeResponse {
    /// Canonical tenant id (UUIDv7).
    pub tenant_id: String,
    /// Tenant name.
    pub name: String,
    /// 'local_allowed', 'byo_required', or 'platform_only'.
    pub byo_mode: String,
    /// The product key the posture was resolved for; null when unscoped.
    pub app: Option<String>,
    /// `row`, `missing_row` or `fault` — WHERE `byo_mode` came from.
    #[schema(required = false, default = "row")]
    pub resolution: String,
}

/// `TenantResponse`: a canonical tenant identity, exactly as `Tenant.to_dict` emits it.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub keycloak_realm: Option<String>,
    pub byo_mode: Option<String>,
    pub kind: Option<String>,
    pub cost_centre: Option<String>,
    #[schema(inline)]
    pub budget_currency: Option<BudgetCurrency>,
    pub archived_at: Option<String>,
    pub owner_key: Option<String>,
    pub external_ref: Option<String>,
    pub created_at: Option<String>,
}

/// `BudgetCurrency` — the closed vocabulary of `ck_tenants_budget_currency`.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub enum BudgetCurrency {
    EUR,
    USD,
    CAD,
}

/// `TenantOfferingRecord`: one per-(tenant, product) posture row.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantOfferingRecord {
    pub app_key: String,
    pub app_name: String,
    pub byo_mode: String,
    /// `null` means the provenance is UNKNOWN — never "no offering".
    pub offering_code: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// `"recorded"` or `"unknown"` — the null above, said out loud.
    pub offering_provenance: String,
}

/// `TenantAppByoModeResponse`: the override just written, plus what it overwrote.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantAppByoModeResponse {
    pub tenant_id: String,
    pub tenant_name: String,
    pub app_id: String,
    pub app_key: String,
    pub byo_mode: String,
    pub offering_code: Option<String>,
    pub prior_offering_code: Option<String>,
    /// The override this upsert OVERWROTE, or null when it created the row.
    #[schema(required = true)]
    pub prior_byo_mode: Option<String>,
}

/// `TenantAppByoModeClearResponse`: the idempotent delete result.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantAppByoModeClearResponse {
    /// The tenant name or id exactly as it was addressed.
    pub tenant: String,
    pub app_key: String,
    pub cleared: bool,
}

fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

/// Validate the raw body as Pydantic's `TenantByoModeUpdate`.
fn parse_byo_update(body: &Bytes) -> Result<String, Refusal> {
    if body.is_empty() {
        return Err(body_error(json!({
            "type": "missing",
            "loc": ["body"],
            "msg": "Field required",
            "input": null,
        })));
    }
    let value: Value = serde_json::from_slice(body).map_err(|error| {
        body_error(json!({
            "type": "json_invalid",
            "loc": ["body"],
            "msg": "JSON decode error",
            "input": {},
            "ctx": {"error": error.to_string()},
        }))
    })?;
    let Some(object) = value.as_object() else {
        return Err(body_error(json!({
            "type": "model_attributes_type",
            "loc": ["body"],
            "msg": "Input should be a valid dictionary or object to extract fields from",
            "input": value,
        })));
    };
    match object.get("byo_mode") {
        None => Err(body_error(json!({
            "type": "missing",
            "loc": ["body", "byo_mode"],
            "msg": "Field required",
            "input": value,
        }))),
        Some(Value::String(mode)) => Ok(mode.clone()),
        Some(other) => Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", "byo_mode"],
            "msg": "Input should be a valid string",
            "input": other,
        }))),
    }
}

/// The reference's in-handler 422 for an unknown `byo_mode` value.
fn invalid_byo_mode() -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        "byo_mode must be 'local_allowed', 'byo_required', or 'platform_only'",
    )
}

fn tenant_response(tenant: TenantRecord) -> TenantResponse {
    TenantResponse {
        id: tenant.id.to_string(),
        name: tenant.name,
        keycloak_realm: tenant.keycloak_realm,
        byo_mode: tenant.byo_mode,
        kind: tenant.kind,
        cost_centre: tenant.cost_centre,
        budget_currency: tenant.budget_currency.as_deref().and_then(|currency| {
            match currency.trim() {
                "EUR" => Some(BudgetCurrency::EUR),
                "USD" => Some(BudgetCurrency::USD),
                "CAD" => Some(BudgetCurrency::CAD),
                _ => None,
            }
        }),
        archived_at: tenant.archived_at.map(py_iso),
        owner_key: tenant.owner_key,
        external_ref: tenant.external_ref,
        created_at: tenant.created_at.map(py_iso),
    }
}

/// `_read_tenant_for_caller`: the tenant-READ rule shared by the per-app GET.
///
/// A non-admin may resolve only its OWN tenant (404 otherwise, never 403 — existence is not
/// leaked); an owned client holding `license-admin` uses the scoped resolver.
async fn read_tenant_for_caller(
    state: &AppState,
    user: &User,
    selector: &str,
) -> Result<TenantRecord, Refusal> {
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    if user.has_realm_role("license-admin") && (user.owner_key.is_some() || user.cross_owner) {
        return tenant_byo::resolve_for_caller(pool, user, selector, "tenant_read").await;
    }
    let caller_tenant = get_tenant_id(state, user).await?;
    let tenant = match Uuid::try_parse(selector) {
        Ok(id) => tenant_byo::get_tenant(pool, id).await.map_err(|error| {
            tracing::error!(%error, "Tenant not found");
            internal_error()
        })?,
        Err(_) => match Uuid::try_parse(&caller_tenant) {
            Ok(id) => tenant_byo::get_tenant(pool, id)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "Tenant not found");
                    internal_error()
                })?
                .filter(|tenant| tenant.name == selector),
            Err(_) => None,
        },
    };
    match tenant {
        Some(tenant) if tenant.id.to_string() == caller_tenant => Ok(tenant),
        _ => Err(Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found")),
    }
}

/// `GET /tenants/{name_or_id}/byo-mode` — the tenant-wide or per-product BYO posture.
///
/// Intentionally tokenless for UUID selectors only: `worker_management`'s trusted-network
/// auth path calls it with no token to enforce `byo_required` on S2S worker runs, and it
/// FAILS OPEN on any non-200. A name selector goes through normal auth.
#[utoipa::path(
    get,
    path = "/tenants/{name_or_id}/byo-mode",
    tag = "tenants",
    operation_id = "get_tenant_byo_mode",
    summary = "Get Tenant Byo Mode",
    description = "Return a tenant's BYO entitlement by UUID or name (internal S2S lookup).\n\nIntentionally tokenless FOR UUID SELECTORS ONLY. A name selector takes the normal auth path.\n\n`?app=<key>` resolves the PER-PRODUCT posture (the override row when one exists, else the tenant-wide value). Three separable outcomes: resolved -> 200 with `resolution: \"row\"`; no such tenant -> 404 `tenant_not_found_in_registry`; selector cannot name one -> 400 `malformed_tenant_selector`; could not be determined -> 503 `byo_mode_unresolved`, unless the caller passed `on_unresolved=fail_closed`.",
    params(
        ("name_or_id" = String, Path, description = "Tenant name or id"),
        TenantByoModeQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantByoModeResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse),
        (status = 503, body = ErrorResponse)
    )
)]
pub async fn get_tenant_byo_mode(
    State(state): State<AppState>,
    Path(name_or_id): Path<String>,
    Query(query): Query<TenantByoModeQuery>,
) -> Result<Json<TenantByoModeResponse>, Refusal> {
    let on_unresolved = query.on_unresolved.as_deref().unwrap_or("fail_closed");
    if !matches!(on_unresolved, "error" | "fail_closed") {
        return Err(body_error(json!({
            "type": "string_pattern_mismatch",
            "loc": ["query", "on_unresolved"],
            "msg": "String should match pattern '^(error|fail_closed)$'",
            "input": on_unresolved,
            "ctx": {"pattern": "^(error|fail_closed)$"},
        })));
    }
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };

    let lookup = tenant_byo::resolve_tenant_selector(pool, &name_or_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "byo_mode_selector_lookup_failed");
            internal_error()
        })?;
    if lookup.outcome == SelectorOutcome::Malformed {
        tracing::warn!(selector = %name_or_id, app = ?query.app, "byo_mode_selector_malformed");
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            json!({
                "error": "malformed_tenant_selector",
                "remedy": "the selector must be a tenant UUID or a name matching ^[a-z0-9][a-z0-9-]*$",
            }),
        ));
    }
    let Some(tenant) = lookup.tenant else {
        // This 404 means byo_required went UNENFORCED for that run (the S2S caller fails
        // open on any non-200), so it must not be silent.
        tracing::warn!(selector = %name_or_id, app = ?query.app, "byo_mode_tenant_unresolved");
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            json!({
                "error": "tenant_not_found_in_registry",
                "tenant_selector": name_or_id,
            }),
        ));
    };

    let mut byo_mode = tenant
        .byo_mode
        .clone()
        .filter(|mode| !mode.is_empty())
        .unwrap_or_else(|| "byo_required".to_owned());
    let mut resolution = RESOLUTION_ROW.to_owned();
    if let Some(app) = query.app.as_deref().filter(|app| !app.is_empty()) {
        let posture = tenant_byo::lookup_tenant_posture(pool, tenant.id, Some(app)).await;
        byo_mode = posture.byo_mode;
        resolution = posture.resolution;
        if resolution == RESOLUTION_FAULT && on_unresolved != "fail_closed" {
            tracing::error!(
                tenant = %tenant.id,
                app = %app,
                "byo_mode_unresolved: refusing to answer 200 with a fail-closed guess"
            );
            return Err(Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({
                    "error": "byo_mode_unresolved",
                    "tenant_id": tenant.id.to_string(),
                    "app": app,
                    "remedy": "retry; this is not a verdict about the tenant",
                }),
            ));
        }
    }

    Ok(Json(TenantByoModeResponse {
        tenant_id: tenant.id.to_string(),
        name: tenant.name,
        byo_mode,
        app: query.app,
        resolution,
    }))
}

/// `PATCH /tenants/{name_or_id}/byo-mode` — set a tenant's BYO LLM entitlement.
///
/// Requires 'license-admin' or 'license-manager', and the caller must OWN the tenant
/// (a grantee gets 403 `tenant_owner_only`).
#[utoipa::path(
    patch,
    path = "/tenants/{name_or_id}/byo-mode",
    tag = "tenants",
    operation_id = "set_tenant_byo_mode",
    summary = "Set Tenant Byo Mode",
    description = "Set a tenant's BYO LLM entitlement (commercial tenants -> 'byo_required').\n\nRequires 'license-admin' or 'license-manager', and the caller must OWN the tenant (a grantee gets 403 `tenant_owner_only`).",
    params(
        ("name_or_id" = String, Path, description = "Tenant name or id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantByoModeUpdate,
    responses(
        (status = 200, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn set_tenant_byo_mode(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
    body: Bytes,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(&user, &OWNER_WRITE_ROLES)?;
    let mode = parse_byo_update(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenant_byo::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_byo_mode").await?;
    if !BYO_MODES.contains(&mode.as_str()) {
        return Err(invalid_byo_mode());
    }
    let Some(tenant) = tenant_byo::set_tenant_byo_mode(pool, tenant.id, &mode)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to set tenant byo mode");
            internal_error()
        })?
    else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found"));
    };
    tracing::info!(tenant = %tenant.id, name = %tenant.name, mode = %mode, "Tenant byo_mode set");
    Ok(Json(tenant_response(tenant)))
}

/// `GET /tenants/{name_or_id}/apps/{app_key}/byo-mode` — the per-product override row.
///
/// This is NOT the effective posture: a 404 means "no override", after which the product
/// resolves to the tenant-wide value. Requires 'license-admin' or 'license-reader'.
#[utoipa::path(
    get,
    path = "/tenants/{name_or_id}/apps/{app_key}/byo-mode",
    tag = "tenants",
    operation_id = "get_tenant_app_byo_mode",
    summary = "Get Tenant App Byo Mode",
    description = "The per-product BYO override for ONE `(tenant, app)`, if there is one. A 404 means \"no override\"; the product then resolves to the tenant-wide `Tenant.byo_mode`.\n\n`offering_code: null` means the provenance is UNKNOWN, never \"no offering\"; `offering_provenance` states that explicitly.\n\nRequires 'license-admin' or 'license-reader'. A non-admin may read only its OWN tenant; a foreign tenant returns 404, never 403.",
    params(
        ("name_or_id" = String, Path, description = "Tenant name or id"),
        ("app_key" = String, Path, description = "Product key (App.key)"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantOfferingRecord),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn get_tenant_app_byo_mode(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, app_key)): Path<(String, String)>,
) -> Result<Json<TenantOfferingRecord>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;
    let tenant = read_tenant_for_caller(&state, &user, &name_or_id).await?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let Some(row) = tenant_byo::get_tenant_app_byo_mode(pool, tenant.id, &app_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to get tenant app byo mode");
            internal_error()
        })?
    else {
        // One status for two causes: distinguishing "no such app" from "no override" would
        // let an unprivileged caller enumerate the app catalogue.
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            "No per-product BYO override for this tenant and app",
        ));
    };
    Ok(Json(TenantOfferingRecord {
        app_key: row.app_key,
        app_name: row.app_name,
        byo_mode: row.byo_mode,
        offering_provenance: if row.offering_code.is_some() {
            "recorded".to_owned()
        } else {
            "unknown".to_owned()
        },
        offering_code: row.offering_code,
        created_at: row.created_at.map(py_iso),
        updated_at: row.updated_at.map(py_iso),
    }))
}

/// `PUT /tenants/{name_or_id}/apps/{app_key}/byo-mode` — set a per-product override.
///
/// Requires 'license-admin' or 'license-manager' AND the app: the tenant's owner must issue
/// it or have brought it in with its own bundle; a grantee must issue it.
#[utoipa::path(
    put,
    path = "/tenants/{name_or_id}/apps/{app_key}/byo-mode",
    tag = "tenants",
    operation_id = "set_tenant_app_byo_mode",
    summary = "Set Tenant App Byo Mode",
    description = "Set a PER-PRODUCT BYO posture override for `(tenant, app)`.\n\nRequires 'license-admin' or 'license-manager' AND the app: the tenant's owner must issue it or have brought it in with its own bundle; a grantee must issue it.",
    params(
        ("name_or_id" = String, Path, description = "Tenant name or id"),
        ("app_key" = String, Path, description = "Product key (App.key)"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantByoModeUpdate,
    responses(
        (status = 200, body = TenantAppByoModeResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn set_tenant_app_byo_mode(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, app_key)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<TenantAppByoModeResponse>, Refusal> {
    require_any_realm_role(&user, &OWNER_WRITE_ROLES)?;
    let mode = parse_byo_update(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant = tenant_byo::resolve_tenant_for_app_setting(
        pool,
        &user,
        &name_or_id,
        &app_key,
        "tenant_app_byo_mode",
    )
    .await?;
    if !BYO_MODES.contains(&mode.as_str()) {
        return Err(invalid_byo_mode());
    }
    let Some(result) = tenant_byo::set_tenant_app_byo_mode(pool, tenant.id, &app_key, &mode)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to set tenant app byo mode");
            internal_error()
        })?
    else {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            "Tenant or app not found",
        ));
    };
    tracing::info!(
        tenant = %result.tenant_id,
        name = %result.tenant_name,
        app = %result.app_key,
        mode = %result.byo_mode,
        "Tenant-app byo_mode set"
    );
    Ok(Json(TenantAppByoModeResponse {
        tenant_id: result.tenant_id.to_string(),
        tenant_name: result.tenant_name,
        app_id: result.app_id.to_string(),
        app_key: result.app_key,
        byo_mode: result.byo_mode,
        offering_code: result.offering_code,
        prior_offering_code: result.prior_offering_code,
        prior_byo_mode: result.prior_byo_mode,
    }))
}

/// `DELETE /tenants/{name_or_id}/apps/{app_key}/byo-mode` — remove a per-product override.
///
/// Idempotent: `cleared` is false when there was nothing to remove. Requires
/// 'license-admin' or 'license-manager' AND the app (same matrix as the PUT; a REVOKED
/// bundle licence is enough here, so offboarding can clean up).
#[utoipa::path(
    delete,
    path = "/tenants/{name_or_id}/apps/{app_key}/byo-mode",
    tag = "tenants",
    operation_id = "clear_tenant_app_byo_mode",
    summary = "Clear Tenant App Byo Mode",
    description = "Remove a per-product BYO posture override, reverting `(tenant, app)` to the tenant-wide `byo_mode`. Requires 'license-admin' or 'license-manager' AND the app (the tenant's owner must issue it or have brought it in with its own bundle; a grantee must issue it). Idempotent.",
    params(
        ("name_or_id" = String, Path, description = "Tenant name or id"),
        ("app_key" = String, Path, description = "Product key (App.key)"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantAppByoModeClearResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn clear_tenant_app_byo_mode(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, app_key)): Path<(String, String)>,
) -> Result<Json<TenantAppByoModeClearResponse>, Refusal> {
    require_any_realm_role(&user, &OWNER_WRITE_ROLES)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant = tenant_byo::resolve_tenant_for_app_setting(
        pool,
        &user,
        &name_or_id,
        &app_key,
        "tenant_app_byo_mode_clear",
    )
    .await?;
    let cleared = tenant_byo::clear_tenant_app_byo_mode(pool, tenant.id, &app_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to clear tenant app byo mode");
            internal_error()
        })?;
    tracing::info!(tenant = %name_or_id, app = %app_key, cleared, "Tenant-app byo_mode clear");
    Ok(Json(TenantAppByoModeClearResponse {
        tenant: name_or_id,
        app_key,
        cleared,
    }))
}
