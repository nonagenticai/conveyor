//! The three licence-token write routes, ported from `main.py` against the pinned `4e8a08d`.
//!
//! | Route | reference |
//! |---|---|
//! | `POST /licenses/{license_id}/tokens` | `main.py:6398` `create_license_token` |
//! | `POST /licenses/{license_id}/operator-token` | `main.py:6758` `create_operator_token` |
//! | `POST /licenses/{license_id}/regenerate-token` | `main.py:4582` `regenerate_token` |
//!
//! Two rules are the whole point of these endpoints and are reproduced literally:
//!
//! - **The expiry policy is decided HERE and re-checked at the DB boundary.** Omitting
//!   `expires_days` no longer means perpetual: an `api` token gets 365 days, an `internal`/`node`
//!   token is refused, and a perpetual token must be a REGISTERED long-lived one. Every refusal is
//!   the reference's literal discriminator plus its literal `remedy`.
//! - **Authority comes from the caller's own verified JWT.** A long-lived mint requires the
//!   `license-admin` REALM role; an operator token requires `platform-admin` AND the licence's
//!   subject to be the caller, so a platform admin cannot mint an admin credential someone else
//!   holds.
//!
//! The DB writes live in [`crate::db::licence_tokens`]; this layer only validates, authorizes and
//! renders the route envelope (`{"detail": …}`, see `error.rs`).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::{User, get_tenant_id, require_any_realm_role};
use crate::db::issuance;
use crate::db::licence_tokens::{
    self as store, CreateTokenError, MintLicence, OPERATOR_GRANTABLE_ROLES,
    OPERATOR_TOKEN_DEFAULT_TTL_MINUTES, OPERATOR_TOKEN_MAX_TTL_MINUTES, RefusalCode,
    RegenerateError,
};
use crate::db::tokens::{effective_status, py_iso};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `require_any_role("license-admin", "license-manager")` — a REALM role.
const WRITE_ROLES: [&str; 2] = ["license-admin", "license-manager"];

/// `RegenerateTokenResponse.message`'s default (serialised on every answer).
pub const REGENERATE_MESSAGE: &str = "Save this token - it won't be shown again";

/// `TokenType`: the three valid token types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Api,
    Node,
    Internal,
}

impl TokenType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Node => "node",
            Self::Internal => "internal",
        }
    }
}

fn default_token_type() -> TokenType {
    TokenType::Api
}

/// The captured `TokenCreateResponse`, field for field.
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenCreateResponse {
    pub token_id: String,
    pub license_id: String,
    /// The RAW token — returned exactly once; only its SHA-256 is stored.
    pub token: String,
    pub token_type: String,
    pub created_at: String,
    /// Nullable, but REQUIRED: the captured Pydantic model has no default, so it is always
    /// serialised (as null for a perpetual token).
    #[schema(required = true)]
    pub expires_at: Option<String>,
    /// Set only when this mint registered a perpetual long-lived token.
    pub long_lived_owner: Option<String>,
}

/// The captured `OperatorTokenResponse` (a `TokenCreateResponse` plus the granted roles).
#[derive(Debug, Serialize, ToSchema)]
pub struct OperatorTokenResponse {
    pub token_id: String,
    pub license_id: String,
    /// The RAW token — returned exactly once; only its SHA-256 is stored.
    pub token: String,
    pub token_type: String,
    pub created_at: String,
    /// Nullable, but REQUIRED (inherits `TokenCreateResponse.expires_at`).
    #[schema(required = true)]
    pub expires_at: Option<String>,
    /// Always null: an operator token is short-lived, never registered.
    pub long_lived_owner: Option<String>,
    /// The roles this one token carries on `/license/introspect`.
    pub roles: Vec<String>,
}

/// The captured `RegenerateTokenResponse`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RegenerateTokenResponse {
    /// The RAW replacement token — returned exactly once.
    pub token: String,
    /// Unused by clients; present because the captured model serialises its default.
    #[schema(
        required = false,
        default = "Save this token - it won't be shown again"
    )]
    pub message: String,
    /// When the new token expires; null means perpetual (until revoked).
    #[schema(value_type = Option<chrono::DateTime<chrono::Utc>>)]
    pub expires_at: Option<String>,
}

/// `POST /licenses/{license_id}/tokens`'s query parameters, documented as FastAPI publishes them.
/// Extraction is done by hand so each refusal is FastAPI's 422 body rather than axum's rejection.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LicenseTokenParams {
    /// Token type: api, node, internal
    #[serde(default = "default_token_type")]
    #[param(default = "api")]
    pub token_type: TokenType,
    /// Expiration in days. Must be >= 1 — 0 and negatives are rejected (422). Omitted: an `api`
    /// token gets the bounded default (365), an `internal`/`node` token is REFUSED unless
    /// `long_lived` is set. Omitting it no longer means perpetual (licenses#244).
    #[param(nullable, minimum = 1)]
    pub expires_days: Option<i64>,
    /// Register a PERPETUAL token in the long-lived register. Requires the `license-admin` role,
    /// `long_lived_owner` and `long_lived_reason`, and no `expires_days`.
    #[serde(default)]
    #[param(default = false)]
    pub long_lived: bool,
    /// Who owns this long-lived token.
    #[param(nullable, max_length = 200)]
    pub long_lived_owner: Option<String>,
    /// Why this token may be perpetual.
    #[param(nullable, max_length = 500)]
    pub long_lived_reason: Option<String>,
}

/// `POST /licenses/{license_id}/operator-token`'s query parameters.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OperatorTokenParams {
    /// Lifetime in minutes (1..60, default 15).
    #[serde(default = "default_ttl_minutes")]
    #[param(default = 15, minimum = 1, maximum = 60)]
    pub ttl_minutes: i64,
}

fn default_ttl_minutes() -> i64 {
    OPERATOR_TOKEN_DEFAULT_TTL_MINUTES
}

/// The validated licence-token query.
struct LicenseTokenQuery {
    token_type: TokenType,
    expires_days: Option<i64>,
    long_lived: bool,
    long_lived_owner: Option<String>,
    long_lived_reason: Option<String>,
}

impl LicenseTokenQuery {
    fn has_registration_fields(&self) -> bool {
        self.long_lived_owner
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || self
                .long_lived_reason
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }
}

fn validation_error(
    location: &'static str,
    kind: &'static str,
    field: &'static str,
    message: &'static str,
    input: Value,
) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": kind,
            "loc": [location, field],
            "msg": message,
            "input": input,
        }]),
    )
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "on" | "1" => Some(true),
        "false" | "f" | "no" | "n" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// The reference's query validation for `POST .../tokens`.
fn validate_license_token_query(
    raw: &BTreeMap<String, String>,
) -> Result<LicenseTokenQuery, Refusal> {
    let token_type = match raw.get("token_type") {
        None => TokenType::Api,
        Some(value) => match value.as_str() {
            "api" => TokenType::Api,
            "node" => TokenType::Node,
            "internal" => TokenType::Internal,
            _ => {
                return Err(validation_error(
                    "query",
                    "enum",
                    "token_type",
                    "Input should be 'api', 'node' or 'internal'",
                    json!(value),
                ));
            }
        },
    };

    let expires_days = match raw.get("expires_days") {
        None => None,
        Some(value) => match value.parse::<i64>() {
            Ok(number) if number < 1 => {
                return Err(validation_error(
                    "query",
                    "greater_than_equal",
                    "expires_days",
                    "Input should be greater than or equal to 1",
                    json!(number),
                ));
            }
            Ok(number) => Some(number),
            Err(_) => {
                return Err(validation_error(
                    "query",
                    "int_parsing",
                    "expires_days",
                    "Input should be a valid integer, unable to parse string as an integer",
                    json!(value),
                ));
            }
        },
    };

    let long_lived = match raw.get("long_lived") {
        None => false,
        Some(value) => match parse_bool(value) {
            Some(parsed) => parsed,
            None => {
                return Err(validation_error(
                    "query",
                    "bool_parsing",
                    "long_lived",
                    "Input should be a valid boolean, unable to interpret input",
                    json!(value),
                ));
            }
        },
    };

    let long_lived_owner = raw.get("long_lived_owner").cloned();
    if let Some(owner) = long_lived_owner.as_deref()
        && owner.chars().count() > 200
    {
        return Err(validation_error(
            "query",
            "string_too_long",
            "long_lived_owner",
            "String should have at most 200 characters",
            json!(owner),
        ));
    }
    let long_lived_reason = raw.get("long_lived_reason").cloned();
    if let Some(reason) = long_lived_reason.as_deref()
        && reason.chars().count() > 500
    {
        return Err(validation_error(
            "query",
            "string_too_long",
            "long_lived_reason",
            "String should have at most 500 characters",
            json!(reason),
        ));
    }

    Ok(LicenseTokenQuery {
        token_type,
        expires_days,
        long_lived,
        long_lived_owner,
        long_lived_reason,
    })
}

/// The reference's `ttl_minutes` validation (`ge=1, le=OPERATOR_TOKEN_MAX_TTL_MINUTES`).
fn validate_ttl(raw: &BTreeMap<String, String>) -> Result<i64, Refusal> {
    let Some(value) = raw.get("ttl_minutes") else {
        return Ok(OPERATOR_TOKEN_DEFAULT_TTL_MINUTES);
    };
    match value.parse::<i64>() {
        Ok(number) if number < 1 => Err(validation_error(
            "query",
            "greater_than_equal",
            "ttl_minutes",
            "Input should be greater than or equal to 1",
            json!(number),
        )),
        Ok(number) if number > OPERATOR_TOKEN_MAX_TTL_MINUTES => Err(validation_error(
            "query",
            "less_than_equal",
            "ttl_minutes",
            "Input should be less than or equal to 60",
            json!(number),
        )),
        Ok(number) => Ok(number),
        Err(_) => Err(validation_error(
            "query",
            "int_parsing",
            "ttl_minutes",
            "Input should be a valid integer, unable to parse string as an integer",
            json!(value),
        )),
    }
}

fn failure(error: impl std::fmt::Display, message: &'static str) -> Refusal {
    tracing::error!(error = %error, "licence token operation failed");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}

/// `_refuse_dependency_licence`: no USER token on a dependency licence (migration 059).
fn dependency_refusal(licence: &MintLicence) -> Refusal {
    tracing::warn!(
        license_id = %licence.id,
        app_id = %licence.app_id,
        "Refusing a user token on a dependency licence: bundle-token only"
    );
    Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "dependency_licence",
            "license_id": licence.id.to_string(),
            "remedy": "a dependency licence is internal-only: services mint it via POST /internal/tenants/{tenant_id}/bundle-token",
        }),
    )
}

/// `_license_state_detail`: the state fields of a 409 `license_not_active` body.
fn license_not_active_detail(
    license_id: &str,
    licence: &MintLicence,
    now: chrono::DateTime<Utc>,
) -> Value {
    json!({
        "error": "license_not_active",
        "license_id": license_id,
        "status": effective_status(&licence.status, licence.starts_at, licence.expires_at, now),
        "stored_status": licence.status,
        "starts_at": licence.starts_at.map(py_iso),
        "expires_at": licence.expires_at.map(py_iso),
    })
}

/// `POST /licenses/{license_id}/tokens`.
#[utoipa::path(
    post,
    path = "/licenses/{license_id}/tokens",
    tag = "tokens",
    operation_id = "create_license_token",
    summary = "Create License Token",
    description = "Create a new token for a license. Requires 'license-admin' or 'license-manager'; long_lived=true also requires 'license-admin'.",
    params(
        LicenseTokenParams,
        ("license_id" = String, Path),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 201, body = TokenCreateResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_license_token(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(license_id): Path<String>,
    Query(raw): Query<BTreeMap<String, String>>,
) -> Result<(StatusCode, Json<TokenCreateResponse>), Refusal> {
    // The reference resolves `get_tenant_id` first and `require_any_role` second (parameter order).
    let tenant_id = get_tenant_id(&state, &user).await?;
    require_any_realm_role(&user, &WRITE_ROLES)?;
    let query = validate_license_token_query(&raw)?;

    // ⛔ A PERPETUAL MINT IS A license-admin DECISION (licenses#244 validator). Checked before any
    // lookup, so the refusal writes nothing.
    if query.long_lived && !user.has_realm_role("license-admin") {
        tracing::warn!(
            licence = %license_id,
            actor = %user.preferred_username,
            "Refusing long-lived mint: actor lacks license-admin"
        );
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "long_lived_requires_license_admin",
                "remedy": "a perpetual (long_lived) token needs the license-admin role; license-manager may mint with expires_days",
            }),
        ));
    }

    let license_uuid = Uuid::try_parse(&license_id)
        .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"))?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(failure("no database connection", "Failed to create token"));
    };
    let now = Utc::now();

    let Some(licence) = store::licence(pool, license_uuid)
        .await
        .map_err(|error| failure(error, "Failed to create token"))?
    else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };
    if licence.tenant_id.to_string() != tenant_id {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    }

    issuance::authorize_issuance(
        pool,
        &user,
        licence.app_id,
        Some(licence.tenant_id),
        "token_create",
    )
    .await?;
    if licence.is_dependency {
        return Err(dependency_refusal(&licence));
    }

    let tenant_row = store::tenant_name_and_archive(pool, licence.tenant_id)
        .await
        .map_err(|error| failure(error, "Failed to create token"))?;
    if let Some((name, archived_at)) = tenant_row.as_ref()
        && archived_at.is_some()
    {
        tracing::warn!(
            licence = %license_id,
            tenant = %tenant_id,
            tenant_name = %name,
            archived_at = ?archived_at,
            "Refusing to mint: tenant is archived"
        );
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "refusing to mint: tenant is archived — un-archive the tenant before creating a token",
        ));
    }

    // A DEAD LICENCE MAY NOT BE ISSUED A NEW CREDENTIAL (#237). `accepts_new_token`, not
    // `is_active`: a forward-dated (`pending`) licence is deliberately allowed.
    if !store::accepts_new_token(&licence, now) {
        tracing::warn!(
            licence = %license_id,
            status = %effective_status(&licence.status, licence.starts_at, licence.expires_at, now),
            stored_status = %licence.status,
            "Refusing to mint: licence is not accepting new tokens"
        );
        return Err(Refusal::detail(
            StatusCode::CONFLICT,
            license_not_active_detail(&license_id, &licence, now),
        ));
    }

    let expires_at = query.expires_days.map(|days| now + Duration::days(days));

    // Registered callers that cannot pass an expiry: keyed on the VERIFIED `(iss, azp)` pair.
    let implicit_registration = if query.has_registration_fields() {
        None
    } else {
        store::implicit_long_lived_registration(
            user.iss.as_deref(),
            user.azp.as_deref(),
            query.token_type.as_str(),
            expires_at.is_some(),
            query.long_lived,
        )
    };
    let registration = if query.long_lived {
        Some(store::LongLivedRegistration {
            owner: query.long_lived_owner.clone().unwrap_or_default(),
            reason: query.long_lived_reason.clone().unwrap_or_default(),
            implicit_client: None,
        })
    } else {
        implicit_registration
    };

    let mut tenant_keys = std::collections::BTreeSet::from([licence.tenant_id.to_string()]);
    if let Some((name, _archived_at)) = tenant_row.as_ref() {
        tenant_keys.insert(name.clone());
    }
    let refusal = if !query.long_lived && query.has_registration_fields() {
        Some(RefusalCode::LongLivedFieldsWithoutLongLived)
    } else {
        store::token_expiry_refusal(
            query.token_type.as_str(),
            expires_at.is_some(),
            registration.as_ref(),
            &tenant_keys,
        )
    };
    if let Some(refusal) = refusal {
        tracing::warn!(
            licence = %license_id,
            policy = %refusal.code(),
            token_type = %query.token_type.as_str(),
            "Refusing to mint: expiry policy"
        );
        return Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"error": refusal.code(), "remedy": refusal.remedy()}),
        ));
    }

    let (token, raw_token) = store::create_token(
        pool,
        license_uuid,
        query.token_type.as_str(),
        expires_at,
        &[],
        registration.as_ref(),
    )
    .await
    .map_err(|error| match error {
        CreateTokenError::Policy(refusal) => Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"error": refusal.code(), "remedy": refusal.remedy()}),
        ),
        other => failure(other, "Failed to create token"),
    })?;

    if registration.is_some() {
        tracing::warn!(
            license_id = %license_id,
            token_id = %token.id,
            token_type = %token.token_type,
            owner = ?token.long_lived_owner,
            actor = %user.preferred_username,
            iss = ?user.iss,
            azp = ?user.azp,
            implicit = registration
                .as_ref()
                .is_some_and(|value| value.implicit_client.is_some()),
            "long_lived_token_registered"
        );
    }
    tracing::info!(token_id = %token.id, %license_id, "Token created");

    Ok((
        StatusCode::CREATED,
        Json(TokenCreateResponse {
            token_id: token.id.to_string(),
            license_id: token.license_id.to_string(),
            token: raw_token,
            token_type: token.token_type,
            created_at: py_iso(token.created_at),
            expires_at: token.expires_at.map(py_iso),
            long_lived_owner: token.long_lived_owner,
        }),
    ))
}

/// `POST /licenses/{license_id}/operator-token`.
#[utoipa::path(
    post,
    path = "/licenses/{license_id}/operator-token",
    tag = "tokens",
    operation_id = "create_operator_token",
    summary = "Create Operator Token",
    description = "Mint a SHORT-LIVED licence token that carries platform-admin on introspection. Requires the caller's own JWT to hold the platform-admin realm role, and the licence's subject to be that caller.",
    params(
        OperatorTokenParams,
        ("license_id" = String, Path),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 201, body = OperatorTokenResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_operator_token(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(license_id): Path<String>,
    Query(raw): Query<BTreeMap<String, String>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<OperatorTokenResponse>), Refusal> {
    // `require_role("platform-admin")` — a REALM role; a licence-token caller is only ever
    // `license-reader`, so an operator token can never mint another one.
    if !user.has_realm_role("platform-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'platform-admin' required",
        ));
    }
    let ttl_minutes = validate_ttl(&raw)?;

    let license_uuid = Uuid::try_parse(&license_id)
        .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid UUID"))?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(failure(
            "no database connection",
            "Failed to create operator token",
        ));
    };
    let now = Utc::now();

    let Some(licence) = store::licence(pool, license_uuid)
        .await
        .map_err(|error| failure(error, "Failed to create operator token"))?
    else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };

    // ⛔ YOURSELF ONLY: the licence's subject must be the caller.
    let subject_sub = match licence.subject_id {
        Some(subject_id) => store::subject_keycloak_sub(pool, subject_id)
            .await
            .map_err(|error| failure(error, "Failed to create operator token"))?,
        None => None,
    };
    if subject_sub.as_deref() != Some(user.sub.as_str()) {
        tracing::warn!(
            actor_sub = %user.sub,
            licence = %license_id,
            "operator_token_refused_not_own_licence"
        );
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    }

    // ⛔ TENANT-CONSISTENT: a header, when sent, must name the licence's own tenant.
    let header_tenant = headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(header_tenant) = header_tenant
        && Uuid::try_parse(header_tenant).ok() != Some(licence.tenant_id)
    {
        tracing::warn!(
            actor_sub = %user.sub,
            licence = %license_id,
            licence_tenant = %licence.tenant_id,
            "operator_token_refused_tenant_mismatch"
        );
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "X-Tenant-Id does not match the licence's tenant",
        ));
    }

    if !store::is_active(&licence, now) {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "refusing to mint: licence is not active",
        ));
    }
    let tenant_row = store::tenant_name_and_archive(pool, licence.tenant_id)
        .await
        .map_err(|error| failure(error, "Failed to create operator token"))?;
    if tenant_row.is_some_and(|(_name, archived_at)| archived_at.is_some()) {
        return Err(Refusal::detail(
            StatusCode::BAD_REQUEST,
            "refusing to mint: tenant is archived",
        ));
    }

    let roles: Vec<String> = OPERATOR_GRANTABLE_ROLES
        .iter()
        .filter(|role| user.has_realm_role(role))
        .map(|role| (*role).to_owned())
        .collect();
    if roles.is_empty() {
        return Err(Refusal::detail(StatusCode::FORBIDDEN, "No grantable role"));
    }

    let expires_at = now + Duration::minutes(ttl_minutes);
    let (token, raw_token) =
        store::create_token(pool, license_uuid, "api", Some(expires_at), &roles, None)
            .await
            .map_err(|error| match error {
                CreateTokenError::Policy(refusal) => Refusal::detail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    json!({"error": refusal.code(), "remedy": refusal.remedy()}),
                ),
                other => failure(other, "Failed to create operator token"),
            })?;

    tracing::warn!(
        actor_sub = %user.sub,
        actor = %user.preferred_username,
        license_id = %license_id,
        tenant_id = %licence.tenant_id,
        token_id = %token.id,
        roles = %roles.join(","),
        expires_at = %py_iso(expires_at),
        "operator_token_minted"
    );

    Ok((
        StatusCode::CREATED,
        Json(OperatorTokenResponse {
            token_id: token.id.to_string(),
            license_id: token.license_id.to_string(),
            token: raw_token,
            token_type: token.token_type,
            created_at: py_iso(token.created_at),
            expires_at: token.expires_at.map(py_iso),
            long_lived_owner: None,
            roles,
        }),
    ))
}

/// `POST /licenses/{license_id}/regenerate-token`.
#[utoipa::path(
    post,
    path = "/licenses/{license_id}/regenerate-token",
    tag = "tokens",
    operation_id = "regenerate_license_token",
    summary = "Regenerate Token",
    description = "Revoke all existing tokens and create a new one. The replacement inherits the expiry of the tokens it replaces. Requires 'license-admin' or 'license-manager'.",
    params(
        ("license_id" = Uuid, Path),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = RegenerateTokenResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn regenerate_license_token(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(license_id): Path<String>,
) -> Result<Json<RegenerateTokenResponse>, Refusal> {
    // The reference's parameter order is `user` then `tenant_id`.
    require_any_realm_role(&user, &WRITE_ROLES)?;
    let tenant_id = get_tenant_id(&state, &user).await?;

    // `license_id: UUID` is a FastAPI PATH parameter: an unparseable one is a 422, not a 400.
    let Ok(license_uuid) = Uuid::try_parse(&license_id) else {
        return Err(validation_error(
            "path",
            "uuid_parsing",
            "license_id",
            "Input should be a valid UUID, invalid character",
            json!(license_id),
        ));
    };
    let Some(pool) = state.pool.as_ref() else {
        return Err(failure(
            "no database connection",
            "Failed to regenerate token",
        ));
    };
    let now = Utc::now();

    let Some(licence) = store::licence(pool, license_uuid)
        .await
        .map_err(|error| failure(error, "Failed to regenerate token"))?
    else {
        return Err(Refusal::detail(StatusCode::NOT_FOUND, "License not found"));
    };
    // Ownership here is 403, not the mint route's 404.
    if licence.tenant_id.to_string() != tenant_id {
        return Err(Refusal::detail(StatusCode::FORBIDDEN, "Not your license"));
    }

    issuance::authorize_issuance(
        pool,
        &user,
        licence.app_id,
        Some(licence.tenant_id),
        "token_regenerate",
    )
    .await?;
    if licence.is_dependency {
        return Err(dependency_refusal(&licence));
    }

    // CHECK BEFORE DESTROY: rotating on a dead licence revokes working tokens and mints a corpse.
    if !store::is_active(&licence, now) {
        tracing::warn!(
            licence = %license_id,
            status = %effective_status(&licence.status, licence.starts_at, licence.expires_at, now),
            stored_status = %licence.status,
            "Refusing to regenerate: licence is not active"
        );
        return Err(Refusal::detail(
            StatusCode::CONFLICT,
            license_not_active_detail(&license_uuid.to_string(), &licence, now),
        ));
    }

    let (_revoked_count, token, raw_token) =
        store::regenerate_token_for_license(pool, license_uuid)
            .await
            .map_err(|error| match error {
                RegenerateError::AllTokensExpired { newest_expiry } => {
                    tracing::warn!(
                        licence = %license_id,
                        newest_expiry = ?newest_expiry,
                        "Refusing to regenerate: every unrevoked token has expired"
                    );
                    Refusal::detail(
                        StatusCode::CONFLICT,
                        json!({
                            "error": "all_tokens_expired",
                            "license_id": license_uuid.to_string(),
                            "remedy": "mint a new token via POST /licenses/{license_id}/tokens",
                        }),
                    )
                }
                RegenerateError::AllTokensRevoked { revoked_count } => {
                    tracing::warn!(
                        licence = %license_id,
                        revoked_count,
                        "Refusing to regenerate: every token is revoked"
                    );
                    Refusal::detail(
                        StatusCode::CONFLICT,
                        json!({
                            "error": "all_tokens_revoked",
                            "license_id": license_uuid.to_string(),
                            "remedy": "mint a new token via POST /licenses/{license_id}/tokens",
                        }),
                    )
                }
                RegenerateError::Policy(refusal) => Refusal::detail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    json!({"error": refusal.code(), "remedy": refusal.remedy()}),
                ),
                other => failure(other, "Failed to regenerate token"),
            })?;

    tracing::info!(license_id = %license_id, "Regenerated token");

    Ok(Json(RegenerateTokenResponse {
        token: raw_token,
        message: REGENERATE_MESSAGE.to_owned(),
        expires_at: token.expires_at.map(py_iso),
    }))
}
