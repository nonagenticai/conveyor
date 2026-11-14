//! The health surface the captured contract fixes: `/health`, `/health/live`, `/health/ready`.
//!
//! Shapes are the capture's, not invented here: `HealthResponse` has six required string fields
//! (`status`, `service`, `version`, `environment`, `timestamp`, `database`), `LivenessResponse` a
//! required `status`, `ReadinessResponse` a required `status` and `database`, and the 503 body is
//! `ErrorResponse` with a free-form `detail`.
//!
//! `/health` never touches the database and reports `database: "not_checked"`. That is a security
//! decision in the Python service, not an omission: the route is public and unauthenticated, so a
//! query there would be a connection-exhaustion amplifier. `/health/ready` is the real check and is
//! cluster-only.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::SERVICE_NAME;
use crate::state::AppState;

/// `GET /health` — six required strings; `database` is the literal `not_checked`.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub environment: String,
    pub timestamp: String,
    pub database: String,
}

/// `GET /health/live` — `{"status":"alive"}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct LivenessResponse {
    pub status: String,
}

/// `GET /health/ready` — `{"status":"ready","database":"connected"}` or a 503.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: String,
}

/// The captured error envelope: `detail` is free-form (a string for the probes, an object for the
/// structured refusals on the data routes).
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub detail: serde_json::Value,
}

/// `GET /health` — process liveness, no database work.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    summary = "Health Check",
    description = "Process liveness (public - no auth required). Opens no database session.",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses((status = 200, body = HealthResponse))
)]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_owned(),
        service: SERVICE_NAME.to_owned(),
        version: state.version.clone(),
        environment: state.environment.clone(),
        // Python's `datetime.now(UTC).isoformat()`: microseconds and a `+00:00` suffix.
        timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
        database: "not_checked".to_owned(),
    })
}

/// `GET /health/live` — liveness, no work at all.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    summary = "Health Live",
    description = "Kubernetes liveness probe (public - no auth required). Does no work.",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses((status = 200, body = LivenessResponse))
)]
pub async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "alive".to_owned(),
    })
}

/// `GET /health/ready` — `SELECT 1` against the service's own database.
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    summary = "Health Ready",
    description = "Kubernetes readiness probe (public - no auth required)\n\nChecks database connectivity before reporting ready.",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = ReadinessResponse),
        (status = 503, body = ErrorResponse, description = "Database not ready")
    )
)]
pub async fn readiness(
    State(state): State<AppState>,
) -> Result<Json<ReadinessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let Some(pool) = state.pool.as_ref() else {
        return Err(not_ready());
    };
    match sqlx::query("SELECT 1").execute(pool).await {
        Ok(_) => Ok(Json(ReadinessResponse {
            status: "ready".to_owned(),
            database: "connected".to_owned(),
        })),
        Err(_) => Err(not_ready()),
    }
}

fn not_ready() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            detail: serde_json::json!("Database not ready"),
        }),
    )
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn the_health_routes_serve_the_captured_shapes() {
        let app = crate::router::router(crate::router::test_state());

        let response = app
            .clone()
            .oneshot(
                Request::get("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["service"], "licensing-service");
        assert_eq!(body["version"], "0.0.0");
        assert_eq!(body["environment"], "test");
        assert_eq!(body["database"], "not_checked");
        let timestamp = body["timestamp"].as_str().expect("timestamp is a string");
        assert!(timestamp.ends_with("+00:00"), "{timestamp}");
        assert_eq!(body.as_object().expect("object").len(), 6);

        let response = app
            .clone()
            .oneshot(
                Request::get("/health/live")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        assert_eq!(
            std::str::from_utf8(&body).expect("utf8"),
            r#"{"status":"alive"}"#
        );

        // The alias the live service serves with `include_in_schema=False`.
        let response = app
            .clone()
            .oneshot(
                Request::get("/licenses/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_without_a_database_is_the_captured_503() {
        let response = crate::router::router(crate::router::test_state())
            .oneshot(
                Request::get("/health/ready")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router answers");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        assert_eq!(
            std::str::from_utf8(&body).expect("utf8"),
            r#"{"detail":"Database not ready"}"#
        );
    }
}
