//! `/api/replay/*` — the replay invocation log.
//!
//! A replay row is written by a `X-Replay-Mode: true` inference; the runner backfills the
//! validation outcome later. Live rows are empty (measured 2026-11-13), so this slice is CRUD
//! parity.
//!
//! The reference's `require_caller` gate is ported here for the whole replay/canary slice: the
//! tenant is the caller-asserted `X-Tenant-Id`, absent (or the nil sentinel) is a 401, and
//! anything that is not a well-formed uuid is a 400. `sanitise_tenant`'s credential substitution
//! is not reproduced because it changes only which counter/log line the refusal produces — a
//! credential in the tenant field is not a uuid either way, so it lands on the same 400 body.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::contract::schemas::{
    HTTPValidationError, InvocationLogListResponse, InvocationLogRow, PatchInvocationLogRequest,
    PatchInvocationLogResponse,
};
use crate::routing::Refusal;
use crate::state::AppState;

/// The reference's `UNATTRIBUTED_TENANT_ID`: the sentinel a caller that never passed
/// license-authz arrives with. It is treated as "no tenant", never as a tenant.
const UNATTRIBUTED_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

/// The live 401 detail, verbatim.
const MISSING_TENANT: &str = "Missing or unresolved X-Tenant-Id; caller is not authenticated";

/// The reference's empty-body detail, verbatim.
const EMPTY_PATCH: &str = "At least one of validation_passed, judge_score, or expected must be set";

const MAX_REPLAY_LIMIT: u32 = 200;

fn default_replay_limit() -> u32 {
    50
}

/// FastAPI's 422 envelope: `{"detail": [...validation rows...]}`. The names are FastAPI's because
/// clients already parse them.
pub(crate) fn validation_error(entry: serde_json::Value) -> Refusal {
    Refusal {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        body: serde_json::json!({"detail": [entry]}),
        headers: Vec::new(),
    }
}

/// The reference's strict `require_caller`: a resolving `X-Tenant-Id` or a refusal.
///
/// Shared by the replay PATCHes and the canary writes (their `require_caller` dependency).
/// `caller_identity`'s permissive fallback is deliberately NOT used here — an unauthenticated
/// caller must not be able to mutate a tenant's rows. The resolved tenant is the same value the
/// reference binds into `WHERE tenant_id = :tenant`, and it always comes from the header, never
/// from a query parameter.
pub(crate) fn require_caller(headers: &HeaderMap) -> Result<uuid::Uuid, Refusal> {
    // No trim: the reference's order is absent-or-sentinel (401) THEN uuid (400), and a padded
    // uuid is not a uuid to it. Trimming here would silently accept one.
    let raw = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if raw.is_empty() || raw == UNATTRIBUTED_TENANT_ID {
        return Err(Refusal::detail(
            StatusCode::UNAUTHORIZED,
            MISSING_TENANT,
            "unauthenticated",
        ));
    }
    uuid::Uuid::parse_str(raw).map_err(|_| {
        // Present-but-malformed is a bad REQUEST (400), never a 404 and never a 500 from the
        // uuid column. A credential-shaped tenant lands here too: it is not a uuid.
        Refusal::detail(
            StatusCode::BAD_REQUEST,
            "Malformed X-Tenant-Id; expected a uuid",
            "malformed_tenant",
        )
    })
}

/// The pool every handler in this slice needs; `None` is the reference envelope's 503.
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
    tracing::warn!(%error, "replay invocation-log operation failed");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, what, "database_error")
}

fn json_body(
    entry: Result<Json<PatchInvocationLogRequest>, JsonRejection>,
) -> Result<PatchInvocationLogRequest, Refusal> {
    entry.map(|Json(body)| body).map_err(|rejection| {
        // FastAPI answers a malformed body with its 422 envelope; axum's native rejection is a
        // 400/415, so the mapping is explicit here.
        validation_error(serde_json::json!({
            "type": "json_invalid",
            "loc": ["body"],
            "msg": rejection.body_text(),
            "input": serde_json::Value::Null
        }))
    })
}

/// The reference's `_build_update_sets` guards: nothing to update is a caller bug (400), and a
/// judge score outside [0.0, 1.0] is pydantic's 422.
fn validate_patch(body: &PatchInvocationLogRequest) -> Result<(), Refusal> {
    if body.validation_passed.is_none() && body.judge_score.is_none() && body.expected.is_null() {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            EMPTY_PATCH,
            "empty_patch",
        ));
    }
    if let Some(score) = body.judge_score
        && !(0.0..=1.0).contains(&score)
    {
        let (kind, msg, ctx) = if score < 0.0 {
            (
                "greater_than_equal",
                "Input should be greater than or equal to 0",
                serde_json::json!({"ge": 0.0}),
            )
        } else {
            (
                "less_than_equal",
                "Input should be less than or equal to 1",
                serde_json::json!({"le": 1.0}),
            )
        };
        return Err(validation_error(serde_json::json!({
            "type": kind,
            "loc": ["body", "judge_score"],
            "msg": msg,
            "input": score,
            "ctx": ctx
        })));
    }
    Ok(())
}

/// The replay row and the two keys it can be addressed by.
enum PatchKey {
    Id(uuid::Uuid),
    Correlation(uuid::Uuid),
}

const PATCH_BY_ID: &str = "UPDATE replay_invocation_log \
     SET validation_passed = COALESCE($1, validation_passed), \
         judge_score = COALESCE($2::float8, judge_score::float8)::numeric, \
         expected = COALESCE($3, expected) \
     WHERE id = $4 AND tenant_id = $5 \
     RETURNING row_to_json(replay_invocation_log)";

const PATCH_BY_CORRELATION: &str = "UPDATE replay_invocation_log \
     SET validation_passed = COALESCE($1, validation_passed), \
         judge_score = COALESCE($2::float8, judge_score::float8)::numeric, \
         expected = COALESCE($3, expected) \
     WHERE correlation_id = $4 AND tenant_id = $5 \
     RETURNING row_to_json(replay_invocation_log)";

/// Apply a non-empty PATCH, scoped to the caller's tenant. Another tenant's row is a 404, not a
/// 403, so the response cannot confirm that a given id exists.
async fn apply_patch(
    pool: &sqlx::PgPool,
    tenant: uuid::Uuid,
    body: &PatchInvocationLogRequest,
    key: PatchKey,
) -> Result<PatchInvocationLogResponse, Refusal> {
    let expected = (!body.expected.is_null()).then_some(&body.expected);
    let row: Option<serde_json::Value> = match key {
        PatchKey::Id(id) => {
            sqlx::query_scalar(PATCH_BY_ID)
                .bind(body.validation_passed)
                .bind(body.judge_score)
                .bind(expected)
                .bind(id)
                .bind(tenant)
                .fetch_optional(pool)
                .await
        }
        PatchKey::Correlation(correlation_id) => {
            sqlx::query_scalar(PATCH_BY_CORRELATION)
                .bind(body.validation_passed)
                .bind(body.judge_score)
                .bind(expected)
                .bind(correlation_id)
                .bind(tenant)
                .fetch_optional(pool)
                .await
        }
    }
    .map_err(|error| failed(error, "replay invocation-log update failed"))?;
    let Some(row) = row else {
        let message = match key {
            PatchKey::Id(id) => format!("replay_invocation_log row {id} not found"),
            PatchKey::Correlation(correlation_id) => format!(
                "no replay_invocation_log row for correlation_id {correlation_id} \
                 (broken replay correlation chain)"
            ),
        };
        return Err(Refusal::detail(StatusCode::NOT_FOUND, message, "not_found"));
    };
    serde_json::from_value(row).map_err(|error| {
        tracing::warn!(%error, "replay invocation-log row did not decode");
        Refusal::detail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "replay invocation-log row did not decode",
            "decode_error",
        )
    })
}

fn path_uuid(field: &str, raw: &str) -> Result<uuid::Uuid, Refusal> {
    uuid::Uuid::parse_str(raw).map_err(|_| {
        validation_error(serde_json::json!({
            "type": "uuid_parsing",
            "loc": ["path", field],
            "msg": "Input should be a valid UUID",
            "input": raw
        }))
    })
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
pub async fn list_invocation_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReplayLogQuery>,
) -> Response {
    let tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    if !(1..=MAX_REPLAY_LIMIT).contains(&query.limit) {
        let (kind, msg) = if query.limit < 1 {
            (
                "greater_than_equal",
                "Input should be greater than or equal to 1",
            )
        } else {
            (
                "less_than_equal",
                "Input should be less than or equal to 200",
            )
        };
        return validation_error(serde_json::json!({
            "type": kind,
            "loc": ["query", "limit"],
            "msg": msg,
            "input": query.limit,
        }))
        .into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };

    // Tenant scope is never caller-supplied: it always comes from the identity header, so a
    // caller can only ever read its own rows. The predicate order matches the reference's.
    let raw: Result<Vec<serde_json::Value>, sqlx::Error> = sqlx::query_scalar(
        "SELECT row_to_json(t) FROM ( \
             SELECT id, correlation_id, worker_name, version, started_at, latency_ms, \
                    validation_passed, judge_score, input, expected, actual \
             FROM replay_invocation_log \
             WHERE tenant_id = $1 \
               AND ($2::text IS NULL OR worker_name = $2) \
               AND ($3::text IS NULL OR version = $3) \
               AND (NOT $4::boolean OR validation_passed IS FALSE) \
             ORDER BY started_at DESC LIMIT $5 \
         ) t",
    )
    .bind(tenant)
    .bind(query.worker_name.as_deref())
    .bind(query.version.as_deref())
    .bind(query.only_failed)
    .bind(i64::from(query.limit))
    .fetch_all(pool)
    .await;

    let raw = match raw {
        Ok(rows) => rows,
        Err(error) => {
            return failed(error, "replay invocation-log read failed").into_response();
        }
    };
    let rows = match raw
        .into_iter()
        .map(serde_json::from_value::<InvocationLogRow>)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "replay invocation-log rows did not decode");
            return Refusal::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay invocation-log rows did not decode",
                "decode_error",
            )
            .into_response();
        }
    };
    Json(InvocationLogListResponse {
        count: rows.len() as i64,
        rows,
    })
    .into_response()
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
pub async fn patch_invocation_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(log_id): Path<String>,
    payload: Result<Json<PatchInvocationLogRequest>, JsonRejection>,
) -> Response {
    let tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let id = match path_uuid("log_id", &log_id) {
        Ok(id) => id,
        Err(refusal) => return refusal.into_response(),
    };
    let body = match json_body(payload) {
        Ok(body) => body,
        Err(refusal) => return refusal.into_response(),
    };
    if let Err(refusal) = validate_patch(&body) {
        return refusal.into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    match apply_patch(pool, tenant, &body, PatchKey::Id(id)).await {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
    }
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
pub async fn patch_invocation_log_by_correlation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(correlation_id): Path<String>,
    payload: Result<Json<PatchInvocationLogRequest>, JsonRejection>,
) -> Response {
    let tenant = match require_caller(&headers) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };
    let correlation_id = match path_uuid("correlation_id", &correlation_id) {
        Ok(id) => id,
        Err(refusal) => return refusal.into_response(),
    };
    let body = match json_body(payload) {
        Ok(body) => body,
        Err(refusal) => return refusal.into_response(),
    };
    if let Err(refusal) = validate_patch(&body) {
        return refusal.into_response();
    }
    let pool = match pool(&state) {
        Ok(pool) => pool,
        Err(refusal) => return refusal.into_response(),
    };
    match apply_patch(pool, tenant, &body, PatchKey::Correlation(correlation_id)).await {
        Ok(response) => Json(response).into_response(),
        Err(refusal) => refusal.into_response(),
    }
}
