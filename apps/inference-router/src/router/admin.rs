//! `/api/admin/*` — tenant inference policy administration.
//!
//! The shapes are a mix of named schemas and free-form objects, exactly as the capture has them:
//! policy documents are validated against the policy model, so the HTTP body is `dict[str, Any]`.
//!
//! Every handler authenticates FIRST, through the reference's two-token verifier: a public ingress
//! host is 404, an unconfigured pod is 503, a missing or invalid token is 401, an `azp` outside the
//! allowlist or a human without an admin role is 403. The handler body runs only for a proven
//! principal.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::admin_auth::AdminPrincipal;
use crate::admin_policy::{self, Actor, BYO_PROVIDERS};
use crate::config::PolicyMode;
use crate::contract::schemas::{
    AnyObject,
    Body_activate_inference_policy_api_admin_tenants__tenant_id__inference_policy_activate_post,
    HTTPValidationError,
};
use crate::policy::PLATFORM_MODEL;
use crate::routing::Refusal;
use crate::state::AppState;

/// The reference's `Depends(require_admin)`: 404 / 503 / 401 / 403, or the principal.
async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<AdminPrincipal, Refusal> {
    state
        .admin()
        .authenticate(headers, &state.config().public_host_suffixes)
        .await
}

/// The reference's `_tenant`: a canonical non-nil uuid, never a platform tenant, then the role.
fn tenant(
    state: &AppState,
    principal: &AdminPrincipal,
    raw: &str,
    write: bool,
) -> Result<Uuid, Refusal> {
    let parsed = Uuid::parse_str(raw.trim()).ok().filter(|id| !id.is_nil());
    let Some(parsed) = parsed else {
        return Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            "tenant_id must be a non-nil uuid",
            "tenant_invalid",
        ));
    };
    let canonical = parsed.to_string();
    if state
        .config()
        .commercial_platform_tenant_ids
        .iter()
        .any(|listed| listed.trim().to_ascii_lowercase() == canonical)
    {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "platform_tenant",
            "platform_tenant",
        ));
    }
    principal.authorize(&canonical, write)?;
    Ok(parsed)
}

/// The reference's `_store`: the policy API is disabled in `off` mode, and a pod without a pool
/// serves nothing rather than pretending.
fn pool(state: &AppState) -> Result<&sqlx::PgPool, Refusal> {
    if state.config().policy_mode == PolicyMode::Off {
        return Err(store_disabled());
    }
    state.deps().ledger.as_ref().ok_or_else(store_disabled)
}

fn store_disabled() -> Refusal {
    Refusal::detail(
        StatusCode::SERVICE_UNAVAILABLE,
        "policy store disabled: INFERENCE_ROUTER_POLICY_MODE=off",
        "policy_store_disabled",
    )
}

/// FastAPI's 422 for a query parameter.
fn query_error(field: &str, message: &str) -> Refusal {
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: json!({
            "detail": [{"type": "value_error", "loc": ["query", field], "msg": message}]
        }),
        headers: Vec::new(),
    }
}

/// FastAPI's 422 for a body field.
fn body_error(field: &str, kind: &str, message: &str, context: Value) -> Refusal {
    let mut detail = json!({
        "type": kind,
        "loc": ["body", field],
        "msg": message,
    });
    if !context.is_null() {
        detail["ctx"] = context;
    }
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: json!({"detail": [detail]}),
        headers: Vec::new(),
    }
}

fn invalid_json() -> Refusal {
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: json!({
            "detail": [{
                "type": "json_invalid",
                "loc": ["body", 0],
                "msg": "JSON decode error",
            }]
        }),
        headers: Vec::new(),
    }
}

fn internal(error: sqlx::Error) -> Response {
    tracing::error!(%error, "admin policy store failed");
    Refusal::detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error",
        "admin_store_error",
    )
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/admin/catalog",
    tag = "admin-tenants",
    summary = "The policy catalogue",
    responses((status = 200, body = inline(AnyObject)))
)]
pub async fn get_catalog(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = require_admin(&state, &headers).await {
        return refusal.into_response();
    }
    let mut names: Vec<&str> = state
        .router()
        .catalog()
        .providers
        .iter()
        .filter(|provider| provider.customer_eligible)
        .map(|provider| provider.name.as_str())
        .collect();
    names.sort_unstable();
    Json(json!({
        "platform": names
            .into_iter()
            .map(|name| json!({"provider": name, "models": [PLATFORM_MODEL]}))
            .collect::<Vec<_>>(),
        "owner": [],
        "byo_providers": BYO_PROVIDERS,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct CallsQuery {
    correlation_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/admin/tenants/{tenant_id}/calls",
    tag = "admin-tenants",
    summary = "One tenant's calls for a correlation id",
    params(
        ("tenant_id" = String, Path),
        ("correlation_id" = String, Query, min_length = 1, max_length = 512)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_tenant_calls(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Query(query): Query<CallsQuery>,
) -> Response {
    let principal = match require_admin(&state, &headers).await {
        Ok(principal) => principal,
        Err(refusal) => return refusal.into_response(),
    };
    let tenant = match tenant(&state, &principal, &tenant_id, false) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let correlation_id = match query.correlation_id {
        None => return query_error("correlation_id", "Field required").into_response(),
        Some(id) if id.chars().count() < 1 => {
            return query_error("correlation_id", "String should have at least 1 character")
                .into_response();
        }
        Some(id) if id.chars().count() > 512 => {
            return query_error(
                "correlation_id",
                "String should have at most 512 characters",
            )
            .into_response();
        }
        Some(id) => id,
    };
    match admin_policy::tenant_calls(pool, tenant, &correlation_id).await {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(error) => internal(error),
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/tenants/{tenant_id}/inference-policy",
    tag = "admin-tenants",
    summary = "Read a tenant's inference policy",
    params(("tenant_id" = String, Path)),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn get_inference_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Response {
    let principal = match require_admin(&state, &headers).await {
        Ok(principal) => principal,
        Err(refusal) => return refusal.into_response(),
    };
    let tenant = match tenant(&state, &principal, &tenant_id, false) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    match admin_policy::describe(pool, tenant).await {
        Ok(body) => Json(body).into_response(),
        Err(error) => internal(error),
    }
}

#[utoipa::path(
    put,
    path = "/api/admin/tenants/{tenant_id}/inference-policy",
    tag = "admin-tenants",
    summary = "Write a tenant's inference policy",
    params(("tenant_id" = String, Path)),
    request_body = serde_json::Value,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn put_inference_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    body: Bytes,
) -> Response {
    let principal = match require_admin(&state, &headers).await {
        Ok(principal) => principal,
        Err(refusal) => return refusal.into_response(),
    };
    let tenant = match tenant(&state, &principal, &tenant_id, true) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let document: Value = match serde_json::from_slice(&body) {
        Ok(document) => document,
        Err(_) => return invalid_json().into_response(),
    };
    let platform_providers: Vec<String> = state
        .router()
        .catalog()
        .providers
        .iter()
        .filter(|provider| provider.customer_eligible)
        .map(|provider| provider.name.clone())
        .collect();
    let (parsed, violations) = admin_policy::validate(&document, &platform_providers);
    let Some(parsed) = parsed else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "policy_invalid", "violations": violations})),
        )
            .into_response();
    };
    if !violations.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": "policy_invalid", "violations": violations})),
        )
            .into_response();
    }
    let actor = Actor {
        kind: principal.kind.as_str().to_owned(),
        ref_hash: principal.ref_hash.clone(),
    };
    match admin_policy::put(pool, tenant, &parsed, &actor).await {
        Ok(result) => Json(result.to_json()).into_response(),
        Err(error) => internal(error),
    }
}

#[derive(Debug, Deserialize)]
pub struct ActivateBody {
    version: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/admin/tenants/{tenant_id}/inference-policy/activate",
    tag = "admin-tenants",
    summary = "Activate a policy version",
    params(("tenant_id" = String, Path)),
    request_body = Body_activate_inference_policy_api_admin_tenants__tenant_id__inference_policy_activate_post,
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn activate_inference_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    body: Bytes,
) -> Response {
    let principal = match require_admin(&state, &headers).await {
        Ok(principal) => principal,
        Err(refusal) => return refusal.into_response(),
    };
    let tenant = match tenant(&state, &principal, &tenant_id, true) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let requested: ActivateBody = match serde_json::from_slice(&body) {
        Ok(requested) => requested,
        Err(_) => return invalid_json().into_response(),
    };
    let version = match requested.version {
        None => {
            return body_error("version", "missing", "Field required", Value::Null).into_response();
        }
        Some(version) if version < 1 => {
            return body_error(
                "version",
                "greater_than_equal",
                "Input should be greater than or equal to 1",
                json!({"ge": 1}),
            )
            .into_response();
        }
        Some(version) => version as i32,
    };
    let actor = Actor {
        kind: principal.kind.as_str().to_owned(),
        ref_hash: principal.ref_hash.clone(),
    };
    match admin_policy::activate(pool, tenant, version, &actor).await {
        Ok(Some(active)) => Json(json!({"active_version": active})).into_response(),
        Ok(None) => Refusal::detail(
            StatusCode::NOT_FOUND,
            "version not found",
            "version_not_found",
        )
        .into_response(),
        Err(error) => internal(error),
    }
}
