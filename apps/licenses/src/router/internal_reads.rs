//! The five internal read routes other services call.
//!
//! Ported from the pinned Python reference (commit `4e8a08d`): `main.py::internal_list_licenses`,
//! `internal_list_tenants`, `internal_list_apps`, `internal_get_subject` and `internal_get_license`.
//! Every one of them is guarded by `require_internal_service` — `service-internal`,
//! `onboarding-internal` or `license-admin`; a `license-reader` alone is refused with 403.
//!
//! The response shapes are the capture's (`contract/openapi.lock.json`), not a rendering of what
//! the queries happen to return: Pydantic emits explicit nulls, so every field is serialised and
//! none is skipped. `AppInfo` and `PlanInfo` are the SAME components the user-licence route
//! already publishes — the capture's `InternalLicenseResponse` references those two by name.
//!
//! Error mapping is the reference's:
//!
//! - a malformed `app_id` / `plan_id` / `license_id` is **400** `"Invalid UUID"` (a client fault,
//!   logged at WARNING there, never an ERROR with a traceback);
//! - a non-UUID `tenant_id` on the licence listing is the reference's **empty list**, not a 400 —
//!   the value is unvalidated and filtered against a UUID column, so the guard exists to stop a
//!   cast failure, not to police the caller;
//! - an unknown subject or licence is **404**; a non-UUID subject reference that matched no
//!   `keycloak_sub` is a 404 too, never a 422 — the parameter is deliberately polymorphic;
//! - anything unexpected is **500** with the reference's static message.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::auth::{User, require_internal_service};
use crate::db::internal_reads::{
    self, InternalAppRow, InternalLicenseRow, InternalSubjectRow, InternalTenantRow, LicenseFilters,
};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::router::user_licenses::{AppInfo, PlanInfo};
use crate::state::AppState;

/// `BudgetCurrency` (`main.py`): `None` = token budgets; `EUR`/`USD`/`CAD` = the currency the
/// tenant's MONEY budget is held in.
// Variants are spelled `Eur`/`Usd`/`Cad` so clippy stays quiet; the wire value is UPPERCASE,
// exactly the enum the capture publishes.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum BudgetCurrency {
    Eur,
    Usd,
    Cad,
}

impl BudgetCurrency {
    /// Pydantic validates the column against the `Literal`; an unknown value fails the response
    /// model, which the reference's top-level `except` turns into a 500. `None` reproduces that
    /// fail-closed path rather than laundering a bad value into a valid-looking one.
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "EUR" => Some(Self::Eur),
            "USD" => Some(Self::Usd),
            "CAD" => Some(Self::Cad),
            _ => None,
        }
    }
}

/// `ck_app_audience`: who holds a licence for this app.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    Human,
    Platform,
    Automation,
}

impl Audience {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "human" => Some(Self::Human),
            "platform" => Some(Self::Platform),
            "automation" => Some(Self::Automation),
            _ => None,
        }
    }
}

/// Enriched license response for internal service consumers.
///
/// `status` is the STORED column, verbatim; `effective_status` is the time-aware verdict for the
/// same row (`active` | `pending` | `expired` | `revoked`). Both are published because this route
/// and `/internal/tenants/{id}/seat-count` are read by billing for the SAME invoice.
// Field nullability is the capture's `required` set: `expires_at` and `metadata` are required and
// nullable, while the three defaulted fields (`is_dependency`, the two overrides), `app` and
// `plan` are optional. Pydantic emits every field, nulls included, so nothing is skipped here.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InternalLicenseResponse {
    pub id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub status: String,
    pub effective_status: String,
    pub starts_at: String,
    #[schema(required = true)]
    pub expires_at: Option<String>,
    #[schema(value_type = Option<Object>, required = true)]
    pub metadata: Option<Value>,
    #[schema(default = false, required = false)]
    pub is_dependency: bool,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    pub app: Option<AppInfo>,
    pub plan: Option<PlanInfo>,
}

/// Tenant identity (uuid -> human name) for internal service consumers.
///
/// `kind` and `cost_centre` travel HERE because this is the one tenant endpoint billing can call:
/// `GET /tenants` requires `license-admin`/`license-reader`, which billing's service account does
/// not hold.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InternalTenantResponse {
    pub id: String,
    pub name: String,
    #[schema(required = true)]
    pub keycloak_realm: Option<String>,
    pub archived: bool,
    pub kind: String,
    #[schema(required = true)]
    pub cost_centre: Option<String>,
    // The capture inlines the three-value enum in the property; `inline` keeps that shape
    // instead of minting a `BudgetCurrency` component the contract does not have.
    #[schema(inline)]
    pub budget_currency: Option<BudgetCurrency>,
    pub owner_key: String,
    pub external_ref: Option<String>,
}

/// App identity (uuid <-> registry key) for internal service consumers.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InternalAppResponse {
    pub id: String,
    pub key: String,
    pub name: String,
    pub revoked: bool,
    // Inline for the same reason as `budget_currency`: the capture has no `Audience` component.
    #[schema(inline)]
    pub audience: Audience,
}

/// Subject identity, including the tenant it BELONGS TO (migration 029).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InternalSubjectResponse {
    pub id: String,
    pub keycloak_sub: String,
    pub keycloak_realm: String,
    /// NULL means UNDECIDED, never "no tenant".
    #[schema(required = true)]
    pub tenant_id: Option<String>,
    #[schema(required = true)]
    pub created_at: Option<String>,
}

/// FastAPI's 422 body for a query value that is not a datetime.
fn bad_datetime(field: &str, raw: &str, reason: &str) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": "datetime_parsing",
            "loc": ["query", field],
            "msg": reason,
            "input": raw,
        }]),
    )
}

/// `as_of`: an RFC 3339 instant, else a naive ISO datetime read as UTC.
///
/// ⚠️ The reference types this field `datetime | None` (not aware), so a naive value is legal
/// there. Treated as UTC here; the instant is unchanged for the UTC sessions this service runs in.
fn parse_as_of(raw: &str) -> Result<DateTime<Utc>, Refusal> {
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return Ok(value.with_timezone(&Utc));
    }
    let normalised = raw.replacen(' ', "T", 1);
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(&normalised, format) {
            return Ok(value.and_utc());
        }
    }
    Err(bad_datetime(
        "as_of",
        raw,
        "Input should be a valid datetime",
    ))
}

/// Pydantic's lax boolean parsing (`true/false/1/0/yes/no/on/off`, case-insensitive).
fn parse_bool(field: &str, raw: &str) -> Result<bool, Refusal> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!([{
                "type": "bool_parsing",
                "loc": ["query", field],
                "msg": "Input should be a valid boolean, unable to interpret input",
                "input": raw,
            }]),
        )),
    }
}

/// A query UUID the reference parses with `UUID(x) if x else None`: an EMPTY value is "no
/// filter", a non-empty malformed one is the 400 the caller sees.
fn optional_uuid(raw: Option<&str>) -> Result<Option<uuid::Uuid>, Refusal> {
    match raw {
        Some(value) if !value.is_empty() => uuid::Uuid::try_parse(value)
            .map(Some)
            .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID")),
        _ => Ok(None),
    }
}

/// The static 500 body, named per route exactly as the reference names it.
fn internal_error(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}

/// `GET /internal/licenses` query parameters.
///
/// Booleans are strings at the extractor and parsed below so a malformed value is FastAPI's 422,
/// not axum's 400. The documented type is still `boolean` with the reference's default.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InternalLicensesQuery {
    #[param(nullable = true)]
    pub tenant_id: Option<String>,
    /// `status` in the reference (the filter reads the stored column, or the effective predicate
    /// when `effective=true`).
    #[param(nullable = true)]
    pub status: Option<String>,
    #[param(nullable = true)]
    pub app_id: Option<String>,
    #[param(nullable = true)]
    pub plan_id: Option<String>,
    /// What `status=active` MEANS: `true` (default) applies the seat-count's three-part rule.
    #[serde(default)]
    #[param(value_type = bool, default = true)]
    pub effective: Option<String>,
    /// Evaluate the window at this instant instead of now. Ignored unless `effective` is set.
    #[param(format = "date-time", nullable = true)]
    pub as_of: Option<String>,
    /// Include soft-deleted licences in an unfiltered or stored-status listing.
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_deleted: Option<String>,
}

/// `GET /internal/tenants` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InternalTenantsQuery {
    /// Archived tenants are excluded by default and returned only here.
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_archived: Option<String>,
}

/// `GET /internal/apps` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InternalAppsQuery {
    /// Revoked apps are excluded by default and returned only here, flagged `revoked: true`.
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_revoked: Option<String>,
}

/// `InternalLicenseResponse` from one row, with the app and plan enrichment the reference builds.
fn license_response(row: InternalLicenseRow, now: DateTime<Utc>) -> InternalLicenseResponse {
    let app = match (&row.app_id, row.app_key, row.app_name) {
        (Some(id), Some(key), Some(name)) => Some(AppInfo {
            id: id.to_string(),
            key,
            name,
        }),
        _ => None,
    };
    let plan = match (
        &row.plan_id,
        row.plan_code,
        row.plan_name,
        row.plan_rate_limit_rpm,
        row.plan_rate_limit_burst,
        row.plan_quota_monthly,
        row.plan_token_budget_monthly,
    ) {
        (Some(id), Some(code), Some(name), rpm, burst, quota, budget) => Some(PlanInfo {
            id: id.to_string(),
            code,
            name,
            rate_limit_rpm: rpm,
            rate_limit_burst: burst,
            quota_monthly: quota,
            token_budget_monthly: budget,
        }),
        _ => None,
    };
    InternalLicenseResponse {
        id: row.id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        subject_id: row.subject_id.to_string(),
        status: row.status.clone(),
        effective_status: internal_reads::effective_status(
            &row.status,
            row.starts_at,
            row.expires_at,
            now,
        ),
        starts_at: py_iso(row.starts_at),
        expires_at: row.expires_at.map(py_iso),
        metadata: row.license_metadata,
        is_dependency: row.is_dependency,
        token_budget_override: row.token_budget_override,
        quota_override: row.quota_override,
        app,
        plan,
    }
}

/// The tenant response, or `None` when a column holds a value the capture's enum forbids (the
/// reference's Pydantic response model refuses it, and the handler turns that into a 500).
fn tenant_response(row: InternalTenantRow) -> Option<InternalTenantResponse> {
    let budget_currency = match row.budget_currency.as_deref() {
        Some(raw) => Some(BudgetCurrency::parse(raw)?),
        None => None,
    };
    Some(InternalTenantResponse {
        id: row.id.to_string(),
        name: row.name,
        keycloak_realm: row.keycloak_realm,
        archived: row.archived,
        kind: row.kind,
        cost_centre: row.cost_centre,
        budget_currency,
        owner_key: row.owner_key,
        external_ref: row.external_ref,
    })
}

/// The app response, or `None` when `audience` holds a value outside the captured enum.
fn app_response(row: InternalAppRow) -> Option<InternalAppResponse> {
    Some(InternalAppResponse {
        id: row.id.to_string(),
        key: row.key,
        name: row.name,
        revoked: row.revoked,
        audience: Audience::parse(&row.audience)?,
    })
}

/// The subject response. `tenant_id` stays null when unstated; `created_at` is the ORM's
/// `isoformat()`, which is what the reference's `to_dict()` emits (and `None` only for a NULL).
fn subject_response(row: InternalSubjectRow) -> InternalSubjectResponse {
    InternalSubjectResponse {
        id: row.id.to_string(),
        keycloak_sub: row.keycloak_sub,
        keycloak_realm: row.keycloak_realm,
        tenant_id: row.tenant_id.map(|id| id.to_string()),
        created_at: Some(py_iso(row.created_at)),
    }
}

/// `GET /internal/licenses`.
///
/// List licenses with enriched app and plan data.
///
/// For internal service consumers (billing, onboarding).
/// Requires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES:
/// service-internal, onboarding-internal or license-admin.
///
/// ⛔ `?status=active` means the same thing it means on the seat-count endpoint: the three-part
/// time-aware rule (`effective=true`, the default), because billing reads both for the same
/// invoice. `effective=false` filters the STORED `status` column verbatim — the operator surface
/// for auditing column drift.
#[utoipa::path(
    get,
    path = "/internal/licenses",
    tag = "internal",
    operation_id = "internal_list_licenses",
    summary = "Internal List Licenses",
    description = "List licenses with enriched app and plan data.\n\nFor internal service consumers (billing, onboarding).\nRequires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES: service-internal, onboarding-internal or license-admin.\n\n⛔ `?status=active` means the same thing it means on the seat-count endpoint: the three-part time-aware rule (`effective=true`, the default), because billing reads both for the same invoice. `effective=false` filters the STORED `status` column verbatim.",
    params(
        InternalLicensesQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<InternalLicenseResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_list_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<InternalLicensesQuery>,
) -> Result<Json<Vec<InternalLicenseResponse>>, Refusal> {
    require_internal_service(&user)?;

    // The reference parses app_id/plan_id BEFORE the tenant guard, so an invalid app_id is a 400
    // even when the tenant value would have short-circuited to an empty list.
    let app_id = optional_uuid(query.app_id.as_deref())?;
    let plan_id = optional_uuid(query.plan_id.as_deref())?;
    let tenant_id = match query.tenant_id.as_deref() {
        Some(raw) => match uuid::Uuid::try_parse(raw) {
            Ok(id) => Some(id),
            // Unvalidated query param filtered against a UUID column: "no match", never a cast
            // failure. (`?tenant_id=` is a Some("") and lands here too, exactly as Python does.)
            Err(_) => return Ok(Json(Vec::new())),
        },
        None => None,
    };
    let effective = match query.effective.as_deref() {
        Some(raw) => parse_bool("effective", raw)?,
        None => true,
    };
    let include_deleted = match query.include_deleted.as_deref() {
        Some(raw) => parse_bool("include_deleted", raw)?,
        None => false,
    };
    let now = Utc::now();
    let as_of = match query.as_of.as_deref() {
        Some(raw) => parse_as_of(raw)?,
        None => now,
    };

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list internal licenses"));
    };
    let filters = LicenseFilters {
        tenant_id,
        app_id,
        plan_id,
        // An empty `?status=` is the reference's falsy "no status filter".
        status: query.status.filter(|value| !value.is_empty()),
        effective,
        as_of,
        include_deleted,
    };
    let rows = internal_reads::list_licenses(pool, &filters)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list internal licenses");
            internal_error("Failed to list internal licenses")
        })?;

    Ok(Json(
        rows.into_iter()
            .map(|row| license_response(row, now))
            .collect(),
    ))
}

/// `GET /internal/licenses/{license_id}`.
///
/// Get a single license with enriched app and plan data.
///
/// Requires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES:
/// service-internal, onboarding-internal or license-admin.
#[utoipa::path(
    get,
    path = "/internal/licenses/{license_id}",
    tag = "internal",
    operation_id = "internal_get_license",
    summary = "Internal Get License",
    description = "Get a single license with enriched app and plan data.\n\nFor internal service consumers.\nRequires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES: service-internal, onboarding-internal or license-admin.",
    params(
        ("license_id" = String, Path, description = "License Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = InternalLicenseResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_get_license(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(license_id): Path<String>,
) -> Result<Json<InternalLicenseResponse>, Refusal> {
    require_internal_service(&user)?;

    // `get_license(UUID(license_id))`: a malformed id is the reference's 400, not a 404.
    let Ok(license_id) = uuid::Uuid::try_parse(&license_id) else {
        return Err(Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"));
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to get internal license"));
    };
    let row = internal_reads::get_license(pool, license_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to get internal license");
            internal_error("Failed to get internal license")
        })?;
    let Some(row) = row else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };
    Ok(Json(license_response(row, Utc::now())))
}

/// `GET /internal/tenants`.
///
/// List tenants as a uuid -> human-name identity map.
///
/// Archived tenants are EXCLUDED by default and returned only with `include_archived=true`: a
/// retired tenant still owns historical cost rows, so a backfill over an old window has to be
/// able to name it. Service actors are ALWAYS included — this is an identity map, not a pick-list.
#[utoipa::path(
    get,
    path = "/internal/tenants",
    tag = "internal",
    operation_id = "internal_list_tenants",
    summary = "Internal List Tenants",
    description = "List tenants as a uuid -> human-name identity map.\n\nArchived tenants are EXCLUDED by default and returned only with `include_archived=true`. Service actors are ALWAYS included: billing resolves tenant uuids it has already billed.\n\nRequires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES.",
    params(
        InternalTenantsQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<InternalTenantResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_list_tenants(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<InternalTenantsQuery>,
) -> Result<Json<Vec<InternalTenantResponse>>, Refusal> {
    require_internal_service(&user)?;

    let include_archived = match query.include_archived.as_deref() {
        Some(raw) => parse_bool("include_archived", raw)?,
        None => false,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list internal tenants"));
    };
    let rows = internal_reads::list_tenants(pool, include_archived)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list internal tenants");
            internal_error("Failed to list internal tenants")
        })?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(response) = tenant_response(row) else {
            tracing::error!("Failed to list internal tenants: value outside the captured enum");
            return Err(internal_error("Failed to list internal tenants"));
        };
        results.push(response);
    }
    Ok(Json(results))
}

/// `GET /internal/apps`.
///
/// List EVERY registered app as a uuid <-> key identity map.
///
/// Revoked apps are excluded by default and returned only with `include_revoked=true`, flagged
/// `revoked: true`: a retired app still owns its historical spend.
#[utoipa::path(
    get,
    path = "/internal/apps",
    tag = "internal",
    operation_id = "internal_list_apps",
    summary = "Internal List Apps",
    description = "List EVERY registered app as a uuid <-> key identity map.\n\nRevoked apps are excluded by default and returned only with `include_revoked=true`, flagged `revoked: true`.\n\nRequires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES.",
    params(
        InternalAppsQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<InternalAppResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_list_apps(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<InternalAppsQuery>,
) -> Result<Json<Vec<InternalAppResponse>>, Refusal> {
    require_internal_service(&user)?;

    let include_revoked = match query.include_revoked.as_deref() {
        Some(raw) => parse_bool("include_revoked", raw)?,
        None => false,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list internal apps"));
    };
    let rows = internal_reads::list_apps(pool, include_revoked)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list internal apps");
            internal_error("Failed to list internal apps")
        })?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(response) = app_response(row) else {
            tracing::error!("Failed to list internal apps: value outside the captured enum");
            return Err(internal_error("Failed to list internal apps"));
        };
        results.push(response);
    }
    Ok(Json(results))
}

/// `GET /internal/subjects/{subject_id}`.
///
/// Read one subject, including `tenant_id` — which tenant this subject belongs to.
///
/// `subject_id` resolves against BOTH identifiers, in this order: `subject.keycloak_sub` (the
/// Keycloak `sub` claim, the form a real caller sends), then `subject.id` (the local uuid7). A
/// non-UUID value that matched no `keycloak_sub` is a 404, not a 422 — the parameter is
/// deliberately polymorphic.
#[utoipa::path(
    get,
    path = "/internal/subjects/{subject_id}",
    tag = "internal",
    operation_id = "internal_get_subject",
    summary = "Internal Get Subject",
    description = "Read one subject, including `tenant_id`.\n\n`subject_id` resolves against BOTH identifiers: `subject.keycloak_sub`, then `subject.id`. A non-UUID value that matched no keycloak_sub is a 404, not a 422.\n\nRequires a service-account JWT carrying one of this service's INTERNAL_SERVICE_ROLES.",
    params(
        ("subject_id" = String, Path, description = "Subject Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = InternalSubjectResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_get_subject(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(subject_id): Path<String>,
) -> Result<Json<InternalSubjectResponse>, Refusal> {
    require_internal_service(&user)?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to read subject"));
    };
    let subject = internal_reads::subject_by_keycloak_sub(pool, &subject_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to read subject");
            internal_error("Failed to read subject")
        })?;
    let subject = match subject {
        Some(subject) => Some(subject),
        None => match uuid::Uuid::try_parse(&subject_id) {
            // A non-UUID string that matched no keycloak_sub is simply "not found".
            Ok(id) => internal_reads::subject_by_id(pool, id).await.map_err(|error| {
                tracing::error!(%error, "Failed to read subject");
                internal_error("Failed to read subject")
            })?,
            Err(_) => None,
        },
    };
    let Some(subject) = subject else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Subject not found"));
    };
    Ok(Json(subject_response(subject)))
}
