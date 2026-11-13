//! `/api/canary/*` — the prompt-canary controller.
//!
//! These routes move a worker's version pointer; the actual `system_prompt` swap lives in
//! worker-management. Live canary rows are empty (measured 2026-11-13), so this slice is CRUD
//! parity, not traffic parity.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::contract::schemas::{
    ConfigurePromptCanaryRequest, ConfigurePromptCanaryResponse, HTTPValidationError,
    PromotePromptRequest, PromotePromptResponse, PromptCanaryState, RollbackPromptRequest,
    RollbackPromptResponse,
};

#[utoipa::path(
    post,
    path = "/api/canary/promote-prompt",
    tag = "canary",
    summary = "Promote the canary version",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = PromotePromptRequest,
    responses(
        (status = 200, body = PromotePromptResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn promote_prompt() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    post,
    path = "/api/canary/prompt/configure",
    tag = "canary",
    summary = "Configure a canary window",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = ConfigurePromptCanaryRequest,
    responses(
        (status = 200, body = ConfigurePromptCanaryResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn configure_prompt() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/canary/prompt/{worker_name}",
    tag = "canary",
    summary = "Read a worker's canary state",
    params(
        ("worker_name" = String, Path),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = PromptCanaryState),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn get_prompt_canary() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    post,
    path = "/api/canary/rollback-prompt",
    tag = "canary",
    summary = "Roll the canary back",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = RollbackPromptRequest,
    responses(
        (status = 200, body = RollbackPromptResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn rollback_prompt() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}
