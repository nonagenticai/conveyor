//! The HTTP surface: the document and the application.
//!
//! `ROUTES` is the registry the server actually mounts and `ApiDoc` is what utoipa documents;
//! `--check-lock` requires the two to agree with the capture in all three directions, so a route
//! cannot exist in one and not the other.

use axum::Json;
use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::state::Shared;

pub mod cache;
pub mod formal;
pub mod health;
pub mod pipeline;
pub mod stats;
pub mod verify;

/// A refusal whose body is the shape the old FastAPI service answered with.
pub type Failure = (StatusCode, Json<Value>);

/// The caller's tenant, from the one header that carries it.
///
/// Fail-closed on purpose: this service has no configured default tenant, and a sentinel identity
/// is exactly the thing that makes a cross-tenant mistake invisible.
pub fn caller_tenant(headers: &HeaderMap) -> Result<String, Failure> {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": "X-Tenant-Id is required"})),
            )
        })
}

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
    ("POST", "/api/verify/ragas"),
    ("POST", "/api/formal/translate"),
    ("POST", "/api/formal/verify"),
    ("GET", "/api/formal/proofs/{claim_id}"),
    ("POST", "/api/formal/proofs/{claim_id}"),
    ("GET", "/api/pipeline/status"),
    ("GET", "/api/pipeline/history/{entity_id}"),
    ("GET", "/api/pipeline/anomalies"),
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
        verify::ragas,
        formal::translate,
        formal::verify,
        formal::get_proofs,
        formal::create_proof,
        pipeline::status,
        pipeline::history,
        pipeline::anomalies,
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
        verify::RagasRequest,
        verify::RagasResponse,
        formal::FormalTranslateRequest,
        formal::FormalTranslateResponse,
        formal::FormalVerifyRequest,
        formal::FormalVerifyResponse,
        formal::ProofStoreRequest,
        formal::ProofStored,
        pipeline::PipelineStatusResponse,
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
        .route("/api/verify/ragas", post(verify::ragas))
        .route("/api/formal/translate", post(formal::translate))
        .route("/api/formal/verify", post(formal::verify))
        .route(
            "/api/formal/proofs/{claim_id}",
            get(formal::get_proofs).post(formal::create_proof),
        )
        .route("/api/pipeline/status", get(pipeline::status))
        .route("/api/pipeline/history/{entity_id}", get(pipeline::history))
        .route("/api/pipeline/anomalies", get(pipeline::anomalies))
        .with_state(state)
}
