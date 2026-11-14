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
use axum::extract::{ConnectInfo, Extension, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::SocketAddr;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{User, bearer, require_any_realm_role, require_tenant_id};
use crate::db::tokens::{ActiveIntrospection, Inactive, Outcome, RevokeOutcome};
use crate::db::{dependencies, issuance};
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

/// Request to revoke a token
#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenRevokeRequest {
    /// Raw token to revoke
    pub token: Option<String>,
    /// License ID to revoke all tokens
    pub license_id: Option<String>,
}

/// Response from token revocation
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenRevokeResponse {
    pub revoked: bool,
    pub message: String,
    // The reference model is `list[str] = Field(default_factory=list)`: the field is NOT
    // nullable and is always serialised, as `[]` when nothing was released. `Option` here only
    // keeps it out of the OpenAPI `required` list; `value_type` keeps the schema an array.
    /// Ids of the tenant's dependency licences revoked with this licence because no remaining active licence requires them (app_requires closure).
    #[schema(value_type = Vec<String>)]
    pub released_dependency_ids: Option<Vec<String>>,
}

/// `POST /license/revoke` — revoke a raw token, or a licence and all its tokens.
///
/// Two arms, exactly as the reference:
///
/// - `token`: known-owner check, then issuance authorization over EVERY licence of the tenant
///   carrying the hash (`_authorize_token_revoke`), then one transaction that revokes every
///   matching row. A non-empty `token` wins over `license_id`.
/// - `license_id`: tenant ownership (`"License not found"` for unknown OR foreign — one message,
///   so the route cannot be used to probe another tenant), `_authorize_licence_revoke` with the
///   dependency widening, then the dependency-release engine.
#[utoipa::path(
    post,
    path = "/license/revoke",
    tag = "tokens",
    operation_id = "revoke_token",
    summary = "Revoke Token Or License",
    description = "Revoke a token or all tokens for a license\n\nRequires 'license-admin' or 'license-manager' role\nVerifies tenant ownership before revocation",
    params(
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = TokenRevokeRequest,
    responses(
        (status = 200, body = TokenRevokeResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn revoke(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(request): Json<TokenRevokeRequest>,
) -> Result<Json<TokenRevokeResponse>, Refusal> {
    // The reference resolves both FastAPI dependencies before the handler body runs, so a
    // wrong role or a missing tenant wins over any body semantics.
    let tenant = require_tenant_id(&user)?;
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(revocation_failed("no database connection"));
    };
    // `require_tenant_id` already validated the shape; keep the reference's ValueError answer.
    let tenant_uuid = Uuid::try_parse(&tenant).map_err(|_| bad_request("Invalid UUID"))?;

    // Python truthiness: an empty string is not a provided field.
    let token = request.token.as_deref().filter(|value| !value.is_empty());
    let license_id = request
        .license_id
        .as_deref()
        .filter(|value| !value.is_empty());

    if let Some(raw_token) = token {
        // `_authorize_token_revoke`: the known-owner gate is unconditional, the licence walk
        // only when the tenant owns a row for this hash.
        issuance::require_known_owner(&user)?;
        let licences = issuance::licences_for_token_hash_in_tenant(pool, raw_token, &tenant)
            .await
            .map_err(revocation_failed)?;
        if !licences.is_empty() {
            issuance::authorize_licence_revoke(
                pool,
                &user,
                &licences,
                Some(tenant_uuid),
                "token_revoke",
                false,
            )
            .await?;
        }

        let (outcome, revoked_count) =
            crate::db::tokens::revoke_token_for_tenant(pool, raw_token, &tenant)
                .await
                .map_err(revocation_failed)?;
        let (revoked, message) = match outcome {
            RevokeOutcome::Revoked if revoked_count > 1 => (
                true,
                format!(
                    "Token revoked successfully ({revoked_count} token records shared this \
                     token; all were revoked)"
                ),
            ),
            RevokeOutcome::Revoked => (true, "Token revoked successfully".to_owned()),
            // Idempotent: the caller asked for a state the row is already in.
            RevokeOutcome::AlreadyRevoked => (true, "Token was already revoked".to_owned()),
            // "not_found" also covers "not your tenant", on purpose.
            RevokeOutcome::NotFound => (false, "Token not found or already invalid".to_owned()),
        };
        return Ok(Json(TokenRevokeResponse {
            revoked,
            message,
            released_dependency_ids: Some(Vec::new()),
        }));
    }

    let Some(raw_license_id) = license_id else {
        return Err(bad_request(
            "Either 'token' or 'license_id' must be provided",
        ));
    };
    let license_id = Uuid::try_parse(raw_license_id).map_err(|_| bad_request("Invalid UUID"))?;

    // Tenant ownership BEFORE any authorization, with one message for unknown and foreign.
    let Some(licence) = crate::db::tokens::licence_for_revoke(pool, license_id)
        .await
        .map_err(revocation_failed)?
    else {
        return Ok(Json(license_not_found()));
    };
    if licence.tenant_id != tenant_uuid {
        return Ok(Json(license_not_found()));
    }

    // Issuer rule, or the tenant owner over bundle-minted / dependency rows.
    issuance::authorize_licence_revoke(
        pool,
        &user,
        std::slice::from_ref(&licence),
        Some(tenant_uuid),
        "licence_revoke",
        true,
    )
    .await?;

    // A still-required DEPENDENCY licence is a 409 for every caller except the cross-owner
    // break-glass, which may force it (the engine logs `dependency_force_revoked`).
    let refuse_required = licence.is_dependency && !user.cross_owner;
    let actor = if user.sub.is_empty() {
        user.azp.clone().unwrap_or_default()
    } else {
        user.sub.clone()
    };
    let outcome = dependencies::revoke_license_releasing_dependencies(
        pool,
        license_id,
        &actor,
        refuse_required,
    )
    .await
    .map_err(revocation_failed)?;

    match outcome {
        dependencies::Outcome::Revoked { released_ids } => {
            let mut message = "License and all tokens revoked successfully".to_owned();
            if !released_ids.is_empty() {
                message.push_str(&format!(
                    "; {} dependency licence(s) no longer required were released",
                    released_ids.len()
                ));
            }
            Ok(Json(TokenRevokeResponse {
                revoked: true,
                message,
                released_dependency_ids: Some(released_ids.iter().map(Uuid::to_string).collect()),
            }))
        }
        // The row vanished between the ownership read and the engine's own read.
        dependencies::Outcome::NotFound => Ok(Json(license_not_found())),
        dependencies::Outcome::DependencyStillRequired { required_by } => {
            let required_by: Vec<Value> = required_by
                .iter()
                .map(|row| {
                    json!({
                        "license_id": row.license_id.to_string(),
                        "tenant_id": row.tenant_id.to_string(),
                        "app_key": row.app_key,
                    })
                })
                .collect();
            Err(Refusal::detail(
                StatusCode::CONFLICT,
                json!({
                    "code": "dependency_still_required",
                    // `error` mirrors `code`: this service's 409 discriminator key.
                    "error": "dependency_still_required",
                    "license_id": raw_license_id,
                    "required_by": required_by,
                    "message": "a remaining active licence on this tenant still requires this \
                                dependency; revoke that licence and the dependency is released",
                }),
            ))
        }
        dependencies::Outcome::EnsureInProgress => Err(Refusal::detail(
            StatusCode::CONFLICT,
            json!({
                "code": "ensure_in_progress",
                "error": "ensure_in_progress",
                "license_id": raw_license_id,
                "message": "a bundle ensure for this tenant is running; retry",
            }),
        )),
    }
}

fn license_not_found() -> TokenRevokeResponse {
    TokenRevokeResponse {
        revoked: false,
        message: "License not found".to_owned(),
        released_dependency_ids: Some(Vec::new()),
    }
}

fn bad_request(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::BAD_REQUEST, message)
}

fn revocation_failed(error: impl std::fmt::Display) -> Refusal {
    tracing::error!(error = %error, "Revocation failed");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Revocation failed")
}
