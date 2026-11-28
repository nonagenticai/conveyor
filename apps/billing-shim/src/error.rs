//! The shim's failure surface.
//!
//! Only two shapes are fixed by the contract: an upstream failure is a `502`
//! `billing_upstream_error` and an unconfigured upstream is a `503`
//! `billing_unconfigured`. A malformed caller request is answered locally as a `400` with
//! its own `type`, so a client mistake is never dressed up as an upstream outage.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// A refusal from the shim.
#[derive(Debug)]
pub enum ShimError {
    /// The caller's request cannot be translated (`400 invalid_request`).
    Invalid(String),
    /// The billing service is tenant-scoped and needs `X-Tenant-Id` (`400 tenant_required`).
    TenantRequired,
    /// `BILLING_SERVICE_URL` resolved to nothing (`503 billing_unconfigured`).
    Unconfigured,
    /// The upstream failed, or answered outside its contract (`502 billing_upstream_error`).
    Upstream(String),
}

impl ShimError {
    /// Wrap an upstream detail string.
    pub fn upstream(detail: impl Into<String>) -> Self {
        Self::Upstream(detail.into())
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    /// The machine-readable failure kind.
    #[serde(rename = "type")]
    kind: &'static str,
    /// The human-readable reason.
    detail: String,
}

impl IntoResponse for ShimError {
    fn into_response(self) -> Response {
        let (status, kind, detail) = match self {
            Self::Invalid(detail) => (StatusCode::BAD_REQUEST, "invalid_request", detail),
            Self::TenantRequired => (
                StatusCode::BAD_REQUEST,
                "tenant_required",
                "X-Tenant-Id is required: billing-service scopes every rollup to one tenant"
                    .to_owned(),
            ),
            Self::Unconfigured => (
                StatusCode::SERVICE_UNAVAILABLE,
                "billing_unconfigured",
                "BILLING_SERVICE_URL is not configured; set it to the billing service base URL"
                    .to_owned(),
            ),
            Self::Upstream(detail) => (StatusCode::BAD_GATEWAY, "billing_upstream_error", detail),
        };
        (status, Json(ErrorBody { kind, detail })).into_response()
    }
}
