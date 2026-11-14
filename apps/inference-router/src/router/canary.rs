//! `/api/canary/*` — the prompt-canary controller.
//!
//! These routes move a worker's version pointer; the actual `system_prompt` swap lives in
//! worker-management. Live canary rows are empty (measured 2026-11-13), so this slice is CRUD
//! parity, not traffic parity.
//!
//! The posture split is the reference's: the three writes gate on `require_caller` (absent tenant
//! is a 401), while the state read uses the permissive `caller_identity` and never fails on a
//! header gap. The table carries no tenant column, so the identity is not a query scope here.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

use crate::contract::schemas::{
    ConfigurePromptCanaryRequest, ConfigurePromptCanaryResponse, HTTPValidationError,
    PromotePromptRequest, PromotePromptResponse, PromptCanaryState, RollbackPromptRequest,
    RollbackPromptResponse,
};
use crate::router::replay::{require_caller, validation_error};
use crate::routing::Refusal;
use crate::state::AppState;

/// The keys the autonomous rollback monitor enforces, and therefore the only keys a configure may
/// carry (the reference derives this from `canary.monitor.ROLLBACK_THRESHOLD_KEYS`). Units:
/// `failure_rate` is a 0-1 fraction, `p95_latency_ms` milliseconds, `cost_usd` US dollars.
const ALLOWED_THRESHOLD_KEYS: [&str; 3] = ["failure_rate", "p95_latency_ms", "cost_usd"];

fn pool(state: &AppState) -> Result<&sqlx::PgPool, Refusal> {
    state.deps().ledger.as_ref().ok_or_else(|| {
        Refusal::detail(
            StatusCode::SERVICE_UNAVAILABLE,
            "ledger database is not configured",
            "ledger_unavailable",
        )
    })
}

fn failed(error: sqlx::Error, what: &'static str) -> Refusal {
    tracing::warn!(%error, "prompt-canary operation failed");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, what, "database_error")
}

fn decode_error(what: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, what, "decode_error")
}

fn body_rejection(rejection: &JsonRejection) -> Refusal {
    validation_error(serde_json::json!({
        "type": "json_invalid",
        "loc": ["body"],
        "msg": rejection.body_text(),
        "input": serde_json::Value::Null
    }))
}

/// The reference's `_WORKER_NAME_RE`: alphanumerics, underscores and hyphens only.
fn validate_worker_name(worker_name: &str) -> Result<(), Refusal> {
    let valid = !worker_name.is_empty()
        && worker_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "worker_name must contain only alphanumerics, underscores, or hyphens",
            "invalid_worker_name",
        ))
    }
}

/// Pydantic's `min_length=1` / `max_length=255` on every body's `worker_name` (422, not 400).
fn validate_worker_name_length(worker_name: &str) -> Result<(), Refusal> {
    let length = worker_name.chars().count();
    if length < 1 {
        return Err(validation_error(serde_json::json!({
            "type": "string_too_short",
            "loc": ["body", "worker_name"],
            "msg": "String should have at least 1 character",
            "input": worker_name,
            "ctx": {"min_length": 1}
        })));
    }
    if length > 255 {
        return Err(validation_error(serde_json::json!({
            "type": "string_too_long",
            "loc": ["body", "worker_name"],
            "msg": "String should have at most 255 characters",
            "input": worker_name,
            "ctx": {"max_length": 255}
        })));
    }
    Ok(())
}

/// Python's `repr()` for a worker name: single quotes unless the value itself has a single quote
/// and no double quote. Only the 404 details render a caller-supplied name this way.
fn py_repr(value: &str) -> String {
    if value.contains('\'') && !value.contains('"') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

fn no_row(worker_name: &str) -> Refusal {
    Refusal::detail(
        StatusCode::NOT_FOUND,
        format!("No canary routing row for worker {}", py_repr(worker_name)),
        "not_found",
    )
}

fn integer_min(field: &str, value: i64, minimum: i64) -> Result<(), Refusal> {
    if value < minimum {
        return Err(validation_error(serde_json::json!({
            "type": "greater_than_equal",
            "loc": ["body", field],
            "msg": format!("Input should be greater than or equal to {minimum}"),
            "input": value,
            "ctx": {"ge": minimum}
        })));
    }
    Ok(())
}

fn integer_range(field: &str, value: i64, minimum: i64, maximum: i64) -> Result<(), Refusal> {
    integer_min(field, value, minimum)?;
    if value > maximum {
        return Err(validation_error(serde_json::json!({
            "type": "less_than_equal",
            "loc": ["body", field],
            "msg": format!("Input should be less than or equal to {maximum}"),
            "input": value,
            "ctx": {"le": maximum}
        })));
    }
    Ok(())
}

fn datetime_rejection(field: &str, raw: &str) -> Refusal {
    validation_error(serde_json::json!({
        "type": "datetime_parsing",
        "loc": ["body", field],
        "msg": "Input should be a valid datetime",
        "input": raw
    }))
}

/// Parse the reference's datetime inputs (RFC 3339, or a naive ISO string treated as UTC).
fn parse_datetime(field: &str, raw: &str) -> Result<DateTime<Utc>, Refusal> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Ok(parsed.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(Utc.from_utc_datetime(&naive));
    }
    Err(datetime_rejection(field, raw))
}

const PROMOTE: &str = "UPDATE prompt_canary_routing \
     SET active_version = COALESCE(canary_version, active_version), \
         canary_version = NULL, \
         canary_percent = 0, \
         canary_started_at = NULL, \
         canary_ends_at = NULL \
     WHERE worker_name = $1 \
     RETURNING active_version";

const ROLLBACK: &str = "UPDATE prompt_canary_routing \
     SET canary_version = NULL, \
         canary_percent = 0, \
         canary_started_at = NULL, \
         canary_ends_at = NULL \
     WHERE worker_name = $1 \
     RETURNING active_version";

const CONFIGURE: &str = "INSERT INTO prompt_canary_routing \
     (worker_name, active_version, canary_version, canary_percent, \
      rollback_thresholds, canary_started_at, canary_ends_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7) \
     ON CONFLICT (worker_name) DO UPDATE SET \
         active_version = EXCLUDED.active_version, \
         canary_version = EXCLUDED.canary_version, \
         canary_percent = EXCLUDED.canary_percent, \
         rollback_thresholds = EXCLUDED.rollback_thresholds, \
         canary_started_at = EXCLUDED.canary_started_at, \
         canary_ends_at = EXCLUDED.canary_ends_at \
     RETURNING row_to_json(prompt_canary_routing)";

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
pub async fn promote_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<PromotePromptRequest>, JsonRejection>,
) -> Response {
    let _tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let body = match payload {
        Ok(Json(body)) => body,
        Err(rejection) => return body_rejection(&rejection).into_response(),
    };
    if let Err(refusal) = validate_worker_name_length(&body.worker_name) {
        return refusal.into_response();
    }
    // The reference does NOT shape-check promote's worker_name (no `_validate_worker_name` call).
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let active: Result<Option<i32>, sqlx::Error> = sqlx::query_scalar(PROMOTE)
        .bind(&body.worker_name)
        .fetch_optional(pool)
        .await;
    match active {
        Ok(Some(active_version)) => Json(PromotePromptResponse {
            worker_name: body.worker_name,
            active_version: i64::from(active_version),
        })
        .into_response(),
        Ok(None) => no_row(&body.worker_name).into_response(),
        Err(error) => failed(error, "prompt canary promote failed").into_response(),
    }
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
pub async fn configure_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ConfigurePromptCanaryRequest>, JsonRejection>,
) -> Response {
    let _tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let body = match payload {
        Ok(Json(body)) => body,
        Err(rejection) => return body_rejection(&rejection).into_response(),
    };

    // Field-level rules first (pydantic's 422 envelope), then the handler's cross-field 400s.
    if let Err(refusal) = validate_worker_name_length(&body.worker_name)
        .and_then(|()| integer_min("active_version", body.active_version, 1))
        .and_then(|()| integer_min("canary_version", body.canary_version, 1))
        .and_then(|()| integer_range("canary_percent", body.canary_percent, 0, 100))
    {
        return refusal.into_response();
    }
    if let Err(refusal) = validate_worker_name(&body.worker_name) {
        return refusal.into_response();
    }
    let started_at = match parse_datetime("canary_started_at", &body.canary_started_at) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    let ends_at = match parse_datetime("canary_ends_at", &body.canary_ends_at) {
        Ok(value) => value,
        Err(refusal) => return refusal.into_response(),
    };
    if body.canary_version == body.active_version {
        return Refusal::detail(
            StatusCode::BAD_REQUEST,
            "canary_version must differ from active_version",
            "same_version",
        )
        .into_response();
    }
    if ends_at <= started_at {
        return Refusal::detail(
            StatusCode::BAD_REQUEST,
            "canary_ends_at must be strictly after canary_started_at",
            "invalid_window",
        )
        .into_response();
    }

    // Allow-listed threshold keys, with the units the monitor enforces spelled out. Extras are
    // rejected so a typo cannot silently persist a threshold the monitor never reads.
    let unknown: Vec<&str> = body
        .rollback_thresholds
        .keys()
        .map(String::as_str)
        .filter(|key| !ALLOWED_THRESHOLD_KEYS.contains(key))
        .collect();
    if !unknown.is_empty() {
        let mut sorted = unknown;
        sorted.sort_unstable();
        return Refusal::detail(
            StatusCode::BAD_REQUEST,
            format!(
                "rollback_thresholds keys must be a subset of \
                 ['cost_usd', 'failure_rate', 'p95_latency_ms']; got unknown: {sorted:?}. \
                 Units: failure_rate is a 0-1 fraction (0.05 == 5%), p95_latency_ms is \
                 milliseconds, cost_usd is US dollars."
            ),
            "unknown_threshold",
        )
        .into_response();
    }
    for (key, value) in &body.rollback_thresholds {
        if !value.is_finite() {
            return Refusal::detail(
                StatusCode::BAD_REQUEST,
                format!("rollback_thresholds[{key:?}] must be finite"),
                "non_finite_threshold",
            )
            .into_response();
        }
    }

    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let thresholds = serde_json::to_value(&body.rollback_thresholds)
        .expect("a BTreeMap<String, f64> is always JSON-serialisable");
    let row: Result<Option<serde_json::Value>, sqlx::Error> = sqlx::query_scalar(CONFIGURE)
        .bind(&body.worker_name)
        .bind(body.active_version)
        .bind(body.canary_version)
        .bind(body.canary_percent)
        .bind(&thresholds)
        .bind(started_at)
        .bind(ends_at)
        .fetch_optional(pool)
        .await;
    match row {
        Ok(Some(row)) => match serde_json::from_value::<ConfigurePromptCanaryResponse>(row) {
            Ok(response) => Json(response).into_response(),
            Err(error) => {
                tracing::warn!(%error, "prompt canary row did not decode");
                decode_error("prompt canary row did not decode").into_response()
            }
        },
        Ok(None) => decode_error("prompt canary upsert returned no row").into_response(),
        Err(error) => failed(error, "prompt canary configure failed").into_response(),
    }
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
pub async fn get_prompt_canary(
    State(state): State<AppState>,
    Path(worker_name): Path<String>,
) -> Response {
    // Permissive, exactly as the reference's `caller_identity`: no header gap is a refusal, and
    // the table has no tenant column for the identity to scope.
    if let Err(refusal) = validate_worker_name(&worker_name) {
        return refusal.into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let row: Result<Option<serde_json::Value>, sqlx::Error> = sqlx::query_scalar(
        "SELECT row_to_json(t) FROM ( \
             SELECT worker_name, active_version, canary_version, canary_percent, \
                    rollback_thresholds, canary_started_at, canary_ends_at \
             FROM prompt_canary_routing WHERE worker_name = $1 \
         ) t",
    )
    .bind(&worker_name)
    .fetch_optional(pool)
    .await;
    match row {
        Ok(Some(row)) => match serde_json::from_value::<PromptCanaryState>(row) {
            Ok(response) => Json(response).into_response(),
            Err(error) => {
                tracing::warn!(%error, "prompt canary row did not decode");
                decode_error("prompt canary row did not decode").into_response()
            }
        },
        Ok(None) => no_row(&worker_name).into_response(),
        Err(error) => failed(error, "prompt canary read failed").into_response(),
    }
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
pub async fn rollback_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<RollbackPromptRequest>, JsonRejection>,
) -> Response {
    let _tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let body = match payload {
        Ok(Json(body)) => body,
        Err(rejection) => return body_rejection(&rejection).into_response(),
    };
    if let Err(refusal) = validate_worker_name_length(&body.worker_name)
        .and_then(|()| validate_worker_name(&body.worker_name))
    {
        return refusal.into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    let active: Result<Option<i32>, sqlx::Error> = sqlx::query_scalar(ROLLBACK)
        .bind(&body.worker_name)
        .fetch_optional(pool)
        .await;
    match active {
        // Idempotent: the surviving `active_version` is the known-good one; the canary pointer is
        // cleared, never promoted.
        Ok(Some(active_version)) => Json(RollbackPromptResponse {
            worker_name: body.worker_name,
            active_version: i64::from(active_version),
        })
        .into_response(),
        Ok(None) => no_row(&body.worker_name).into_response(),
        Err(error) => failed(error, "prompt canary rollback failed").into_response(),
    }
}
