//! Configuration from the environment.
//!
//! The names are the live service's (`INFERENCE_ROUTER_*`), deliberately: the deployment reuses
//! the existing ConfigMap mount and Secret, so the cutover is a deployment, not a config rewrite.
//! `from_lookup` exists so the parser is testable without mutating process-global environment
//! variables (tests run in parallel in one process).

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// The port the live service binds when `BIND_ADDR` is absent.
pub const DEFAULT_PORT: u16 = 8150;

/// The live deployment's provider catalog mount (`settings.py:532`).
pub const DEFAULT_PROVIDERS_CONFIG: &str = "/config/providers.yaml";

/// Everything the process reads before it serves a request.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// The listener address (`BIND_ADDR`, else `0.0.0.0:$INFERENCE_ROUTER_PORT`).
    pub bind: SocketAddr,
    /// The service's own database. Required to serve; `--print-openapi` never reads it.
    pub database_url: Option<String>,
    /// The provider catalog YAML (the live ConfigMap's mount path by default).
    pub providers_config_path: PathBuf,
    /// `INFERENCE_ROUTER_ENVIRONMENT` — `development` also enables the docs routes.
    pub environment: String,
    /// `ENABLE_DOCS=true` (the live deployment sets it) or `environment == development`.
    pub docs_enabled: bool,
    /// How long a request waits for a per-provider concurrency slot before it is shed as
    /// `queue_rejected` (`PROVIDER_SLOT_WAIT_TIMEOUT`, default 30 s).
    pub provider_slot_wait_seconds: f64,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR is not a socket address: {0}")]
    InvalidBind(String),
    #[error("INFERENCE_ROUTER_PORT is not a port number: {0}")]
    InvalidPort(String),
    #[error("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT is not a positive number of seconds: {0}")]
    InvalidSlotWait(String),
    #[error("INFERENCE_ROUTER_DATABASE_URL (or DATABASE_URL) is required to serve")]
    MissingDatabaseUrl,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let bind = match non_empty(lookup("BIND_ADDR")) {
            Some(raw) => raw
                .parse()
                .map_err(|_| ConfigError::InvalidBind(raw.clone()))?,
            None => {
                let port = match non_empty(lookup("INFERENCE_ROUTER_PORT")) {
                    Some(raw) => raw
                        .parse()
                        .map_err(|_| ConfigError::InvalidPort(raw.clone()))?,
                    None => DEFAULT_PORT,
                };
                SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))
            }
        };

        // The live Secret supplies `INFERENCE_ROUTER_DATABASE_URL`; `DATABASE_URL` is the
        // workspace-wide name tests and local runs use. The service-specific one wins.
        let database_url = non_empty(lookup("INFERENCE_ROUTER_DATABASE_URL"))
            .or_else(|| non_empty(lookup("DATABASE_URL")));

        let providers_config_path = non_empty(lookup("INFERENCE_ROUTER_PROVIDERS_CONFIG_PATH"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PROVIDERS_CONFIG));

        let environment = non_empty(lookup("INFERENCE_ROUTER_ENVIRONMENT"))
            .unwrap_or_else(|| "production".to_owned());

        // Mirrors the live service exactly (`src/main.py`): development, or an explicit
        // `ENABLE_DOCS=true`. The live deployment sets the latter, so `/openapi.json` is served.
        let docs_enabled = environment == "development"
            || non_empty(lookup("ENABLE_DOCS"))
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));

        let provider_slot_wait_seconds: f64 =
            match non_empty(lookup("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidSlotWait(raw.clone()))?,
                None => 30.0,
            };
        if provider_slot_wait_seconds <= 0.0 || !provider_slot_wait_seconds.is_finite() {
            return Err(ConfigError::InvalidSlotWait(
                provider_slot_wait_seconds.to_string(),
            ));
        }

        Ok(Self {
            bind,
            database_url,
            providers_config_path,
            environment,
            docs_enabled,
            provider_slot_wait_seconds,
        })
    }

    /// The database URL, or a refusal that names the variable rather than its value.
    pub fn database_url(&self) -> Result<&str, ConfigError> {
        self.database_url
            .as_deref()
            .ok_or(ConfigError::MissingDatabaseUrl)
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    #[test]
    fn defaults_match_the_live_service() {
        let config = parse(&[]).expect("defaults parse");
        assert_eq!(config.bind, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8150)));
        assert_eq!(config.database_url, None);
        assert_eq!(
            config.providers_config_path,
            PathBuf::from("/config/providers.yaml")
        );
        assert_eq!(config.environment, "production");
        assert!(!config.docs_enabled);
        assert_eq!(config.provider_slot_wait_seconds, 30.0);
    }

    #[test]
    fn the_slot_wait_must_be_positive() {
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "0")]),
            Err(ConfigError::InvalidSlotWait("0".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "soon")]),
            Err(ConfigError::InvalidSlotWait("soon".to_owned()))
        );
        assert!(parse(&[("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT", "45")]).is_ok());
    }

    #[test]
    fn bind_addr_wins_over_the_port() {
        let config = parse(&[
            ("BIND_ADDR", "127.0.0.1:9000"),
            ("INFERENCE_ROUTER_PORT", "7000"),
        ])
        .expect("BIND_ADDR parses");
        assert_eq!(config.bind, SocketAddr::from(([127, 0, 0, 1], 9000)));
    }

    #[test]
    fn port_is_read_when_bind_addr_is_absent() {
        let config = parse(&[("INFERENCE_ROUTER_PORT", "8151")]).expect("port parses");
        assert_eq!(config.bind, SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8151)));
    }

    #[test]
    fn invalid_bind_and_port_fail_closed() {
        assert_eq!(
            parse(&[("BIND_ADDR", "not-a-socket")]),
            Err(ConfigError::InvalidBind("not-a-socket".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_PORT", "soon")]),
            Err(ConfigError::InvalidPort("soon".to_owned()))
        );
    }

    #[test]
    fn the_service_database_url_wins_over_the_workspace_one() {
        let config = parse(&[
            ("INFERENCE_ROUTER_DATABASE_URL", "postgres://service/db"),
            ("DATABASE_URL", "postgres://workspace/db"),
        ])
        .expect("both parse");
        assert_eq!(
            config.database_url.as_deref(),
            Some("postgres://service/db")
        );

        let workspace_only = parse(&[("DATABASE_URL", "postgres://workspace/db")]).expect("parses");
        assert_eq!(
            workspace_only.database_url.as_deref(),
            Some("postgres://workspace/db")
        );
    }

    #[test]
    fn blank_values_are_absent_not_empty() {
        let config = parse(&[("DATABASE_URL", "  ")]).expect("parses");
        assert_eq!(config.database_url, None);
        assert_eq!(config.environment, "production");
    }

    #[test]
    fn docs_follow_the_live_service_rule() {
        assert!(
            parse(&[("ENABLE_DOCS", "TRUE")])
                .expect("parses")
                .docs_enabled
        );
        assert!(
            parse(&[("INFERENCE_ROUTER_ENVIRONMENT", "development")])
                .expect("parses")
                .docs_enabled
        );
        assert!(
            !parse(&[("ENABLE_DOCS", "false")])
                .expect("parses")
                .docs_enabled
        );
    }
}
