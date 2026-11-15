//! The tenant, and only from the header.
//!
//! The references have no default tenant: a request carrying no `X-Tenant-Id` is refused `401
//! AUTHENTICATION_REQUIRED` (captured for every service in this group). The rewrite keeps that
//! direction deliberately — a sentinel tenant is how one caller's data becomes another's, and a
//! constant in the fixtures is a sanitisation artefact, so the header is what the caller must send.

use axum::extract::FromRequestParts;
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use uuid::Uuid;

use super::error::ApiError;

/// The header every captured operation is keyed on.
pub const TENANT_HEADER: HeaderName = HeaderName::from_static("x-tenant-id");

/// A caller's tenant, parsed from `X-Tenant-Id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tenant(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for Tenant {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(raw) = parts.headers.get(&TENANT_HEADER) else {
            return Err(ApiError::unauthorized());
        };
        let raw = raw.to_str().map_err(|_| {
            ApiError::validation("X-Tenant-Id is not a valid tenant identifier")
        })?;
        let tenant = Uuid::parse_str(raw.trim()).map_err(|_| {
            ApiError::validation(format!("X-Tenant-Id {raw:?} is not a UUID"))
        })?;
        Ok(Tenant(tenant))
    }
}
