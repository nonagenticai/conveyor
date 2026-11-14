//! Tenant access grants and per-tenant offering provisioning: six of the nineteen tenant
//! operations (#45).
//!
//! | Route | Reference |
//! |---|---|
//! | `PUT /tenants/{name_or_id}/grants/{owner_key}` | `main.py::put_tenant_grant` |
//! | `DELETE /tenants/{name_or_id}/grants/{owner_key}` | `main.py::delete_tenant_grant` |
//! | `GET /tenants/{name_or_id}/grants` | `main.py::list_tenant_grants` |
//! | `GET /tenants/{name_or_id}/offerings` | `main.py::list_tenant_offerings` |
//! | `POST /tenants/{name_or_id}/offerings/{app_key}/{code}` | `main.py::provision_tenant_offering` |
//! | `DELETE /tenants/{name_or_id}/offerings/{app_key}/{code}` | `main.py::unprovision_tenant_offering` |
//!
//! Authorization is the reference's: grants are OWNER-ONLY (`_resolve_owned_tenant`), the
//! offerings write pair is `_resolve_tenant_for_app_setting`, and the offerings read mirrors
//! `get_tenant` (`_read_tenant_for_caller`). The wire shapes are the four locked schemas,
//! explicit nulls included.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::db::tenant_offerings::{self, TenantGrantRow};
use crate::db::tenants_crud;
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `TenantGrantResponse` — `TenantAppGrant.to_dict`'s five properties.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantGrantResponse {
    pub tenant_id: String,
    pub grantee_owner_key: String,
    pub granted_by: String,
    pub created_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// One per-(tenant, product) posture row. `offering_code: None` means UNKNOWN provenance, never
/// "no offering"; `offering_provenance` is the same fact said out loud.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantOfferingRecord {
    pub app_key: String,
    pub app_name: String,
    pub byo_mode: String,
    pub offering_code: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub offering_provenance: String,
}

/// What an offering provision actually materialized: the two flags are the honest report, each
/// false when the offering carried nothing to write on that axis.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantOfferingProvisionResponse {
    pub tenant_id: String,
    pub tenant_name: String,
    pub app_key: String,
    pub code: String,
    pub byo_mode: String,
    pub markup_rule_provisioned: bool,
    pub access_charge_provisioned: bool,
}

/// Result of unprovisioning an offering from a tenant. Idempotent.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantOfferingUnprovisionResponse {
    pub tenant: String,
    pub app_key: String,
    pub code: String,
    pub posture_cleared: bool,
    pub note: String,
}

/// The reference's fixed disclosure on every unprovision answer.
const UNPROVISION_NOTE: &str = "billing markup rule left in place (no billing delete endpoint)";

fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

fn grant_response(row: TenantGrantRow) -> TenantGrantResponse {
    TenantGrantResponse {
        tenant_id: row.tenant_id.to_string(),
        grantee_owner_key: row.grantee_owner_key,
        granted_by: row.granted_by,
        created_at: Some(py_iso(row.created_at)),
        revoked_at: row.revoked_at.map(py_iso),
    }
}

/// `_grant_target` — the OWNER-ONLY tenant resolution plus the grantee-owner existence check.
///
/// A grantee (non-owner) is 403 `tenant_owner_only` before this returns; an unknown grantee key is
/// 404 `owner_not_found`.
async fn grant_target(
    pool: &sqlx::PgPool,
    user: &User,
    name_or_id: &str,
    owner_key: &str,
) -> Result<tenants_crud::TenantRow, Refusal> {
    let tenant = tenants_crud::resolve_owned_tenant(
        pool,
        user,
        name_or_id,
        &format!("tenant_grant:{owner_key}"),
    )
    .await?;
    if !tenant_offerings::owner_exists(pool, owner_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to resolve grantee owner");
            internal_error()
        })?
    {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            json!({"error": "owner_not_found", "owner_key": owner_key}),
        ));
    }
    Ok(tenant)
}

/// `PUT /tenants/{name_or_id}/grants/{owner_key}` — let another app act on this tenant.
#[utoipa::path(
    put,
    path = "/tenants/{name_or_id}/grants/{owner_key}",
    tag = "tenants",
    operation_id = "put_tenant_grant",
    summary = "Put Tenant Grant",
    description = "Let ANOTHER app act on this tenant. Owner (or break-glass) only.\n\nThe grantee may then read the tenant BY ID and mint / revoke licences for the apps it issues. It may not rename, archive, change BYO mode, or grant onward. Idempotent: an existing active grant is returned with 200.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("owner_key" = String, Path, description = "Owner Key"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantGrantResponse),
        (status = 201, body = TenantGrantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn put_tenant_grant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, owner_key)): Path<(String, String)>,
) -> Result<(StatusCode, Json<TenantGrantResponse>), Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant = grant_target(pool, &user, &name_or_id, &owner_key).await?;
    if owner_key == tenant.owner_key {
        return Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({
                "error": "grant_to_owner",
                "message": "the tenant's owning app needs no grant on its own tenant",
            }),
        ));
    }
    let granted_by = match user.azp.as_deref() {
        Some(azp) => format!("{}/{}", user.realm().unwrap_or_default(), azp),
        None => user.sub.clone(),
    };
    let (grant, created) =
        tenant_offerings::put_tenant_grant(pool, tenant.id, &owner_key, &granted_by)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to put tenant grant");
                internal_error()
            })?;
    tracing::warn!(
        tenant = %tenant.id,
        owner = %tenant.owner_key,
        grantee = %owner_key,
        by = %granted_by,
        created,
        "tenant_grant_put"
    );
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(grant_response(grant))))
}

/// `DELETE /tenants/{name_or_id}/grants/{owner_key}` — revoke another app's grant.
#[utoipa::path(
    delete,
    path = "/tenants/{name_or_id}/grants/{owner_key}",
    tag = "tenants",
    operation_id = "delete_tenant_grant",
    summary = "Delete Tenant Grant",
    description = "Revoke another app's grant on this tenant. Owner (or break-glass) only.\n\nStamps `revoked_at` (history kept). Licences the grantee already minted are NOT revoked — revoke those explicitly; this only stops further issuance.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("owner_key" = String, Path, description = "Owner Key"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn delete_tenant_grant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, owner_key)): Path<(String, String)>,
) -> Result<StatusCode, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant = grant_target(pool, &user, &name_or_id, &owner_key).await?;
    let active: Vec<TenantGrantRow> = tenant_offerings::list_tenant_grants(pool, tenant.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list tenant grants");
            internal_error()
        })?;
    let has_active = active
        .iter()
        .any(|grant| grant.grantee_owner_key == owner_key);
    let revoked = tenant_offerings::revoke_tenant_grant(pool, tenant.id, &owner_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to revoke tenant grant");
            internal_error()
        })?;
    if !has_active || !revoked {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            json!({"error": "grant_not_found", "owner_key": owner_key}),
        ));
    }
    tracing::warn!(
        tenant = %tenant.id,
        owner = %tenant.owner_key,
        grantee = %owner_key,
        realm = ?user.realm(),
        azp = ?user.azp,
        "tenant_grant_revoked"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /tenants/{name_or_id}/grants` — the ACTIVE grants on a tenant.
#[utoipa::path(
    get,
    path = "/tenants/{name_or_id}/grants",
    tag = "tenants",
    operation_id = "list_tenant_grants",
    summary = "List Tenant Grants",
    description = "The ACTIVE grants on a tenant. Owner (or break-glass) only.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<TenantGrantResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_tenant_grants(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
) -> Result<Json<Vec<TenantGrantResponse>>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenants_crud::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_grant_list").await?;
    let grants = tenant_offerings::list_tenant_grants(pool, tenant.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list tenant grants");
            internal_error()
        })?;
    Ok(Json(grants.into_iter().map(grant_response).collect()))
}

/// `GET /tenants/{name_or_id}/offerings` — what this tenant has been provisioned, per product.
#[utoipa::path(
    get,
    path = "/tenants/{name_or_id}/offerings",
    tag = "offerings",
    operation_id = "list_tenant_offerings",
    summary = "List Tenant Offerings",
    description = "What this tenant has been provisioned, per product.\n\nRequires 'license-admin' or 'license-reader'. This service owns only HALF of what provisioning writes: the per-product posture row here, and — via S2S — the tenant's markup rule and access charge in billing_service. `offering_code: null` means the provenance is UNKNOWN, not 'no offering'. An empty list means the tenant carries NO per-product overrides.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<TenantOfferingRecord>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_tenant_offerings(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
) -> Result<Json<Vec<TenantOfferingRecord>>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    // `_read_tenant_for_caller`, exactly as `get_tenant` implements it.
    let tenant = if user.has_realm_role("license-admin")
        && (user.owner_key.is_some() || user.cross_owner)
    {
        tenants_crud::resolve_for_licensed_admin(pool, &user, &name_or_id, "tenant_read").await?
    } else {
        let caller = get_tenant_id(&state, &user).await?;
        tenants_crud::read_own_tenant(pool, &caller, &name_or_id).await?
    };
    let rows = tenant_offerings::list_offerings(pool, tenant.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list tenant offerings");
            internal_error()
        })?
        .ok_or_else(|| Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found"))?;
    Ok(Json(
        rows.into_iter()
            .map(|row| TenantOfferingRecord {
                app_key: row.app_key,
                app_name: row.app_name,
                byo_mode: row.byo_mode,
                offering_provenance: if row.offering_code.is_some() {
                    "recorded".to_owned()
                } else {
                    "unknown".to_owned()
                },
                offering_code: row.offering_code,
                created_at: Some(py_iso(row.created_at)),
                updated_at: Some(py_iso(row.updated_at)),
            })
            .collect(),
    ))
}

/// `POST /tenants/{name_or_id}/offerings/{app_key}/{code}` — provision an offering onto a tenant.
#[utoipa::path(
    post,
    path = "/tenants/{name_or_id}/offerings/{app_key}/{code}",
    tag = "offerings",
    operation_id = "provision_tenant_offering",
    summary = "Provision Tenant Offering",
    description = "Provision an offering onto a tenant (materialize its usage policy).\n\nRequires 'license-admin' or 'license-manager' AND the app: the tenant's owner must issue it or have brought it in with its own bundle; a grantee must issue it. Writes the per-(tenant, product) posture override, pushes the tenant's billing markup rule when the offering carries a markup/surcharge, and the billing access charge when the offering is priced or bundles an allowance. A markup push that billing refuses or cannot be reached is recorded and does not fail the request (`markup_rule_provisioned: false`). A failed access-charge write restores the prior posture and answers 502.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("app_key" = String, Path, description = "App Key"),
        ("code" = String, Path, description = "Code"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantOfferingProvisionResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn provision_tenant_offering(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, app_key, code)): Path<(String, String, String)>,
) -> Result<Json<TenantOfferingProvisionResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let result = tenant_offerings::provision_tenant_offering(
        pool,
        &user,
        &name_or_id,
        &app_key,
        &code,
        state.metering.as_deref(),
    )
    .await?;
    Ok(Json(TenantOfferingProvisionResponse {
        tenant_id: result.tenant_id,
        tenant_name: result.tenant_name,
        app_key: result.app_key,
        code: result.code,
        byo_mode: result.byo_mode,
        markup_rule_provisioned: result.markup_rule_provisioned,
        access_charge_provisioned: result.access_charge_provisioned,
    }))
}

/// `DELETE /tenants/{name_or_id}/offerings/{app_key}/{code}` — clear the per-product posture.
#[utoipa::path(
    delete,
    path = "/tenants/{name_or_id}/offerings/{app_key}/{code}",
    tag = "offerings",
    operation_id = "unprovision_tenant_offering",
    summary = "Unprovision Tenant Offering",
    description = "Unprovision an offering from a tenant: clear the per-(tenant, product) posture override, reverting to the tenant-wide `byo_mode`. Requires 'license-admin' or 'license-manager' AND the app. Idempotent. The billing markup rule is left in place — billing_service has no markup-rule delete endpoint — and re-provisioning overwrites it.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("app_key" = String, Path, description = "App Key"),
        ("code" = String, Path, description = "Code"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = TenantOfferingUnprovisionResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn unprovision_tenant_offering(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((name_or_id, app_key, code)): Path<(String, String, String)>,
) -> Result<Json<TenantOfferingUnprovisionResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let cleared =
        tenant_offerings::unprovision_tenant_offering(pool, &user, &name_or_id, &app_key, &code)
            .await?;
    Ok(Json(TenantOfferingUnprovisionResponse {
        tenant: name_or_id,
        app_key,
        code,
        posture_cleared: cleared,
        note: UNPROVISION_NOTE.to_owned(),
    }))
}
