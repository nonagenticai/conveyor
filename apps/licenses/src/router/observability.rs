//! `GET /metrics` — the Prometheus scrape endpoint.
//!
//! The endpoint is deliberately NOT a documented JSON operation: it is absent from `ROUTES` and
//! from the `OpenApi` document, so `--check-lock` (which compares documented operations) does not
//! see it. It IS in the auth middleware's public set (`auth::PUBLIC_ENDPOINTS`), because the
//! scraper carries no credential.
//!
//! The families live in `crate::metrics`, which owns both collectors the Python reference
//! registered on this path: licence/token expiry (`metrics::expiry`) and the tenant/subject
//! invariant (`metrics::tenant_subject`). This handler only renders them — the response is always
//! 200, whatever the database does; a failed collector reports itself with
//! `..._collector_up 0` rather than by failing the scrape.

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// `GET /metrics`. Always 200: the scrape must answer even when the database does not.
pub async fn metrics(State(state): State<AppState>) -> Response {
    let body = crate::metrics::render(&state.pool).await;
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        body,
    )
        .into_response()
}
