//! The health surface the captured contract fixes: `/`, `/health`, `/health/live`,
//! `/health/ready`.
//!
//! Shapes are not invented here — they are the captured document's (`HealthResponse` has one
//! optional `status`, `LivenessResponse` one optional `alive`, `ReadinessResponse` a required
//! `ready`, and the root is a free-form string map). The readiness route has no peer to gate on,
//! exactly as the live service states.

use std::collections::BTreeMap;

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::SERVICE_NAME;

/// The free-form string map `GET /` returns.
pub type RootResponse = BTreeMap<String, String>;

/// `GET /health` — `HealthResponse.status` defaults to `ok` and is not required.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(required = false, default = "ok")]
    pub status: String,
}

/// `GET /health/live` — `LivenessResponse.alive` defaults to `true` and is not required.
#[derive(Debug, Serialize, ToSchema)]
pub struct LivenessResponse {
    #[schema(required = false, default = true)]
    pub alive: bool,
}

/// `GET /health/ready` — `ReadinessResponse.ready` is required.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadinessResponse {
    pub ready: bool,
}

/// `GET /` — service metadata, every value a string.
#[utoipa::path(
    get,
    path = "/",
    tag = "health",
    summary = "Root",
    description = "Root endpoint with service metadata.",
    // Inline, as in the capture: the root schema is not a named component there either.
    responses((status = 200, body = inline(RootResponse)))
)]
pub async fn root() -> Json<RootResponse> {
    let mut body = BTreeMap::new();
    body.insert("service".to_owned(), SERVICE_NAME.to_owned());
    body.insert("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned());
    body.insert("docs".to_owned(), "/docs".to_owned());
    body.insert("health".to_owned(), "/health".to_owned());
    body.insert("readiness".to_owned(), "/health/ready".to_owned());
    Json(body)
}

/// `GET /health` — `{"status":"ok"}`.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    summary = "Health check",
    description = "Basic health check endpoint.",
    responses((status = 200, body = HealthResponse))
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
    })
}

/// `GET /health/live` — `{"alive":true}`.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "health",
    summary = "Liveness probe",
    description = "Kubernetes liveness probe. Always returns 200 if the process is alive.",
    responses((status = 200, body = LivenessResponse))
)]
pub async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse { alive: true })
}

/// `GET /health/ready` — `{"ready":true}`.
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    summary = "Readiness probe",
    description = "Readiness probe. The router has no peer dependency to gate on.",
    responses((status = 200, body = ReadinessResponse))
)]
pub async fn readiness() -> Json<ReadinessResponse> {
    Json(ReadinessResponse { ready: true })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn the_health_routes_serve_the_captured_shapes() {
        let app = crate::router::router();

        let cases = [
            ("/health", r#"{"status":"ok"}"#),
            ("/health/live", r#"{"alive":true}"#),
            ("/health/ready", r#"{"ready":true}"#),
        ];
        for (path, expected) in cases {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).expect("request"))
                .await
                .expect("router answers");
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
                .await
                .expect("body");
            assert_eq!(
                std::str::from_utf8(&body).expect("utf8"),
                expected,
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn the_root_lists_the_metadata_fields() {
        let response = crate::router::router()
            .oneshot(Request::get("/").body(Body::empty()).expect("request"))
            .await
            .expect("router answers");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(body["service"], "inference-router");
        assert_eq!(body["health"], "/health");
        assert_eq!(body["readiness"], "/health/ready");
        assert_eq!(body["docs"], "/docs");
        assert!(body["version"].is_string());
    }
}
