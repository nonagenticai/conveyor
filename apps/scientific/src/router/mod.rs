//! The HTTP surface: the document and the application.
//!
//! `ROUTES` is the registry the server actually mounts and `ApiDoc` is what utoipa documents;
//! `--check-lock` requires the two to agree with the capture in all three directions, so a route
//! cannot exist in one and not the other.

use axum::Router;
use axum::routing::{get, post};

use crate::state::Shared;

pub mod cache;
pub mod health;
pub mod stats;

/// Every operation this service serves, in the spelling the contract uses.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("GET", "/status"),
    ("GET", "/api/cache/stats"),
    ("POST", "/api/cache/invalidate"),
    ("POST", "/api/cache/clear"),
    ("POST", "/api/stats/meta-analysis"),
    ("POST", "/api/stats/confidence-interval"),
    ("POST", "/api/stats/trend"),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    info(title = "Scientific Processor API", version = "0.1.0"),
    paths(
        health::health_check,
        health::liveness,
        health::readiness,
        health::status,
        cache::cache_stats,
        cache::cache_invalidate,
        cache::cache_clear,
        stats::meta_analysis,
        stats::confidence_interval,
        stats::trend,
    ),
    components(schemas(
        health::HealthStatus,
        health::LivenessStatus,
        health::ReadinessStatus,
        cache::CacheStatsResponse,
        cache::CacheInvalidateRequest,
        cache::CacheClearResponse,
        stats::StudyResult,
        stats::MetaAnalysisRequest,
        stats::MetaAnalysisResponse,
        stats::ConfidenceIntervalRequest,
        stats::ConfidenceIntervalResponse,
        stats::SeriesPoint,
        stats::TrendRequest,
        stats::TrendResponse,
    ))
)]
pub struct ApiDoc;

/// The document the service publishes (and `--check-lock` compares to the capture).
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

/// The HTTP application the binary serves.
pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/status", get(health::status))
        .route("/api/cache/stats", get(cache::cache_stats))
        .route("/api/cache/invalidate", post(cache::cache_invalidate))
        .route("/api/cache/clear", post(cache::cache_clear))
        .route("/api/stats/meta-analysis", post(stats::meta_analysis))
        .route(
            "/api/stats/confidence-interval",
            post(stats::confidence_interval),
        )
        .route("/api/stats/trend", post(stats::trend))
        .with_state(state)
}
