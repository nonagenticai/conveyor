//! Auth configuration, parsed from an injectable lookup so it is testable without touching
//! the process environment.

use std::fmt;

/// The audience every person token must carry unless configured otherwise.
pub const DEFAULT_AUDIENCE: &str = "conveyor";

/// Why the auth configuration is unusable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuthConfigError {
    /// A boolean variable held something that is not 1/0/true/false.
    #[error("{name} must be 1, 0, true or false (got {value:?})")]
    InvalidBool { name: &'static str, value: String },
    /// Development auth turns an unauthenticated header into a principal; that is never
    /// allowed in a production process.
    #[error(
        "CONVEYOR_DEV_AUTH is set but APP_ENV is not a development/test value \
         (development, dev, test, local): development authentication is refused outside \
         development"
    )]
    DevAuthInProduction,
    /// With dev auth off, the OIDC issuer is the only source of identity.
    #[error("CONVEYOR_OIDC_ISSUER must be set when CONVEYOR_DEV_AUTH is off")]
    MissingIssuer,
}

/// Authentication inputs. `dev_auth` is `CONVEYOR_DEV_AUTH`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthConfig {
    pub dev_auth: bool,
    pub oidc_issuer: Option<String>,
    pub oidc_audience: String,
    pub oidc_client_id: String,
    /// `CONVEYOR_OIDC_JWKS_URL`; defaults to `<issuer>/protocol/openid-connect/certs`
    /// (Keycloak's shape).
    pub oidc_jwks_url: Option<String>,
}

impl AuthConfig {
    /// Development mode: dev auth on, no issuer needed.
    pub fn development() -> Self {
        Self {
            dev_auth: true,
            oidc_issuer: None,
            oidc_audience: DEFAULT_AUDIENCE.to_owned(),
            oidc_client_id: DEFAULT_AUDIENCE.to_owned(),
            oidc_jwks_url: None,
        }
    }

    /// Parse from an injectable lookup. Empty values count as unset.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, AuthConfigError> {
        let get = |k: &str| get(k).filter(|v| !v.trim().is_empty());
        let dev_auth = match get("CONVEYOR_DEV_AUTH") {
            None => false,
            Some(v) => parse_bool("CONVEYOR_DEV_AUTH", &v)?,
        };
        let oidc_audience =
            get("CONVEYOR_OIDC_AUDIENCE").unwrap_or_else(|| DEFAULT_AUDIENCE.into());
        // CONTRACT §9's transport is one client; absent an explicit client id, the audience
        // names it (the template's and agent's default).
        let oidc_client_id =
            get("CONVEYOR_OIDC_CLIENT_ID").unwrap_or_else(|| oidc_audience.clone());
        Ok(Self {
            dev_auth,
            oidc_issuer: get("CONVEYOR_OIDC_ISSUER"),
            oidc_audience,
            oidc_client_id,
            oidc_jwks_url: get("CONVEYOR_OIDC_JWKS_URL"),
        })
    }

    /// The cross-field rules, applied when a process starts serving (not by `migrate`, which
    /// authenticates nobody).
    ///
    /// The environment check is fail-closed: [`is_development_env`] is the allowlist, so
    /// `APP_ENV=Production`, `APP_ENV= production `, `APP_ENV=staging` and an empty value are
    /// all production here.
    pub fn validate(&self, app_env: &str) -> Result<(), AuthConfigError> {
        if self.dev_auth && !is_development_env(app_env) {
            return Err(AuthConfigError::DevAuthInProduction);
        }
        if !self.dev_auth && self.oidc_issuer.is_none() {
            return Err(AuthConfigError::MissingIssuer);
        }
        Ok(())
    }

    /// The issuer's JWKS endpoint.
    pub fn jwks_url(&self) -> Option<String> {
        let issuer = self.oidc_issuer.as_ref()?;
        Some(
            self.oidc_jwks_url
                .clone()
                .unwrap_or_else(|| format!("{issuer}/protocol/openid-connect/certs")),
        )
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::from_lookup(|_| None).expect("an empty lookup is valid")
    }
}

/// True only for a value that is **explicitly** a development or test environment.
///
/// This is the fail-closed allowlist used by every `APP_ENV` check: `trim` + ASCII lowercase,
/// then one of `development`, `dev`, `test`, `local`. Anything else — `Production`,
/// ` production `, `staging`, `prod`, or an empty string — is treated as production, so a
/// misspelled environment cannot silently enable development authentication.
pub fn is_development_env(app_env: &str) -> bool {
    matches!(
        app_env.trim().to_ascii_lowercase().as_str(),
        "development" | "dev" | "test" | "local"
    )
}

fn parse_bool(name: &'static str, raw: &str) -> Result<bool, AuthConfigError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AuthConfigError::InvalidBool {
            name,
            value: raw.to_owned(),
        }),
    }
}

impl fmt::Display for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never prints a secret: there is none here, but the shape stays greppable.
        write!(
            f,
            "AuthConfig {{ dev_auth: {}, issuer: {:?}, audience: {:?}, client_id: {:?} }}",
            self.dev_auth, self.oidc_issuer, self.oidc_audience, self.oidc_client_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<AuthConfig, AuthConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        AuthConfig::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn defaults_are_oidc_with_the_conveyor_audience() {
        let c = cfg(&[]).unwrap();
        assert!(!c.dev_auth);
        assert_eq!(c.oidc_audience, "conveyor");
        assert_eq!(c.oidc_client_id, "conveyor");
        assert!(c.jwks_url().is_none());
    }

    #[test]
    fn client_id_defaults_to_the_audience_and_jwks_to_the_issuer() {
        let c = cfg(&[
            (
                "CONVEYOR_OIDC_ISSUER",
                "https://auth.example/realms/conveyor",
            ),
            ("CONVEYOR_OIDC_AUDIENCE", "conveyor-api"),
        ])
        .unwrap();
        assert_eq!(c.oidc_client_id, "conveyor-api");
        assert_eq!(
            c.jwks_url().as_deref(),
            Some("https://auth.example/realms/conveyor/protocol/openid-connect/certs")
        );
    }

    #[test]
    fn dev_auth_parses_and_empty_means_unset() {
        for v in ["1", "true", "YES", "on"] {
            assert!(cfg(&[("CONVEYOR_DEV_AUTH", v)]).unwrap().dev_auth, "{v}");
        }
        for v in ["0", "false", "no", "off"] {
            assert!(!cfg(&[("CONVEYOR_DEV_AUTH", v)]).unwrap().dev_auth, "{v}");
        }
        assert!(!cfg(&[("CONVEYOR_DEV_AUTH", "  ")]).unwrap().dev_auth);
        assert!(matches!(
            cfg(&[("CONVEYOR_DEV_AUTH", "maybe")]).unwrap_err(),
            AuthConfigError::InvalidBool { .. }
        ));
    }

    #[test]
    fn production_refuses_dev_auth_and_requires_an_issuer() {
        let dev = AuthConfig::development();
        assert_eq!(
            dev.validate("production").unwrap_err(),
            AuthConfigError::DevAuthInProduction
        );
        assert!(dev.validate("development").is_ok());
        assert!(dev.validate("test").is_ok());

        let oidc = cfg(&[]).unwrap();
        assert_eq!(
            oidc.validate("production").unwrap_err(),
            AuthConfigError::MissingIssuer
        );
        let with_issuer = cfg(&[("CONVEYOR_OIDC_ISSUER", "https://auth.example")]).unwrap();
        assert!(with_issuer.validate("production").is_ok());
    }

    #[test]
    fn only_an_explicit_development_value_is_development() {
        for value in ["development", "DEV", " dev ", "Test", "LOCAL", "local\n"] {
            assert!(is_development_env(value), "{value:?} is development");
        }
        // Fail closed: a capitalised, padded, misspelled, unknown or empty value is production.
        for value in [
            "Production",
            " PRODUCTION ",
            "prod",
            "staging",
            "",
            "   ",
            "develop",
        ] {
            assert!(!is_development_env(value), "{value:?} is not development");
        }

        let dev = AuthConfig::development();
        assert_eq!(
            dev.validate("Production").unwrap_err(),
            AuthConfigError::DevAuthInProduction,
            "APP_ENV=Production must not serve with dev auth"
        );
        assert_eq!(
            dev.validate(" production ").unwrap_err(),
            AuthConfigError::DevAuthInProduction
        );
        assert_eq!(
            dev.validate("staging").unwrap_err(),
            AuthConfigError::DevAuthInProduction
        );
    }
}
