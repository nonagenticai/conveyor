//! Server configuration from an injectable lookup, so tests do not mutate the process
//! environment.
//!
//! ⚠️ The `DATABASE_*` defaults point at the compose stack (CONTRACT §10) and carry the
//! LOCAL DEVELOPMENT passwords from `scripts/db-roles.sql`. A deployed process sets them
//! explicitly; the default exists so `cargo run -p conveyor-server -- migrate` works on a
//! fresh checkout without a shell full of exports.

use std::net::SocketAddr;

use conveyor_auth::{AuthConfig, AuthConfigError};

/// The app binds this unless `BIND_ADDR` says otherwise (CONTRACT §10).
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8930";

/// The runtime role through pgbouncer (transaction pooling).
pub const DEFAULT_RUNTIME_URL: &str =
    "postgres://conveyor_runtime:conveyor_runtime_dev@127.0.0.1:6439/conveyor_dev";

/// The migration role, direct to Postgres. The migrator takes a SESSION advisory lock,
/// which cannot exclude a second replica through a transaction pooler.
pub const DEFAULT_MIGRATE_URL: &str =
    "postgres://conveyor_migrate:conveyor_migrate_dev@127.0.0.1:5439/conveyor_dev";

/// The runtime role, direct: `LISTEN` is session-scoped (S02b uses this for the SSE
/// doorbell), and pgbouncer returns the connection between statements.
pub const DEFAULT_LISTEN_URL: &str =
    "postgres://conveyor_runtime:conveyor_runtime_dev@127.0.0.1:5439/conveyor_dev";

/// Why the configuration is unusable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A cross-field auth rule failed.
    #[error("{0}")]
    Auth(#[from] AuthConfigError),
    /// A variable is present but malformed.
    #[error("{name} is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

/// A configuration value that must never reach a log line.
///
/// `Config` derives `Debug` and is printed by startup diagnostics; a client secret stored as
/// a bare `String` would be one `{config:?}` away from a log. The inner value is reachable
/// only through [`Secret::expose`], which is called exactly where a client is built.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// The value.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// True when the value is empty after trimming.
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Everything a process needs.
#[derive(Clone, Debug)]
pub struct Config {
    /// `APP_ENV`, normalised (trimmed + ASCII lowercase). Anything that is not an explicit
    /// development/test value is treated as production.
    pub app_env: String,
    /// `BIND_ADDR`.
    pub bind_addr: SocketAddr,
    /// `DATABASE_URL` — runtime through pgbouncer.
    pub database_url: String,
    /// `DATABASE_MIGRATE_URL` — owner, direct.
    pub database_migrate_url: String,
    /// `DATABASE_LISTEN_URL` — runtime, direct.
    pub database_listen_url: String,
    /// `CONVEYOR_*`.
    pub auth: AuthConfig,
    /// `LICENCES_BASE_URL` (S02b's licence.sync).
    pub licences_base_url: Option<String>,
    /// `APP_KEY`; the licence is looked up for this application.
    pub app_key: String,
    /// `SERVICE_TOKEN_URL` — the secure-apps token endpoint the S2S credential mints from.
    pub service_token_url: Option<String>,
    /// `SERVICE_CLIENT_ID` — `conveyor-backend`.
    pub service_client_id: Option<String>,
    /// `SERVICE_CLIENT_SECRET` — the secure-apps client secret.
    pub service_client_secret: Option<Secret>,
    /// `PROVISIONING_BASE_URL` — the Platform wizard's provision call.
    pub provisioning_base_url: Option<String>,
    /// `BILLING_BASE_URL` — the cost/usage reads.
    pub billing_base_url: Option<String>,
    /// `KEYCLOAK_ADMIN_URL` — the public Keycloak host (realm `conveyor` lives there).
    pub keycloak_admin_url: Option<String>,
    /// `KEYCLOAK_ADMIN_REALM` — normally `conveyor`.
    pub keycloak_admin_realm: Option<String>,
    /// `KEYCLOAK_ADMIN_CLIENT_ID` — normally the realm's `conveyor-backend`.
    pub keycloak_admin_client_id: Option<String>,
    /// `KEYCLOAK_ADMIN_CLIENT_SECRET` — that client's secret.
    pub keycloak_admin_client_secret: Option<Secret>,
    /// `JOB_LEASE_SECS`.
    pub job_lease_secs: u64,
}

impl Config {
    /// Read the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Testable core. Empty values count as unset.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |k: &str| get(k).filter(|v| !v.trim().is_empty());
        // Normalise once, here: trim + ASCII lowercase. Every check below is a fail-closed
        // allowlist (`is_development_env`), so `Production`, ` production `, `staging` and an
        // empty value are all production.
        let app_env = get("APP_ENV").unwrap_or_else(|| "development".into());
        let app_env = app_env.trim().to_ascii_lowercase();
        let bind_raw = get("BIND_ADDR").unwrap_or_else(|| DEFAULT_BIND_ADDR.into());
        let bind_addr = bind_raw
            .trim()
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::Invalid {
                name: "BIND_ADDR",
                reason: format!("{e} (expected host:port, e.g. 0.0.0.0:8930)"),
            })?;
        let job_lease_secs = match get("JOB_LEASE_SECS") {
            None => 60,
            Some(raw) => raw
                .trim()
                .parse::<u64>()
                .map_err(|e: std::num::ParseIntError| ConfigError::Invalid {
                    name: "JOB_LEASE_SECS",
                    reason: e.to_string(),
                })?
                .max(1),
        };
        Ok(Self {
            app_env,
            bind_addr,
            database_url: get("DATABASE_URL").unwrap_or_else(|| DEFAULT_RUNTIME_URL.into()),
            database_migrate_url: get("DATABASE_MIGRATE_URL")
                .unwrap_or_else(|| DEFAULT_MIGRATE_URL.into()),
            database_listen_url: get("DATABASE_LISTEN_URL")
                .unwrap_or_else(|| DEFAULT_LISTEN_URL.into()),
            auth: AuthConfig::from_lookup(get)?,
            licences_base_url: get("LICENCES_BASE_URL"),
            app_key: get("APP_KEY").unwrap_or_else(|| "conveyor".into()),
            service_token_url: get("SERVICE_TOKEN_URL"),
            service_client_id: get("SERVICE_CLIENT_ID"),
            service_client_secret: get("SERVICE_CLIENT_SECRET").map(Secret),
            provisioning_base_url: get("PROVISIONING_BASE_URL"),
            billing_base_url: get("BILLING_BASE_URL"),
            keycloak_admin_url: get("KEYCLOAK_ADMIN_URL"),
            keycloak_admin_realm: get("KEYCLOAK_ADMIN_REALM"),
            keycloak_admin_client_id: get("KEYCLOAK_ADMIN_CLIENT_ID"),
            keycloak_admin_client_secret: get("KEYCLOAK_ADMIN_CLIENT_SECRET").map(Secret),
            job_lease_secs,
        })
    }

    /// The secure-apps credential (`conveyor-backend`). `Err` names the missing setting —
    /// the Platform route turns it into a `503` that says which variable is absent.
    pub fn service_credentials(
        &self,
    ) -> Result<conveyor_auth::ClientCredentials, conveyor_auth::TokenError> {
        conveyor_auth::ClientCredentials::new(
            self.service_token_url.as_deref().unwrap_or_default(),
            self.service_client_id.as_deref().unwrap_or_default(),
            self.service_client_secret
                .as_ref()
                .map(Secret::expose)
                .unwrap_or_default(),
        )
    }

    /// The realm-`conveyor` admin credential. Its token endpoint is derived from the realm
    /// name, so one setting (`KEYCLOAK_ADMIN_REALM`) cannot drift from the other.
    pub fn keycloak_admin_credentials(
        &self,
    ) -> Result<conveyor_auth::ClientCredentials, conveyor_auth::TokenError> {
        use conveyor_auth::TokenError;
        let base = self
            .keycloak_admin_url
            .as_deref()
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/');
        if base.is_empty() {
            return Err(TokenError::Unconfigured("KEYCLOAK_ADMIN_URL"));
        }
        let realm = self
            .keycloak_admin_realm
            .as_deref()
            .unwrap_or("conveyor")
            .trim();
        if realm.is_empty() {
            return Err(TokenError::Unconfigured("KEYCLOAK_ADMIN_REALM"));
        }
        let client_id = self
            .keycloak_admin_client_id
            .as_deref()
            .unwrap_or_default()
            .trim();
        if client_id.is_empty() {
            return Err(TokenError::Unconfigured("KEYCLOAK_ADMIN_CLIENT_ID"));
        }
        let token_url = format!("{base}/realms/{realm}/protocol/openid-connect/token");
        conveyor_auth::ClientCredentials::new(
            &token_url,
            client_id,
            self.keycloak_admin_client_secret
                .as_ref()
                .map(Secret::expose)
                .unwrap_or_default(),
        )
    }

    /// The rules a SERVING process must satisfy. `migrate` and `worker` do not authenticate
    /// a request, so they must not be blocked by an auth setting (a migration on a box with
    /// no OIDC issuer is legitimate).
    pub fn validate_auth(&self) -> Result<(), ConfigError> {
        self.auth.validate(&self.app_env).map_err(ConfigError::from)
    }

    /// True unless `APP_ENV` is an explicit development/test value
    /// (`development`, `dev`, `test`, `local`).
    ///
    /// Fail-closed on purpose: `APP_ENV=Production` is production too, so a capitalisation or
    /// spelling difference cannot enable development authentication.
    pub fn is_production(&self) -> bool {
        !conveyor_auth::is_development_env(&self.app_env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn defaults_match_the_compose_stack_and_never_port_8000() {
        let c = cfg(&[]).unwrap();
        assert_eq!(c.bind_addr.port(), 8930);
        assert_eq!(c.bind_addr.to_string(), "0.0.0.0:8930");
        assert_eq!(c.app_env, "development");
        assert_eq!(c.app_key, "conveyor");
        assert_eq!(c.job_lease_secs, 60);
        assert!(c.database_url.contains(":6439/"));
        assert!(c.database_migrate_url.contains(":5439/"));
        assert!(c.database_listen_url.contains(":5439/"));
    }

    #[test]
    fn bind_addr_is_explicit_and_malformed_values_are_refused() {
        assert_eq!(
            cfg(&[("BIND_ADDR", "127.0.0.1:9999")])
                .unwrap()
                .bind_addr
                .port(),
            9999
        );
        assert!(matches!(
            cfg(&[("BIND_ADDR", "8930")]).unwrap_err(),
            ConfigError::Invalid {
                name: "BIND_ADDR",
                ..
            }
        ));
        assert!(matches!(
            cfg(&[("JOB_LEASE_SECS", "soon")]).unwrap_err(),
            ConfigError::Invalid {
                name: "JOB_LEASE_SECS",
                ..
            }
        ));
        assert_eq!(cfg(&[("JOB_LEASE_SECS", "0")]).unwrap().job_lease_secs, 1);
    }

    #[test]
    fn production_refuses_dev_auth_through_the_serving_validation() {
        let c = cfg(&[("APP_ENV", "production"), ("CONVEYOR_DEV_AUTH", "1")]).unwrap();
        assert!(matches!(
            c.validate_auth().unwrap_err(),
            ConfigError::Auth(AuthConfigError::DevAuthInProduction)
        ));
        // The same config parses: only serving refuses it, so `migrate` in production is
        // not blocked by an authentication setting it does not use.
        assert!(c.is_production());
    }

    #[test]
    fn app_env_is_normalized_and_an_unknown_value_is_production() {
        let capitalised = cfg(&[("APP_ENV", " Production "), ("CONVEYOR_DEV_AUTH", "1")]).unwrap();
        assert_eq!(capitalised.app_env, "production", "trimmed and lowercased");
        assert!(capitalised.is_production());
        assert!(
            matches!(
                capitalised.validate_auth().unwrap_err(),
                ConfigError::Auth(AuthConfigError::DevAuthInProduction)
            ),
            "APP_ENV=Production must refuse dev auth"
        );

        let test = cfg(&[("APP_ENV", " TEST ")]).unwrap();
        assert_eq!(test.app_env, "test");
        assert!(!test.is_production());
        assert!(
            test.validate_auth()
                .unwrap_err()
                .to_string()
                .contains("OIDC")
        );

        // Not a development value the allowlist knows: production, not a third mode.
        assert!(cfg(&[("APP_ENV", "staging")]).unwrap().is_production());
        assert!(cfg(&[("APP_ENV", "production")]).unwrap().is_production());
    }
}
