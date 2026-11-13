//! `/api/admin/*` — tenant inference policy administration.
//!
//! The shapes are a mix of named schemas and free-form objects, exactly as the capture has them:
//! policy documents are validated against the policy model, so the HTTP body is `dict[str, Any]`.
//!
//! Every handler authenticates FIRST, through the reference's two-token verifier: a public ingress
//! host is 404, an unconfigured pod is 503, a missing or invalid token is 401, an `azp` outside the
//! allowlist or a human without an admin role is 403. The handler body runs only for a proven
//! principal.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::admin_auth::AdminPrincipal;
use crate::contract::schemas::{
    AnyObject,
    Body_activate_inference_policy_api_admin_tenants__tenant_id__inference_policy_activate_post,
    HTTPValidationError,
};
use crate::routing::Refusal;
use crate::state::AppState;

/// The reference's `Depends(require_admin)`: 404 / 503 / 401 / 403, or the principal.
async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<AdminPrincipal, Refusal> {
    state
        .admin()
        .authenticate(headers, &state.config().public_host_suffixes)
        .await
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
    StatusCode::NOT_IMPLEMENTED.into_response()
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
pub async fn list_tenant_calls(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = require_admin(&state, &headers).await {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
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
pub async fn get_inference_policy(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = require_admin(&state, &headers).await {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
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
pub async fn put_inference_policy(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = require_admin(&state, &headers).await {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
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
) -> Response {
    if let Err(refusal) = require_admin(&state, &headers).await {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}
