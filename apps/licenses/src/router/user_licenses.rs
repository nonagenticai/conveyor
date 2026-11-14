//! `GET /licenses/user/{subject_id}` — the per-subject licence read.
//!
//! Ported from `main.py::list_user_licenses` (pinned commit 4e8a08d). `subject_id` is a
//! caller-controlled path parameter, so the role gate alone authorizes nothing about *whose*
//! licences are returned. The reference's three tiers are reproduced here, in its order:
//!
//! - `license-admin` — any subject, NO tenant filter (the cross-tenant operator surface).
//! - a known owning app (`user.owner_key` set by the auth middleware, migration 050) — it acts
//!   on behalf of an end user, so it may name another subject, but the result stays SCOPED TO
//!   THE TENANT it is acting for.
//! - everyone else — own subject only (`subject.keycloak_sub == user.sub`) or 403, tenant
//!   filtered.
//!
//! The reference's `_ensure_owner_resolved(user)` has no counterpart here: the middleware's
//! `attach_and_authorize` already resolved `owner_key`/`cross_owner` from `api_client`, and the
//! route must not re-resolve (a second query could disagree with the identity it authorized).

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::db::user_licenses::{self, LicenseRow};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// App information for license response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppInfo {
    pub id: String,
    pub key: String,
    pub name: String,
}

/// Plan information for license response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanInfo {
    pub id: String,
    pub code: String,
    pub name: String,
    pub rate_limit_rpm: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub quota_monthly: Option<i32>,
    pub token_budget_monthly: Option<i32>,
}

/// License response
// Field order and nullability follow `models.py::LicenseResponse`; every field is serialised
// (Pydantic emits nulls by default), so none may be skipped.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LicenseResponse {
    pub id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub app_id: String,
    pub plan_id: String,
    // The reference's Pydantic model has no default for these four: they are REQUIRED and
    // nullable. utoipa treats a bare `Option<T>` as optional, so `required = true` restores
    // the captured `required` set without changing the wire value.
    #[schema(required = true)]
    pub region_id: Option<String>,
    pub status: String,
    pub starts_at: String,
    #[schema(required = true)]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[schema(required = true)]
    pub revoked_at: Option<String>,
    pub effective_status: Option<String>,
    // Nullable object, not a bare `Value`: the capture is `anyOf [{type: object}, {type: null}]`,
    // and utoipa emits `any` for an untyped `Value`.
    #[schema(value_type = Option<Object>, required = true)]
    pub metadata: Option<Value>,
    pub metering_provisioned_at: Option<String>,
    pub metering_verified_at: Option<String>,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    // `default = false` on a non-Option field still reads as required to utoipa; the capture
    // has the default but no required entry.
    #[schema(default = false, required = false)]
    pub is_dependency: bool,
    pub bundle_id: Option<String>,
    pub app: Option<AppInfo>,
    pub plan: Option<PlanInfo>,
}

/// `License.effective_status`: time-aware validity, the single rule the reference exposes.
///
/// A stored status outside `active` is returned VERBATIM, never normalised. For a stored
/// `active` row, `pending` (starts_at in the future) is reported before `expired`, so a row
/// that is somehow both reads as the earlier defect. The licence's own `status` stays
/// structural and is emitted unchanged.
fn effective_status(
    stored: &str,
    starts_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if stored != "active" {
        return stored.to_owned();
    }
    if starts_at > now {
        return "pending".to_owned();
    }
    if expires_at.is_some_and(|expires_at| expires_at < now) {
        return "expired".to_owned();
    }
    "active".to_owned()
}

/// Build the response exactly as the reference's `to_dict()` + enrichment loop does.
///
/// `app` is set only when the joined app row exists; `plan` is deliberately left null — this
/// route enriches the app and never populates a plan (the reference builds it in the loop, not
/// `_enrich_license_response`).
fn response(row: LicenseRow, now: DateTime<Utc>) -> LicenseResponse {
    let app = match (row.app_key, row.app_name) {
        (Some(key), Some(name)) => Some(AppInfo {
            id: row.app_id.to_string(),
            key,
            name,
        }),
        _ => None,
    };
    LicenseResponse {
        id: row.id.to_string(),
        subject_id: row.subject_id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        app_id: row.app_id.to_string(),
        plan_id: row.plan_id.to_string(),
        region_id: row.region_id.map(|id| id.to_string()),
        status: row.status.clone(),
        starts_at: crate::db::tokens::py_iso(row.starts_at),
        expires_at: row.expires_at.map(crate::db::tokens::py_iso),
        created_at: crate::db::tokens::py_iso(row.created_at),
        updated_at: crate::db::tokens::py_iso(row.updated_at),
        revoked_at: row.revoked_at.map(crate::db::tokens::py_iso),
        effective_status: Some(effective_status(
            &row.status,
            row.starts_at,
            row.expires_at,
            now,
        )),
        metadata: row.license_metadata,
        metering_provisioned_at: row.metering_provisioned_at.map(crate::db::tokens::py_iso),
        metering_verified_at: row.metering_verified_at.map(crate::db::tokens::py_iso),
        token_budget_override: row.token_budget_override,
        quota_override: row.quota_override,
        is_dependency: row.is_dependency.unwrap_or(false),
        bundle_id: row.bundle_id.map(|id| id.to_string()),
        app,
        plan: None,
    }
}

/// `_internal_error(500, …)`: the reference's generic 500 body.
fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}

/// `subject_id` may arrive as the Keycloak `sub` (the common case) or the internal UUID.
async fn resolve_subject(
    pool: &sqlx::PgPool,
    reference: &str,
) -> Result<Option<user_licenses::Subject>, sqlx::Error> {
    if let Some(subject) = user_licenses::subject_by_keycloak_sub(pool, reference).await? {
        return Ok(Some(subject));
    }
    match uuid::Uuid::try_parse(reference) {
        // `except ValueError: pass` — a non-UUID that named no subject is a 404, not a 400.
        Ok(id) => user_licenses::subject_by_id(pool, id).await,
        Err(_) => Ok(None),
    }
}

/// `GET /licenses/user/{subject_id}`.
#[utoipa::path(
    get,
    path = "/licenses/user/{subject_id}",
    tag = "licenses",
    summary = "List User Licenses",
    description = "Get all licenses for a specific subject/user\n\nRequires 'license-admin' or 'license-reader' role.\n\nAUTHORIZATION (cross-tenant IDOR closed 2026-07-27):\n``subject_id`` is a caller-controlled path parameter, so the role check\nalone authorizes NOTHING about *whose* licenses are returned. Three tiers:\n\n- ``license-admin`` — the cross-tenant operator surface (same contract as\n  ``GET /licenses/admin/all``). Any subject, no tenant filter. This is what\n  portal-bff and provisioning hold (see auth/config.py:97).\n- a known owning app (verified ``azp`` in ``api_client``, migration 050 —\n  e.g. fr-backend / w2v-backend) — acts on\n  behalf of an end user, so it may ask for a subject that is not itself,\n  but the result is SCOPED TO THE TENANT it is acting for. That is the\n  cross-tenant half of the hole, and it is the half that mattered: the\n  w2v license gate legitimately calls this route with only\n  ``license-reader``/``license-manager`` (no ``license-admin``), so a flat\n  caller==subject rule would have broken it.\n- everyone else — own subject only, tenant-filtered.",
    params(
        ("subject_id" = String, Path, description = "Subject Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<LicenseResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_user_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(subject_id): Path<String>,
) -> Result<Json<Vec<LicenseResponse>>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };

    // Subject resolution happens BEFORE any tenant/self authorization, exactly as the
    // reference does: an unknown subject is a 404 even for a caller who would fail later.
    let subject = match resolve_subject(pool, &subject_id).await {
        Ok(Some(subject)) => subject,
        Ok(None) => {
            return Err(Refusal::detail(StatusCode::NOT_FOUND, "Subject not found"));
        }
        Err(error) => {
            tracing::error!(%error, "Subject lookup failed");
            return Err(internal_error());
        }
    };

    let tenant_filter = if user.has_realm_role("license-admin") {
        // The operator tier: any subject, no tenant filter.
        None
    } else {
        let tenant = get_tenant_id(&state, &user).await?;
        // Migration 050: a known owning app acts on behalf of end users. An ordinary caller
        // (no owner_key, no cross_owner) may only ask for ITSELF — compared against the
        // RESOLVED subject's keycloak_sub because that is what the verified token's `sub` is.
        if user.owner_key.is_none()
            && !user.cross_owner
            && subject.keycloak_sub.as_deref() != Some(user.sub.as_str())
        {
            return Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                "Not authorized to read another subject's licenses",
            ));
        }
        // `get_tenant_id` already canonicalised this; a parse failure is internal, and
        // mapping it to 500 is safer than silently dropping the filter.
        Some(uuid::Uuid::try_parse(&tenant).map_err(|_| internal_error())?)
    };

    let rows = match user_licenses::list_for_subject(pool, subject.id, tenant_filter).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "Failed to list licenses");
            return Err(internal_error());
        }
    };

    let now = Utc::now();
    Ok(Json(
        rows.into_iter().map(|row| response(row, now)).collect(),
    ))
}
