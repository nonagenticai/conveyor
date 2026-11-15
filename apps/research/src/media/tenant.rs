//! Who is calling, and for which tenant.
//!
//! The reference resolves this through its auth dependency, which under
//! `ALLOW_TRUSTED_NETWORK_DEFAULT_TENANT` falls back to a configured default. **This port has no
//! default.** `X-Tenant-Id` is required, and a request without it answers the reference's own 401
//! rather than quietly reading someone's rows — the fleet rule is that no service carries a
//! tenant-id sentinel, and a captured constant is a sanitisation artefact, not a default.
//!
//! The parity harness sends the sanitised constant explicitly, so the capture still pins the
//! tenant: a port that ignored the header would fail to reproduce it.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use super::AppState;
use super::error::ApiError;

/// The caller's tenant.
#[derive(Debug, Clone, Copy)]
pub struct Tenant(pub Uuid);

impl FromRequestParts<AppState> for Tenant {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(ApiError::unauthorized)?;
        let tenant = Uuid::parse_str(raw.trim()).map_err(|_| ApiError::unauthorized())?;
        Ok(Tenant(tenant))
    }
}
