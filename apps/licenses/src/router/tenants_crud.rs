//! The tenant registry HTTP surface: eight of the nineteen tenant operations (#45, slice 45).
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Route | Reference |
//! |---|---|
//! | `GET /tenants` | `main.py::list_tenants` |
//! | `POST /tenants` | `main.py::create_tenant` |
//! | `GET /tenants/{name_or_id}` | `main.py::get_tenant` |
//! | `PATCH /tenants/{name_or_id}` | `main.py::rename_tenant` |
//! | `PUT /tenants/{tenant_id}` | `main.py::register_tenant` |
//! | `PATCH /tenants/{name_or_id}/archive` | `main.py::set_tenant_archived` |
//! | `PUT /tenants/{name_or_id}/budget-currency` | `main.py::set_tenant_budget_currency` |
//! | `PATCH /tenants/{name_or_id}/cost-centre` | `main.py::set_tenant_cost_centre` |
//!
//! Authorization is the reference's, not a simplification:
//!
//! - the listings/read/admits are realm-role gates (`license-admin`, `license-reader`,
//!   `tenant-registry-reader` on the list) plus the owner scoping in `db::tenants_crud`;
//! - the tenant-WIDE writes (rename, archive, budget currency, cost centre) are OWNER-ONLY:
//!   a grantee gets 403 `tenant_owner_only`; `cross_owner` is admitted, audited;
//! - create/register are owner-scoped writes: an unregistered client is 403
//!   `unknown_api_client` (never a default owner).
//!
//! Error mapping is the reference's: FastAPI's 422 array for a body/path validation
//! failure, the route envelope `{"detail": …}` everywhere else, and the reference's exact
//! 409 objects for name/external-ref conflicts and an ambiguous name.

use axum::Extension;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{User, get_tenant_id, looks_like_jwt, require_any_realm_role};
use crate::db::issuance::require_known_owner;
use crate::db::owners::{owner_can_access_tenant, resolve_realm_owner};
use crate::db::tenants_crud::{
    self, EnsureTenant, Mutated, NewTenant, TenantRow, TenantWriteError,
};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::internal_reads::BudgetCurrency;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `models._TENANT_NAME_SHAPE`, as the capture publishes it.
const NAME_PATTERN: &str = "^[a-z0-9][a-z0-9-]*$";
/// `main.FORWARDED_USER_AUTH_HEADER`.
const FORWARDED_USER_AUTH_HEADER: &str = "x-forwarded-user-authorization";
/// `main.BUDGET_CURRENCY_CHANGE_ROLE`.
const BUDGET_CURRENCY_CHANGE_ROLE: &str = "platform-admin";
/// `main.BUDGET_CURRENCY_FORWARDED_OWNER_ROLES`.
const BUDGET_CURRENCY_FORWARDED_OWNER_ROLES: [&str; 2] = ["super_admin", "platform-admin"];
/// `ck_tenants_kind`, as the capture's inline enum.
const TENANT_KINDS: [&str; 5] = ["customer", "internal", "service", "test", "unclassified"];
/// `ck_tenants_budget_currency`, as the capture's inline enum.
const BUDGET_CURRENCIES: [&str; 3] = ["EUR", "USD", "CAD"];

/// `ck_tenants_kind` for the `TenantRegister` schema.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TenantKind {
    Customer,
    Internal,
    Service,
    Test,
    Unclassified,
}

fn budget_currency_value(raw: &str) -> Option<BudgetCurrency> {
    match raw {
        "EUR" => Some(BudgetCurrency::Eur),
        "USD" => Some(BudgetCurrency::Usd),
        "CAD" => Some(BudgetCurrency::Cad),
        _ => None,
    }
}

/// `TenantResponse` — `Tenant.to_dict`'s eleven properties, explicit nulls included.
///
/// The capture's `required` set is `id` + `name` only: every other field defaults to None
/// on the Pydantic model (and is still emitted), so `Option` here is exactly right.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub keycloak_realm: Option<String>,
    pub byo_mode: Option<String>,
    #[schema(inline)]
    pub budget_currency: Option<BudgetCurrency>,
    pub kind: Option<String>,
    pub cost_centre: Option<String>,
    pub owner_key: Option<String>,
    pub external_ref: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: Option<String>,
}

/// `TenantCreate` — `name` required; the rest optional.
#[derive(Debug, ToSchema)]
pub struct TenantCreate {
    #[schema(max_length = 255, pattern = "^[a-z0-9][a-z0-9-]*$")]
    pub name: String,
    #[schema(max_length = 255)]
    pub keycloak_realm: Option<String>,
    #[schema(max_length = 64)]
    pub cost_centre: Option<String>,
    #[schema(inline)]
    pub budget_currency: Option<BudgetCurrency>,
    #[schema(min_length = 1, max_length = 255)]
    pub external_ref: Option<String>,
}

/// `TenantRegister` — the explicit-id upsert body.
#[derive(Debug, ToSchema)]
pub struct TenantRegister {
    #[schema(max_length = 255, pattern = "^[a-z0-9][a-z0-9-]*$")]
    pub name: String,
    #[schema(max_length = 255)]
    pub keycloak_realm: Option<String>,
    #[schema(inline)]
    pub kind: Option<TenantKind>,
    #[schema(max_length = 64)]
    pub cost_centre: Option<String>,
    #[schema(inline)]
    pub budget_currency: Option<BudgetCurrency>,
}

/// `TenantRename` — `name` required.
#[derive(Debug, ToSchema)]
pub struct TenantRename {
    #[schema(max_length = 255, pattern = "^[a-z0-9][a-z0-9-]*$")]
    pub name: String,
}

/// `TenantArchiveUpdate` — `archived` required.
#[derive(Debug, ToSchema)]
pub struct TenantArchiveUpdate {
    pub archived: bool,
}

/// `TenantBudgetCurrencyUpdate` — `budget_currency` required and never null.
#[derive(Debug, ToSchema)]
pub struct TenantBudgetCurrencyUpdate {
    #[schema(inline)]
    pub budget_currency: BudgetCurrency,
}

/// `TenantCostCentreUpdate` — `cost_centre` required; `null` CLEARS it.
#[derive(Debug, ToSchema)]
pub struct TenantCostCentreUpdate {
    #[schema(required = true, max_length = 64)]
    pub cost_centre: Option<String>,
}

/// `GET /tenants` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListTenantsQuery {
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_archived: Option<String>,
    #[serde(default)]
    #[param(value_type = bool, default = false)]
    pub include_service_actors: Option<String>,
}

fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

fn tenant_not_found() -> Refusal {
    Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found")
}

/// `TenantResponse` from one row, with the capture's exact `to_dict` rendering.
fn tenant_response(row: TenantRow) -> Result<TenantResponse, Refusal> {
    let budget_currency = match row.budget_currency.as_deref() {
        Some(raw) => Some(budget_currency_value(raw).ok_or_else(internal_error)?),
        None => None,
    };
    Ok(TenantResponse {
        id: row.id.to_string(),
        name: row.name,
        keycloak_realm: row.keycloak_realm,
        byo_mode: Some(row.byo_mode),
        budget_currency,
        kind: Some(row.kind),
        cost_centre: row.cost_centre,
        owner_key: Some(row.owner_key),
        external_ref: row.external_ref,
        archived_at: row.archived_at.map(py_iso),
        created_at: Some(py_iso(row.created_at)),
    })
}

fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

fn body_object(body: &Bytes) -> Result<serde_json::Map<String, Value>, Refusal> {
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
    match value {
        Value::Object(object) => Ok(object),
        other => Err(body_error(json!({
            "type": "model_attributes_type",
            "loc": ["body"],
            "msg": "Input should be a valid dictionary or object to extract fields from",
            "input": other,
        }))),
    }
}

fn pattern_matches(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The context Pydantic needs to echo the WHOLE body in a `missing` error.
fn body_value(object: &serde_json::Map<String, Value>) -> Value {
    Value::Object(object.clone())
}

fn string_value(
    field: &str,
    value: &Value,
    max: Option<usize>,
    min: Option<usize>,
    pattern: Option<&str>,
) -> Result<String, Refusal> {
    let Value::String(text) = value else {
        return Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", field],
            "msg": "Input should be a valid string",
            "input": value,
        })));
    };
    let length = text.chars().count();
    if let Some(max) = max
        && length > max
    {
        return Err(body_error(json!({
            "type": "string_too_long",
            "loc": ["body", field],
            "msg": format!("String should have at most {max} characters"),
            "input": text,
            "ctx": {"max_length": max},
        })));
    }
    if let Some(min) = min
        && length < min
    {
        return Err(body_error(json!({
            "type": "string_too_short",
            "loc": ["body", field],
            "msg": format!("String should have at least {min} character{}", if min == 1 { "" } else { "s" }),
            "input": text,
            "ctx": {"min_length": min},
        })));
    }
    if let Some(pattern) = pattern
        && !pattern_matches(text)
    {
        return Err(body_error(json!({
            "type": "string_pattern_mismatch",
            "loc": ["body", field],
            "msg": format!("String should match pattern '{pattern}'"),
            "input": text,
            "ctx": {"pattern": pattern},
        })));
    }
    Ok(text.clone())
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    max: Option<usize>,
    pattern: Option<&str>,
) -> Result<String, Refusal> {
    let Some(value) = object.get(field) else {
        return Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": body_value(object),
        })));
    };
    string_value(field, value, max, None, pattern)
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    max: Option<usize>,
    min: Option<usize>,
) -> Result<Option<String>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => string_value(field, value, max, min, None).map(Some),
    }
}

fn required_nullable_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    max: Option<usize>,
) -> Result<Option<String>, Refusal> {
    match object.get(field) {
        None => Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": body_value(object),
        }))),
        Some(Value::Null) => Ok(None),
        Some(value) => string_value(field, value, max, None, None).map(Some),
    }
}

fn literal_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    allowed: &[&'static str],
    required: bool,
) -> Result<Option<&'static str>, Refusal> {
    let value = match object.get(field) {
        None if required => {
            return Err(body_error(json!({
                "type": "missing",
                "loc": ["body", field],
                "msg": "Field required",
                "input": body_value(object),
            })));
        }
        None | Some(Value::Null) => return Ok(None),
        Some(value) => value,
    };
    if let Value::String(raw) = value
        && let Some(choice) = allowed
            .iter()
            .copied()
            .find(|choice| *choice == raw.as_str())
    {
        return Ok(Some(choice));
    }
    let expected = allowed
        .iter()
        .map(|choice| format!("'{choice}'"))
        .collect::<Vec<_>>();
    let expected = match expected.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    };
    Err(body_error(json!({
        "type": "literal_error",
        "loc": ["body", field],
        "msg": format!("Input should be {expected}"),
        "input": value,
        "ctx": {"expected": expected},
    })))
}

fn bool_field(object: &serde_json::Map<String, Value>, field: &str) -> Result<bool, Refusal> {
    let Some(value) = object.get(field) else {
        return Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": body_value(object),
        })));
    };
    match value {
        Value::Bool(flag) => Ok(*flag),
        Value::Number(number) if number.as_i64() == Some(0) => Ok(false),
        Value::Number(number) if number.as_i64() == Some(1) => Ok(true),
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(bool_error(field, value)),
        },
        _ => Err(bool_error(field, value)),
    }
}

fn bool_error(field: &str, value: &Value) -> Refusal {
    body_error(json!({
        "type": "bool_type",
        "loc": ["body", field],
        "msg": "Input should be a valid boolean",
        "input": value,
    }))
}

struct CreateBody {
    name: String,
    keycloak_realm: Option<String>,
    cost_centre: Option<String>,
    budget_currency: Option<&'static str>,
    external_ref: Option<String>,
}

fn parse_create(body: &Bytes) -> Result<CreateBody, Refusal> {
    let object = body_object(body)?;
    Ok(CreateBody {
        name: required_string(&object, "name", Some(255), Some(NAME_PATTERN))?,
        keycloak_realm: optional_string(&object, "keycloak_realm", Some(255), None)?,
        cost_centre: optional_string(&object, "cost_centre", Some(64), None)?,
        budget_currency: literal_field(&object, "budget_currency", &BUDGET_CURRENCIES, false)?,
        external_ref: optional_string(&object, "external_ref", Some(255), Some(1))?,
    })
}

struct RegisterBody {
    name: String,
    keycloak_realm: Option<String>,
    kind: Option<&'static str>,
    cost_centre: Option<String>,
    budget_currency: Option<&'static str>,
}

fn parse_register(body: &Bytes) -> Result<RegisterBody, Refusal> {
    let object = body_object(body)?;
    Ok(RegisterBody {
        name: required_string(&object, "name", Some(255), Some(NAME_PATTERN))?,
        keycloak_realm: optional_string(&object, "keycloak_realm", Some(255), None)?,
        kind: literal_field(&object, "kind", &TENANT_KINDS, false)?,
        cost_centre: optional_string(&object, "cost_centre", Some(64), None)?,
        budget_currency: literal_field(&object, "budget_currency", &BUDGET_CURRENCIES, false)?,
    })
}

fn parse_rename(body: &Bytes) -> Result<String, Refusal> {
    let object = body_object(body)?;
    required_string(&object, "name", Some(255), Some(NAME_PATTERN))
}

fn parse_archive(body: &Bytes) -> Result<bool, Refusal> {
    let object = body_object(body)?;
    bool_field(&object, "archived")
}

fn parse_budget_currency(body: &Bytes) -> Result<&'static str, Refusal> {
    let object = body_object(body)?;
    literal_field(&object, "budget_currency", &BUDGET_CURRENCIES, true)?.ok_or_else(internal_error)
}

fn parse_cost_centre(body: &Bytes) -> Result<Option<String>, Refusal> {
    let object = body_object(body)?;
    required_nullable_string(&object, "cost_centre", Some(64))
}

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

/// `auth.tenant.validate_tenant_uuid` as a PATH dependency: a non-UUID is the
/// reference's 400 with the route envelope (the middleware's own envelope is not used
/// inside a handler).
fn path_tenant_uuid(raw: &str) -> Result<String, Refusal> {
    match Uuid::try_parse(raw) {
        Ok(id) => Ok(id.to_string()),
        Err(_) => {
            let shown = if raw.chars().count() <= 32 && pattern_matches(raw) {
                format!("'{raw}'")
            } else {
                format!("<redacted, {} chars>", raw.chars().count())
            };
            Err(Refusal::detail(
                StatusCode::BAD_REQUEST,
                format!(
                    "Invalid path tenant_id {shown}: must be a UUID. Legacy text labels \
                     (e.g. 'n8n-dev') are tenant_name values, not tenant_ids."
                ),
            ))
        }
    }
}

fn name_taken(name: &str, context: &str) -> Refusal {
    Refusal::detail(
        StatusCode::CONFLICT,
        json!({
            "error": "tenant_name_taken",
            "name": name,
            "message": format!("tenant name already in use{context}: '{name}'"),
        }),
    )
}

fn database_error(error: TenantWriteError) -> Refusal {
    match error {
        TenantWriteError::Database(error) => {
            tracing::error!(%error, "tenant write database fault");
            internal_error()
        }
        other => {
            tracing::error!(%other, "unexpected tenant write refusal");
            internal_error()
        }
    }
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `GET /tenants` — the registry listing. Requires `license-admin`, `license-reader` or
/// `tenant-registry-reader`.
#[utoipa::path(
    get,
    path = "/tenants",
    tag = "tenants",
    operation_id = "list_tenants",
    summary = "List Tenants",
    description = "List tenants (newest first).\n\nRequires 'license-admin', 'tenant-registry-reader', or 'license-reader'. A non-admin caller sees only its own tenant; `license-admin` sees the tenants its own app owns (plus granted), while `tenant-registry-reader` and `cross_owner` keep the full roster.",
    params(
        ListTenantsQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<TenantResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Query(query): Query<ListTenantsQuery>,
) -> Result<Json<Vec<TenantResponse>>, Refusal> {
    require_any_realm_role(
        &user,
        &["license-admin", "license-reader", "tenant-registry-reader"],
    )?;
    let include_archived = match query.include_archived.as_deref() {
        Some(raw) => parse_bool("include_archived", raw)?,
        None => false,
    };
    let include_service_actors = match query.include_service_actors.as_deref() {
        Some(raw) => parse_bool("include_service_actors", raw)?,
        None => false,
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let rows = if user.has_realm_role("tenant-registry-reader")
        || (user.has_realm_role("license-admin") && user.cross_owner)
    {
        tenants_crud::list_tenants(pool, include_archived, include_service_actors, None).await
    } else if user.has_realm_role("license-admin") && user.owner_key.is_some() {
        tenants_crud::list_tenants(
            pool,
            include_archived,
            include_service_actors,
            user.owner_key.as_deref(),
        )
        .await
    } else {
        // A non-admin sees only its own tenant row — archived and service included,
        // because this is an identity answer, not a pick-list.
        let caller = get_tenant_id(&state, &user).await?;
        let id = Uuid::try_parse(&caller).map_err(|_| internal_error())?;
        let own = tenants_crud::get_tenant(pool, id).await.map_err(|error| {
            tracing::error!(%error, "Failed to list tenants");
            internal_error()
        })?;
        let mut out = Vec::with_capacity(1);
        if let Some(row) = own {
            out.push(tenant_response(row)?);
        }
        return Ok(Json(out));
    };
    let rows = rows.map_err(|error| {
        tracing::error!(%error, "Failed to list tenants");
        internal_error()
    })?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(tenant_response(row)?);
    }
    Ok(Json(out))
}

/// `GET /tenants/{name_or_id}` — resolve a tenant by UUID or name.
#[utoipa::path(
    get,
    path = "/tenants/{name_or_id}",
    tag = "tenants",
    operation_id = "get_tenant",
    summary = "Get Tenant",
    description = "Resolve a tenant by UUID or name. Requires 'license-admin' or 'license-reader'.\n\nA non-admin caller may resolve only its OWN tenant; a foreign tenant is 404, never 403, so the customer roster cannot be enumerated.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
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
pub async fn get_tenant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let row = if user.has_realm_role("license-admin")
        && (user.owner_key.is_some() || user.cross_owner)
    {
        tenants_crud::resolve_for_licensed_admin(pool, &user, &name_or_id, "tenant_read").await?
    } else {
        let caller = get_tenant_id(&state, &user).await?;
        tenants_crud::read_own_tenant(pool, &caller, &name_or_id).await?
    };
    Ok(Json(tenant_response(row)?))
}

/// `POST /tenants` — create a canonical tenant under the caller's owning app.
#[utoipa::path(
    post,
    path = "/tenants",
    tag = "tenants",
    operation_id = "create_tenant",
    summary = "Create Tenant",
    description = "Create a canonical tenant under the CALLER's owning app.\n\nRequires 'license-admin' or 'license-manager'. `external_ref` sent and already registered by this app on a LIVE tenant returns 200 with that tenant; a name taken by a LIVE tenant of THIS app is 409 `tenant_name_taken`.",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantCreate,
    responses(
        (status = 200, body = TenantResponse),
        (status = 201, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    body: Bytes,
) -> Result<(StatusCode, Json<TenantResponse>), Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let parsed = parse_create(&body)?;
    let owner = require_known_owner(&user)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    if let Some(external_ref) = parsed.external_ref.as_deref()
        && let Some(prior) = tenants_crud::get_tenant_by_external_ref(pool, &owner, external_ref)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to create tenant");
                internal_error()
            })?
    {
        return Ok((StatusCode::OK, Json(tenant_response(prior)?)));
    }
    let new = NewTenant {
        name: &parsed.name,
        keycloak_realm: parsed.keycloak_realm.as_deref(),
        cost_centre: parsed.cost_centre.as_deref(),
        budget_currency: parsed.budget_currency,
        owner_key: &owner,
        external_ref: parsed.external_ref.as_deref(),
    };
    let row = tenants_crud::create_tenant(pool, &new)
        .await
        .map_err(|error| match error {
            TenantWriteError::NameTaken => name_taken(&parsed.name, " by this app"),
            other => database_error(other),
        })?;
    tracing::info!(
        tenant = %row.id,
        name = %row.name,
        owner = %row.owner_key,
        sub = %user.sub,
        "Tenant created"
    );
    Ok((StatusCode::CREATED, Json(tenant_response(row)?)))
}

/// `PUT /tenants/{tenant_id}` — register a tenant whose UUID is already committed elsewhere.
#[utoipa::path(
    put,
    path = "/tenants/{tenant_id}",
    tag = "tenants",
    operation_id = "register_tenant",
    summary = "Register Tenant",
    description = "Register a tenant with a CALLER-SUPPLIED id, idempotently.\n\nRequires 'license-admin' or 'license-manager'. Re-registering the same id returns 200 with the existing row UNCHANGED. 409 `tenant_owned_by_other_app` when the id belongs to another app that has not granted the caller access; 409 `tenant_name_taken` when this app already has a different LIVE tenant with this name.",
    params(
        ("tenant_id" = String, Path, description = "Tenant Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantRegister,
    responses(
        (status = 200, body = TenantResponse),
        (status = 201, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn register_tenant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(tenant_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<TenantResponse>), Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let tenant_id = path_tenant_uuid(&tenant_id)?;
    let parsed = parse_register(&body)?;
    let owner = require_known_owner(&user)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let id = Uuid::try_parse(&tenant_id).map_err(|_| internal_error())?;
    if let Some(existing) = tenants_crud::get_tenant(pool, id).await.map_err(|error| {
        tracing::error!(%error, "Failed to register tenant");
        internal_error()
    })? {
        if existing.owner_key != owner {
            if user.cross_owner {
                tenants_crud::audit_cross_owner(&user, "tenant_register", &existing);
            } else if owner_can_access_tenant(pool, &owner, &tenant_id)
                .await
                .unwrap_or(false)
            {
                if parsed.name != existing.name {
                    tracing::warn!(
                        tenant = %existing.id,
                        owner = %existing.owner_key,
                        grantee = %owner,
                        stored_name = %existing.name,
                        body_name = %parsed.name,
                        azp = ?user.azp,
                        "tenant_register_by_grantee_name_ignored"
                    );
                }
            } else {
                return Err(Refusal::detail(
                    StatusCode::CONFLICT,
                    json!({
                        "error": "tenant_owned_by_other_app",
                        "message": "this tenant id is registered to a different owning app",
                    }),
                ));
            }
        }
        return Ok((StatusCode::OK, Json(tenant_response(existing)?)));
    }
    let new = EnsureTenant {
        id,
        name: &parsed.name,
        keycloak_realm: parsed.keycloak_realm.as_deref(),
        kind: parsed.kind,
        cost_centre: parsed.cost_centre.as_deref(),
        budget_currency: parsed.budget_currency,
        owner_key: &owner,
    };
    let row = tenants_crud::ensure_tenant(pool, &new)
        .await
        .map_err(|error| {
            match error {
                TenantWriteError::NameTaken => name_taken(&parsed.name, " by this app"),
                // `ensure_tenant`'s ClientSafeError arms: the message is caller-authored
                // (the id and the caller's own body), so the route may echo it.
                TenantWriteError::PrincipalId(message)
                | TenantWriteError::InvalidValue(message) => {
                    Refusal::detail(StatusCode::CONFLICT, message)
                }
                other => database_error(other),
            }
        })?;
    tracing::info!(
        tenant = %row.id,
        name = %row.name,
        owner = %row.owner_key,
        kind = %row.kind,
        cost_centre = ?row.cost_centre,
        sub = %user.sub,
        "Tenant registered with explicit id"
    );
    Ok((StatusCode::CREATED, Json(tenant_response(row)?)))
}

/// `PATCH /tenants/{name_or_id}` — rename a tenant. Owner only.
#[utoipa::path(
    patch,
    path = "/tenants/{name_or_id}",
    tag = "tenants",
    operation_id = "rename_tenant",
    summary = "Rename Tenant",
    description = "Rename a tenant. Requires 'license-admin' or 'license-manager' on a tenant the caller OWNS (grantee: 403 `tenant_owner_only`).\n\nIdempotent: renaming to the current name returns 200 unchanged.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantRename,
    responses(
        (status = 200, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn rename_tenant(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
    body: Bytes,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let new_name = parse_rename(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenants_crud::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_rename").await?;
    let updated = tenants_crud::rename_tenant(pool, tenant.id, &new_name)
        .await
        .map_err(|error| match error {
            TenantWriteError::NameTaken => name_taken(&new_name, ""),
            TenantWriteError::InvalidName => Refusal::detail(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "invalid tenant name: '{new_name}' — lowercase letters, digits and \
                     hyphens only, and it may not start with a hyphen"
                ),
            ),
            other => database_error(other),
        })?;
    match updated {
        Mutated::Row(row) => {
            tracing::info!(tenant = %row.id, name = %row.name, "Tenant renamed");
            Ok(Json(tenant_response(row)?))
        }
        Mutated::NotFound => Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            format!("tenant not found: {name_or_id}"),
        )),
    }
}

/// `PATCH /tenants/{name_or_id}/archive` — retire or restore a tenant. Owner only.
#[utoipa::path(
    patch,
    path = "/tenants/{name_or_id}/archive",
    tag = "tenants",
    operation_id = "set_tenant_archived",
    summary = "Set Tenant Archived",
    description = "Retire a tenant, or restore one. Requires 'license-admin' or 'license-manager' on a tenant the caller OWNS (grantee: 403 `tenant_owner_only`).\n\nRevokes NOTHING: licences, tokens, budgets and grants survive. Restoring refuses 409 when a LIVE tenant of the same app now holds the archived row's name or external_ref.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantArchiveUpdate,
    responses(
        (status = 200, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn set_tenant_archived(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
    body: Bytes,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let archived = parse_archive(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenants_crud::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_archive").await?;
    let selected = tenant.id.to_string();
    let updated = tenants_crud::set_tenant_archived(pool, tenant.id, archived)
        .await
        .map_err(|error| match error {
            TenantWriteError::NameTaken => Refusal::detail(
                StatusCode::CONFLICT,
                json!({
                    "error": "tenant_name_taken",
                    "tenant_id": selected,
                    "message": "a live tenant of this app now holds this tenant's name; rename the archived tenant before restoring it",
                }),
            ),
            TenantWriteError::ExternalRefTaken => Refusal::detail(
                StatusCode::CONFLICT,
                json!({
                    "error": "tenant_external_ref_taken",
                    "tenant_id": selected,
                    "message": "a live tenant of this app now holds this tenant's external_ref; the archived tenant cannot be restored beside it",
                }),
            ),
            other => database_error(other),
        })?;
    match updated {
        Mutated::Row(row) => {
            tracing::info!(
                tenant = %row.id,
                name = %row.name,
                archived,
                "Tenant archive state set"
            );
            Ok(Json(tenant_response(row)?))
        }
        Mutated::NotFound => Err(Refusal::detail(
            StatusCode::NOT_FOUND,
            format!("tenant not found: {name_or_id}"),
        )),
    }
}

/// The claims the forwarded-user arm contributes: `(actor, may_change_currency)`.
struct ForwardedAuthority {
    actor: String,
    allow_change: bool,
}

fn forwarded_invalid() -> Refusal {
    Refusal::detail(
        StatusCode::UNAUTHORIZED,
        json!({
            "error": "forwarded_user_token_invalid",
            "message": "X-Forwarded-User-Authorization could not be verified",
        }),
    )
}

fn realm_of(issuer: &str) -> Option<&str> {
    issuer
        .trim_end_matches('/')
        .split_once("/realms/")
        .map(|(_, realm)| realm)
        .filter(|realm| !realm.is_empty())
}

/// `_verify_forwarded_user_token` + `_enforce_forwarded_realm_owner`.
///
/// Verified with the SAME JWKS verifier the service uses for `Authorization`; the
/// reference narrows this one to `USER_TRUSTED_ISSUERS` (default: the `portal` realm).
/// A token that cannot be verified is a 401 — never a silent downgrade to the caller's
/// own authority.
async fn forwarded_authority(
    state: &AppState,
    user: &User,
    headers: &HeaderMap,
    tenant: &TenantRow,
) -> Result<Option<ForwardedAuthority>, Refusal> {
    let Some(raw) = headers.get(FORWARDED_USER_AUTH_HEADER) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| forwarded_invalid())?;
    let (scheme, token) = raw.split_once(' ').unwrap_or((raw, ""));
    let token = token.trim();
    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() || !looks_like_jwt(token) {
        return Err(forwarded_invalid());
    }
    let Some(claims) = state.verifier.verify(token).await else {
        return Err(forwarded_invalid());
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    if let Some(issuer) = claims.get("iss").and_then(Value::as_str)
        && let Some(realm) = realm_of(issuer)
    {
        let owner = resolve_realm_owner(pool, realm).await.map_err(|error| {
            tracing::warn!(%error, realm, "realm_owner_resolution_failed: refused");
            Refusal::detail(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({
                    "error": "realm_owner_unresolved",
                    "message": "the forwarded token realm's owner could not be resolved",
                }),
            )
        })?;
        if let Some(owner) = owner
            && !owner_can_access_tenant(pool, &owner, &tenant.id.to_string())
                .await
                .unwrap_or(false)
        {
            tracing::warn!(realm, %owner, tenant = %tenant.id, "realm_owner_mismatch");
            return Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                crate::auth::realm_owner_mismatch_detail(),
            ));
        }
    }
    let actor = claims
        .get("sub")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let owner_roles: Vec<&str> = claims
        .get("realm_access")
        .and_then(|access| access.get("roles"))
        .and_then(Value::as_array)
        .map(|roles| roles.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let allow_change = user.has_realm_role("license-admin")
        && owner_roles
            .iter()
            .any(|role| BUDGET_CURRENCY_FORWARDED_OWNER_ROLES.contains(role));
    Ok(Some(ForwardedAuthority {
        actor,
        allow_change,
    }))
}

/// `PUT /tenants/{name_or_id}/budget-currency` — set a tenant's money-budget currency.
#[utoipa::path(
    put,
    path = "/tenants/{name_or_id}/budget-currency",
    tag = "tenants",
    operation_id = "set_tenant_budget_currency",
    summary = "Set Tenant Budget Currency",
    description = "Set a tenant's money-budget currency. Requires 'license-admin', 'license-manager' or 'platform-admin' on a tenant the caller OWNS (grantee: 403 `tenant_owner_only`).\n\nnull -> a currency: any caller. A currency -> a DIFFERENT currency: the owner capability only (`platform-admin`, or a verified forwarded portal `super_admin`/`platform-admin` token via a `license-admin` relay), else 409 `budget_currency_immutable`. An archived tenant is 409 `tenant_archived`.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Forwarded-User-Authorization" = Option<String>, Header, description = "Optional `Bearer <portal JWT>` of the END USER on whose behalf a `license-admin` relay calls. Verified with the same JWKS checks as `Authorization`; a realm with a `realm_owner` row may only name that owner's (or granted) tenants. If its realm roles include `super_admin` or `platform-admin`, the currency may be SWITCHED."),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantBudgetCurrencyUpdate,
    responses(
        (status = 200, body = TenantResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn set_tenant_budget_currency(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(
        &user,
        &[
            "license-admin",
            "license-manager",
            BUDGET_CURRENCY_CHANGE_ROLE,
        ],
    )?;
    let currency = parse_budget_currency(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenants_crud::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_budget_currency")
            .await?;
    let mut actor = user.sub.clone();
    let mut allow_change = user.has_realm_role(BUDGET_CURRENCY_CHANGE_ROLE);
    if let Some(forwarded) = forwarded_authority(&state, &user, &headers, &tenant).await? {
        actor = forwarded.actor;
        allow_change = allow_change || forwarded.allow_change;
    }
    let updated = tenants_crud::set_tenant_budget_currency(
        pool,
        tenant.id,
        currency,
        &actor,
        user.azp.as_deref(),
        allow_change,
    )
    .await
    .map_err(|error| match error {
        tenants_crud::BudgetCurrencyError::Archived => Refusal::detail(
            StatusCode::CONFLICT,
            json!({
                "error": "tenant_archived",
                "message": "an archived tenant's budget_currency is not changed",
            }),
        ),
        tenants_crud::BudgetCurrencyError::Immutable => Refusal::detail(
            StatusCode::CONFLICT,
            json!({
                "error": "budget_currency_immutable",
                "message": "budget_currency may only move from null to a currency; switching currency requires the owner capability (platform-admin, or a forwarded portal super_admin token via a license-admin relay)",
            }),
        ),
        tenants_crud::BudgetCurrencyError::Database(error) => {
            tracing::error!(%error, "Failed to set tenant budget currency");
            internal_error()
        }
    })?;
    let Some(row) = updated else {
        return Err(tenant_not_found());
    };
    Ok(Json(tenant_response(row)?))
}

/// `PATCH /tenants/{name_or_id}/cost-centre` — set or clear a tenant's cost centre.
#[utoipa::path(
    patch,
    path = "/tenants/{name_or_id}/cost-centre",
    tag = "tenants",
    operation_id = "set_tenant_cost_centre",
    summary = "Set Tenant Cost Centre",
    description = "Set or clear a tenant's chargeback cost centre. Requires 'license-admin' or 'license-manager' on a tenant the caller OWNS (grantee: 403 `tenant_owner_only`).\n\n`{\"cost_centre\": null}` CLEARS the value and is a deliberate operation, not a malformed request.",
    params(
        ("name_or_id" = String, Path, description = "Name Or Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TenantCostCentreUpdate,
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
pub async fn set_tenant_cost_centre(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(name_or_id): Path<String>,
    body: Bytes,
) -> Result<Json<TenantResponse>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let cost_centre = parse_cost_centre(&body)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    let tenant =
        tenants_crud::resolve_owned_tenant(pool, &user, &name_or_id, "tenant_cost_centre").await?;
    let updated = tenants_crud::set_tenant_cost_centre(pool, tenant.id, cost_centre.as_deref())
        .await
        .map_err(|error| match error {
            TenantWriteError::InvalidCostCentre(message) => {
                Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, message)
            }
            other => database_error(other),
        })?;
    let Some(row) = updated else {
        return Err(tenant_not_found());
    };
    tracing::info!(
        tenant = %row.id,
        name = %row.name,
        cost_centre = ?cost_centre,
        "Tenant cost_centre set"
    );
    Ok(Json(tenant_response(row)?))
}
