//! `/api/replay/*` — the replay invocation log.
//!
//! A replay row is written by a `X-Replay-Mode: true` inference; the runner backfills the
//! validation outcome later. Live rows are empty (measured 2026-11-13), so this slice is CRUD
//! parity.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::contract::schemas::{
    HTTPValidationError, InvocationLogListResponse, PatchInvocationLogRequest,
    PatchInvocationLogResponse,
};

fn default_replay_limit() -> u32 {
    50
}

/// The listing filters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReplayLogQuery {
    /// Filter by worker name.
    #[serde(default)]
    #[param(nullable)]
    pub worker_name: Option<String>,
    /// Filter by prompt version.
    #[serde(default)]
    #[param(nullable)]
    pub version: Option<String>,
    /// Return only rows where validation_passed is false (failed examples).
    #[serde(default)]
    #[param(default = false)]
    pub only_failed: bool,
    #[serde(default = "default_replay_limit")]
    #[param(default = 50, minimum = 1, maximum = 200)]
    pub limit: u32,
}

#[utoipa::path(
    get,
    path = "/api/replay/invocation-log",
    tag = "replay",
    summary = "List replay invocation-log rows",
    params(
        ReplayLogQuery,
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = InvocationLogListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_invocation_log() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    patch,
    path = "/api/replay/invocation-log/{log_id}",
    tag = "replay",
    summary = "Backfill a replay row",
    params(
        ("log_id" = String, Path, format = "uuid"),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = PatchInvocationLogRequest,
    responses(
        (status = 200, body = PatchInvocationLogResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn patch_invocation_log() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    patch,
    path = "/api/replay/invocation-log/by-correlation/{correlation_id}",
    tag = "replay",
    summary = "Backfill every row of a correlation id",
    params(
        ("correlation_id" = String, Path, format = "uuid"),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header)
    ),
    request_body = PatchInvocationLogRequest,
    responses(
        (status = 200, body = PatchInvocationLogResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn patch_invocation_log_by_correlation() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}
