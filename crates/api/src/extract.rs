//! Request extractors built on the auth crate.
//!
//! [`Caller`] is a newtype because `Session` and `FromRequestParts` are both foreign to this
//! crate (the orphan rule). It caches the session in the request extensions, so a handler
//! that takes several guards authenticates once per request.
//!
//! [`RequireLicence`] is the ONE licence gate (CONTRACT §9). It reads `app_licence` through
//! the runtime pool and nothing else: no request path calls the licences service, so an
//! outage of that service cannot change a request's answer — only the sync job's freshness
//! can, and staleness is itself a refusal.

use std::ops::Deref;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use chrono::Utc;
use conveyor_auth::Session;
use conveyor_auth::licence::{Verdict, evaluate};

use crate::error::ApiError;
use crate::state::AppState;

/// The authenticated caller of this request.
#[derive(Clone, Debug)]
pub struct Caller(pub Session);

impl Deref for Caller {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(session) = parts.extensions.get::<Session>() {
            return Ok(Self(session.clone()));
        }
        let session = state.auth.session(&state.pool, &parts.headers).await?;
        parts.extensions.insert(session.clone());
        Ok(Self(session))
    }
}

/// The caller's identity and memberships, with the selected tenant when there is one.
///
/// `/api/me` is the only route that takes this: it must answer with the membership list even
/// when the caller named no tenant, and it must still report the active tenant when one WAS
/// selected (a licence bearer's tenant, or a single membership's only choice).
#[derive(Clone, Debug)]
pub struct CallerIdentity(
    pub conveyor_auth::IdentitySession,
    pub Option<conveyor_auth::Session>,
);

impl FromRequestParts<AppState> for CallerIdentity {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let (identity, active) = state
            .auth
            .identity_or_active(&state.pool, &parts.headers)
            .await?;
        Ok(Self(identity, active))
    }
}

/// Proof that the caller's tenant holds an active, unexpired, freshly-synced entitlement.
///
/// A refusal is `403 licence_required` problem+json carrying the specific reason in
/// `detail`; the body never includes the upstream licence payload.
#[derive(Clone, Debug)]
pub struct RequireLicence;

impl FromRequestParts<AppState> for RequireLicence {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let caller = Caller::from_request_parts(parts, state).await?;
        let row =
            conveyor_store::licence::get(&state.pool, caller.tenant, &state.config.app_key).await?;
        match evaluate(row.as_ref(), Utc::now()) {
            Verdict::Allowed => Ok(Self),
            Verdict::Refused(reason) => {
                tracing::warn!(
                    tenant = %caller.tenant,
                    reason = reason.code(),
                    "licence refused"
                );
                Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "licence_required",
                    reason.detail(),
                ))
            }
        }
    }
}
