//! Liveness and readiness (CONTRACT §5).
//!
//! `/health` and `/health/live` answer without touching the database: a database outage
//! must not make the orchestrator restart a healthy process. `/health/ready` is the one
//! probe that checks Postgres, through the runtime pool.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

/// The health document.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// `ok` when the probe succeeded.
    pub status: String,
}

impl Health {
    fn ok() -> Self {
        Self {
            status: "ok".to_owned(),
        }
    }
}

/// Liveness: the process is serving HTTP. No database.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses((status = 200, description = "The process is up", body = Health))
)]
pub async fn health() -> Json<Health> {
    Json(Health::ok())
}

/// Liveness, explicit name. No database.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    responses((status = 200, description = "The process is up", body = Health))
)]
pub async fn live() -> Json<Health> {
    Json(Health::ok())
}

/// Readiness: one `store::ping` through the runtime pool (so it also proves the pooler path).
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses(
        (status = 200, description = "The database answered", body = Health),
        (status = 503, description = "The database did not answer", body = crate::error::ProblemDetail)
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    match conveyor_store::ping(&state.pool).await {
        Ok(_) => Ok(Json(Health::ok())),
        Err(error) => {
            tracing::warn!(error = %error, "readiness probe failed");
            Err(ApiError::service_unavailable("database is not ready"))
        }
    }
}
