//! Deployment configuration, read once at startup.
//!
//! The inbound auth settings are the reference's (`auth/jwt_verify.py`): the issuer allowlist is
//! `KEYCLOAK_TRUSTED_ISSUERS` (CSV of full issuer URLs) when set, otherwise derived from
//! `KEYCLOAK_URL` plus the two realms that mint tokens for this service (`portal`, `secure-apps`).
//! An empty allowlist is not "trust nobody yet" — verification refuses every token, which is the
//! fail-closed behaviour the reference documents.

use crate::{DEFAULT_ENVIRONMENT, DEFAULT_PORT, DEFAULT_VERSION};

/// Realms whose tokens the service accepts when only `KEYCLOAK_URL` is configured.
pub const DEFAULT_TRUSTED_REALMS: [&str; 2] = ["portal", "secure-apps"];

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub environment: String,
    pub version: String,
    /// Full issuer URLs; empty means "no trusted issuer" and every JWT is refused.
    pub trusted_issuers: Vec<String>,
    /// `INTROSPECT_REQUIRE_AUTH` (default false — see the reference's long rationale).
    pub introspect_require_auth: bool,
    /// `SERVICE_ACTOR_ID`: the actor recorded when no human acted.
    pub service_actor_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("PORT is not a port number: {0}")]
    Port(String),
    #[error("DATABASE_URL is not set")]
    DatabaseUrl,
}

/// `_env_bool`: absent → default; otherwise one of `1/true/yes/on` (case-insensitive).
pub fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

/// The reference's issuer allowlist resolution (`jwt_verify.trusted_issuers`).
///
/// `KEYCLOAK_TRUSTED_ISSUERS` wins; each entry is trimmed and loses a trailing `/`. Otherwise
/// `KEYCLOAK_URL` plus [`DEFAULT_TRUSTED_REALMS`]. Deliberately not derived from `KEYCLOAK_REALM`
/// (that realm returns 404 for JWKS; see the reference's `jwt_verify` docstring).
pub fn trusted_issuers() -> Vec<String> {
    if let Ok(explicit) = std::env::var("KEYCLOAK_TRUSTED_ISSUERS") {
        let issuers: Vec<String> = explicit
            .split(',')
            .map(|entry| entry.trim().trim_end_matches('/').to_owned())
            .filter(|entry| !entry.is_empty())
            .collect();
        if !issuers.is_empty() {
            return issuers;
        }
    }
    let base = std::env::var("KEYCLOAK_URL")
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .unwrap_or_default();
    if base.is_empty() {
        return Vec::new();
    }
    DEFAULT_TRUSTED_REALMS
        .iter()
        .map(|realm| format!("{base}/realms/{realm}"))
        .collect()
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let port = match std::env::var("PORT") {
            Ok(value) => value.parse::<u16>().map_err(|_| ConfigError::Port(value))?,
            Err(_) => DEFAULT_PORT,
        };
        Ok(Self {
            port,
            environment: std::env::var("ENVIRONMENT")
                .unwrap_or_else(|_| DEFAULT_ENVIRONMENT.to_owned()),
            version: std::env::var("VERSION").unwrap_or_else(|_| DEFAULT_VERSION.to_owned()),
            trusted_issuers: trusted_issuers(),
            introspect_require_auth: env_bool("INTROSPECT_REQUIRE_AUTH", false),
            service_actor_id: std::env::var("SERVICE_ACTOR_ID")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }

    pub fn database_url(&self) -> Result<String, ConfigError> {
        std::env::var("DATABASE_URL").map_err(|_| ConfigError::DatabaseUrl)
    }

    pub fn bind(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}
