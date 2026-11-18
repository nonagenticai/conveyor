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

use crate::evidence_review::{
    corpus, evidence, flow, imports, jobs, reviews, screening, snapshots,
};
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
    // Evidence review (#319). The colon commands are the reference's own spelling; axum's
    // matcher has no dynamic suffixes, so `rewrite_colon_commands` turns `{id}:cmd` into
    // `{id}/cmd` before routing. The contract keeps the colon path.
    ("POST", "/api/reviews"),
    ("GET", "/api/reviews"),
    ("GET", "/api/reviews/{review_id}"),
    ("PATCH", "/api/reviews/{review_id}"),
    ("GET", "/api/reviews/{review_id}/state-history"),
    ("POST", "/api/reviews/{review_id}/protocol-versions"),
    ("GET", "/api/reviews/{review_id}/protocol-versions"),
    (
        "POST",
        "/api/reviews/{review_id}/protocol-versions/{protocol_version_id}:lock",
    ),
    ("POST", "/api/reviews/{review_id}:activate"),
    ("POST", "/api/reviews/{review_id}:pause"),
    ("POST", "/api/reviews/{review_id}:resume"),
    ("POST", "/api/reviews/{review_id}:close"),
    ("POST", "/api/reviews/{review_id}:reopen"),
    ("POST", "/api/reviews/{review_id}:archive"),
    ("POST", "/api/reviews/{review_id}/archive"),
    ("GET", "/api/reviews/{review_id}/flow"),
    ("POST", "/api/reviews/{review_id}/flow:rebuild"),
    ("POST", "/api/reviews/{review_id}/flow:audit"),
    ("POST", "/api/reviews/{review_id}/exports"),
    ("GET", "/api/reviews/{review_id}/exports"),
    ("GET", "/api/reviews/{review_id}/exports/{export_id}:verify"),
    ("GET", "/api/reviews/{review_id}/exports/{export_id}"),
    ("POST", "/api/reviews/{review_id}/deduplication-runs"),
    ("GET", "/api/reviews/{review_id}/deduplication-runs"),
    ("GET", "/api/reviews/{review_id}/duplicate-clusters"),
    (
        "POST",
        "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}:resolve",
    ),
    (
        "GET",
        "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolutions",
    ),
    ("POST", "/api/reviews/{review_id}/assignments"),
    ("GET", "/api/reviews/{review_id}/screening/queue"),
    ("POST", "/api/reviews/{review_id}/decisions"),
    ("GET", "/api/reviews/{review_id}/decisions"),
    ("GET", "/api/reviews/{review_id}/conflicts"),
    (
        "POST",
        "/api/reviews/{review_id}/conflicts/{conflict_id}:unblind",
    ),
    (
        "POST",
        "/api/reviews/{review_id}/conflicts/{conflict_id}:resolve",
    ),
    ("GET", "/api/reviews/{review_id}/stages"),
    ("POST", "/api/reviews/{review_id}/imports"),
    ("GET", "/api/reviews/{review_id}/imports"),
    ("GET", "/api/reviews/{review_id}/records"),
    ("GET", "/api/reviews/{review_id}/records/{review_record_id}"),
    ("GET", "/api/reviews/{review_id}/quarantine"),
    ("POST", "/api/reviews/{review_id}/corpus-versions"),
    ("GET", "/api/reviews/{review_id}/corpus-versions"),
    (
        "GET",
        "/api/reviews/{review_id}/corpus-versions/{corpus_version_id}/members",
    ),
    ("POST", "/api/reviews/{review_id}/research-objects"),
    ("GET", "/api/reviews/{review_id}/research-objects"),
    (
        "POST",
        "/api/reviews/{review_id}/research-objects/{research_object_id}:revise",
    ),
    (
        "POST",
        "/api/reviews/{review_id}/research-objects/{research_object_id}/extractions",
    ),
    ("POST", "/api/reviews/{review_id}/assertions"),
    ("POST", "/api/reviews/{review_id}/acceptances"),
    ("GET", "/api/reviews/{review_id}/acceptances"),
    ("POST", "/api/reviews/{review_id}/evidence-snapshots"),
    ("GET", "/api/reviews/{review_id}/evidence-snapshots"),
    (
        "GET",
        "/api/reviews/{review_id}/evidence-snapshots/{snapshot_id}",
    ),
    ("GET", "/api/review-jobs/{job_id}"),
    ("POST", "/api/review-jobs/{job_id}:cancel"),
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
        reviews::create_review,
        reviews::list_reviews,
        reviews::get_review,
        reviews::patch_review,
        reviews::state_history,
        reviews::create_protocol_version,
        reviews::list_protocol_versions,
        reviews::lock_protocol_version,
        reviews::activate,
        reviews::pause,
        reviews::resume,
        reviews::close,
        reviews::reopen,
        reviews::archive,
        reviews::archive_slash,
        flow::get_flow,
        flow::rebuild_flow,
        flow::audit_flow,
        flow::create_export,
        flow::list_exports,
        flow::verify_export,
        flow::get_export,
        screening::create_deduplication_run,
        screening::list_deduplication_runs,
        screening::list_duplicate_clusters,
        screening::resolve_duplicate_cluster,
        screening::get_resolution_chain,
        screening::create_assignments,
        screening::screening_queue,
        screening::record_decision,
        screening::list_decisions,
        screening::list_conflicts,
        screening::unblind_conflict,
        screening::adjudicate_conflict,
        screening::list_stages,
        imports::create_import,
        imports::list_imports,
        imports::list_records,
        imports::get_record,
        imports::list_quarantine,
        corpus::seal_corpus,
        corpus::list_corpus_versions,
        corpus::list_corpus_members,
        evidence::create_research_object,
        evidence::list_research_objects,
        evidence::revise_research_object,
        evidence::record_extraction,
        evidence::record_assertion,
        evidence::decide_acceptance,
        evidence::list_acceptances,
        snapshots::create_evidence_snapshot,
        snapshots::list_evidence_snapshots,
        snapshots::get_evidence_snapshot,
        jobs::get_review_job,
        jobs::cancel_review_job,
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
    let app = Router::new()
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
        // Evidence review (#319). Registered on the rewritten spelling; the contract paths
        // carry the reference's `:command` suffix.
        .route(
            "/api/reviews",
            get(reviews::list_reviews).post(reviews::create_review),
        )
        .route(
            "/api/reviews/{review_id}",
            get(reviews::get_review).patch(reviews::patch_review),
        )
        .route(
            "/api/reviews/{review_id}/state-history",
            get(reviews::state_history),
        )
        .route(
            "/api/reviews/{review_id}/protocol-versions",
            get(reviews::list_protocol_versions).post(reviews::create_protocol_version),
        )
        .route(
            "/api/reviews/{review_id}/protocol-versions/{protocol_version_id}/lock",
            post(reviews::lock_protocol_version),
        )
        .route("/api/reviews/{review_id}/activate", post(reviews::activate))
        .route("/api/reviews/{review_id}/pause", post(reviews::pause))
        .route("/api/reviews/{review_id}/resume", post(reviews::resume))
        .route("/api/reviews/{review_id}/close", post(reviews::close))
        .route("/api/reviews/{review_id}/reopen", post(reviews::reopen))
        .route(
            "/api/reviews/{review_id}/archive",
            post(reviews::archive_slash),
        )
        .route("/api/reviews/{review_id}/flow", get(flow::get_flow))
        .route(
            "/api/reviews/{review_id}/flow/rebuild",
            post(flow::rebuild_flow),
        )
        .route(
            "/api/reviews/{review_id}/flow/audit",
            post(flow::audit_flow),
        )
        .route(
            "/api/reviews/{review_id}/exports",
            get(flow::list_exports).post(flow::create_export),
        )
        .route(
            "/api/reviews/{review_id}/exports/{export_id}",
            get(flow::get_export),
        )
        .route(
            "/api/reviews/{review_id}/exports/{export_id}/verify",
            get(flow::verify_export),
        )
        .route(
            "/api/reviews/{review_id}/deduplication-runs",
            get(screening::list_deduplication_runs).post(screening::create_deduplication_run),
        )
        .route(
            "/api/reviews/{review_id}/duplicate-clusters",
            get(screening::list_duplicate_clusters),
        )
        .route(
            "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolve",
            post(screening::resolve_duplicate_cluster),
        )
        .route(
            "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolutions",
            get(screening::get_resolution_chain),
        )
        .route(
            "/api/reviews/{review_id}/assignments",
            post(screening::create_assignments),
        )
        .route(
            "/api/reviews/{review_id}/screening/queue",
            get(screening::screening_queue),
        )
        .route(
            "/api/reviews/{review_id}/decisions",
            get(screening::list_decisions).post(screening::record_decision),
        )
        .route(
            "/api/reviews/{review_id}/conflicts",
            get(screening::list_conflicts),
        )
        .route(
            "/api/reviews/{review_id}/conflicts/{conflict_id}/unblind",
            post(screening::unblind_conflict),
        )
        .route(
            "/api/reviews/{review_id}/conflicts/{conflict_id}/resolve",
            post(screening::adjudicate_conflict),
        )
        .route(
            "/api/reviews/{review_id}/stages",
            get(screening::list_stages),
        )
        .route(
            "/api/reviews/{review_id}/imports",
            get(imports::list_imports).post(imports::create_import),
        )
        .route(
            "/api/reviews/{review_id}/records",
            get(imports::list_records),
        )
        .route(
            "/api/reviews/{review_id}/records/{review_record_id}",
            get(imports::get_record),
        )
        .route(
            "/api/reviews/{review_id}/quarantine",
            get(imports::list_quarantine),
        )
        .route(
            "/api/reviews/{review_id}/corpus-versions",
            get(corpus::list_corpus_versions).post(corpus::seal_corpus),
        )
        .route(
            "/api/reviews/{review_id}/corpus-versions/{corpus_version_id}/members",
            get(corpus::list_corpus_members),
        )
        .route(
            "/api/reviews/{review_id}/research-objects",
            get(evidence::list_research_objects).post(evidence::create_research_object),
        )
        .route(
            "/api/reviews/{review_id}/research-objects/{research_object_id}/revise",
            post(evidence::revise_research_object),
        )
        .route(
            "/api/reviews/{review_id}/research-objects/{research_object_id}/extractions",
            post(evidence::record_extraction),
        )
        .route(
            "/api/reviews/{review_id}/assertions",
            post(evidence::record_assertion),
        )
        .route(
            "/api/reviews/{review_id}/acceptances",
            get(evidence::list_acceptances).post(evidence::decide_acceptance),
        )
        .route(
            "/api/reviews/{review_id}/evidence-snapshots",
            get(snapshots::list_evidence_snapshots).post(snapshots::create_evidence_snapshot),
        )
        .route(
            "/api/reviews/{review_id}/evidence-snapshots/{snapshot_id}",
            get(snapshots::get_evidence_snapshot),
        )
        .route("/api/review-jobs/{job_id}", get(jobs::get_review_job))
        .route(
            "/api/review-jobs/{job_id}/cancel",
            post(jobs::cancel_review_job),
        )
        // conveyor#299: the tenant gate. It resolves the caller's tenant (licence carriers or a
        // refused plain header) and writes the bare id back into X-Tenant-Id, so every handler's
        // `caller_tenant` keeps reading what it always read.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::authenticate,
        ))
        .with_state(state);
    // The reference spells lifecycle commands inside the last path segment
    // (`/reviews/{id}:activate`). matchit has no dynamic suffixes, so the rewrite happens
    // before the app routes; the OpenAPI document and `ROUTES` keep the reference's spelling.
    let rewritten = tower::ServiceBuilder::new()
        .map_request(rewrite_colon_commands)
        .service(app);
    Router::new().fallback_service(rewritten)
}

/// `/api/reviews/{id}:activate` -> `/api/reviews/{id}/activate`, query preserved.
fn rewrite_colon_commands(mut request: axum::extract::Request) -> axum::extract::Request {
    let uri = request.uri().clone();
    let Some((prefix, last)) = uri.path().rsplit_once('/') else {
        return request;
    };
    let Some((head, command)) = last.rsplit_once(':') else {
        return request;
    };
    if head.is_empty() || command.is_empty() || command.contains(':') {
        return request;
    }
    let query = uri
        .query()
        .map(|value| format!("?{value}"))
        .unwrap_or_default();
    if let Ok(rewritten) = format!("{prefix}/{head}/{command}{query}").parse() {
        *request.uri_mut() = rewritten;
    }
    request
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
