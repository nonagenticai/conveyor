//! The licence-admin read surface and the budget write.
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Route | Reference |
//! |---|---|
//! | `GET /licenses` | `main.py::list_licenses` |
//! | `GET /licenses/admin/all` | `main.py::list_all_licenses_admin` |
//! | `PATCH /licenses/{license_id}/budget` | `main.py::set_license_budget` |
//! | `POST /licenses/admin/metering/reconcile` | `main.py::reconcile_metering_provisioning` |
//!
//! The reconcile route is a THIN WRAPPER over
//! `commands::reconcile_metering::run_metering_reconcile` — the same runner the
//! `reconcile-metering` CronJob command calls — so the operator route and the scheduled sweep
//! cannot drift apart.
//!
//! Enrichment is `_enrich_license_response`: the `app` **and** `plan` objects are both
//! populated here (the user-licence route deliberately never populates a plan).
//!
//! Authorization, exactly as the reference:
//!
//! - both listings require `license-admin` or `license-reader` (realm roles only) and are
//!   scoped to the caller's reconciled tenant; the admin read is the cross-tenant
//!   operator surface and therefore needs `license-admin` ALONE;
//! - the budget write requires `license-admin` AND `_authorize_issuance` — the caller's
//!   owner must ISSUE the licence's app and own the tenant or hold an ACTIVE grant on it;
//!   `cross_owner` bypasses the latter two, audited. It is deliberately NOT
//!   `_resolve_owned_tenant`: fr holds a GRANT on conveyor, never ownership.
//!
//! Error mapping is the reference's: a malformed query UUID is **400** `"Invalid UUID"`
//! (the handlers parse them with `UUID(x) if x else None`), a malformed path UUID or body
//! is FastAPI's **422** validation shape, an unknown licence is **404**
//! `"License not found"`, and anything unexpected is **500** with the reference's static
//! per-route message.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Extension;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::billing::metering::MeteringProvisioner;
use crate::commands::reconcile_metering::{MeteringReconcileResult, run_metering_reconcile};
use crate::db::internal_reads::effective_status;
use crate::db::issuance;
use crate::db::licence_admin::{self, LicenceFilters, LicenceRow};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::router::user_licenses::{AppInfo, LicenseResponse, PlanInfo};
use crate::state::AppState;

/// `GET /licenses` query parameters.
///
/// Every id is a string here and parsed in the handler: the reference parses them with
/// `UUID(x) if x else None`, and a malformed value is its 400, not FastAPI's 422.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LicensesQuery {
    /// Stored `status` column, verbatim (no effective/time-window mode on this route).
    #[param(nullable = true)]
    pub status: Option<String>,
    #[param(nullable = true)]
    pub app_id: Option<String>,
    #[param(nullable = true)]
    pub plan_id: Option<String>,
    #[param(nullable = true)]
    pub subject_id: Option<String>,
}

/// `GET /licenses/admin/all` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdminLicensesQuery {
    #[param(nullable = true)]
    pub status: Option<String>,
    /// Include soft-deleted licences (`deleted_at` set, migration 036). `false` (default)
    /// matches what this endpoint returned before `LICENSE_SOFT_DELETE` was armed — a
    /// deleted licence is simply absent. `true` is the AUDIT view: the retained row IS
    /// this service's record of the deletion (there is no audit table), so `deleted_at` /
    /// `deleted_by` / `delete_reason` are only readable through it.
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_deleted: Option<String>,
}

/// `POST /licenses/admin/metering/reconcile` query parameters.
///
/// Documented as FastAPI publishes them. Extraction is by hand (`BTreeMap<String, String>`) so
/// each refusal is FastAPI's 422 body rather than axum's rejection. `license_id` is a plain
/// STRING on the wire — the reference's `str | None` — and a non-UUID value is its 400.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReconcileMeteringQuery {
    /// Max licences repaired in one sweep
    #[serde(default)]
    #[param(default = 500, minimum = 1, maximum = 5000)]
    pub limit: i64,
    /// Re-examine licences already stamped metering_verified_at instead of skipping them. Use
    /// to re-measure coverage on demand.
    #[serde(default)]
    #[param(default = false)]
    pub revalidate: bool,
    /// Measure coverage against billing and report it, writing nothing — no pushes to billing,
    /// no stamps.
    #[serde(default)]
    #[param(default = false)]
    pub dry_run: bool,
    /// DESTRUCTIVE, default off. Push the licence's EFFECTIVE quota (its `quota_override`, else
    /// `plan.quota_monthly`) over a billing quota row that DIFFERS from it, destroying the
    /// hand-set billing value that made it differ. Without this, a divergent row is left alone
    /// and reported in `quota_drift_license_ids`. Has no effect where billing holds no row
    /// (that is a plain create) or where the configured quota could not be read.
    #[serde(default)]
    #[param(default = false)]
    pub force_plan_quota: bool,
    /// Restrict the sweep to this ONE licence id (a canary). Narrows the normal selection,
    /// never widens it: a licence that is not live, or whose EFFECTIVE metering (override or
    /// plan) owes billing nothing, is still not swept. 400 if not a UUID. Omit to sweep the
    /// estate.
    #[param(nullable = true)]
    pub license_id: Option<String>,
}

/// `LicenseBudgetUpdate` (`main.py`): set or clear one licence's own monthly token budget.
///
/// `token_budget_override` is REQUIRED, deliberately: `null` CLEARS the override, so an
/// omitted key must not be indistinguishable from an explicit clear. 0 is a VALUE (block
/// all), not "unset". The handler validates the raw body so an omitted key is FastAPI's
/// 422 rather than axum's own rejection body.
#[derive(Debug, ToSchema)]
pub struct LicenseBudgetUpdate {
    /// This licence's own monthly LLM token budget, overriding the plan's
    /// `token_budget_monthly`; 0 = block all. `null` CLEARS the override so the licence
    /// returns to the plan's value.
    #[schema(required = true, minimum = 0)]
    pub token_budget_override: Option<i32>,
    /// Why the budget changed. Recorded in the audit log line.
    pub reason: Option<String>,
}

/// `_enrich_license_response`: the licence row plus BOTH nested objects.
fn response(row: LicenceRow, now: DateTime<Utc>) -> LicenseResponse {
    let app = match (row.app_key, row.app_name) {
        (Some(key), Some(name)) => Some(AppInfo {
            id: row.app_id.to_string(),
            key,
            name,
        }),
        _ => None,
    };
    let plan = match (row.plan_code, row.plan_name) {
        (Some(code), Some(name)) => Some(PlanInfo {
            id: row.plan_id.to_string(),
            code,
            name,
            rate_limit_rpm: row.plan_rate_limit_rpm,
            rate_limit_burst: row.plan_rate_limit_burst,
            quota_monthly: row.plan_quota_monthly,
            token_budget_monthly: row.plan_token_budget_monthly,
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
        starts_at: py_iso(row.starts_at),
        expires_at: row.expires_at.map(py_iso),
        created_at: py_iso(row.created_at),
        updated_at: py_iso(row.updated_at),
        revoked_at: row.revoked_at.map(py_iso),
        effective_status: Some(effective_status(
            &row.status,
            row.starts_at,
            row.expires_at,
            now,
        )),
        metadata: row.license_metadata,
        metering_provisioned_at: row.metering_provisioned_at.map(py_iso),
        metering_verified_at: row.metering_verified_at.map(py_iso),
        token_budget_override: row.token_budget_override,
        quota_override: row.quota_override,
        is_dependency: row.is_dependency.unwrap_or(false),
        bundle_id: row.bundle_id.map(|id| id.to_string()),
        app,
        plan,
    }
}

/// The reference's static 500 body, named per route exactly as it names it.
fn internal_error(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
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

/// Pydantic's lax boolean parsing (`true/false/1/0/yes/no/on/off`, case-insensitive) for
/// the natively-typed `include_deleted` query parameter.
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

/// FastAPI's 422 for a malformed typed path parameter.
fn uuid_parsing(field: &str, raw: &str) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": "uuid_parsing",
            "loc": ["path", field],
            "msg": "Input should be a valid UUID",
            "input": raw,
        }]),
    )
}

/// FastAPI's 422 for one field of the JSON body.
fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

/// The resolved `LicenseBudgetUpdate`, in the reference's `int | None` terms.
struct BudgetUpdate {
    token_budget_override: Option<i64>,
    reason: Option<String>,
}

/// Validate the raw request body as Pydantic's `LicenseBudgetUpdate`.
///
/// Written out rather than derived so the refusals are FastAPI's 422 array — the shape
/// every generated client of this route already expects — instead of axum's extractor
/// rejection. `token_budget_override` missing is a 422, never a clear.
fn parse_budget_update(body: &Bytes) -> Result<BudgetUpdate, Refusal> {
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
    let token_budget_override = match object.get("token_budget_override") {
        None => {
            return Err(body_error(json!({
                "type": "missing",
                "loc": ["body", "token_budget_override"],
                "msg": "Field required",
                "input": value,
            })));
        }
        Some(Value::Null) => None,
        Some(Value::Number(number)) => {
            let Some(integer) = number.as_i64() else {
                return Err(body_error(json!({
                    "type": "int_from_float",
                    "loc": ["body", "token_budget_override"],
                    "msg": "Input should be a valid integer, got a number with a fractional part",
                    "input": number,
                })));
            };
            if integer < 0 {
                return Err(body_error(json!({
                    "type": "greater_than_equal",
                    "loc": ["body", "token_budget_override"],
                    "msg": "Input should be greater than or equal to 0",
                    "input": integer,
                    "ctx": {"ge": 0},
                })));
            }
            Some(integer)
        }
        Some(other) => {
            return Err(body_error(json!({
                "type": "int_type",
                "loc": ["body", "token_budget_override"],
                "msg": "Input should be a valid integer",
                "input": other,
            })));
        }
    };
    let reason = match object.get("reason") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(other) => {
            return Err(body_error(json!({
                "type": "string_type",
                "loc": ["body", "reason"],
                "msg": "Input should be a valid string",
                "input": other,
            })));
        }
    };
    Ok(BudgetUpdate {
        token_budget_override,
        reason,
    })
}

/// `GET /licenses` — list licences with optional filters, scoped to the caller's tenant.
///
/// Requires 'license-admin' or 'license-reader' role.
/// Filters by tenant_id from X-Tenant-Id header (or JWT sub for internal calls).
#[utoipa::path(
    get,
    path = "/licenses",
    tag = "licenses",
    operation_id = "list_licenses",
    summary = "List Licenses",
    description = "List all licenses with optional filters.\n\nRequires 'license-admin' or 'license-reader' role.\nFilters by tenant_id from X-Tenant-Id header (or JWT sub for internal calls). Soft-deleted licences are never returned here; the audit view is `GET /licenses/admin/all?include_deleted=true`.",
    params(
        LicensesQuery,
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
pub async fn list_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<LicensesQuery>,
) -> Result<Json<Vec<LicenseResponse>>, Refusal> {
    // FastAPI resolves the `get_tenant_id` dependency before the role dependency, so a
    // caller with no tenant context sees the 401 before any 403.
    let tenant = get_tenant_id(&state, &user).await?;
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;

    let subject_id = optional_uuid(query.subject_id.as_deref())?;
    let app_id = optional_uuid(query.app_id.as_deref())?;
    let plan_id = optional_uuid(query.plan_id.as_deref())?;
    let tenant_id =
        uuid::Uuid::try_parse(&tenant).map_err(|_| internal_error("Failed to list licenses"))?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list licenses"));
    };
    let filters = LicenceFilters {
        tenant_id: Some(tenant_id),
        subject_id,
        app_id,
        plan_id,
        status: query.status.as_deref().filter(|value| !value.is_empty()),
        // The per-tenant listing deliberately does not offer the audit view.
        include_deleted: false,
    };
    let rows = licence_admin::list_licenses(pool, &filters)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list licenses");
            internal_error("Failed to list licenses")
        })?;

    let now = Utc::now();
    Ok(Json(
        rows.into_iter().map(|row| response(row, now)).collect(),
    ))
}

/// `GET /licenses/admin/all` — list licences across ALL tenants. Requires `license-admin`.
///
/// Deliberately bypasses tenant filtering: the caller is the platform-admin surface on
/// portal-bff acting as a service account on behalf of a cross-tenant operator. Per-tenant
/// listing remains at `GET /licenses`.
#[utoipa::path(
    get,
    path = "/licenses/admin/all",
    tag = "licenses",
    operation_id = "list_all_licenses_admin",
    summary = "List All Licenses Admin",
    description = "List licenses across ALL tenants. Requires `license-admin` role.\n\nDeliberately bypasses tenant_id filtering. `include_deleted=true` is the AUDIT view: the retained row IS this service's record of the deletion, so `deleted_at` / `deleted_by` / `delete_reason` are only readable through it.",
    params(
        AdminLicensesQuery,
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
pub async fn list_admin_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<AdminLicensesQuery>,
) -> Result<Json<Vec<LicenseResponse>>, Refusal> {
    require_any_realm_role(&user, &["license-admin"])?;
    let include_deleted = match query.include_deleted.as_deref() {
        Some(raw) => parse_bool("include_deleted", raw)?,
        None => false,
    };

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list all licenses"));
    };
    let filters = LicenceFilters {
        tenant_id: None,
        subject_id: None,
        app_id: None,
        plan_id: None,
        status: query.status.as_deref().filter(|value| !value.is_empty()),
        include_deleted,
    };
    let rows = licence_admin::list_licenses(pool, &filters)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list all licenses");
            internal_error("Failed to list all licenses")
        })?;

    let now = Utc::now();
    Ok(Json(
        rows.into_iter().map(|row| response(row, now)).collect(),
    ))
}

/// `PATCH /licenses/{license_id}/budget` — set or clear a licence's own token budget.
///
/// Requires 'license-admin' AND the licence-write rule (`_authorize_issuance`): the
/// caller's owner must ISSUE the licence's app AND own the tenant or hold an ACTIVE
/// `tenant_app_grant` on it; `cross_owner` bypasses, audited.
///
/// `token_budget_override=null` clears the override and returns the licence to the plan's
/// budget. `reason` is optional and lands on the audit line. The column is written and
/// committed before the response; the reference's post-response billing push is not part
/// of this slice.
#[utoipa::path(
    patch,
    path = "/licenses/{license_id}/budget",
    tag = "licenses",
    operation_id = "set_license_budget",
    summary = "Set License Budget",
    description = "Set or clear a licence's own monthly token budget (`token_budget_override`).\n\nRequires 'license-admin' AND the licence-write rule (`_authorize_issuance`): the caller's owner must ISSUE the licence's app AND own the tenant or hold an ACTIVE `tenant_app_grant` on it; `cross_owner` bypasses, audited.\n\n`token_budget_override=null` clears the override and returns the licence to the plan's budget. `reason` is optional and lands on the audit line.",
    params(
        ("license_id" = String, Path, format = "uuid", description = "License Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = LicenseBudgetUpdate,
    responses(
        (status = 200, body = LicenseResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn set_license_budget(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(license_id): Path<String>,
    body: Bytes,
) -> Result<Json<LicenseResponse>, Refusal> {
    // `require_role("license-admin")`: realm role only, the reference's exact detail text.
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Ok(parsed_id) = uuid::Uuid::try_parse(&license_id) else {
        return Err(uuid_parsing("license_id", &license_id));
    };
    let license_id = parsed_id;
    let update = parse_budget_update(&body)?;
    // The reference's `int` is unbounded; the column is int4, so an out-of-range value
    // fails the write and the top-level handler answers its 500. Reproduce that status
    // before the database sees a value it cannot store.
    let token_budget_override = match update.token_budget_override {
        Some(value) => {
            Some(i32::try_from(value).map_err(|_| internal_error("Failed to set license budget"))?)
        }
        None => None,
    };

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to set license budget"));
    };

    // The licence is resolved BEFORE authorization, exactly as the reference does: an
    // unknown id is a 404 even for a caller who would fail the ownership rule.
    let Some(license) = licence_admin::get_license(pool, license_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to set license budget");
            internal_error("Failed to set license budget")
        })?
    else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };
    issuance::authorize_issuance(
        pool,
        &user,
        license.app_id,
        Some(license.tenant_id),
        "license_budget_set",
    )
    .await?;

    let previous = license.token_budget_override;
    let Some(updated) =
        licence_admin::set_token_budget_override(pool, license_id, token_budget_override)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to set license budget");
                internal_error("Failed to set license budget")
            })?
    else {
        // Raced a hard delete between the two statements.
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };

    // `license_budget_override_changed`: this service has no audit table, so the log line
    // IS the record (same mechanism as `budget_currency_changed`).
    tracing::info!(
        license = %updated.id,
        tenant = %updated.tenant_id,
        from = ?previous,
        to = ?token_budget_override,
        reason = ?update.reason,
        actor = %user.sub,
        azp = ?user.azp,
        realm = ?user.realm(),
        at = %updated.updated_at.to_rfc3339(),
        "license_budget_override_changed"
    );

    Ok(Json(response(updated, Utc::now())))
}

/// FastAPI's 422 entry for one query parameter.
fn query_error(field: &str, kind: &str, message: &str, input: Value) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": kind,
            "loc": ["query", field],
            "msg": message,
            "input": input,
        }]),
    )
}

/// `limit: int = Query(500, ge=1, le=5000)`.
fn parse_limit(raw: Option<&str>) -> Result<i64, Refusal> {
    let Some(value) = raw else {
        return Ok(500);
    };
    match value.parse::<i64>() {
        Ok(number) if number < 1 => Err(query_error(
            "limit",
            "greater_than_equal",
            "Input should be greater than or equal to 1",
            json!(number),
        )),
        Ok(number) if number > 5_000 => Err(query_error(
            "limit",
            "less_than_equal",
            "Input should be less than or equal to 5000",
            json!(number),
        )),
        Ok(number) => Ok(number),
        Err(_) => Err(query_error(
            "limit",
            "int_parsing",
            "Input should be a valid integer, unable to parse string as an integer",
            json!(value),
        )),
    }
}

/// The reference's `user.preferred_username or user.sub`, the sweep's audit actor.
fn actor(user: &User) -> &str {
    if user.preferred_username.is_empty() {
        &user.sub
    } else {
        &user.preferred_username
    }
}

/// `POST /licenses/admin/metering/reconcile` — measure each live licence's metering coverage
/// AGAINST BILLING and repair it, estate-wide.
///
/// Requires `license-admin` (realm role only). Deliberately cross-tenant, like
/// `GET /licenses/admin/all`: the caller is the platform-admin surface acting for a cross-tenant
/// operator, and the gap this repairs is estate-wide.
///
/// A THIN WRAPPER in code: it contributes exactly the guard, the five query parameters and the
/// actor name. All behaviour lives in `run_metering_reconcile`, the same runner the scheduled
/// `reconcile-metering` command uses, so the route and the CronJob cannot drift apart. The
/// scheduled path exists because the licences service account does NOT hold `license-admin` and
/// never should.
///
/// `license_id` NARROWS the selection and never widens it; `revalidate=true` re-examines
/// already-stamped licences; `dry_run=true` measures and writes nothing; `force_plan_quota=true`
/// is the default-off, DESTRUCTIVE opt-in to push the effective quota over a divergent billing
/// row.
#[utoipa::path(
    post,
    path = "/licenses/admin/metering/reconcile",
    tag = "licenses",
    operation_id = "reconcile_metering_provisioning",
    summary = "Reconcile Metering Provisioning",
    description = "Measure each live licence's metering coverage AGAINST BILLING, and repair it.\n\nRequires `license-admin`. Deliberately cross-tenant, same as `GET /licenses/admin/all` — the caller is the platform-admin surface acting for a cross-tenant operator, and the gap this repairs is estate-wide.\n\nA THIN WRAPPER in code: it contributes exactly two things, the `license-admin` check and the actor name for the audit log line. All behaviour lives in `run_metering_reconcile`, so this route and the scheduled `reconcile-metering` command cannot drift apart.",
    params(
        ReconcileMeteringQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = MeteringReconcileResult),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn reconcile_metering_provisioning(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(raw): Query<BTreeMap<String, String>>,
    metering: Option<Extension<Arc<MeteringProvisioner>>>,
) -> Result<Json<MeteringReconcileResult>, Refusal> {
    // `require_any_role("license-admin")`: realm roles only, the reference's exact detail text.
    // FastAPI solves this dependency BEFORE it validates query parameters, so it wins over a 422.
    require_any_realm_role(&user, &["license-admin"])?;

    let limit = parse_limit(raw.get("limit").map(String::as_str))?;
    let revalidate = match raw.get("revalidate") {
        Some(value) => parse_bool("revalidate", value)?,
        None => false,
    };
    let dry_run = match raw.get("dry_run") {
        Some(value) => parse_bool("dry_run", value)?,
        None => false,
    };
    let force_plan_quota = match raw.get("force_plan_quota") {
        Some(value) => parse_bool("force_plan_quota", value)?,
        None => false,
    };
    // The runner's own refusal: a typo'd canary id must never be read as "that licence is
    // already fine". An empty value is a string, so it is refused exactly like any other
    // non-UUID instead of quietly meaning "no filter".
    let license_ids = match raw.get("license_id") {
        None => None,
        Some(value) => match uuid::Uuid::try_parse(value) {
            Ok(id) => Some(vec![id]),
            Err(_) => {
                return Err(Refusal::detail(
                    StatusCode::BAD_REQUEST,
                    "license_id must be a UUID",
                ));
            }
        },
    };

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Metering reconcile scan failed"));
    };
    let Some(Extension(billing)) = metering else {
        return Err(internal_error("Metering reconcile is not configured"));
    };
    let result = run_metering_reconcile(
        pool,
        limit,
        revalidate,
        dry_run,
        force_plan_quota,
        license_ids.as_deref(),
        actor(&user),
        billing.as_ref(),
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "Metering reconcile scan failed");
        internal_error("Metering reconcile scan failed")
    })?;
    Ok(Json(result))
}
