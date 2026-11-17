//! `media-discovery`, the second half of `apps/research`.
//!
//! The wire contract is fixed by `contract/media/openapi.lock.json`, captured from the reference
//! service (23 operations over 20 paths, 19 schemas). This module serves that contract; the
//! operations that are not wired yet are listed by `--check-lock`, which refuses to claim a
//! complete port while any captured operation has no route.
//!
//! Three reference behaviours are reproduced deliberately, not "fixed":
//!   * `POST /api/downloads` answers `501` and creates no job.
//!   * an unknown `media_type` is not a validation error — it matches nothing.
//!   * an empty `name` is a valid collection name.

pub mod contract;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod scoring;
pub mod tenant;
pub mod transcript;

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post};
use serde_json::Value;
use sqlx::PgPool;

use error::ApiError;

/// The operations this module actually serves, in `METHOD path` spelling.
///
/// This list is the one the lock check compares, so it must never name a route that is not wired:
/// an operation added here without a handler is a claim, not a port.
pub const SERVED: &[(&str, &str)] = &[
    // health and status, outside the /api prefix
    ("GET", "/health"),
    ("GET", "/health/ready"),
    ("GET", "/health/live"),
    ("GET", "/status"),
    ("GET", "/api/search/sources"),
    // downloads — refusals the reference itself answers
    ("POST", "/api/downloads"),
    ("GET", "/api/downloads/{job_id}"),
    // items
    ("GET", "/api/items/{item_id}"),
    ("GET", "/api/items/{item_id}/related"),
    ("GET", "/api/items/{item_id}/transcript"),
    ("GET", "/api/items/{item_id}/score"),
    ("POST", "/api/items/{item_id}/score"),
    // collections
    ("GET", "/api/collections"),
    ("POST", "/api/collections"),
    ("POST", "/api/collections/import"),
    ("GET", "/api/collections/{collection_id}"),
    ("DELETE", "/api/collections/{collection_id}"),
    ("POST", "/api/collections/{collection_id}/items"),
    ("DELETE", "/api/collections/{collection_id}/items/{item_id}"),
    // scoring
    ("POST", "/api/score"),
    ("POST", "/api/score/batch"),
    ("GET", "/api/score/report/{session_id}"),
    // search
    ("POST", "/api/search"),
];

/// The document endpoints, which live outside `/api` and outside the capture.
pub const DOCS: [(&str, &str); 2] = [("GET", "/openapi.json"), ("GET", "/docs")];

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_host: String,
    pub bind_port: u16,
    pub enable_docs: bool,
    pub service_name: String,
    pub version: String,
    pub database_url: Option<String>,
    pub max_connections: u32,
    /// The licences service's introspection endpoint (`LICENSE_INTROSPECT_URL`). Unset means
    /// every credential is refused — the reference's own fail-closed behaviour.
    pub license_introspect_url: Option<String>,
    /// The service `subject_id`s permitted to vouch for a bare `X-Tenant-Id`
    /// (`MEDIA_DISCOVERY_ONBEHALF_SUBJECTS`, CSV). Empty means NO bare tenant is honoured.
    pub onbehalf_subjects: Vec<String>,
    /// Wall-clock budget for one introspection (`LICENSE_INTROSPECT_TIMEOUT`, seconds; the
    /// reference's 10 s default).
    pub license_introspect_timeout: std::time::Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_host: std::env::var("BIND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            bind_port: std::env::var("BIND_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8809),
            enable_docs: std::env::var("ENABLE_DOCS")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            service_name: "media-discovery".to_string(),
            version: "0.1.0".to_string(),
            database_url: std::env::var("DATABASE_URL").ok(),
            max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
            license_introspect_url: std::env::var("LICENSE_INTROSPECT_URL")
                .ok()
                .map(|url| url.trim().to_string())
                .filter(|url| !url.is_empty()),
            onbehalf_subjects: std::env::var("MEDIA_DISCOVERY_ONBEHALF_SUBJECTS")
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|subject| !subject.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            license_introspect_timeout: std::time::Duration::from_secs_f64(
                std::env::var("LICENSE_INTROSPECT_TIMEOUT")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .filter(|seconds: &f64| *seconds > 0.0)
                    .unwrap_or(10.0),
            ),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.bind_host, self.bind_port)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: Option<PgPool>,
    /// The client used for licence introspection. One per process, bounded by the configured
    /// timeout; the reference opened a client per call, which this does not need to reproduce.
    pub http: reqwest::Client,
}

impl AppState {
    fn http_client(config: &Config) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(config.license_introspect_timeout)
            .build()
            .expect("building the HTTP client for licence introspection")
    }

    pub fn new(config: Config) -> Self {
        let http = Self::http_client(&config);
        Self {
            config: Arc::new(config),
            pool: None,
            http,
        }
    }

    pub fn with_pool(config: Config, pool: PgPool) -> Self {
        let http = Self::http_client(&config);
        Self {
            config: Arc::new(config),
            pool: Some(pool),
            http,
        }
    }

    /// The pool, or a 500 if the app was built without one.
    ///
    /// A real boot always has one (`main` connects before serving); only a test that builds the
    /// router without a database can reach the error, and it must not silently answer an empty
    /// list where a row was expected.
    pub fn pool(&self) -> Result<&PgPool, ApiError> {
        self.pool.as_ref().ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred",
            )
        })
    }
}

/// The document this service serves: the capture, verbatim.
pub fn openapi() -> Value {
    contract::lock().unwrap_or(Value::Null)
}

pub fn build_app(state: AppState) -> Router {
    let router = Router::new()
        .route("/health", get(routes::health::health))
        .route("/health/ready", get(routes::health::health_ready))
        .route("/health/live", get(routes::health::health_live))
        .route("/status", get(routes::health::status))
        .route("/api/search/sources", get(routes::health::search_sources))
        .route("/api/downloads", post(routes::downloads::downloads_create))
        .route(
            "/api/downloads/{job_id}",
            get(routes::downloads::downloads_get),
        )
        .route("/api/items/{item_id}", get(routes::items::get_item))
        .route(
            "/api/items/{item_id}/related",
            get(routes::items::get_related),
        )
        .route(
            "/api/items/{item_id}/transcript",
            get(routes::items::get_transcript),
        )
        .route(
            "/api/items/{item_id}/score",
            get(routes::items::get_item_score).post(routes::items::score_item),
        )
        .route(
            "/api/collections",
            get(routes::collections::list).post(routes::collections::create),
        )
        .route("/api/collections/import", post(routes::collections::import))
        .route(
            "/api/collections/{collection_id}",
            get(routes::collections::get).delete(routes::collections::delete),
        )
        .route(
            "/api/collections/{collection_id}/items",
            post(routes::collections::add_items),
        )
        .route(
            "/api/collections/{collection_id}/items/{item_id}",
            delete(routes::collections::remove_item),
        )
        .route("/api/score", post(routes::scoring::score))
        .route("/api/score/batch", post(routes::scoring::score_batch))
        .route(
            "/api/score/report/{session_id}",
            get(routes::scoring::report),
        )
        .route("/api/search", post(routes::search::search));

    let router = if state.config.enable_docs {
        router
            .route("/openapi.json", get(|| async { axum::Json(openapi()) }))
            .route(
                "/docs",
                get(|| async {
                    axum::response::Html(
                        "<!doctype html><title>media-discovery</title><p>See /openapi.json.",
                    )
                }),
            )
    } else {
        router
    };

    router.with_state(state)
}
