//! The super-admin Platform surface: tenant list, the New-tenant wizard, and the
//! tenant-admin recovery.
//!
//! These are **routes**, not registry actions, and the reason is mechanical: the action
//! engine's external kind cannot send a service credential, and provisioning is a
//! multi-step authenticated orchestration (Keycloak user → provision → local rows → one
//! email). The andon cord is the precedent.
//!
//! Authorisation is `platform_admin` (`Session.is_super_admin`), read from the database on
//! every request. The extractor is [`CallerIdentity`] — NOT `Caller` — because a super-admin
//! is not required to be a member of any tenant, and `Caller` refuses a caller who names no
//! tenant.
//!
//! ## The create sequence, and why a retry is safe
//!
//! 1. find-or-create the Keycloak user in realm `conveyor` (idempotent by email);
//! 2. `POST /api/provision` with a DETERMINISTIC idempotency key (email + slug) — a retry is
//!    the same operation, which is why there is no attempt ledger;
//! 3. converge the local rows: tenant(id = provisioning's), invited person, `admin`
//!    membership — one scoped transaction, idempotent by tenant id;
//! 4. send ONE `UPDATE_PASSWORD` email. A failure here is a warning: the identity and the
//!    tenant exist, and the super-admin can retry the email by retrying the create.
//!
//! A failure before step 3 leaves no local row. A failure after it leaves a valid tenant
//! whose licence the periodic `licence.sync` will confirm.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use conveyor_auth::ServiceError;
use conveyor_domain::TenantId;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CallerIdentity;
use crate::request_id::RequestId;
use crate::state::AppState;

/// One tenant in the Platform list.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantView {
    /// `tenant.id` (the same id provisioning minted).
    pub id: String,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub members: i64,
    pub admins: i64,
    /// The conveyor entitlement, when one has been synced.
    pub licence: Option<LicenceView>,
}

/// The synced entitlement badge.
#[derive(Debug, Serialize, ToSchema)]
pub struct LicenceView {
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub checked_at: DateTime<Utc>,
}

/// One admin of a tenant.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminView {
    pub user_id: String,
    pub email: Option<String>,
    pub display_name: String,
    pub role: String,
    pub user_status: String,
}

/// A tenant and its admins.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantDetailView {
    pub tenant: TenantView,
    pub admins: Vec<AdminView>,
}

/// The body of the New-tenant wizard.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTenantBody {
    /// Display name.
    pub name: String,
    /// Canonical slug (`^[a-z0-9][a-z0-9-]*$`).
    pub slug: String,
    /// The first admin's email — the identity key.
    pub admin_email: String,
    /// The first admin's display name (optional; the email local part is used when absent).
    #[serde(default)]
    pub admin_name: Option<String>,
}

/// What provisioning did, as the wizard reports it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProvisioningView {
    pub status: String,
    pub replayed: bool,
    pub tenant_id: String,
    pub license_id: Option<String>,
    pub warnings: Vec<String>,
    pub partial_failures: Vec<String>,
}

/// The answer to a successful create or retry.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedTenantView {
    pub tenant: TenantView,
    pub admins: Vec<AdminView>,
    pub provisioning: ProvisioningView,
    /// True when the one set-password email was sent.
    pub email_sent: bool,
}

/// The body of the recovery invite.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InviteAdminBody {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// The answer to a recovery invite.
#[derive(Debug, Serialize, ToSchema)]
pub struct InviteAdminView {
    pub invite_id: String,
    pub email: String,
    pub email_sent: bool,
}

/// `GET /api/platform/tenants` — every tenant, newest first.
#[utoipa::path(
    get,
    path = "/api/platform/tenants",
    tag = "platform",
    responses((status = 200, description = "Every tenant", body = Vec<TenantView>))
)]
pub async fn list(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
) -> Result<Json<Vec<TenantView>>, ApiError> {
    require_super_admin(&identity)?;
    let rows = conveyor_store::platform::list(&state.pool, &state.config.app_key)
        .await
        .map_err(platform_error)?;
    Ok(Json(rows.into_iter().map(tenant_view).collect()))
}

/// `GET /api/platform/tenants/{id}` — one tenant and its admins.
#[utoipa::path(
    get,
    path = "/api/platform/tenants/{id}",
    tag = "platform",
    params(("id" = String, Path, description = "Tenant id")),
    responses(
        (status = 200, description = "The tenant", body = TenantDetailView),
        (status = 404, description = "No such tenant")
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
    Path(id): Path<Uuid>,
) -> Result<Json<TenantDetailView>, ApiError> {
    require_super_admin(&identity)?;
    let tenant = tenant_id(id)?;
    let Some(found) = conveyor_store::platform::detail(&state.pool, tenant, &state.config.app_key)
        .await
        .map_err(platform_error)?
    else {
        return Err(ApiError::not_found("tenant"));
    };
    Ok(Json(TenantDetailView {
        tenant: tenant_view(found.summary),
        admins: found.admins.into_iter().map(admin_view).collect(),
    }))
}

/// `POST /api/platform/tenants` — provision a tenant.
#[utoipa::path(
    post,
    path = "/api/platform/tenants",
    tag = "platform",
    request_body = CreateTenantBody,
    responses(
        (status = 201, description = "Provisioned", body = CreatedTenantView),
        (status = 409, description = "The slug is taken"),
        (status = 503, description = "Provisioning is not configured")
    )
)]
pub async fn create(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
    request_id: axum::Extension<RequestId>,
    Json(body): Json<CreateTenantBody>,
) -> Result<(StatusCode, Json<CreatedTenantView>), ApiError> {
    require_super_admin(&identity)?;
    let name = required(&body.name, "name")?;
    let slug = validate_slug(&body.slug)?;
    let email = validate_email(&body.admin_email)?;
    let admin_name = body
        .admin_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(email));

    // A local slug collision is a clear 409 before any external call.
    if conveyor_store::tenants::id_by_slug(&state.pool, slug)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .is_some()
    {
        return Err(ApiError::conflict(format!(
            "a tenant with slug {slug:?} already exists"
        )));
    }

    let provisioning = state
        .services
        .provisioning
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("provisioning is not configured"))?;
    let keycloak = state
        .services
        .keycloak
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("keycloak admin is not configured"))?;

    let user = keycloak
        .ensure_user(email, admin_name)
        .await
        .map_err(service_error)?;

    let request = conveyor_auth::ProvisionRequest::new(email, admin_name, slug, user.id)
        .map_err(service_error)?;
    let result = provisioning
        .provision(&request)
        .await
        .map_err(service_error)?;
    let tenant = result.tenant().map_err(|error| {
        ApiError::internal(format!(
            "provisioning returned an unusable tenant id: {error}"
        ))
    })?;

    conveyor_store::platform::create(
        &state.pool,
        conveyor_store::platform::NewTenant {
            tenant,
            slug,
            name,
            email,
            display_name: admin_name,
            actor: identity.principal.user_id.as_uuid(),
            request_id: Some(request_id.0.0.as_str()),
        },
    )
    .await
    .map_err(platform_error)?;

    let email_sent = match keycloak.send_update_password_email(user.id).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "the set-password email failed; the tenant is still usable");
            false
        }
    };

    // The licence row is the synced, authoritative value — never hand-written here. A
    // failure is not fatal: the tenant exists and the periodic `licence.sync` retries.
    if let Some(sync) = &state.services.licence_sync {
        match sync.fetch(tenant, &state.config.app_key).await {
            Ok(outcome) => {
                let update = conveyor_store::licence::LicenceUpsert {
                    status: outcome.status,
                    expires_at: outcome.expires_at,
                    raw: outcome.raw,
                };
                if let Err(error) = conveyor_store::licence::upsert(
                    &state.pool,
                    tenant,
                    &state.config.app_key,
                    &update,
                )
                .await
                {
                    tracing::warn!(%error, "licence upsert after provisioning failed; the job will retry");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "licence sync after provisioning failed; the job will retry");
            }
        }
    }

    let view = created_view(&state, tenant, &result, email_sent).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// `POST /api/platform/tenants/{id}/invite-admin` — the lost-all-admins recovery.
#[utoipa::path(
    post,
    path = "/api/platform/tenants/{id}/invite-admin",
    tag = "platform",
    params(("id" = String, Path, description = "Tenant id")),
    request_body = InviteAdminBody,
    responses(
        (status = 201, description = "Invited", body = InviteAdminView),
        (status = 404, description = "No such tenant")
    )
)]
pub async fn invite_admin(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
    request_id: axum::Extension<RequestId>,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteAdminBody>,
) -> Result<(StatusCode, Json<InviteAdminView>), ApiError> {
    require_super_admin(&identity)?;
    let tenant = tenant_id(id)?;
    let email = validate_email(&body.email)?;
    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(email));

    let keycloak = state
        .services
        .keycloak
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("keycloak admin is not configured"))?;
    let user = keycloak
        .ensure_user(email, name)
        .await
        .map_err(service_error)?;

    let invite_id = conveyor_store::platform::invite_admin(
        &state.pool,
        tenant,
        email,
        identity.principal.user_id.as_uuid(),
        Some(request_id.0.0.as_str()),
    )
    .await
    .map_err(platform_error)?;

    let email_sent = match keycloak.send_update_password_email(user.id).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "the set-password email failed; the invite still binds at sign-in");
            false
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(InviteAdminView {
            invite_id: invite_id.to_string(),
            email: email.to_owned(),
            email_sent,
        }),
    ))
}

/// `POST /api/platform/tenants/{id}/retry` — re-run provisioning for a tenant whose
/// licence is pending.
#[utoipa::path(
    post,
    path = "/api/platform/tenants/{id}/retry",
    tag = "platform",
    params(("id" = String, Path, description = "Tenant id")),
    responses(
        (status = 200, description = "Re-run", body = CreatedTenantView),
        (status = 404, description = "No such tenant")
    )
)]
pub async fn retry(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
    request_id: axum::Extension<RequestId>,
    Path(id): Path<Uuid>,
) -> Result<Json<CreatedTenantView>, ApiError> {
    require_super_admin(&identity)?;
    let tenant = tenant_id(id)?;
    let Some(found) = conveyor_store::platform::detail(&state.pool, tenant, &state.config.app_key)
        .await
        .map_err(platform_error)?
    else {
        return Err(ApiError::not_found("tenant"));
    };
    let Some((_user_id, email, display_name)) =
        conveyor_store::platform::first_admin_email(&state.pool, tenant)
            .await
            .map_err(platform_error)?
    else {
        return Err(ApiError::conflict(
            "this tenant has no admin email to provision with; invite an admin first",
        ));
    };

    let provisioning = state
        .services
        .provisioning
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("provisioning is not configured"))?;
    let keycloak = state
        .services
        .keycloak
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("keycloak admin is not configured"))?;

    let user = keycloak
        .ensure_user(&email, &display_name)
        .await
        .map_err(service_error)?;
    let request =
        conveyor_auth::ProvisionRequest::new(&email, &display_name, &found.summary.slug, user.id)
            .map_err(service_error)?;
    let result = provisioning
        .provision(&request)
        .await
        .map_err(service_error)?;

    conveyor_store::platform::create(
        &state.pool,
        conveyor_store::platform::NewTenant {
            tenant,
            slug: &found.summary.slug,
            name: &found.summary.name,
            email: &email,
            display_name: &display_name,
            actor: identity.principal.user_id.as_uuid(),
            request_id: Some(request_id.0.0.as_str()),
        },
    )
    .await
    .map_err(platform_error)?;

    let view = created_view(&state, tenant, &result, false).await?;
    Ok(Json(view))
}

/// Re-read the tenant after a create and build its response.
async fn created_view(
    state: &AppState,
    tenant: TenantId,
    result: &conveyor_auth::ProvisionResult,
    email_sent: bool,
) -> Result<CreatedTenantView, ApiError> {
    let found = conveyor_store::platform::detail(&state.pool, tenant, &state.config.app_key)
        .await
        .map_err(platform_error)?
        .ok_or_else(|| ApiError::internal("the tenant row vanished after create"))?;
    Ok(CreatedTenantView {
        tenant: tenant_view(found.summary),
        admins: found.admins.into_iter().map(admin_view).collect(),
        provisioning: ProvisioningView {
            status: result.status.clone(),
            replayed: result.replayed,
            tenant_id: result.tenant_id.to_string(),
            license_id: result
                .app_license()
                .map(|licence| licence.license_id.clone()),
            warnings: result.warnings.clone(),
            partial_failures: result.partial_failures.clone(),
        },
        email_sent,
    })
}

fn tenant_view(row: conveyor_store::platform::TenantSummary) -> TenantView {
    TenantView {
        id: row.id.to_string(),
        slug: row.slug,
        name: row.name,
        status: row.status,
        created_at: row.created_at,
        members: row.members,
        admins: row.admins,
        licence: row.licence.map(|licence| LicenceView {
            status: licence.status,
            expires_at: licence.expires_at,
            checked_at: licence.checked_at,
        }),
    }
}

fn admin_view(row: conveyor_store::platform::AdminRow) -> AdminView {
    AdminView {
        user_id: row.user_id.to_string(),
        email: row.email,
        display_name: row.display_name,
        role: row.role,
        user_status: row.user_status,
    }
}

fn require_super_admin(identity: &conveyor_auth::IdentitySession) -> Result<(), ApiError> {
    if identity.is_super_admin {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "the platform surface is super-admin only",
        ))
    }
}

fn tenant_id(id: Uuid) -> Result<TenantId, ApiError> {
    TenantId::from_uuid(id).map_err(|_| ApiError::bad_request("a tenant id must not be nil"))
}

fn required<'a>(value: &'a str, field: &str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field} must not be empty")))
    } else {
        Ok(value)
    }
}

/// `^[a-z0-9][a-z0-9-]*$` — the same grammar provisioning and licences enforce.
pub fn validate_slug(raw: &str) -> Result<&str, ApiError> {
    let slug = raw.trim();
    let mut chars = slug.chars();
    let first_ok = matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit());
    if !first_ok || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(ApiError::bad_request(
            "slug must be lowercase letters, digits and hyphens, starting with a letter or digit",
        ));
    }
    Ok(slug)
}

/// The same single-`@`, dotted-domain grammar provisioning accepts.
pub fn validate_email(raw: &str) -> Result<&str, ApiError> {
    let email = raw.trim();
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ApiError::bad_request(
            "admin_email must be a single email address",
        ));
    };
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || email.contains(char::is_whitespace)
    {
        return Err(ApiError::bad_request(
            "admin_email must be a single email address",
        ));
    }
    Ok(email)
}

fn platform_error(error: conveyor_store::platform::PlatformError) -> ApiError {
    use conveyor_store::platform::PlatformError as E;
    match error {
        E::SlugTaken(slug) => {
            ApiError::conflict(format!("a tenant with slug {slug:?} already exists"))
        }
        E::EmailNotHuman => {
            ApiError::unprocessable("that email belongs to an agent or service principal")
        }
        E::NotFound => ApiError::not_found("tenant"),
        E::Database(error) => ApiError::internal(error.to_string()),
    }
}

fn service_error(error: ServiceError) -> ApiError {
    let detail = error.to_string();
    match error {
        ServiceError::Unconfigured(_) => ApiError::service_unavailable(detail),
        ServiceError::Token(conveyor_auth::TokenError::Unconfigured(_)) => {
            ApiError::service_unavailable(detail)
        }
        ServiceError::Refused { status: 404, .. } => ApiError::not_found(detail),
        ServiceError::Refused { status: 409, .. } => ApiError::conflict(detail),
        _ => ApiError::new(StatusCode::BAD_GATEWAY, "upstream_failed", detail),
    }
}
