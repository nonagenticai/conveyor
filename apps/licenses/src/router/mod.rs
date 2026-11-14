//! The HTTP surface.
//!
//! `ROUTES` is the single declaration of what this service exposes: `router()` builds axum from it
//! and `--check-lock` compares it to the captured contract. A route that exists in one and not the
//! other is a failure, so a handler cannot be added or dropped silently.
//!
//! The auth middleware is attached to the whole router and decides by path: public endpoints and
//! paths outside the reference's protected prefixes pass through untouched; everything else needs
//! a verified credential. Applying it here (rather than per route) is what makes "a new route is
//! protected by default" true.

pub mod health;
pub mod licenses;
pub mod schemas;
pub mod tokens;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use crate::auth::{JwtVerifier, UnavailableJwks};
use crate::config::Config;
use crate::state::AppState;

/// `(METHOD, path)` for every operation this service registers.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("GET", "/licenses/active"),
    ("POST", "/license/introspect"),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        health::health,
        health::liveness,
        health::readiness,
        licenses::active,
        tokens::introspect
    ),
    components(schemas(
        health::ErrorResponse,
        health::HealthResponse,
        health::LivenessResponse,
        health::ReadinessResponse,
        schemas::HTTPValidationError,
        schemas::ValidationError,
        tokens::TokenIntrospectRequest,
        tokens::TokenIntrospectResponse
    )),
    tags(
        (name = "health", description = "Health and readiness probes"),
        (name = "licenses", description = "Licenses and entitlements"),
        (name = "tokens", description = "API token introspection and revocation")
    )
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
        .route("/licenses/active", get(licenses::active))
        .route("/license/introspect", post(tokens::introspect))
        // The live service also answers `/licenses/health` (a gateway-prefix alias it registers
        // with `include_in_schema=False`), so it is registered here but is not in `ROUTES`.
        .route("/licenses/health", get(health::health))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::authenticate,
        ))
}

/// A state with no database and no trusted issuer, for tests that exercise routing/health.
pub fn test_state() -> AppState {
    AppState {
        pool: None,
        environment: "test".to_owned(),
        version: "0.0.0".to_owned(),
        config: Arc::new(Config {
            port: 0,
            environment: "test".to_owned(),
            version: "0.0.0".to_owned(),
            trusted_issuers: Vec::new(),
            introspect_require_auth: false,
            service_actor_id: None,
        }),
        verifier: Arc::new(JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks))),
    }
}
