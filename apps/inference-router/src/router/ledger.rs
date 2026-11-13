//! `/api/internal/*` — reads over the `inference_call` ledger.
//!
//! Auth is the shared `X-Internal-Secret` (fail closed: 503 when unconfigured, 401 on a missing or
//! wrong header), plus the capture allowlist on `/{id}/capture`. The SQL and the documents live in
//! `crate::reads`; this module is the HTTP shell: query parsing, the reference's range rules
//! (`days` 1..=90, `limit` 1..=500 — FastAPI answers 422 and axum does not), and the responses.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::contract::schemas::{AnyObject, HTTPValidationError};
use crate::reads::{self, MAX_DAYS, MAX_LIMIT};
use crate::routing::Refusal;
use crate::state::AppState;

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

/// The shared `X-Internal-Secret` guard, refused before anything else runs.
fn guard(state: &AppState, headers: &HeaderMap) -> Result<(), Refusal> {
    crate::internal::require(headers, state.config())
}

/// FastAPI's implicit 422 for an out-of-range query value.
fn validation_error(field: &str, message: &str) -> Refusal {
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: serde_json::json!({
            "detail": [{"type": "value_error", "loc": ["query", field], "msg": message}]
        }),
        headers: Vec::new(),
    }
}

fn parse_timestamp(field: &str, raw: Option<&str>) -> Result<Option<DateTime<Utc>>, Refusal> {
    match raw {
        None => Ok(None),
        Some(value) => reads::parse_timestamp(value)
            .map(Some)
            .ok_or_else(|| validation_error(field, "Input should be a valid datetime")),
    }
}

fn check_days(days: u32) -> Result<i32, Refusal> {
    if (1..=MAX_DAYS as u32).contains(&days) {
        Ok(days as i32)
    } else {
        Err(validation_error(
            "days",
            "Input should be greater than or equal to 1 and less than or equal to 90",
        ))
    }
}

fn check_limit(limit: u32) -> Result<i64, Refusal> {
    if (1..=MAX_LIMIT as u32).contains(&limit) {
        Ok(limit as i64)
    } else {
        Err(validation_error(
            "limit",
            "Input should be greater than or equal to 1 and less than or equal to 500",
        ))
    }
}

fn check_source(source: &str) -> Result<(), Refusal> {
    if matches!(source, "benchmark" | "live" | "unknown" | "all") {
        Ok(())
    } else {
        Err(validation_error(
            "source",
            "String should match pattern '^(benchmark|live|unknown|all)$'",
        ))
    }
}

fn pool(state: &AppState) -> Result<&sqlx::PgPool, Refusal> {
    state.deps().ledger.as_ref().ok_or_else(|| {
        Refusal::detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "ledger database is not configured",
            "ledger_unavailable",
        )
    })
}

async fn answer(read: Result<serde_json::Value, sqlx::Error>) -> Response {
    match read {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            tracing::warn!(%error, "internal ledger read failed");
            Refusal::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal read failed",
                "read_failed",
            )
            .into_response()
        }
    }
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
pub async fn list_inference_calls(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallListQuery>,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    let limit = match check_limit(query.limit) {
        Ok(limit) => limit,
        Err(refusal) => return refusal.into_response(),
    };
    let since = match parse_timestamp("since", query.since.as_deref()) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    let until = match parse_timestamp("until", query.until.as_deref()) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let filters = reads::CallFilters {
        correlation_id: query.correlation_id,
        caller: query.caller,
        since,
        until,
        limit,
    };
    answer(reads::calls(pool, &filters).await).await
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
pub async fn pinning_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PinningSummaryQuery>,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_source(&query.source) {
        return refusal.into_response();
    }
    let since = match parse_timestamp("since", query.since.as_deref()) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let filters = reads::PinningFilters {
        since,
        source: query.source,
    };
    answer(reads::pinning_summary(pool, &filters).await).await
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
pub async fn repeat_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RepeatGroupsQuery>,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    if let Err(refusal) = check_source(&query.source) {
        return refusal.into_response();
    }
    let limit = match check_limit(query.limit) {
        Ok(limit) => limit,
        Err(refusal) => return refusal.into_response(),
    };
    let since = match parse_timestamp("since", query.since.as_deref()) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let filters = reads::RepeatFilters {
        since,
        temperature: query.temperature,
        caller: query.caller,
        served_provider: query.served_provider,
        source: query.source,
        seeded: query.seeded,
        limit,
    };
    answer(reads::repeat_groups(pool, &filters).await).await
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
pub async fn calls_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SummaryQuery>,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    let days = match check_days(query.days) {
        Ok(days) => days,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    answer(reads::summary(pool, days).await).await
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
pub async fn capture_call(
    State(state): State<AppState>,
    Path(call_id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    // The same allowlist that decides whether a body may be STORED decides whether it may be
    // READ. Caller-asserted like every identity header here; empty (the live state) refuses all.
    let app_id = headers
        .get("x-app-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim();
    let allowed = state
        .config()
        .inference_capture_allowed_callers
        .iter()
        .any(|caller| caller.trim() == app_id);
    if app_id.is_empty() || !allowed {
        return Refusal::detail(
            StatusCode::FORBIDDEN,
            "captured bodies are readable only by INFERENCE_CAPTURE_ALLOWED_CALLERS",
            "capture_forbidden",
        )
        .into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    match reads::capture(pool, call_id).await {
        Ok(Some(value)) => Json(value).into_response(),
        Ok(None) => {
            Refusal::detail(StatusCode::NOT_FOUND, "not captured", "not_captured").into_response()
        }
        Err(error) => {
            tracing::warn!(%error, "internal ledger read failed");
            Refusal::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal read failed",
                "read_failed",
            )
            .into_response()
        }
    }
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
pub async fn served_models_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TimelineQuery>,
) -> Response {
    if let Err(refusal) = guard(&state, &headers) {
        return refusal.into_response();
    }
    let days = match check_days(query.days) {
        Ok(days) => days,
        Err(refusal) => return refusal.into_response(),
    };
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    answer(reads::timeline(pool, days).await).await
}
