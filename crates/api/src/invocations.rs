#![forbid(unsafe_code)]
//! `GET /api/invocations/{id}` — the poll half of the async invocation surface (#579).
//!
//! The submit half is `POST /api/actions/<name>/invocations` with `Prefer: respond-async`; it
//! answers `202` and this route answers what the synchronous call would have. The shape is the
//! estate's, not this file's: w2v answers `202` for an image ("202, not 200: an image is minutes
//! of GPU work") and polls by job id, agent for eval runs, mcp for runs, coding for jobs.
//!
//! Three answers, no others:
//! - `202` while the invocation is running, carrying only the id and the status;
//! - the settled status and body once it is `done` or `failed` — the SAME body the synchronous
//!   call would have returned, so a caller changes how it waits, not what it reads;
//! - `404` for an id that does not exist **in the caller's tenant**. The two cases are not
//!   distinguished on purpose: "another tenant's invocation" must not be observable.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

/// `GET /api/invocations/{id}`.
pub async fn get(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Response {
    let invocation =
        match conveyor_store::async_invocation::read(&state.pool, caller.tenant, id).await {
            Ok(Some(invocation)) => invocation,
            Ok(None) => {
                return ApiError::new(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    "no invocation with that id",
                )
                .into_response();
            }
            Err(error) => {
                tracing::error!(%error, %id, "read an async invocation");
                return ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "the invocation could not be read",
                )
                .into_response();
            }
        };

    if invocation.status == "running" {
        return (
            StatusCode::ACCEPTED,
            [(header::LOCATION, format!("/api/invocations/{id}"))],
            Json(serde_json::json!({
                "invocation_id": id,
                "status": "running",
            })),
        )
            .into_response();
    }

    // Settled. `finish` refuses to write one without the other, so the fallback below is
    // unreachable rather than tolerated: a row that somehow lacks its answer is a defect, and
    // it must not read as a success.
    let status = invocation
        .response_status
        .and_then(|code| u16::try_from(code).ok())
        .and_then(|code| StatusCode::from_u16(code).ok())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = invocation.response_body.unwrap_or_else(
        || serde_json::json!({"detail": "the invocation has no recorded response"}),
    );
    (status, Json(body)).into_response()
}
