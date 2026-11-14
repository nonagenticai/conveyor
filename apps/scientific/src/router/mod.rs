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

/// A value that is safe to interpolate into an object key as one segment.
///
/// The history and proof keys are built by string interpolation (`history/{tenant}/{entity}/…`), so
/// a caller-supplied `..` or `/` would move the prefix rather than select inside it — one tenant
/// reading another's records. The old service interpolated the same way and inherited the escape;
/// this port refuses the value instead, with the shape a path parameter can produce.
pub fn safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
}

/// The caller's tenant, from the one header that carries it.
///
/// Fail-closed on purpose: this service has no configured default tenant, and a sentinel identity
/// is exactly the thing that makes a cross-tenant mistake invisible.
pub fn caller_tenant(headers: &HeaderMap) -> Result<String, Failure> {
    let tenant = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"detail": "X-Tenant-Id is required"})),
            )
        })?;
    if !safe_segment(&tenant) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"detail": "X-Tenant-Id is not a tenant identifier"})),
        ));
    }
    Ok(tenant)
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
        formal::Language,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant_headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-tenant-id", value.parse().expect("header value"));
        headers
    }

    #[test]
    fn a_segment_that_can_move_the_prefix_is_refused() {
        assert!(safe_segment("0195f48b-d000-7000-8000-000000000001"));
        assert!(safe_segment("entity_1"));
        for bad in ["", ".", "..", "../other", "a/b", "a\\b"] {
            assert!(!safe_segment(bad), "{bad:?} must not be a key segment");
        }
        assert!(!safe_segment(&"x".repeat(201)));
    }

    #[test]
    fn a_tenant_that_is_not_an_identifier_is_a_400() {
        let (status, _) = caller_tenant(&tenant_headers("../other")).expect_err("refused");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = caller_tenant(&HeaderMap::new()).expect_err("refused");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            caller_tenant(&tenant_headers("t1")).expect("accepted"),
            "t1"
        );
    }
}
