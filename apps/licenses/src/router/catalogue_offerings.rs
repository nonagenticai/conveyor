//! The four catalogue OFFERING routes.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Route | reference |
//! |---|---|
//! | `POST /offerings` | `main.py:8092` `create_offering` |
//! | `GET /offerings` | `main.py:8178` `list_offerings` |
//! | `GET /offerings/{app_key}/{code}` | `main.py:8193` `get_offering` |
//! | `DELETE /offerings/{app_key}/{code}` | `main.py:8212` `delete_offering` |
//!
//! Authorization is the reference's, literally:
//!
//! - all four routes need the `license-admin` REALM role — unlike the catalogue APP/PLAN reads,
//!   `license-reader` is NOT enough (403 `Role 'license-admin' required`);
//! - `POST` and `DELETE` additionally need the caller's owner to ISSUE the addressed app
//!   (`_authorize_app_admin`, 403 `app_not_administrable_by_caller`; `cross_owner` bypasses,
//!   audited). The check runs only when the app EXISTS: an unknown app key skips it and falls
//!   through to the DAL, which answers `App not found: <key>` (create) or `deleted: false`
//!   (delete) — the reference's order;
//! - `POST` returns `200`, not `201`: the reference's decorator declares no status code.
//!
//! The two DAL-authored refusals are STRING details, not validation arrays: the reference raises
//! `HTTPException(422, detail=str(e))` from `create_offering` for an invalid `byo_mode` /
//! `access_cadence`. A Pydantic body error keeps the FastAPI 422 ARRAY, which is why the body is
//! parsed by hand here. A second consecutive `IntegrityError` is the reference's 409 with the
//! `Concurrent modification of offering '<app>/<code>'; retry the request` detail.
//!
//! `DELETE` has no 404 arm: the reference answers `200 {"deleted": false}` for an unknown app or
//! an absent pair, and nothing references an offering.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use utoipa::{IntoParams, ToSchema};

use crate::auth::User;
use crate::db::catalogue_offerings::{self, CreateOffering, NewOffering, OfferingRow};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::catalogue_apps::authorize_app_admin;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `OfferingCreate.access_cadence`: the closed set, carrying the create-time default.
///
/// A separate type (and `inline`) for the same reason as `AppCreateAudience`: the default belongs
/// to the REQUEST only, and a `$ref` would not match the captured inline schema.
#[derive(Debug, Clone, Copy, serde::Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(default = "monthly")]
pub enum OfferingAccessCadence {
    Monthly,
    OneTime,
}

impl OfferingAccessCadence {
    fn wire(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::OneTime => "one_time",
        }
    }
}

/// `OfferingCreate` (`main.py:7972`), field for field and bound for bound.
///
/// `byo_mode` is a plain string here and validated by the DAL, exactly as the reference does; the
/// six usage fields plus the two money fields carry Pydantic's `ge=0` as `minimum = 0`.
#[derive(Debug, ToSchema)]
pub struct OfferingCreate {
    /// Product key (App.key), e.g. 'w2v-image'.
    pub app_key: String,
    /// Offering code, unique per product (e.g. 'free-high-markup').
    #[schema(max_length = 64)]
    pub code: String,
    /// Human-readable offering name.
    #[schema(max_length = 255)]
    pub name: String,
    /// 'local_allowed', 'byo_required', or 'platform_only'
    pub byo_mode: String,
    /// Relative markup percent on this product's BYO usage.
    #[schema(minimum = 0)]
    pub markup_pct: Option<f64>,
    #[schema(minimum = 0)]
    pub surcharge_input_per_1m_usd: Option<f64>,
    #[schema(minimum = 0)]
    pub surcharge_output_per_1m_usd: Option<f64>,
    #[schema(minimum = 0)]
    pub surcharge_cache_creation_per_1m_usd: Option<f64>,
    #[schema(minimum = 0)]
    pub surcharge_cache_read_per_1m_usd: Option<f64>,
    #[schema(minimum = 0)]
    pub surcharge_per_request_usd: Option<f64>,
    /// Fixed access fee for this product; NULL = free access.
    #[schema(minimum = 0)]
    pub access_price_usd: Option<f64>,
    #[schema(inline, required = false)]
    pub access_cadence: OfferingAccessCadence,
    /// Monthly included-usage credit (USD); NULL = none.
    #[schema(minimum = 0)]
    pub included_allowance_usd: Option<f64>,
    pub notes: Option<String>,
}

/// `OfferingResponse` (`main.py:8030`): every field present, nullable ones explicit `null`.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct OfferingResponse {
    pub id: String,
    pub app_id: String,
    #[schema(required = true)]
    pub app_key: Option<String>,
    pub code: String,
    pub name: String,
    pub byo_mode: String,
    #[schema(required = true)]
    pub markup_pct: Option<f64>,
    #[schema(required = true)]
    pub surcharge_input_per_1m_usd: Option<f64>,
    #[schema(required = true)]
    pub surcharge_output_per_1m_usd: Option<f64>,
    #[schema(required = true)]
    pub surcharge_cache_creation_per_1m_usd: Option<f64>,
    #[schema(required = true)]
    pub surcharge_cache_read_per_1m_usd: Option<f64>,
    #[schema(required = true)]
    pub surcharge_per_request_usd: Option<f64>,
    #[schema(required = true)]
    pub access_price_usd: Option<f64>,
    pub access_cadence: String,
    #[schema(required = true)]
    pub included_allowance_usd: Option<f64>,
    #[schema(required = true)]
    pub notes: Option<String>,
    #[schema(required = true)]
    pub created_at: Option<String>,
    #[schema(required = true)]
    pub updated_at: Option<String>,
}

/// `OfferingListResponse`: the `GET /offerings` envelope.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct OfferingListResponse {
    pub offerings: Vec<OfferingResponse>,
}

/// `OfferingDeleteResponse`: idempotent, so `deleted` may be false.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct OfferingDeleteResponse {
    pub app_key: String,
    pub code: String,
    pub deleted: bool,
}

/// `GET /offerings` query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OfferingsQuery {
    /// Filter to one product (App.key).
    #[param(nullable = true)]
    pub app: Option<String>,
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

/// A required string with Pydantic's `max_length` check (characters, not bytes).
fn sized_string(
    object: &Map<String, Value>,
    field: &str,
    whole: &Value,
    max_length: usize,
) -> Result<String, Refusal> {
    let value = required_string(object, field, whole)?;
    if value.chars().count() > max_length {
        return Err(body_error(json!({
            "type": "string_too_long",
            "loc": ["body", field],
            "msg": format!("String should have at most {max_length} characters"),
            "input": value,
            "ctx": {"max_length": max_length},
        })));
    }
    Ok(value)
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(other) => Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", field],
            "msg": "Input should be a valid string",
            "input": other,
        }))),
    }
}

/// `float | None = Field(None, ge=0)`: absent and null both mean None; numbers and numeric strings
/// are accepted (Pydantic's lax mode); a negative value is the array body with `ge`.
fn optional_float(object: &Map<String, Value>, field: &str) -> Result<Option<f64>, Refusal> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let number = match value {
        Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
        Value::String(text) => match text.trim().parse::<f64>() {
            Ok(number) => number,
            Err(_) => {
                return Err(body_error(json!({
                    "type": "float_parsing",
                    "loc": ["body", field],
                    "msg": "Input should be a valid number, unable to parse string as a number",
                    "input": value,
                })));
            }
        },
        other => {
            return Err(body_error(json!({
                "type": "float_type",
                "loc": ["body", field],
                "msg": "Input should be a valid number",
                "input": other,
            })));
        }
    };
    if number.is_nan() || number < 0.0 {
        return Err(body_error(json!({
            "type": "greater_than_equal",
            "loc": ["body", field],
            "msg": "Input should be greater than or equal to 0",
            "input": value,
            "ctx": {"ge": 0.0},
        })));
    }
    Ok(Some(number))
}

fn cadence_literal_error(input: Value) -> Refusal {
    body_error(json!({
        "type": "literal_error",
        "loc": ["body", "access_cadence"],
        "msg": "Input should be 'monthly' or 'one_time'",
        "input": input,
        "ctx": {"expected": "'monthly' or 'one_time'"},
    }))
}

/// `Literal["monthly", "one_time"] = "monthly"`.
fn access_cadence(object: &Map<String, Value>) -> Result<OfferingAccessCadence, Refusal> {
    match object.get("access_cadence") {
        None => Ok(OfferingAccessCadence::Monthly),
        Some(Value::String(raw)) => match raw.as_str() {
            "monthly" => Ok(OfferingAccessCadence::Monthly),
            "one_time" => Ok(OfferingAccessCadence::OneTime),
            _ => Err(cadence_literal_error(Value::String(raw.clone()))),
        },
        Some(other) => Err(cadence_literal_error(other.clone())),
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

struct OfferingCreateValues {
    app_key: String,
    code: String,
    name: String,
    byo_mode: String,
    markup_pct: Option<f64>,
    surcharge_input_per_1m_usd: Option<f64>,
    surcharge_output_per_1m_usd: Option<f64>,
    surcharge_cache_creation_per_1m_usd: Option<f64>,
    surcharge_cache_read_per_1m_usd: Option<f64>,
    surcharge_per_request_usd: Option<f64>,
    access_price_usd: Option<f64>,
    access_cadence: OfferingAccessCadence,
    included_allowance_usd: Option<f64>,
    notes: Option<String>,
}

/// Validate the raw body as Pydantic's `OfferingCreate`, in the model's declaration order.
fn parse_offering_create(body: &Bytes) -> Result<OfferingCreateValues, Refusal> {
    let (whole, object) = body_object(body)?;
    let app_key = required_string(&object, "app_key", &whole)?;
    let code = sized_string(&object, "code", &whole, 64)?;
    let name = sized_string(&object, "name", &whole, 255)?;
    let byo_mode = required_string(&object, "byo_mode", &whole)?;
    let markup_pct = optional_float(&object, "markup_pct")?;
    let surcharge_input_per_1m_usd = optional_float(&object, "surcharge_input_per_1m_usd")?;
    let surcharge_output_per_1m_usd = optional_float(&object, "surcharge_output_per_1m_usd")?;
    let surcharge_cache_creation_per_1m_usd =
        optional_float(&object, "surcharge_cache_creation_per_1m_usd")?;
    let surcharge_cache_read_per_1m_usd =
        optional_float(&object, "surcharge_cache_read_per_1m_usd")?;
    let surcharge_per_request_usd = optional_float(&object, "surcharge_per_request_usd")?;
    let access_price_usd = optional_float(&object, "access_price_usd")?;
    let access_cadence = access_cadence(&object)?;
    let included_allowance_usd = optional_float(&object, "included_allowance_usd")?;
    let notes = optional_string(&object, "notes")?;
    Ok(OfferingCreateValues {
        app_key,
        code,
        name,
        byo_mode,
        markup_pct,
        surcharge_input_per_1m_usd,
        surcharge_output_per_1m_usd,
        surcharge_cache_creation_per_1m_usd,
        surcharge_cache_read_per_1m_usd,
        surcharge_per_request_usd,
        access_price_usd,
        access_cadence,
        included_allowance_usd,
        notes,
    })
}

/// Python's `repr()` for the two DAL-authored messages.
///
/// The reference interpolates `{byo_mode!r}`, so the quotes are part of the caller-visible text.
fn python_repr(value: &str) -> String {
    if value.contains('\'') && !value.contains('"') {
        format!("\"{value}\"")
    } else {
        format!("'{value}'")
    }
}

fn offering_response(row: OfferingRow) -> OfferingResponse {
    OfferingResponse {
        id: row.id.to_string(),
        app_id: row.app_id.to_string(),
        app_key: Some(row.app_key),
        code: row.code,
        name: row.name,
        byo_mode: row.byo_mode,
        markup_pct: row.markup_pct,
        surcharge_input_per_1m_usd: row.surcharge_input_per_1m_usd,
        surcharge_output_per_1m_usd: row.surcharge_output_per_1m_usd,
        surcharge_cache_creation_per_1m_usd: row.surcharge_cache_creation_per_1m_usd,
        surcharge_cache_read_per_1m_usd: row.surcharge_cache_read_per_1m_usd,
        surcharge_per_request_usd: row.surcharge_per_request_usd,
        access_price_usd: row.access_price_usd,
        access_cadence: row.access_cadence,
        included_allowance_usd: row.included_allowance_usd,
        notes: row.notes,
        created_at: Some(py_iso(row.created_at)),
        updated_at: Some(py_iso(row.updated_at)),
    }
}

/// `POST /offerings` — upsert an offering by `(app_key, code)`.
#[utoipa::path(
    post,
    path = "/offerings",
    tag = "offerings",
    operation_id = "create_offering",
    summary = "Create Offering",
    description = "Create or update a per-product offering (usage-policy template).\n\nRequires 'license-admin' AND that the caller's owner issues the app (403\n``app_not_administrable_by_caller``; cross_owner bypasses, audited). An\nunknown app stays 404. Idempotent per ``(app_key, code)``.",
    request_body = OfferingCreate,
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = OfferingResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_offering(
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
    let offering = parse_offering_create(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to create offering"));
    };

    // The reference authorizes BEFORE the DAL, and only when the app exists: an unknown key skips
    // the owner check and becomes the DAL's 404.
    let app = catalogue_offerings::app_by_key(pool, &offering.app_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create offering");
            internal_error("Failed to create offering")
        })?;
    if let Some(app) = app {
        authorize_app_admin(
            pool,
            &user,
            app,
            "offering_upsert",
            "Failed to create offering",
        )
        .await?;
    }

    let new = NewOffering {
        app_key: offering.app_key.clone(),
        code: offering.code.clone(),
        name: offering.name,
        byo_mode: offering.byo_mode.clone(),
        markup_pct: offering.markup_pct,
        surcharge_input_per_1m_usd: offering.surcharge_input_per_1m_usd,
        surcharge_output_per_1m_usd: offering.surcharge_output_per_1m_usd,
        surcharge_cache_creation_per_1m_usd: offering.surcharge_cache_creation_per_1m_usd,
        surcharge_cache_read_per_1m_usd: offering.surcharge_cache_read_per_1m_usd,
        surcharge_per_request_usd: offering.surcharge_per_request_usd,
        access_price_usd: offering.access_price_usd,
        access_cadence: offering.access_cadence.wire().to_owned(),
        included_allowance_usd: offering.included_allowance_usd,
        notes: offering.notes,
    };
    match catalogue_offerings::create_offering(pool, &new)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create offering");
            internal_error("Failed to create offering")
        })?
    {
        CreateOffering::Upserted(row) => {
            let row = *row;
            tracing::info!(
                app = %row.app_key,
                code = %row.code,
                byo_mode = %row.byo_mode,
                "Offering upserted"
            );
            Ok(Json(offering_response(row)).into_response())
        }
        CreateOffering::AppNotFound => Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            format!("App not found: {}", offering.app_key),
        )),
        CreateOffering::InvalidByoMode(value) => Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("invalid byo_mode: {}", python_repr(&value)),
        )),
        CreateOffering::InvalidAccessCadence(value) => Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("invalid access_cadence: {}", python_repr(&value)),
        )),
        CreateOffering::Conflict => Err(Refusal::detail(
            StatusCode::CONFLICT,
            format!(
                "Concurrent modification of offering '{}/{}'; retry the request",
                offering.app_key, offering.code
            ),
        )),
    }
}

/// `GET /offerings` — every offering, oldest first, optionally filtered to one product.
#[utoipa::path(
    get,
    path = "/offerings",
    tag = "offerings",
    operation_id = "list_offerings",
    summary = "List Offerings",
    description = "List offerings, optionally filtered to one product. Requires 'license-admin'.",
    params(
        OfferingsQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = OfferingListResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_offerings(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<OfferingsQuery>,
) -> Result<Json<OfferingListResponse>, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to list offerings"));
    };
    let rows = catalogue_offerings::list_offerings(pool, query.app.as_deref())
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to list offerings");
            internal_error("Failed to list offerings")
        })?;
    Ok(Json(OfferingListResponse {
        offerings: rows.into_iter().map(offering_response).collect(),
    }))
}

/// `GET /offerings/{app_key}/{code}` — one offering, or 404.
#[utoipa::path(
    get,
    path = "/offerings/{app_key}/{code}",
    tag = "offerings",
    operation_id = "get_offering",
    summary = "Get Offering",
    description = "Get a single offering by ``(app_key, code)``. Requires 'license-admin'.",
    params(
        ("app_key" = String, Path, description = "App Key"),
        ("code" = String, Path, description = "Code"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = OfferingResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn get_offering(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((app_key, code)): Path<(String, String)>,
) -> Result<Json<OfferingResponse>, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to get offering"));
    };
    let row = catalogue_offerings::get_offering(pool, &app_key, &code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to get offering");
            internal_error("Failed to get offering")
        })?;
    let Some(row) = row else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "Offering not found"));
    };
    Ok(Json(offering_response(row)))
}

/// `DELETE /offerings/{app_key}/{code}` — idempotent, no 404 arm.
#[utoipa::path(
    delete,
    path = "/offerings/{app_key}/{code}",
    tag = "offerings",
    operation_id = "delete_offering",
    summary = "Delete Offering",
    description = "Delete an offering by ``(app_key, code)``. Idempotent.\n\nRequires 'license-admin' AND that the caller's owner issues the app (403\n``app_not_administrable_by_caller``; cross_owner bypasses, audited). An\nunknown app has no offering, so it answers ``deleted: false`` as before.",
    params(
        ("app_key" = String, Path, description = "App Key"),
        ("code" = String, Path, description = "Code"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = OfferingDeleteResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn delete_offering(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((app_key, code)): Path<(String, String)>,
) -> Result<Json<OfferingDeleteResponse>, Refusal> {
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error("Failed to delete offering"));
    };

    let app = catalogue_offerings::app_by_key(pool, &app_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete offering");
            internal_error("Failed to delete offering")
        })?;
    if let Some(app) = app {
        authorize_app_admin(
            pool,
            &user,
            app,
            "offering_delete",
            "Failed to delete offering",
        )
        .await?;
    }

    let deleted = catalogue_offerings::delete_offering(pool, &app_key, &code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete offering");
            internal_error("Failed to delete offering")
        })?;
    tracing::info!(app = %app_key, code = %code, deleted, "Offering delete");
    Ok(Json(OfferingDeleteResponse {
        app_key,
        code,
        deleted,
    }))
}
