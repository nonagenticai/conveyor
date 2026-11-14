//! The seven catalogue PLAN and REGION routes.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Route | reference |
//! |---|---|
//! | `POST /plans` | `main.py:9733` `create_plan` |
//! | `PATCH /plans/{plan_id}` | `main.py:9796` `update_plan` |
//! | `GET /plans` | `main.py:9874` `list_plans` |
//! | `GET /plans/{plan_id}` | `main.py:9909` `get_plan` |
//! | `DELETE /plans/{plan_id}` | `main.py:10188` `delete_plan` |
//! | `POST /regions` | `main.py:9958` `create_region` |
//! | `GET /regions` | `main.py:10014` `list_regions` |
//!
//! Authorization is the reference's, literally:
//!
//! - every route needs the `license-admin` role OR, for the three reads,
//!   `license-admin`/`license-reader`;
//! - the three writes additionally need the caller's owner to ISSUE the plan's app
//!   (`_authorize_app_admin`, 403 `app_not_administrable_by_caller`; `cross_owner` bypasses,
//!   audited). `create_plan` resolves the app (404) BEFORE that check and the region (404)
//!   AFTER it — the reference's order.
//!
//! `PlanResponse` is the SAME schema `GET /apps/{app_id}/plans` publishes, so it is reused from
//! [`crate::router::catalogue_apps`] rather than declared twice (utoipa names components by
//! type name; a second `PlanResponse` would be a duplicate component).
//!
//! Bodies are parsed by hand, as in the catalogue APP routes: a refusal must be FastAPI's 422
//! array, not an axum rejection.
//!
//! The plan update's changed `token_budget_monthly` is re-pushed to billing AFTER the response,
//! through [`catalogue_plans::push_effective_token_budgets`]; see that function for the exact
//! selection and failure contract.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::{User, require_any_realm_role};
use crate::db::catalogue_apps::{self, PlanRow};
use crate::db::catalogue_plans::{self, DeletePlanOutcome, NewPlan, PlanChanges};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::catalogue_apps::{PlanResponse, authorize_app_admin};
use crate::router::health::ErrorResponse;
use crate::router::licence_admin::parse_bool;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

const READ_ROLES: [&str; 2] = ["license-admin", "license-reader"];

/// `PlanCreate` (`main.py:3154`), the captured schema.
///
/// `quota_monthly` / `token_budget_monthly` default to the METERED values (1,000,000 and
/// 50,000,000) when omitted; an explicit `null` is the only way to ask for unlimited. The
/// defaults live in the parser, not on these fields: Pydantic places them beside the nullable
/// `anyOf`, which the contract projection ignores, and duplicating them here would invite a
/// second source of truth.
#[derive(Debug, ToSchema)]
pub struct PlanCreate {
    /// App ID
    pub app_id: Uuid,
    /// Plan code (e.g., 'test', 'trial', 'full')
    pub code: String,
    /// Plan name
    pub name: String,
    /// Plan description
    pub description: Option<String>,
    /// Region ID (null for global)
    pub region_id: Option<Uuid>,
    /// Duration in days (null for perpetual)
    pub duration_days: Option<i32>,
    /// Requests per minute (null for default 60)
    pub rate_limit_rpm: Option<i32>,
    /// Burst allowance (null for default 10)
    pub rate_limit_burst: Option<i32>,
    /// Monthly request quota; 0 = block all, null = unlimited.
    #[schema(minimum = 0)]
    pub quota_monthly: Option<i32>,
    /// Monthly LLM token budget; 0 = block all, null = unlimited.
    #[schema(minimum = 0)]
    pub token_budget_monthly: Option<i32>,
}

/// `PlanUpdate` (`main.py:3200`), the captured schema: every field optional, and an explicit
/// `null` is a real value (it clears the column) except on `revoked`, which the route refuses.
#[derive(Debug, ToSchema)]
pub struct PlanUpdate {
    /// Plan name
    pub name: Option<String>,
    /// Plan description
    pub description: Option<String>,
    /// Duration in days
    pub duration_days: Option<i32>,
    /// Requests per minute
    pub rate_limit_rpm: Option<i32>,
    /// Burst allowance
    pub rate_limit_burst: Option<i32>,
    /// Monthly request quota; 0 = block all, null = unlimited
    #[schema(minimum = 0)]
    pub quota_monthly: Option<i32>,
    /// Monthly LLM token budget; 0 = block all, null = unlimited
    #[schema(minimum = 0)]
    pub token_budget_monthly: Option<i32>,
    /// Retire (true) or un-retire (false) the plan.
    pub revoked: Option<bool>,
}

/// `RegionCreate` (`main.py:3082`).
#[derive(Debug, ToSchema)]
pub struct RegionCreate {
    /// Region code (e.g., 'us', 'eu')
    pub code: String,
    /// Region name
    pub name: String,
    /// Whether region is active
    #[schema(required = false, default = true)]
    pub active: bool,
}

/// `RegionResponse` (`main.py:3090`), the captured required set.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct RegionResponse {
    pub id: String,
    pub code: String,
    pub name: String,
    pub active: bool,
    pub created_at: String,
}

/// `GET /plans` query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlansQuery {
    /// Filter by app id
    #[param(nullable = true)]
    pub app_id: Option<String>,
    /// Filter by region id
    #[param(nullable = true)]
    pub region_id: Option<String>,
}

/// `GET /regions` query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegionsQuery {
    /// Only list active regions
    #[serde(default)]
    #[param(value_type = bool, default = true)]
    pub active_only: Option<String>,
}

/// The reference's static 500 body, named per route exactly as it names it.
fn internal_error(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}

fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
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

/// `Option<T>` presence-aware: absent → `None`; null → `Some(None)`; value → `Some(Some(v))`.
fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Option<String>>, Refusal> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(text)) => Ok(Some(Some(text.clone()))),
        Some(other) => Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", field],
            "msg": "Input should be a valid string",
            "input": other,
        }))),
    }
}

/// Required, non-null UUID (`app_id`).
fn required_uuid(object: &Map<String, Value>, field: &str, whole: &Value) -> Result<Uuid, Refusal> {
    match object.get(field) {
        None => Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": whole,
        }))),
        Some(value) => parse_uuid(field, value),
    }
}

fn optional_uuid(object: &Map<String, Value>, field: &str) -> Result<Option<Uuid>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => parse_uuid(field, value).map(Some),
    }
}

fn parse_uuid(field: &str, value: &Value) -> Result<Uuid, Refusal> {
    match value {
        Value::String(raw) => Uuid::try_parse(raw).map_err(|_| {
            let (found, position) = raw
                .char_indices()
                .find(|(_, character)| !character.is_ascii_hexdigit() && *character != '-')
                .map(|(index, character)| (character.to_string(), index + 1))
                .unwrap_or_else(|| ("the end of the input".to_owned(), raw.chars().count() + 1));
            body_error(json!({
                "type": "uuid_parsing",
                "loc": ["body", field],
                "msg": format!(
                    "Input should be a valid UUID, invalid character: expected an optional \
                     prefix of `urn:uuid:` followed by [0-9a-fA-F-], found `{found}` at {position}"
                ),
                "input": raw,
            }))
        }),
        other => Err(body_error(json!({
            "type": "uuid_type",
            "loc": ["body", field],
            "msg": "Input should be a valid UUID",
            "input": other,
        }))),
    }
}

/// `Option<i32>` presence-aware, with Pydantic's lax coercion (numeric strings are accepted)
/// and an optional `ge=0` bound. `field` names both the loc and, in `bound`, the bound.
fn optional_int(
    object: &Map<String, Value>,
    field: &str,
    minimum_zero: bool,
) -> Result<Option<Option<i32>>, Refusal> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(None));
    }
    let parsed = match value {
        Value::Number(number) => match number.as_i64() {
            Some(int) => int,
            None => match number.as_f64() {
                Some(float) if float.fract() == 0.0 => float as i64,
                _ => {
                    return Err(body_error(json!({
                        "type": "int_from_float",
                        "loc": ["body", field],
                        "msg": "Input should be a valid integer, got a number with a fractional part",
                        "input": value,
                    })));
                }
            },
        },
        Value::String(text) => match text.trim().parse::<i64>() {
            Ok(int) => int,
            Err(_) => {
                return Err(body_error(json!({
                    "type": "int_parsing",
                    "loc": ["body", field],
                    "msg": "Input should be a valid integer, unable to parse string as an integer",
                    "input": value,
                })));
            }
        },
        other => {
            return Err(body_error(json!({
                "type": "int_type",
                "loc": ["body", field],
                "msg": "Input should be a valid integer",
                "input": other,
            })));
        }
    };
    let Ok(int) = i32::try_from(parsed) else {
        return Err(body_error(json!({
            "type": "int_parsing",
            "loc": ["body", field],
            "msg": "Input should be a valid integer, unable to parse string as an integer",
            "input": value,
        })));
    };
    if minimum_zero && int < 0 {
        return Err(body_error(json!({
            "type": "greater_than_equal",
            "loc": ["body", field],
            "msg": "Input should be greater than or equal to 0",
            "input": int,
            "ctx": {"ge": 0},
        })));
    }
    Ok(Some(Some(int)))
}

/// `Option<bool>` presence-aware; Pydantic's lax strings/numerals are honoured.
fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Option<bool>>, Refusal> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    match value {
        Value::Null => Ok(Some(None)),
        Value::Bool(flag) => Ok(Some(Some(*flag))),
        Value::Number(number) if number.as_i64() == Some(0) => Ok(Some(Some(false))),
        Value::Number(number) if number.as_i64() == Some(1) => Ok(Some(Some(true))),
        Value::String(text)
            if text.eq_ignore_ascii_case("true")
                || text == "1"
                || text.eq_ignore_ascii_case("yes")
                || text.eq_ignore_ascii_case("on") =>
        {
            Ok(Some(Some(true)))
        }
        Value::String(text)
            if text.eq_ignore_ascii_case("false")
                || text == "0"
                || text.eq_ignore_ascii_case("no")
                || text.eq_ignore_ascii_case("off") =>
        {
            Ok(Some(Some(false)))
        }
        Value::String(_) | Value::Number(_) => Err(body_error(json!({
            "type": "bool_parsing",
            "loc": ["body", field],
            "msg": "Input should be a valid boolean, unable to interpret input",
            "input": value,
        }))),
        other => Err(body_error(json!({
            "type": "bool_type",
            "loc": ["body", field],
            "msg": "Input should be a valid boolean",
            "input": other,
        }))),
    }
}

/// Decode a JSON request body into `(whole, object)`, with FastAPI's exact refusals.
fn body_object(body: &Bytes) -> Result<(Value, Map<String, Value>), Refusal> {
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
    let Value::Object(object) = &value else {
        return Err(body_error(json!({
            "type": "model_attributes_type",
            "loc": ["body"],
            "msg": "Input should be a valid dictionary or object to extract fields from",
            "input": value,
        })));
    };
    Ok((value.clone(), object.clone()))
}

struct PlanCreateValues {
    app_id: Uuid,
    code: String,
    name: String,
    description: Option<String>,
    region_id: Option<Uuid>,
    duration_days: Option<i32>,
    rate_limit_rpm: Option<i32>,
    rate_limit_burst: Option<i32>,
    quota_monthly: Option<i32>,
    token_budget_monthly: Option<i32>,
}

/// Validate the raw body as Pydantic's `PlanCreate`.
///
/// The METERED defaults are here because Pydantic applies them to an ABSENT key only: an
/// explicit null is the caller asking for unlimited and must stay null.
fn parse_plan_create(body: &Bytes) -> Result<PlanCreateValues, Refusal> {
    let (whole, object) = body_object(body)?;
    let app_id = required_uuid(&object, "app_id", &whole)?;
    let code = required_string(&object, "code", &whole)?;
    let name = required_string(&object, "name", &whole)?;
    let description = optional_string(&object, "description")?.unwrap_or(None);
    let region_id = optional_uuid(&object, "region_id")?;
    let duration_days = optional_int(&object, "duration_days", false)?.unwrap_or(None);
    let rate_limit_rpm = optional_int(&object, "rate_limit_rpm", false)?.unwrap_or(None);
    let rate_limit_burst = optional_int(&object, "rate_limit_burst", false)?.unwrap_or(None);
    let quota_monthly = match optional_int(&object, "quota_monthly", true)? {
        None => Some(1_000_000),
        Some(value) => value,
    };
    let token_budget_monthly = match optional_int(&object, "token_budget_monthly", true)? {
        None => Some(50_000_000),
        Some(value) => value,
    };
    Ok(PlanCreateValues {
        app_id,
        code,
        name,
        description,
        region_id,
        duration_days,
        rate_limit_rpm,
        rate_limit_burst,
        quota_monthly,
        token_budget_monthly,
    })
}

/// Validate the raw body as Pydantic's `PlanUpdate`; presence is preserved per field.
fn parse_plan_update(body: &Bytes) -> Result<PlanChanges, Refusal> {
    let (_whole, object) = body_object(body)?;
    Ok(PlanChanges {
        name: optional_string(&object, "name")?,
        description: optional_string(&object, "description")?,
        duration_days: optional_int(&object, "duration_days", false)?,
        rate_limit_rpm: optional_int(&object, "rate_limit_rpm", false)?,
        rate_limit_burst: optional_int(&object, "rate_limit_burst", false)?,
        quota_monthly: optional_int(&object, "quota_monthly", true)?,
        token_budget_monthly: optional_int(&object, "token_budget_monthly", true)?,
        revoked: optional_bool(&object, "revoked")?,
    })
}

struct RegionCreateValues {
    code: String,
    name: String,
    active: bool,
}

fn parse_region_create(body: &Bytes) -> Result<RegionCreateValues, Refusal> {
    let (whole, object) = body_object(body)?;
    let code = required_string(&object, "code", &whole)?;
    let name = required_string(&object, "name", &whole)?;
    let active = optional_bool(&object, "active")?.flatten().unwrap_or(true);
    Ok(RegionCreateValues { code, name, active })
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

fn region_response(row: catalogue_plans::RegionRow) -> RegionResponse {
    RegionResponse {
        id: row.id.to_string(),
        code: row.code,
        name: row.name,
        active: row.active,
        created_at: py_iso(row.created_at),
    }
}

/// A SQLSTATE class 23 (`IntegrityError`) — the reference's except clause.
fn is_integrity(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database)
        if database.code().is_some_and(|code| code.starts_with("23")))
}

/// `POST /plans` — create a plan scoped to an app (and optionally a region).
#[utoipa::path(
    post,
    path = "/plans",
    tag = "plans",
    operation_id = "create_plan",
    summary = "Create Plan",
    description = "Create a new plan\n\nRequires 'license-admin' role AND that the caller's owner issues the app (403 ``app_not_administrable_by_caller``; cross_owner bypasses, audited).",
    request_body = PlanCreate,
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 201, body = PlanResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_plan(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    body: Bytes,
) -> Result<Response, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let plan = parse_plan_create(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to create plan"));
    };

    // The reference's order: app 404, then the owner rule, then the region 404.
    let app = catalogue_apps::get_app(pool, plan.app_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create plan");
            internal_error("Failed to create plan")
        })?;
    if app.is_none() {
        return Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            format!("App {} not found", plan.app_id),
        ));
    }
    authorize_app_admin(
        pool,
        &user,
        plan.app_id,
        "plan_create",
        "Failed to create plan",
    )
    .await?;

    if let Some(region_id) = plan.region_id {
        let region = catalogue_plans::get_region(pool, region_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to create plan");
                internal_error("Failed to create plan")
            })?;
        if region.is_none() {
            return Err(Refusal::detail(
                StatusCode::NOT_FOUND,
                format!("Region {region_id} not found"),
            ));
        }
    }

    let new_plan = NewPlan {
        app_id: plan.app_id,
        code: plan.code.clone(),
        name: plan.name,
        description: plan.description,
        region_id: plan.region_id,
        duration_days: plan.duration_days,
        rate_limit_rpm: plan.rate_limit_rpm,
        rate_limit_burst: plan.rate_limit_burst,
        quota_monthly: plan.quota_monthly,
        token_budget_monthly: plan.token_budget_monthly,
    };
    match catalogue_plans::create_plan(pool, &new_plan).await {
        Ok(row) => {
            tracing::info!(plan = %row.id, code = %row.code, "Plan created");
            Ok((StatusCode::CREATED, Json(plan_response(row))).into_response())
        }
        Err(error) if is_integrity(&error) => Err(Refusal::detail(
            StatusCode::CONFLICT,
            format!(
                "Plan with code '{}' already exists for this app/region",
                plan.code
            ),
        )),
        Err(error) => {
            tracing::error!(%error, "Failed to create plan");
            Err(internal_error("Failed to create plan"))
        }
    }
}

/// `PATCH /plans/{plan_id}` — update the fields the caller SENT, then re-push a changed budget.
#[utoipa::path(
    patch,
    path = "/plans/{plan_id}",
    tag = "plans",
    operation_id = "update_plan",
    summary = "Update Plan",
    description = "Update an existing plan's mutable fields\n\nRequires 'license-admin' role AND that the caller's owner issues the plan's app (403 ``app_not_administrable_by_caller``; cross_owner bypasses, audited).\n\nA change to ``token_budget_monthly`` is pushed to billing after the response for every LIVE licence on the plan whose budget is plan-driven.",
    request_body = PlanUpdate,
    params(
        ("plan_id" = String, Path, description = "Plan Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = PlanResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn update_plan(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(plan_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Ok(plan_uuid) = Uuid::try_parse(&plan_id) else {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            format!("Invalid plan ID: {plan_id}"),
        ));
    };
    let changes = parse_plan_update(&body)?;
    if changes.is_empty() {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "No fields to update",
        ));
    }
    if changes.revoked == Some(None) {
        // plans.revoked is NOT NULL; an explicit null would otherwise surface as a 500.
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "revoked must be true or false",
        ));
    }
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to update plan"));
    };

    let existing = catalogue_plans::get_plan(pool, plan_uuid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to update plan");
            internal_error("Failed to update plan")
        })?;
    let Some(existing) = existing else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Plan not found"));
    };
    authorize_app_admin(
        pool,
        &user,
        existing.app_id,
        "plan_update",
        "Failed to update plan",
    )
    .await?;

    let previous_budget = existing.token_budget_monthly;
    let plan = catalogue_plans::update_plan(pool, plan_uuid, &changes)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to update plan");
            internal_error("Failed to update plan")
        })?;
    let Some(plan) = plan else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Plan not found"));
    };

    tracing::info!(plan = %plan.id, code = %plan.code, "Plan updated");
    if changes.token_budget_monthly.is_some() && plan.token_budget_monthly != previous_budget {
        // After the update has committed, exactly as the reference's BackgroundTasks does after
        // the response: `tokio::spawn` is the strong-reference equivalent. The result is
        // recorded and logged; nothing it finds can reach this response.
        spawn_budget_repush(
            &state,
            plan.id,
            format!(
                "plan {} token_budget_monthly {} -> {}",
                plan.id,
                render_budget(previous_budget),
                render_budget(plan.token_budget_monthly)
            ),
        );
    }
    Ok(Json(plan_response(plan)).into_response())
}

/// Python's rendering of an `int | None` in the reference's reason string: `None`.
fn render_budget(value: Option<i32>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| value.to_string())
}

fn spawn_budget_repush(state: &AppState, plan_id: Uuid, reason: String) {
    let (Some(pool), Some(metering)) = (state.pool.clone(), state.metering.clone()) else {
        return;
    };
    let billing = state.billing.clone();
    tokio::spawn(async move {
        let result = catalogue_plans::push_effective_token_budgets(
            &pool,
            Some(metering.as_ref()),
            billing.as_deref(),
            plan_id,
            &reason,
        )
        .await;
        tracing::info!(
            %reason,
            pushed = result.pushed.len(),
            torn_down = result.torn_down.len(),
            failed = result.failed.len(),
            skipped_money_tenant = result.skipped_money_tenant.len(),
            skipped_sentinel = result.skipped_sentinel.len(),
            "plan token budget re-push finished"
        );
    });
}

/// `GET /plans` — every non-revoked plan, optionally filtered by app and/or region.
#[utoipa::path(
    get,
    path = "/plans",
    tag = "plans",
    operation_id = "list_plans",
    summary = "List Plans",
    description = "List all plans with optional filters\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        PlansQuery,
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
pub async fn list_plans(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<PlansQuery>,
) -> Result<Json<Vec<PlanResponse>>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    // `UUID(app_id) if app_id else None`: an empty string is falsy in the reference too.
    let app_id = match query.app_id.as_deref().filter(|raw| !raw.is_empty()) {
        Some(raw) => Some(
            Uuid::try_parse(raw)
                .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"))?,
        ),
        None => None,
    };
    let region_id = match query.region_id.as_deref().filter(|raw| !raw.is_empty()) {
        Some(raw) => Some(
            Uuid::try_parse(raw)
                .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"))?,
        ),
        None => None,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list plans"));
    };
    let rows = catalogue_plans::list_plans(pool, app_id, region_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list plans");
            internal_error("Failed to list plans")
        })?;
    Ok(Json(rows.into_iter().map(plan_response).collect()))
}

/// `GET /plans/{plan_id}` — one plan, or 404.
#[utoipa::path(
    get,
    path = "/plans/{plan_id}",
    tag = "plans",
    operation_id = "get_plan",
    summary = "Get Plan",
    description = "Get plan by ID\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        ("plan_id" = String, Path, description = "Plan Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = PlanResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn get_plan(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(plan_id): Path<String>,
) -> Result<Json<PlanResponse>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let Ok(plan_uuid) = Uuid::try_parse(&plan_id) else {
        return Err(Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"));
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to get plan"));
    };
    let row = catalogue_plans::get_plan(pool, plan_uuid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to get plan");
            internal_error("Failed to get plan")
        })?;
    let Some(row) = row else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Plan not found"));
    };
    Ok(Json(plan_response(row)))
}

/// `DELETE /plans/{plan_id}` — hard delete, refused while a licence references the plan.
#[utoipa::path(
    delete,
    path = "/plans/{plan_id}",
    tag = "plans",
    operation_id = "delete_plan",
    summary = "Delete Plan",
    description = "Hard-delete a plan. Fails with 409 if licenses still reference it.\n\nRequires 'license-admin' role AND that the caller's owner issues the plan's app (403 ``app_not_administrable_by_caller``; cross_owner bypasses, audited).",
    params(
        ("plan_id" = String, Path, description = "Plan Id"),
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
pub async fn delete_plan(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(plan_id): Path<String>,
) -> Result<StatusCode, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Ok(plan_uuid) = Uuid::try_parse(&plan_id) else {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "Invalid plan_id format",
        ));
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to delete plan"));
    };

    let existing = catalogue_plans::get_plan(pool, plan_uuid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete plan");
            internal_error("Failed to delete plan")
        })?;
    let Some(existing) = existing else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Plan not found"));
    };
    // Before the 409: a non-issuer must not learn anything about the plan's dependencies.
    authorize_app_admin(
        pool,
        &user,
        existing.app_id,
        "plan_delete",
        "Failed to delete plan",
    )
    .await?;

    match catalogue_plans::delete_plan(pool, plan_uuid)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete plan");
            internal_error("Failed to delete plan")
        })? {
        DeletePlanOutcome::Deleted => Ok(StatusCode::NO_CONTENT),
        DeletePlanOutcome::Conflict => Err(Refusal::detail(
            StatusCode::CONFLICT,
            "Cannot delete plan: licenses still reference it",
        )),
        DeletePlanOutcome::NotFound => {
            Err(Refusal::detail(StatusCode::NOT_FOUND, "Plan not found"))
        }
    }
}

/// `POST /regions` — create a region, idempotently by code.
#[utoipa::path(
    post,
    path = "/regions",
    tag = "regions",
    operation_id = "create_region",
    summary = "Create Region",
    description = "Create a new region (idempotent — returns existing region if code matches)\n\nRequires 'license-admin' role",
    request_body = RegionCreate,
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = RegionResponse),
        (status = 201, body = RegionResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_region(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    body: Bytes,
) -> Result<Response, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let region = parse_region_create(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to create region"));
    };

    // Idempotent BEFORE any insert: an existing code is returned unchanged with a 200.
    let existing = catalogue_plans::get_region_by_code(pool, &region.code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create region");
            internal_error("Failed to create region")
        })?;
    if let Some(row) = existing {
        return Ok((StatusCode::OK, Json(region_response(row))).into_response());
    }

    match catalogue_plans::create_region(pool, &region.code, &region.name, region.active).await {
        Ok(row) => {
            tracing::info!(region = %row.id, code = %row.code, "Region created");
            Ok((StatusCode::CREATED, Json(region_response(row))).into_response())
        }
        // Lost the read-then-insert race on the unique `region.code` index. 200-with-the-
        // existing-row, because this route's docstring promises idempotence; re-raise when the
        // re-read finds nothing, because then the violated constraint was NOT the code race.
        Err(error) if is_integrity(&error) => {
            match catalogue_plans::get_region_by_code(pool, &region.code).await {
                Ok(Some(row)) => {
                    tracing::info!(region = %row.id, code = %row.code, "Region already exists; returning it");
                    Ok((StatusCode::OK, Json(region_response(row))).into_response())
                }
                _ => Err(internal_error("Failed to create region")),
            }
        }
        Err(error) => {
            tracing::error!(%error, "Failed to create region");
            Err(internal_error("Failed to create region"))
        }
    }
}

/// `GET /regions` — active regions by default, every region with `?active_only=false`.
#[utoipa::path(
    get,
    path = "/regions",
    tag = "regions",
    operation_id = "list_regions",
    summary = "List Regions",
    description = "List all regions\n\nRequires 'license-admin' or 'license-reader' role",
    params(
        RegionsQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<RegionResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_regions(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<RegionsQuery>,
) -> Result<Json<Vec<RegionResponse>>, Refusal> {
    require_any_realm_role(&user, &READ_ROLES)?;
    let active_only = match query.active_only.as_deref() {
        Some(raw) => parse_bool("active_only", raw)?,
        None => true,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list regions"));
    };
    let rows = catalogue_plans::list_regions(pool, active_only)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list regions");
            internal_error("Failed to list regions")
        })?;
    Ok(Json(rows.into_iter().map(region_response).collect()))
}
