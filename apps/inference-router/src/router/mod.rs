//! The HTTP surface.
//!
//! `ROUTES` is the single declaration of what this service exposes: `router()` builds axum from
//! it and `--check-lock` compares it to the captured contract. A route that exists in one and not
//! the other is a failure, so a handler cannot be added or dropped silently.

pub mod health;

use axum::Router;
use axum::routing::get;

/// `(METHOD, path)` for every operation this service registers.
///
/// The migration slices grow this in lockstep with their handlers; `--check-lock` reports the
/// difference in both directions until the set equals the captured contract.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "inference-router",
        description = "OpenAI-compatible inference proxy with intelligent routing and failover \
                       across configurable provider backends.",
        version = env!("CARGO_PKG_VERSION")
    ),
    paths(health::root, health::health, health::liveness, health::readiness),
    components(schemas(
        health::HealthResponse,
        health::LivenessResponse,
        health::ReadinessResponse
    ))
)]
struct ApiDoc;

/// The document the service publishes (and `--check-lock` compares to the capture).
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

/// The HTTP application the binary serves.
pub fn router() -> Router {
    Router::new()
        .route("/", get(health::root))
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
}
