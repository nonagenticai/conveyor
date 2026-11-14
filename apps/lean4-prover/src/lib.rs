//! lean4-prover — Lean 4 formal verification as a REST API.
//!
//! Rust port of the FastAPI service. It is a thin I/O shell: the Lean work is
//! done by `lake env lean` in a subprocess, exactly as before. Nothing here is
//! Apple-specific, which is why it runs in the cluster rather than on a mini.

pub mod admission;
pub mod auth;
pub mod config;
pub mod contract;
pub mod error;
pub mod metrics;
pub mod models;
pub mod routes;
pub mod runner;
pub mod state;

use axum::http::{HeaderName, HeaderValue, Method, header};
use tower_http::cors::{Any, CorsLayer};

use crate::state::AppState;

/// The full application: routes, the licence gate, request counting and CORS.
///
/// Layer order matters. `observe` is outermost so it counts every request,
/// including the 401s the gate produces. The licence layer sits inside it and
/// is the layer that knows the tenant, which is why the attribution line is
/// emitted there rather than in `observe`.
pub fn build_app(state: AppState) -> axum::Router {
    let mut app = routes::router(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::licence_layer,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            routes::observe,
        ));

    if !state.cfg.cors_allow_origins.is_empty() {
        app = app.layer(cors(&state.cfg.cors_allow_origins));
    }
    app
}

/// Headers a browser caller may send. Enumerated, not `Any`: see `cors`.
fn allowed_headers() -> Vec<HeaderName> {
    vec![
        header::ACCEPT,
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ORIGIN,
        HeaderName::from_static("x-tenant-id"),
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("traceparent"),
    ]
}

fn cors(origins: &[String]) -> CorsLayer {
    // ⚠️ `allow_headers(Any)` may not be combined with `allow_credentials(true)`.
    // tower-http PANICS at construction —
    //   "Invalid CORS configuration: Cannot combine
    //    `Access-Control-Allow-Credentials: true` with `Access-Control-Allow-Headers: *`"
    // — so this is a BOOT failure, not a rejected request. It is what put the
    // first cluster deploy into CrashLoopBackOff on hp2 (2026-11-13): the Python
    // service had tolerated the same combination, so the port inherited a config
    // that only the new stack rejects. The wildcard is also invalid for
    // credentialed requests per the Fetch spec; the headers are listed instead.
    // Methods are enumerated for the same reason as headers: tower-http asserts
    // `Credentials: true` cannot coexist with `Methods: *` either — the second
    // assertion the unit test below caught, after the headers one had already
    // been fixed.
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::HEAD, Method::OPTIONS])
        .allow_headers(allowed_headers());
    if origins.iter().any(|origin| origin == "*") {
        // A wildcard origin cannot be combined with credentials either; the
        // FastAPI service made the same trade.
        layer.allow_origin(Any)
    } else {
        let allowed: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|origin| origin.parse::<HeaderValue>().ok())
            .collect();
        layer.allow_origin(allowed).allow_credentials(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Regression for the 2026-11-13 hp2 CrashLoopBackOff. `allow_headers(Any)`
    /// with `allow_credentials(true)` makes tower-http panic while the layer is
    /// CONSTRUCTED, so the failure mode is "the process never serves a single
    /// request", not a malformed response. Constructing the layer is therefore
    /// the assertion that matters; the preflight then checks the header list is
    /// still usable by the browser callers.
    #[tokio::test]
    async fn credentialed_cors_allows_the_tenant_header() {
        let app = axum::Router::new()
            .route("/api/prove", axum::routing::post(|| async { "ok" }))
            .layer(cors(&["https://fr.nonagentic.ai".to_string()]));

        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/prove")
                    .header(header::ORIGIN, "https://fr.nonagentic.ai")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "content-type,x-tenant-id",
                    )
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("preflight");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .expect("credentials header"),
            "true"
        );
        let allowed = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .expect("allow-headers")
            .to_str()
            .expect("ascii")
            .to_ascii_lowercase();
        assert!(allowed.contains("x-tenant-id"), "got {allowed}");
        assert!(allowed.contains("content-type"), "got {allowed}");

        // Same assertion as the headers one, for the second wildcard tower-http
        // refuses to combine with credentials.
        let methods = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .expect("allow-methods")
            .to_str()
            .expect("ascii")
            .to_ascii_uppercase();
        assert!(methods.contains("POST"), "got {methods}");
        assert!(methods.contains("GET"), "got {methods}");
    }
}
