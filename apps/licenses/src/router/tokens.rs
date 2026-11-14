//! `/license/introspect` — the fleet's authentication primitive.
//!
//! The endpoint is public by default: the token in the body is the credential. The reference's
//! `INTROSPECT_REQUIRE_AUTH` gate is reproduced (default off), including its loopback exemption,
//! because the service's own opaque-token middleware path is a loopback caller with no credential
//! to present.
//!
//! ⚠️ The response model is shared with `GET /licenses/active` in the reference. `/active` leaves
//! the token-only fields null and fills the pick diagnostics; this route leaves the diagnostics
//! null. Both serialise EVERY field (Pydantic includes defaults), so `None` must emit `null`, not
//! vanish.

use axum::Json;
use axum::extract::State;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::net::SocketAddr;
use utoipa::ToSchema;

use crate::auth::bearer;
use crate::db::tokens::{ActiveIntrospection, Inactive, Outcome};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// The request body. `token` is required; an empty string is a valid body and introspects to
/// `token_not_found`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenIntrospectRequest {
    /// Raw token to introspect
    pub token: String,
}

/// The captured `TokenIntrospectResponse`, field for field and in the model's order.
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenIntrospectResponse {
    pub active: bool,
    pub tenant_id: Option<String>,
    pub license_id: Option<String>,
    pub subject_id: Option<String>,
    pub subject_keycloak_sub: Option<String>,
    pub subject_actor_sub: Option<String>,
    pub subject_kind: Option<String>,
    pub byo_mode: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub app: Option<Value>,
    #[schema(value_type = Option<Object>)]
    pub plan: Option<Value>,
    pub status: Option<String>,
    pub starts_at: Option<String>,
    pub expires_at: Option<String>,
    pub token_expires_at: Option<String>,
    pub region_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<Value>,
    pub reason: Option<String>,
    pub roles: Option<Vec<String>>,
    pub candidate_count: Option<i64>,
    pub ambiguous: Option<bool>,
    pub subject_match: Option<bool>,
}

impl TokenIntrospectResponse {
    /// An inactive answer: only `active` and `reason` are set; every other field is null.
    pub fn inactive(inactive: &Inactive) -> Self {
        Self {
            active: false,
            tenant_id: None,
            license_id: None,
            subject_id: None,
            subject_keycloak_sub: None,
            subject_actor_sub: None,
            subject_kind: None,
            byo_mode: None,
            app: None,
            plan: None,
            status: None,
            starts_at: None,
            expires_at: None,
            token_expires_at: None,
            region_id: None,
            metadata: None,
            reason: Some(inactive.reason().to_owned()),
            roles: None,
            candidate_count: None,
            ambiguous: None,
            subject_match: None,
        }
    }

    /// An active answer from the introspection payload.
    pub fn active(data: ActiveIntrospection) -> Self {
        Self {
            active: true,
            tenant_id: Some(data.tenant_id),
            license_id: Some(data.license_id),
            subject_id: Some(data.subject_id),
            subject_keycloak_sub: data.subject_keycloak_sub,
            subject_actor_sub: data.subject_actor_sub,
            subject_kind: data.subject_kind,
            byo_mode: Some(data.byo_mode),
            app: Some(data.app),
            plan: Some(data.plan),
            status: Some(data.status),
            starts_at: Some(data.starts_at),
            expires_at: data.expires_at,
            token_expires_at: data.token_expires_at,
            region_id: data.region_id,
            metadata: data.metadata,
            reason: None,
            roles: Some(data.roles),
            candidate_count: None,
            ambiguous: None,
            subject_match: None,
        }
    }
}

/// `INTROSPECT_REQUIRE_AUTH` armed: every non-loopback caller must present a verified JWT.
///
/// A verified JWT is the only credential accepted here. The middleware's opaque-token branch is
/// deliberately not reused: it would introspect a token in order to decide whether to introspect
/// a token, recursing through the route being gated.
async fn require_caller(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    peer: Option<SocketAddr>,
) -> Result<(), Refusal> {
    let loopback = peer.is_some_and(|addr| addr.ip().is_loopback());
    if !state.config.introspect_require_auth || loopback {
        return Ok(());
    }
    let token = bearer(headers.get("authorization").and_then(|v| v.to_str().ok()));
    match token {
        Some(token) if state.verifier.verify(token).await.is_some() => Ok(()),
        _ => {
            tracing::warn!("introspect_unauthenticated_refused: INTROSPECT_REQUIRE_AUTH is armed");
            Err(Refusal::detail(
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "introspect_requires_auth"}),
            )
            .with_header("www-authenticate", "Bearer"))
        }
    }
}

/// The TCP peer address, when the server supplied one.
///
/// `ConnectInfo<SocketAddr>` is only present when the app is served with
/// `into_make_service_with_connect_info`; a test that builds the router directly has no peer, and
/// the reference treats an unknown peer as NOT loopback (fail closed). This extractor always
/// succeeds and hands back `None` instead of rejecting the request.
pub struct Peer(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for Peer {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Peer(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(addr)| *addr),
        ))
    }
}

/// `POST /license/introspect`.
#[utoipa::path(
    post,
    path = "/license/introspect",
    tag = "tokens",
    summary = "Introspect Token",
    description = "Introspect a token to check validity and get license info (public by default; the token in the body is the auth).",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TokenIntrospectRequest,
    responses(
        (status = 200, body = TokenIntrospectResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn introspect(
    State(state): State<AppState>,
    Peer(peer): Peer,
    headers: axum::http::HeaderMap,
    Json(request): Json<TokenIntrospectRequest>,
) -> Result<Json<TokenIntrospectResponse>, Refusal> {
    require_caller(&state, &headers, peer).await?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    match crate::db::tokens::introspect(pool, &request.token).await {
        Ok(Outcome::Active(data)) => Ok(Json(TokenIntrospectResponse::active(*data))),
        Ok(Outcome::Inactive(reason)) => Ok(Json(TokenIntrospectResponse::inactive(&reason))),
        Err(error) => {
            // The reference logs the real cause and returns a generic detail: the endpoint is
            // public, so a driver/DSN string must never reach the response.
            tracing::error!(%error, "Token introspection failed");
            Err(internal_error())
        }
    }
}

fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}
