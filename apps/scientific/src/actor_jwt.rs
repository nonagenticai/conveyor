//! The verified-user rung: `X-Actor-Token`, a Keycloak user JWT carried beside the licence.
//!
//! The licence proves the tenant; it cannot prove a person. This module is how a human is named,
//! and it is ported from the reference's `_human_on_licence` rung (`api/dependencies.py`):
//!
//! * the signature is verified against the realm's JWKS — never decoded and trusted;
//! * only asymmetric algorithms are accepted (an `HS*` token signed with the public key as an
//!   HMAC secret is the classic key-confusion attack);
//! * `iss` is pinned, `aud` is required, and `azp` is checked against an allowlist when one is
//!   configured;
//! * `sub` is the attribution; the role comes from realm and client roles, and a Keycloak service
//!   account (`preferred_username` starting `service-account-`) is a MACHINE even though it has a
//!   perfectly good `sub`.
//!
//! Every refusal is the same 401 with the same text. Telling a caller *which* check failed is a
//! probing oracle; the distinctions go to the log.

use std::collections::HashMap;
use std::sync::Arc;

use conveyor_auth::JwksCache;
use conveyor_auth::jwks::HttpJwks;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

use crate::actor::{Actor, Role};
use crate::problem::Problem;

/// The realm user names Keycloak mints for a client's service account.
const SERVICE_ACCOUNT_PREFIX: &str = "service-account-";

/// The one refusal every credential failure is reported as.
const REFUSED: &str = "the presented actor token could not be verified";

pub struct ActorVerifier {
    issuer: String,
    audience: String,
    jwks_url: String,
    /// Empty means "any client in the realm"; non-empty is an allowlist of `azp` values.
    azp: Vec<String>,
    lead_roles: Vec<String>,
    cache: JwksCache,
    /// TEST ONLY. Unreachable from configuration: the only production constructor is
    /// `from_config`, which always builds the JWKS cache above.
    test_mode: bool,
}

#[derive(Debug, Default, Deserialize)]
struct Access {
    #[serde(default)]
    roles: Vec<String>,
}

impl std::fmt::Debug for ActorVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActorVerifier")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("jwks_url", &self.jwks_url)
            .field("azp", &self.azp)
            .field("lead_roles", &self.lead_roles)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    realm_access: Option<Access>,
    #[serde(default)]
    resource_access: Option<HashMap<String, Access>>,
}

impl ActorVerifier {
    /// Build from configuration. `None` when any part is missing: an unverifiable actor token is
    /// refused at the call site rather than accepted on the strength of a half-configured check.
    pub fn from_config(
        config: &crate::config::Config,
    ) -> Result<Option<Self>, conveyor_auth::AuthError> {
        let (Some(issuer), Some(audience), Some(jwks_url)) = (
            config.actor_issuer.clone(),
            config.actor_audience.clone(),
            config.actor_jwks_url.clone(),
        ) else {
            return Ok(None);
        };
        let source: Arc<dyn conveyor_auth::JwksSource> = Arc::new(HttpJwks::new()?);
        Ok(Some(Self::with_cache(
            issuer,
            audience,
            jwks_url,
            config.actor_azp.clone(),
            config.lead_roles.clone(),
            JwksCache::new(source),
        )))
    }

    /// Injectable construction, so the verifier is tested against a static key without a realm.
    pub fn with_cache(
        issuer: String,
        audience: String,
        jwks_url: String,
        azp: Vec<String>,
        lead_roles: Vec<String>,
        cache: JwksCache,
    ) -> Self {
        Self {
            issuer,
            audience,
            jwks_url,
            azp,
            lead_roles,
            cache,
            test_mode: false,
        }
    }

    /// TEST ONLY — a verifier for route tests that must not reach a realm.
    ///
    /// Tokens are `"<subject>|<role>"`, where role is `reviewer` or `lead`. Nothing in
    /// configuration can select this mode: `from_config` is the only production constructor.
    #[doc(hidden)]
    pub fn for_tests(lead_roles: Vec<String>) -> Self {
        let source: Arc<dyn conveyor_auth::JwksSource> = Arc::new(HttpJwks::new().expect("client"));
        let mut verifier = Self::with_cache(
            "test-issuer".to_owned(),
            "test-audience".to_owned(),
            "test-jwks".to_owned(),
            Vec::new(),
            lead_roles,
            JwksCache::new(source),
        );
        verifier.test_mode = true;
        verifier
    }

    pub async fn verify(&self, token: &str) -> Result<Actor, Problem> {
        if self.test_mode {
            let (subject, role) = token.split_once('|').ok_or_else(refused)?;
            if subject.trim().is_empty() {
                return Err(refused());
            }
            return Ok(Actor {
                tenant: uuid::Uuid::nil(),
                subject: Some(subject.trim().to_owned()),
                role: if role.trim() == "lead" {
                    Role::Lead
                } else {
                    Role::Reviewer
                },
                is_service_principal: false,
                display_name: None,
            });
        }
        let header = decode_header(token).map_err(|_| refused())?;
        if !is_asymmetric(header.alg) {
            tracing::warn!("actor token refused: symmetric algorithm");
            return Err(refused());
        }
        let kid = header.kid.as_deref().ok_or_else(refused)?;
        let key: DecodingKey = self.cache.key(&self.jwks_url, kid).await.map_err(|error| {
            tracing::warn!(%error, "actor token refused: no key");
            refused()
        })?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.validate_exp = true;
        validation.leeway = 0;

        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|error| {
                tracing::warn!(%error, "actor token refused: verification");
                refused()
            })?
            .claims;

        if !self.azp.is_empty() {
            let azp = claims.azp.as_deref().unwrap_or_default();
            if !self.azp.iter().any(|allowed| allowed == azp) {
                tracing::warn!(%azp, "actor token refused: azp");
                return Err(refused());
            }
        }

        let subject = claims.sub.trim().to_owned();
        if subject.is_empty() {
            tracing::warn!("actor token refused: no sub");
            return Err(refused());
        }

        let username = claims.preferred_username.clone().unwrap_or_default();
        let is_service_principal = username
            .trim()
            .to_ascii_lowercase()
            .starts_with(SERVICE_ACCOUNT_PREFIX);
        let roles = self.roles(&claims);
        let role = if roles
            .iter()
            .any(|held| self.lead_roles.iter().any(|lead| lead == held))
        {
            Role::Lead
        } else {
            Role::Reviewer
        };
        let display_name = if is_service_principal {
            None
        } else {
            claims
                .name
                .filter(|name| !name.trim().is_empty())
                .or_else(|| Some(username).filter(|name| !name.trim().is_empty()))
        };

        Ok(Actor {
            tenant: uuid::Uuid::nil(), // the licence supplies the tenant
            subject: Some(subject),
            role,
            is_service_principal,
            display_name,
        })
    }

    /// Every role the token carries: realm roles first, then every client's roles. The reference
    /// scans every grant because Keycloak's `fullScopeAllowed` puts them all in the token.
    fn roles(&self, claims: &Claims) -> Vec<String> {
        let mut roles = claims
            .realm_access
            .as_ref()
            .map(|access| access.roles.clone())
            .unwrap_or_default();
        if let Some(resources) = &claims.resource_access {
            for access in resources.values() {
                roles.extend(access.roles.iter().cloned());
            }
        }
        roles
    }
}

fn refused() -> Problem {
    Problem::new(
        axum::http::StatusCode::UNAUTHORIZED,
        "authentication_required",
        "Authentication Required",
        REFUSED,
    )
}

fn is_asymmetric(algorithm: Algorithm) -> bool {
    !matches!(
        algorithm,
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
    )
}
