//! The six catalogue APP routes.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Route | reference |
//! |---|---|
//! | `GET /apps` | `main.py:9549` `list_apps` |
//! | `POST /apps` | `main.py:6920` `create_app` |
//! | `GET /apps/{app_id}` | `main.py:9575` `get_app` |
//! | `DELETE /apps/{app_id}` | `main.py:10063` `delete_app` |
//! | `GET /apps/{app_id}/plans` | `main.py:9613` `list_app_plans` |
//! | `GET /apps/{app_id}/licenses` | `main.py:9664` `list_app_licenses` |
//!
//! Authorization is the reference's, literally:
//!
//! - the four reads need the `license-admin` OR `license-reader` REALM role;
//! - `POST` and `DELETE` need `license-admin` alone, and `POST` additionally needs a
//!   KNOWN owning client (`_require_known_owner`), which becomes the app's issuer;
//! - `DELETE` additionally needs the caller's owner to ISSUE the app
//!   (`app_not_administrable_by_caller`, checked after the 404 and BEFORE the 409);
//!   `cross_owner` bypasses, audited.
//!
//! `GET /apps/{app_id}/licenses` is NOT a cross-tenant read for a `license-reader`: apps
//! are a global catalogue, so the app id says nothing about tenancy and the caller's own
//! tenant is the filter (the leak closed 2026-07-27). A `license-admin` keeps the
//! cross-tenant operator view.
//!
//! Every refusal is the route envelope (`{"detail": …}`, see `error.rs`).

use std::collections::BTreeMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::db::catalogue_apps::{self, AppRow, PlanRow};
use crate::db::issuance;
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::internal_reads::Audience;
use crate::router::licence_admin::{parse_bool, response as licence_response};
use crate::router::schemas::HTTPValidationError;
use crate::router::user_licenses::LicenseResponse;
use crate::state::AppState;

const READ_ROLES: [&str; 2] = ["license-admin", "license-reader"];

/// `AppCreate.audience`: the same closed enum, carrying the create-time default.
///
/// A separate type from [`Audience`] because the default belongs to the REQUEST only: a
/// field-level `default` on an inlined enum makes utoipa emit an `allOf` overlay, which is
/// not the captured shape. On the type, with the field merely made optional, it is.
#[derive(Debug, Clone, Copy, serde::Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(default = "platform")]
pub enum AppCreateAudience {
    Human,
    Platform,
    Automation,
}

/// `AppCreate` (`main.py:3107`): the create body, field for field.
///
/// `issuers` omitted = the caller's own owner; only a `cross_owner` client may name
/// another. `audience` defaults to `platform` — a new app is not offered to people until
/// declared. The `min_length=1` on `issuers` is validation, not schema: the contract
/// projection drops array bounds, and the manual parser below still refuses `[]`.
#[derive(Debug, ToSchema)]
pub struct AppCreate {
    /// Unique app key
    pub key: String,
    /// App name
    pub name: String,
    /// App description
    pub description: Option<String>,
    #[schema(inline, required = false)]
    pub audience: AppCreateAudience,
    /// Owning apps (tenant_owner keys) that may issue this app's licences. Omitted = the
    /// caller's own owning app.
    pub issuers: Option<Vec<String>>,
}

/// Values the manual body parser resolved. `audience` is already the closed enum.
struct AppCreateValues {
    key: String,
    name: String,
    description: Option<String>,
    audience: AppCreateAudience,
    issuers: Option<Vec<String>>,
}

/// `AppResponse` (`main.py:3136`), the captured required set.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct AppResponse {
    pub id: String,
    pub key: String,
    pub name: String,
    /// Nullable AND required: Pydantic serialises the explicit null.
    #[schema(required = true)]
    pub description: Option<String>,
    pub revoked: bool,
    #[schema(inline)]
    pub audience: Audience,
    pub created_at: String,
}

/// `PlanResponse` (`main.py:3230`), the captured required set.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct PlanResponse {
    pub id: String,
    pub app_id: String,
    pub code: String,
    pub name: String,
    /// Nullable AND required (Pydantic has no default on it).
    #[schema(required = true)]
    pub description: Option<String>,
    /// Nullable AND required (global plan = null).
    #[schema(required = true)]
    pub region_id: Option<String>,
    /// Nullable AND required (perpetual plan = null).
    #[schema(required = true)]
    pub duration_days: Option<i32>,
    /// Defaulted to null, so NOT required in the capture.
    pub rate_limit_rpm: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub quota_monthly: Option<i32>,
    pub token_budget_monthly: Option<i32>,
    pub revoked: bool,
    pub created_at: String,
}

/// `GET /apps/{app_id}/plans` query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AppPlansQuery {
    /// Also return revoked (retired) plans. Default false: the listing is the catalogue of
    /// plans a new licence can be put on.
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_revoked: Option<String>,
}

/// `DELETE /apps/{app_id}` query parameters, documentation-only: the handler extracts the
/// raw map so an invalid value is FastAPI's 422 body, not axum's rejection.
///
/// `nullable` is the capture's: Pydantic spells an optional bounded integer as
/// `anyOf: [{type: integer, minimum: 0}, {type: "null"}]`, not a bare nullable-free integer.
#[derive(IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteAppQuery {
    /// Acknowledge the blast radius: the exact live licence count, as returned by an
    /// unconfirmed 409.
    #[param(nullable = true, minimum = 0)]
    pub confirm_licenses: Option<i64>,
}

/// The reference's static 500 body, named per route exactly as it names it.
fn internal_error(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}

fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

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

fn required_string(
    object: &Map<String, Value>,
    field: &str,
    whole: &Value,
) -> Result<String, Refusal> {
    match object.get(field) {
        None => Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": whole,
        }))),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(other) => Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", field],
            "msg": "Input should be a valid string",
            "input": other,
        }))),
    }
}

fn literal_error(input: Value) -> Refusal {
    body_error(json!({
        "type": "literal_error",
        "loc": ["body", "audience"],
        "msg": "Input should be 'human', 'platform' or 'automation'",
        "input": input,
        "ctx": {"expected": "'human', 'platform' or 'automation'"},
    }))
}

/// Validate the raw body as Pydantic's `AppCreate`.
///
/// Written out rather than derived so a refusal is FastAPI's 422 array (the shape every
/// generated client of this route expects) instead of an axum rejection. `issuers: []` is
/// refused by the model's `min_length=1`; only an ABSENT or null `issuers` falls back to
/// the caller's owner.
fn parse_app_create(body: &Bytes) -> Result<AppCreateValues, Refusal> {
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

    let key = required_string(object, "key", &value)?;
    let name = required_string(object, "name", &value)?;
    let description = match object.get("description") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(other) => {
            return Err(body_error(json!({
                "type": "string_type",
                "loc": ["body", "description"],
                "msg": "Input should be a valid string",
                "input": other,
            })));
        }
    };
    let audience = match object.get("audience") {
        None => AppCreateAudience::Platform,
        Some(Value::String(raw)) => match raw.as_str() {
            "human" => AppCreateAudience::Human,
            "platform" => AppCreateAudience::Platform,
            "automation" => AppCreateAudience::Automation,
            _ => return Err(literal_error(Value::String(raw.clone()))),
        },
        Some(other) => return Err(literal_error(other.clone())),
    };
    let issuers = match object.get("issuers") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => {
            if items.is_empty() {
                return Err(body_error(json!({
                    "type": "too_short",
                    "loc": ["body", "issuers"],
                    "msg": "List should have at least 1 item after validation, not 0",
                    "input": items,
                    "ctx": {"min_length": 1},
                })));
            }
            let mut out = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                match item {
                    Value::String(text) => out.push(text.clone()),
                    other => {
                        return Err(body_error(json!({
                            "type": "string_type",
                            "loc": ["body", "issuers", index],
                            "msg": "Input should be a valid string",
                            "input": other,
                        })));
                    }
                }
            }
            Some(out)
        }
        Some(other) => {
            return Err(body_error(json!({
                "type": "list_type",
                "loc": ["body", "issuers"],
                "msg": "Input should be a valid list",
                "input": other,
            })));
        }
    };

    Ok(AppCreateValues {
        key,
        name,
        description,
        audience,
        issuers,
    })
}

fn audience_wire(audience: AppCreateAudience) -> &'static str {
    match audience {
        AppCreateAudience::Human => "human",
        AppCreateAudience::Platform => "platform",
        AppCreateAudience::Automation => "automation",
    }
}

fn audience_from_db(raw: &str) -> Option<Audience> {
    match raw {
        "human" => Some(Audience::Human),
        "platform" => Some(Audience::Platform),
        "automation" => Some(Audience::Automation),
        _ => None,
    }
}

fn app_response(row: AppRow) -> Option<AppResponse> {
    Some(AppResponse {
        id: row.id.to_string(),
        key: row.key,
        name: row.name,
        description: row.description,
        revoked: row.revoked,
        audience: audience_from_db(&row.audience)?,
        created_at: py_iso(row.created_at),
    })
}

fn plan_response(row: PlanRow) -> PlanResponse {
    PlanResponse {
        id: row.id.to_string(),
        app_id: row.app_id.to_string(),
        code: row.code,
        name: row.name,
        description: row.description,
        region_id: row.region_id.map(|id| id.to_string()),
        duration_days: row.duration_days,
        rate_limit_rpm: row.rate_limit_rpm,
        rate_limit_burst: row.rate_limit_burst,
        quota_monthly: row.quota_monthly,
        token_budget_monthly: row.token_budget_monthly,
        revoked: row.revoked,
        created_at: py_iso(row.created_at),
    }
}

fn app_json(status: StatusCode, row: AppRow, message: &'static str) -> Result<Response, Refusal> {
    let key = row.key.clone();
    let Some(body) = app_response(row) else {
        // Pydantic refuses a value outside the audience Literal; the reference's
        // top-level except turns that into the route's static 500.
        tracing::error!(key = %key, "app row has an audience outside ck_app_audience");
        return Err(internal_error(message));
    };
    Ok((status, Json(body)).into_response())
}

/// `UUID(app_id)` first, then the registry key — the reference's exact order, so a key
/// that happens to parse as a UUID is only ever looked up by id.
async fn find_app(pool: &PgPool, raw: &str) -> Result<Option<AppRow>, sqlx::Error> {
    match Uuid::try_parse(raw) {
        Ok(id) => catalogue_apps::get_app(pool, id).await,
        Err(_) => catalogue_apps::get_app_by_key(pool, raw).await,
    }
}

/// `GET /apps` — list the non-revoked catalogue, `ORDER BY key`.
#[utoipa::path(
    get,
    path = "/apps",
    tag = "apps",
    operation_id = "list_apps",
    summary = "List Apps",
    description = "List all apps\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<AppResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_apps(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
) -> Result<Json<Vec<AppResponse>>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list apps"));
    };
    let rows = catalogue_apps::list_apps(pool).await.map_err(|error| {
        tracing::error!(%error, "Failed to list apps");
        internal_error("Failed to list apps")
    })?;
    let body = rows
        .into_iter()
        .map(|row| app_response(row).ok_or_else(|| internal_error("Failed to list apps")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(body))
}

/// `POST /apps` — register an app and its issuers, idempotently by key.
#[utoipa::path(
    post,
    path = "/apps",
    tag = "apps",
    operation_id = "create_app",
    summary = "Create App",
    description = "Create a new app (idempotent — returns existing app if key matches)\n\nRequires 'license-admin' role",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = AppCreate,
    responses(
        (status = 200, body = AppResponse),
        (status = 201, body = AppResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_app(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    body: Bytes,
) -> Result<Response, Refusal> {
    // `require_role("license-admin")`: realm role only, the reference's exact detail text.
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let app = parse_app_create(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to create app"));
    };

    // Idempotent BEFORE any issuer/owner check: an existing key is returned unchanged.
    let existing = catalogue_apps::get_app_by_key(pool, &app.key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create app");
            internal_error("Failed to create app")
        })?;
    if let Some(row) = existing {
        return app_json(StatusCode::OK, row, "Failed to create app");
    }

    let caller_owner = issuance::require_known_owner(&user)?;
    let issuers = app.issuers.unwrap_or_else(|| vec![caller_owner.clone()]);

    // Issuer squatting: naming ANOTHER owner as issuer would let any license-admin client
    // hand that owner (or deny it) an app it never registered. Only the caller's own owner
    // may be named; cross_owner (break-glass) keeps full freedom, audited.
    let mut foreign: Vec<String> = issuers
        .iter()
        .filter(|owner| **owner != caller_owner)
        .cloned()
        .collect();
    foreign.sort();
    foreign.dedup();
    if !foreign.is_empty() {
        if !user.cross_owner {
            tracing::warn!(
                action = "issuer_not_caller_owner",
                key = %app.key,
                owner = %caller_owner,
                issuers = %foreign.join(","),
                azp = ?user.azp,
                "issuer_not_caller_owner: refused app create"
            );
            return Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                json!({
                    "error": "issuer_not_caller_owner",
                    "issuers": foreign,
                    "message": format!("owner '{caller_owner}' may only name itself as an issuer"),
                }),
            ));
        }
        issuance::audit_cross_owner(
            &user,
            "app_create_foreign_issuers",
            None,
            &format!("key={} issuers={}", app.key, foreign.join(",")),
        );
    }

    let mut unknown: Vec<String> = Vec::new();
    for owner in &issuers {
        let exists = catalogue_apps::owner_exists(pool, owner)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to create app");
                internal_error("Failed to create app")
            })?;
        if !exists {
            unknown.push(owner.clone());
        }
    }
    if let Some(first) = unknown.first() {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            json!({
                "error": "owner_not_found",
                "owner_key": first,
                "message": format!("issuer(s) are not tenant_owner keys: {}", unknown.join(", ")),
            }),
        ));
    }

    match catalogue_apps::create_app(
        pool,
        &app.key,
        &app.name,
        app.description.as_deref(),
        audience_wire(app.audience),
        &issuers,
    )
    .await
    {
        Ok(row) => {
            tracing::info!(app = %row.id, key = %row.key, "App created");
            app_json(StatusCode::CREATED, row, "Failed to create app")
        }
        // Lost the read-then-insert race on the unique `apps.key` index. 200-with-the-
        // existing-row, not 409: the docstring promises idempotence and the caller cannot
        // tell whether it lost the race or arrived second. Re-raise when the re-read finds
        // nothing, because then the violated constraint was NOT the key race.
        Err(sqlx::Error::Database(database))
            if database.code().is_some_and(|code| code.starts_with("23")) =>
        {
            match catalogue_apps::get_app_by_key(pool, &app.key).await {
                Ok(Some(row)) => {
                    tracing::info!(app = %row.id, key = %row.key, "App already exists; returning it");
                    app_json(StatusCode::OK, row, "Failed to create app")
                }
                _ => Err(internal_error("Failed to create app")),
            }
        }
        Err(error) => {
            tracing::error!(%error, "Failed to create app");
            Err(internal_error("Failed to create app"))
        }
    }
}

/// `GET /apps/{app_id}` — by UUID first, then by key; unknown is 404.
#[utoipa::path(
    get,
    path = "/apps/{app_id}",
    tag = "apps",
    operation_id = "get_app",
    summary = "Get App",
    description = "Get app by ID\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        ("app_id" = String, Path, description = "App Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = AppResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn get_app(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(app_id): Path<String>,
) -> Result<Json<AppResponse>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to get app"));
    };
    let row = find_app(pool, &app_id).await.map_err(|error| {
        tracing::error!(%error, "Failed to get app");
        internal_error("Failed to get app")
    })?;
    let Some(row) = row else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "App not found"));
    };
    let body = app_response(row).ok_or_else(|| internal_error("Failed to get app"))?;
    Ok(Json(body))
}

/// `GET /apps/{app_id}/plans` — the app's plans, revoked excluded unless asked.
#[utoipa::path(
    get,
    path = "/apps/{app_id}/plans",
    tag = "apps",
    operation_id = "list_app_plans",
    summary = "List App Plans",
    description = "List all plans for a specific app\n\nRevoked plans are excluded unless ``include_revoked=true``.\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        AppPlansQuery,
        ("app_id" = String, Path, description = "App Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<PlanResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_app_plans(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(app_id): Path<String>,
    Query(query): Query<AppPlansQuery>,
) -> Result<Json<Vec<PlanResponse>>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let include_revoked = match query.include_revoked.as_deref() {
        Some(raw) => parse_bool("include_revoked", raw)?,
        None => false,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list app plans"));
    };
    let app = find_app(pool, &app_id).await.map_err(|error| {
        tracing::error!(%error, "Failed to list app plans");
        internal_error("Failed to list app plans")
    })?;
    let Some(app) = app else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "App not found"));
    };
    let rows = catalogue_apps::list_plans(pool, app.id, include_revoked)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list app plans");
            internal_error("Failed to list app plans")
        })?;
    Ok(Json(rows.into_iter().map(plan_response).collect()))
}

/// `GET /apps/{app_id}/licenses` — the app's licences; a non-admin sees only its tenant.
#[utoipa::path(
    get,
    path = "/apps/{app_id}/licenses",
    tag = "apps",
    operation_id = "list_app_licenses",
    summary = "List App Licenses",
    description = "List all licenses for a specific app\n\nRequires 'license-admin' or 'license-reader' role.\n\nNon-admin callers see only their own tenant's licenses; a ``license-admin`` remains the cross-tenant operator surface.",
    params(
        ("app_id" = String, Path, description = "App Id"),
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
pub async fn list_app_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(app_id): Path<String>,
) -> Result<Json<Vec<LicenseResponse>>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list app licenses"));
    };
    // The app is resolved BEFORE the tenant filter, exactly as the reference: an unknown
    // app is a 404 even for a caller with no tenant context.
    let app = find_app(pool, &app_id).await.map_err(|error| {
        tracing::error!(%error, "Failed to list app licenses");
        internal_error("Failed to list app licenses")
    })?;
    let Some(app) = app else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "App not found"));
    };

    let tenant_id = if user.has_realm_role("license-admin") {
        None
    } else {
        let tenant = get_tenant_id(&state, &user).await?;
        Some(Uuid::try_parse(&tenant).map_err(|_| internal_error("Failed to list app licenses"))?)
    };
    let rows = catalogue_apps::list_app_licences(pool, app.id, tenant_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list app licenses");
            internal_error("Failed to list app licenses")
        })?;
    let now = Utc::now();
    Ok(Json(
        rows.into_iter()
            .map(|row| licence_response(row, now))
            .collect(),
    ))
}

/// `DELETE /apps/{app_id}` — blast-radius-gated, cross-tenant hard delete.
#[utoipa::path(
    delete,
    path = "/apps/{app_id}",
    tag = "apps",
    operation_id = "delete_app",
    summary = "Delete App",
    description = "Hard-delete an app and cascade to its plans, licenses, and tokens.",
    params(
        DeleteAppQuery,
        ("app_id" = String, Path, description = "App Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn delete_app(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(app_id): Path<String>,
    Query(raw): Query<BTreeMap<String, String>>,
) -> Result<StatusCode, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Ok(uid) = Uuid::try_parse(&app_id) else {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "Invalid app_id format",
        ));
    };
    let confirm_licenses = match raw.get("confirm_licenses") {
        None => None,
        Some(value) => match value.parse::<i64>() {
            Ok(number) if number < 0 => {
                return Err(query_error(
                    "confirm_licenses",
                    "greater_than_equal",
                    "Input should be greater than or equal to 0",
                    json!(number),
                ));
            }
            Ok(number) => Some(number),
            Err(_) => {
                return Err(query_error(
                    "confirm_licenses",
                    "int_parsing",
                    "Input should be a valid integer, unable to parse string as an integer",
                    json!(value),
                ));
            }
        },
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to delete app"));
    };

    let radius = catalogue_apps::app_cascade_radius(pool, uid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete app");
            internal_error("Failed to delete app")
        })?;
    let Some(radius) = radius else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "App not found"));
    };
    // Before the 409: a non-issuer must not learn the blast radius either.
    authorize_app_admin(pool, &user, uid, "app_delete", "Failed to delete app").await?;

    if radius.licenses > 0 && confirm_licenses != Some(radius.licenses) {
        // Fail CLOSED. The destruction is unrecoverable and this service has no audit
        // table, so the refusal — and the counts in it — are the only chance an operator
        // gets to see the radius before it happens.
        let reason = if confirm_licenses.is_some() {
            "cascade_count_mismatch"
        } else {
            "unconfirmed"
        };
        tracing::warn!(
            app_id = %uid,
            reason = %reason,
            licenses = radius.licenses,
            tokens = radius.tokens,
            plans = radius.plans,
            tenants = radius.tenants.len(),
            confirm_licenses = ?confirm_licenses,
            actor = %user.sub,
            "app_delete_refused"
        );
        return Err(Refusal::detail(
            StatusCode::CONFLICT,
            json!({
                "error": if confirm_licenses.is_some() { "cascade_count_mismatch" } else { "cascade_requires_confirmation" },
                "app_id": uid.to_string(),
                "licenses": radius.licenses,
                "tokens": radius.tokens,
                "plans": radius.plans,
                "tenants": radius.tenants,
                "remedy": format!(
                    "re-issue with ?confirm_licenses={} to destroy {} licence(s) across {} tenant(s)",
                    radius.licenses,
                    radius.licenses,
                    radius.tenants.len()
                ),
            }),
        ));
    }

    if radius.licenses > 0 {
        // A confirmed cross-tenant hard delete is the single most destructive thing this
        // API can do and nothing else records it.
        tracing::error!(
            app_id = %uid,
            licenses = radius.licenses,
            tokens = radius.tokens,
            plans = radius.plans,
            tenants = %radius.tenants.join(","),
            actor = %user.sub,
            "app_delete_confirmed"
        );
    }

    let deleted = catalogue_apps::delete_app(pool, uid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete app");
            internal_error("Failed to delete app")
        })?;
    if !deleted {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "App not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `_authorize_app_admin`: the caller's owner must ISSUE the app. `cross_owner` bypasses,
/// audited. A DB fault is the route's 500, never a pass.
///
/// Shared with the catalogue PLAN routes; `message` is the calling route's static 500 text.
pub(crate) async fn authorize_app_admin(
    pool: &PgPool,
    user: &User,
    app_id: Uuid,
    action: &str,
    message: &'static str,
) -> Result<(), Refusal> {
    let owner = issuance::require_known_owner(user)?;
    let issuers = issuance::app_issuers(pool, app_id).await.map_err(|error| {
        tracing::error!(%error, message, "app admin authorization failed");
        internal_error(message)
    })?;
    if issuers.iter().any(|issuer| issuer == &owner) {
        return Ok(());
    }
    if user.cross_owner {
        issuance::audit_cross_owner(user, action, None, &format!("app_id={app_id}"));
        return Ok(());
    }
    tracing::warn!(
        action = %action,
        owner = %owner,
        app_id = %app_id,
        azp = ?user.azp,
        "app_not_administrable_by_caller"
    );
    Err(Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "app_not_administrable_by_caller",
            "message": format!("owner '{owner}' does not issue this app and may not administer it"),
        }),
    ))
}
