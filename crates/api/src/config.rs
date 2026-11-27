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
    /// `CONVEYOR_GAME_HOSTS` — the normalised game host(s). A request on one of these is
    /// served the walk tree only; empty means the game host is not configured.
    pub game_hosts: Vec<String>,
    /// `CONVEYOR_GAME_PUBLIC_URL` — the canonical game origin the operator `/walk/*` 308
    /// points at. Derived as `https://<first game host>` when unset; never read from the
    /// request `Host`, so the redirect cannot be pointed at another origin.
    pub game_public_url: Option<String>,
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
    /// `BUNDLE_MINTER_CLIENT_ID` — the platform-owned `conveyor-bundle-minter`, the only client
    /// the licences mint route accepts a request from.
    pub bundle_minter_client_id: Option<String>,
    /// `BUNDLE_MINTER_CLIENT_SECRET` — that client's secret.
    pub bundle_minter_client_secret: Option<Secret>,
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
    /// `CONVEYOR_PUBLIC_TENANT` — the slug whose published rows the unauthenticated design
    /// site serves. Unset means the live window is closed: every live route answers 404.
    pub public_tenant: Option<String>,
    /// `CONVEYOR_GIT_SHA` — the commit the image was built from, carried by the live snapshot.
    pub git_sha: Option<String>,
    /// `CONVEYOR_JOB_TOKEN_KEY` — the runner's HMAC key (32 bytes). Unset is fail-closed: no token
    /// can be minted or verified, so every job byte route answers 503 and a dispatch is a
    /// permanent failure rather than a guess at a default key (#144, ADR 0007 §Decision 4).
    pub job_token_key: Option<conveyor_auth::job_token::JobTokenKey>,
    /// `CONVEYOR_RUNNER_IMAGES` — `kind=image[,kind=image]`. The dispatch resolves a job's kind
    /// here; a kind that is absent has no image and is refused, never defaulted.
    pub runner_images: Vec<(String, String)>,
    /// `CONVEYOR_RUNNER_BASE_URL` — the in-cluster address a dispatched pod calls back on. Unset is
    /// fail-closed: an instruction without a callable base URL is never dispatched.
    pub runner_base_url: Option<String>,
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
            game_hosts: parse_game_hosts(get("CONVEYOR_GAME_HOSTS")),
            game_public_url: game_public_url(
                get("CONVEYOR_GAME_PUBLIC_URL"),
                get("CONVEYOR_GAME_HOSTS"),
            ),
            licences_base_url: get("LICENCES_BASE_URL"),
            app_key: get("APP_KEY").unwrap_or_else(|| "conveyor".into()),
            service_token_url: get("SERVICE_TOKEN_URL"),
            service_client_id: get("SERVICE_CLIENT_ID"),
            service_client_secret: get("SERVICE_CLIENT_SECRET").map(Secret),
            bundle_minter_client_id: get("BUNDLE_MINTER_CLIENT_ID"),
            bundle_minter_client_secret: get("BUNDLE_MINTER_CLIENT_SECRET").map(Secret),
            provisioning_base_url: get("PROVISIONING_BASE_URL"),
            billing_base_url: get("BILLING_BASE_URL"),
            keycloak_admin_url: get("KEYCLOAK_ADMIN_URL"),
            keycloak_admin_realm: get("KEYCLOAK_ADMIN_REALM"),
            keycloak_admin_client_id: get("KEYCLOAK_ADMIN_CLIENT_ID"),
            keycloak_admin_client_secret: get("KEYCLOAK_ADMIN_CLIENT_SECRET").map(Secret),
            job_lease_secs,
            public_tenant: get("CONVEYOR_PUBLIC_TENANT"),
            git_sha: get("CONVEYOR_GIT_SHA"),
            job_token_key: match get("CONVEYOR_JOB_TOKEN_KEY") {
                None => None,
                Some(raw) => Some(conveyor_auth::job_token::JobTokenKey::parse(&raw).map_err(
                    |error| ConfigError::Invalid {
                        name: "CONVEYOR_JOB_TOKEN_KEY",
                        reason: error.to_string(),
                    },
                )?),
            },
            runner_images: parse_runner_images(get("CONVEYOR_RUNNER_IMAGES"))?,
            runner_base_url: get("CONVEYOR_RUNNER_BASE_URL"),
        })
    }

    /// The image a job of `kind` runs in, when the deployment configured one.
    pub fn runner_image(&self, kind: &str) -> Option<&str> {
        self.runner_images
            .iter()
            .find(|(configured, _)| configured == kind)
            .map(|(_, image)| image.as_str())
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

    /// The mint's own credential (`conveyor-bundle-minter`), or the name of the setting it is
    /// missing.
    ///
    /// The mint must **not** authenticate as the backend: the licences mint route is owner-scoped
    /// and `conveyor-backend`'s `api_client` row belongs to `fr`, so a role grant alone would still
    /// be refused. A missing setting is the error — never a fallback to
    /// [`Self::service_credentials`], which would seal a token nobody asked for.
    pub fn bundle_minter_credentials(&self) -> Result<conveyor_auth::ClientCredentials, String> {
        let missing = |name: &str| format!("bundle minter credential not configured: {name}");
        let client_id = self
            .bundle_minter_client_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| missing("BUNDLE_MINTER_CLIENT_ID"))?;
        let secret = self
            .bundle_minter_client_secret
            .as_ref()
            .map(Secret::expose)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| missing("BUNDLE_MINTER_CLIENT_SECRET"))?;
        conveyor_auth::ClientCredentials::new(
            self.service_token_url.as_deref().unwrap_or_default(),
            client_id,
            secret,
        )
        .map_err(|error| format!("bundle minter credential: {error}"))
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
        self.auth
            .validate(&self.app_env)
            .map_err(ConfigError::from)?;
        // The game host and its client are one feature: a host without a client id would
        // serve a walk whose /config.json cannot start a sign-in, and a client id without a
        // host would silently widen the azp allowlist for no deployed surface. Fail closed.
        match (
            self.game_hosts.is_empty(),
            self.auth.oidc_game_client_id.is_some(),
        ) {
            (false, false) => {
                return Err(ConfigError::Invalid {
                    name: "CONVEYOR_GAME_OIDC_CLIENT_ID",
                    reason: "CONVEYOR_GAME_HOSTS is set but no game client id is configured".into(),
                });
            }
            (true, true) => {
                return Err(ConfigError::Invalid {
                    name: "CONVEYOR_GAME_HOSTS",
                    reason: "CONVEYOR_GAME_OIDC_CLIENT_ID is set but no game host is configured"
                        .into(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    /// True when `host` (a `Host` header or a configured value) is one of the game hosts.
    pub fn is_game_host(&self, host: &str) -> bool {
        let host = normalize_host(host);
        !host.is_empty() && self.game_hosts.iter().any(|configured| configured == &host)
    }

    /// The game client id `/config.json` answers on the game host.
    pub fn game_client_id(&self) -> Option<&str> {
        self.auth.oidc_game_client_id.as_deref()
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

/// Canonicalise a `Host` header or configured host: trim, ASCII-lowercase, strip a numeric
/// port, strip a trailing dot. Handles `[v6]:port`; a bare IPv6 literal keeps its colons.
pub(crate) fn normalize_host(raw: &str) -> String {
    let host = raw.trim().to_ascii_lowercase();
    if let Some(rest) = host.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return rest[..end].trim_end_matches('.').to_owned();
    }
    let host = if host.matches(':').count() == 1 {
        match host.rsplit_once(':') {
            Some((name, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
                name.to_owned()
            }
            _ => host,
        }
    } else {
        host
    };
    host.trim_end_matches('.').to_owned()
}

/// `CONVEYOR_GAME_HOSTS`: comma-separated, normalised, de-duplicated, order preserved.
fn parse_game_hosts(raw: Option<String>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let mut hosts: Vec<String> = Vec::new();
    for entry in raw.split(',') {
        let host = normalize_host(entry);
        if !host.is_empty() && !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    hosts
}

/// The canonical game origin for the `/walk` 308. Configuration only, never the request
/// `Host`; when unset it is `https://<first configured game host>`.
fn game_public_url(raw: Option<String>, hosts: Option<String>) -> Option<String> {
    if let Some(raw) = raw {
        let url = raw.trim().trim_end_matches('/');
        if !url.is_empty() {
            return Some(url.to_owned());
        }
    }
    parse_game_hosts(hosts)
        .first()
        .map(|host| format!("https://{host}"))
}

/// `kind=image[,kind=image]`. An empty value is "no runner configured", not an error; a malformed
/// entry is, because silently dropping a kind would turn a dispatch into a confusing permanent
/// failure much later.
fn parse_runner_images(raw: Option<String>) -> Result<Vec<(String, String)>, ConfigError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let mut images = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((kind, image)) = entry.split_once('=') else {
            return Err(ConfigError::Invalid {
                name: "CONVEYOR_RUNNER_IMAGES",
                reason: format!("{entry:?} is not kind=image"),
            });
        };
        let (kind, image) = (kind.trim(), image.trim());
        if kind.is_empty() || image.is_empty() {
            return Err(ConfigError::Invalid {
                name: "CONVEYOR_RUNNER_IMAGES",
                reason: format!("{entry:?} has an empty kind or image"),
            });
        }
        if images
            .iter()
            .any(|(configured, _): &(String, String)| configured == kind)
        {
            return Err(ConfigError::Invalid {
                name: "CONVEYOR_RUNNER_IMAGES",
                reason: format!("{kind:?} is configured twice"),
            });
        }
        images.push((kind.to_owned(), image.to_owned()));
    }
    Ok(images)
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

    /// #776: the mint authenticates as its own client, and a deployment that forgot it is told
    /// which variable is absent instead of silently minting as the backend.
    #[test]
    fn the_minter_uses_its_own_credential_and_refuses_without_one() {
        const TOKEN_URL: &str =
            "https://auth.example/realms/secure-apps/protocol/openid-connect/token";

        let c = cfg(&[
            ("SERVICE_TOKEN_URL", TOKEN_URL),
            ("BUNDLE_MINTER_CLIENT_ID", "conveyor-bundle-minter"),
            ("BUNDLE_MINTER_CLIENT_SECRET", "minter-secret"),
        ])
        .unwrap();
        assert_eq!(
            c.bundle_minter_credentials().unwrap().client_id(),
            "conveyor-bundle-minter"
        );

        // The backend credential is configured and must NOT be used as a fallback.
        let c = cfg(&[
            ("SERVICE_TOKEN_URL", TOKEN_URL),
            ("SERVICE_CLIENT_ID", "conveyor-backend"),
            ("SERVICE_CLIENT_SECRET", "backend-secret"),
            ("BUNDLE_MINTER_CLIENT_ID", "conveyor-bundle-minter"),
        ])
        .unwrap();
        assert_eq!(
            c.bundle_minter_credentials().unwrap_err(),
            "bundle minter credential not configured: BUNDLE_MINTER_CLIENT_SECRET"
        );

        let c = cfg(&[("SERVICE_TOKEN_URL", TOKEN_URL)]).unwrap();
        assert_eq!(
            c.bundle_minter_credentials().unwrap_err(),
            "bundle minter credential not configured: BUNDLE_MINTER_CLIENT_ID"
        );
    }
}
