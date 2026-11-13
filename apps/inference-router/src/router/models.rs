//! `GET /api/models` — the tenant-scoped model listing.
//!
//! The listing is derived from the live provider catalog and narrowed by the tenant policy; the
//! handler lands with the auth/policy slice. The route and its contract are fixed here.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::contract::schemas::{HTTPValidationError, ModelListResponse};

#[utoipa::path(
    get,
    path = "/api/models",
    tag = "models",
    summary = "List models",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ModelListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_models() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}
