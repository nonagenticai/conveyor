//! The HTTP surface.
//!
//! `ROUTES` is the single declaration of what this service exposes: `router()` builds axum from it
//! and `--check-lock` compares it to the captured contract. A route that exists in one and not the
//! other is a failure, so a handler cannot be added or dropped silently.
//!
//! Slice 1 declares the health surface only. The remaining 40 operations are the next slices; the
//! lock check names each one it is missing rather than passing a vacuous subset.

pub mod health;

use axum::Router;
use axum::routing::get;

use crate::state::AppState;

/// `(METHOD, path)` for every operation this service registers.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(health::health, health::liveness, health::readiness),
    components(schemas(
        health::ErrorResponse,
        health::HealthResponse,
        health::LivenessResponse,
        health::ReadinessResponse
    )),
    tags((name = "health", description = "Health and readiness probes"))
)]
pub struct ApiDoc;

/// The document the service publishes (and `--check-lock` compares to the capture).
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

/// The HTTP application the binary serves.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        // The live service also answers `/licenses/health` (a gateway-prefix alias it registers
        // with `include_in_schema=False`), so it is registered here but is not in `ROUTES`.
        .route("/licenses/health", get(health::health))
        .with_state(state)
}

/// A state with no database, for the health tests (the readiness route reports 503).
pub fn test_state() -> AppState {
    AppState {
        pool: None,
        environment: "test".to_owned(),
        version: "0.0.0".to_owned(),
    }
}
