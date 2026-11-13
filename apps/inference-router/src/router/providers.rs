//! `GET /api/providers` and `GET /api/providers/{name}/models` — catalog enumeration for UIs.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::contract::schemas::{
    HTTPValidationError, ProviderListResponse, ProviderModelListResponse,
};

#[utoipa::path(
    get,
    path = "/api/providers",
    tag = "providers",
    summary = "List providers",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ProviderListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_providers() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[utoipa::path(
    get,
    path = "/api/providers/{name}/models",
    tag = "providers",
    summary = "List one provider's models",
    params(
        ("name" = String, Path),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ProviderModelListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_provider_models() -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}
