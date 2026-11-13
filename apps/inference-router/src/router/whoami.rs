//! `GET /api/whoami` — what the licence server says about the caller's own token.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::contract::schemas::WhoamiResponse;

#[utoipa::path(
    get,
    path = "/api/whoami",
    tag = "whoami",
    summary = "Resolve the caller's licence token",
    responses((status = 200, body = WhoamiResponse))
)]
pub async fn whoami() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}
