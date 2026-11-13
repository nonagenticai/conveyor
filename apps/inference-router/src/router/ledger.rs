//! `/api/internal/*` — reads over the `inference_call` ledger.
//!
//! Auth is the shared `X-Internal-Secret` (fail closed: 503 when unconfigured, 401 on a missing or
//! wrong header). The reads land with the ledger slice; the contract is fixed here.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::contract::schemas::{AnyObject, HTTPValidationError};
use crate::routing::Refusal;
use crate::state::AppState;

/// The shared `X-Internal-Secret` guard, refused before anything else runs.
fn guard(state: &AppState, headers: &HeaderMap) -> Result<(), Refusal> {
    crate::internal::require(headers, state.config())
}

fn default_call_limit() -> u32 {
    100
}

fn default_days() -> u32 {
    1
}

fn default_timeline_days() -> u32 {
    30
}

fn default_source() -> String {
    "all".to_owned()
}

/// Filters for the call list.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CallListQuery {
    #[serde(default)]
    #[param(nullable)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    #[param(nullable)]
    pub caller: Option<String>,
    #[serde(default)]
    #[param(nullable, format = "date-time")]
    pub since: Option<String>,
    #[serde(default)]
    #[param(nullable, format = "date-time")]
    pub until: Option<String>,
    #[serde(default = "default_call_limit")]
    #[param(default = 100, minimum = 1, maximum = 500)]
    pub limit: u32,
}

/// Filters for the pinning summary.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PinningSummaryQuery {
    #[serde(default)]
    #[param(nullable, format = "date-time")]
    pub since: Option<String>,
    #[serde(default = "default_source")]
    #[param(default = "all", pattern = "^(benchmark|live|unknown|all)$")]
    pub source: String,
}

/// Filters for the repeat-group report.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RepeatGroupsQuery {
    #[serde(default)]
    #[param(nullable, format = "date-time")]
    pub since: Option<String>,
    #[serde(default)]
    #[param(nullable)]
    pub temperature: Option<f64>,
    #[serde(default)]
    #[param(nullable)]
    pub caller: Option<String>,
    #[serde(default)]
    #[param(nullable)]
    pub served_provider: Option<String>,
    #[serde(default = "default_source")]
    #[param(default = "all", pattern = "^(benchmark|live|unknown|all)$")]
    pub source: String,
    #[serde(default)]
    #[param(nullable)]
    pub seeded: Option<bool>,
    #[serde(default = "default_call_limit")]
    #[param(default = 100, minimum = 1, maximum = 500)]
    pub limit: u32,
}

/// The summary window.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SummaryQuery {
    #[serde(default = "default_days")]
    #[param(default = 1, minimum = 1, maximum = 90)]
    pub days: u32,
}

/// The served-model timeline window.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TimelineQuery {
    #[serde(default = "default_timeline_days")]
    #[param(default = 30, minimum = 1, maximum = 90)]
    pub days: u32,
}

#[utoipa::path(
    get,
    path = "/api/internal/inference-calls",
    tag = "ledger",
    summary = "List recorded inference calls",
    params(
        CallListQuery,
        ("X-Internal-Secret" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_inference_calls(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/internal/inference-calls/pinning-summary",
    tag = "ledger",
    summary = "Pinning summary",
    params(
        PinningSummaryQuery,
        ("X-Internal-Secret" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn pinning_summary(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/internal/inference-calls/repeat-groups",
    tag = "ledger",
    summary = "Repeat-group report",
    params(
        RepeatGroupsQuery,
        ("X-Internal-Secret" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn repeat_groups(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/internal/inference-calls/summary",
    tag = "ledger",
    summary = "Call summary",
    params(
        SummaryQuery,
        ("X-Internal-Secret" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn calls_summary(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/internal/inference-calls/{call_id}/capture",
    tag = "ledger",
    summary = "One captured call",
    params(
        ("call_id" = i64, Path),
        ("X-Internal-Secret" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn capture_call(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/internal/served-models/timeline",
    tag = "ledger",
    summary = "Served-model timeline",
    params(
        TimelineQuery,
        ("X-Internal-Secret" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = inline(AnyObject)),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn served_models_timeline(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    StatusCode::NOT_IMPLEMENTED.into_response()
}
