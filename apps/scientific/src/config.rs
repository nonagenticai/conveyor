//! Configuration from the environment.
//!
//! The names are the old service's (`SCIENTIFIC_PROCESSOR_*`), deliberately: the deployment's
//! existing ConfigMap and Secret are reused when the cutover happens, so that step is a deployment,
//! not a config rewrite. `from_lookup` exists so the parser is testable without mutating
//! process-global environment variables.

use std::net::SocketAddr;
use std::time::Duration;

/// The port the old service binds when nothing says otherwise.
pub const DEFAULT_PORT: u16 = 8808;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR is not a socket address: {0}")]
    InvalidBind(String),
    #[error("SCIENTIFIC_PROCESSOR_CACHE_TTL_SECONDS is not a number: {0}")]
    InvalidTtl(String),
    #[error("SCIENTIFIC_PROCESSOR_CACHE_MAX_ENTRIES is not an integer: {0}")]
    InvalidMaxEntries(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub bind: SocketAddr,
    /// This service's own database. Absent until the pipeline/sandbox store lands; the surviving
    /// statistics and cache routes do not read it.
    pub database_url: Option<String>,
    pub cache_ttl: Duration,
    pub cache_max_entries: usize,
    /// The downstream the `/api/formal` routes delegate to.
    pub lean4_prover_url: Option<String>,
    /// The downstream `/api/verify` uses for model judgements.
    pub inference_router_url: Option<String>,
    /// The downstream the verification helpers call for embeddings.
    pub pytorch_backend_url: Option<String>,
}

fn lookup<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

impl Config {
    /// Parse from an explicit environment, which is what makes this testable.
    pub fn from_lookup(vars: &[(String, String)]) -> Result<Self, ConfigError> {
        let raw_bind = lookup(vars, "BIND_ADDR")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("0.0.0.0:{DEFAULT_PORT}"));
        let bind = raw_bind
            .parse()
            .map_err(|_| ConfigError::InvalidBind(raw_bind.clone()))?;

        let ttl_seconds = lookup(vars, "SCIENTIFIC_PROCESSOR_CACHE_TTL_SECONDS")
            .map(|raw| {
                raw.parse::<f64>()
                    .map_err(|_| ConfigError::InvalidTtl(raw.to_owned()))
            })
            .transpose()?
            .unwrap_or(300.0);
        let cache_max_entries = lookup(vars, "SCIENTIFIC_PROCESSOR_CACHE_MAX_ENTRIES")
            .map(|raw| {
                raw.parse::<usize>()
                    .map_err(|_| ConfigError::InvalidMaxEntries(raw.to_owned()))
            })
            .transpose()?
            .unwrap_or(1000);

        Ok(Self {
            bind,
            database_url: lookup(vars, "DATABASE_URL").map(str::to_owned),
            cache_ttl: Duration::from_secs_f64(ttl_seconds.max(0.0)),
            cache_max_entries,
            lean4_prover_url: lookup(vars, "SCIENTIFIC_PROCESSOR_LEAN4_PROVER_URL")
                .map(str::to_owned),
            inference_router_url: lookup(vars, "SCIENTIFIC_PROCESSOR_INFERENCE_ROUTER_URL")
                .map(str::to_owned),
            pytorch_backend_url: lookup(vars, "SCIENTIFIC_PROCESSOR_PYTORCH_BACKEND_URL")
                .map(str::to_owned),
        })
    }

    /// Parse from this process's environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        let vars: Vec<(String, String)> = std::env::vars().collect();
        Self::from_lookup(&vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn the_defaults_are_the_old_service_s() {
        let config = Config::from_lookup(&vars(&[])).expect("defaults parse");
        assert_eq!(config.bind.port(), DEFAULT_PORT);
        assert_eq!(config.cache_ttl, Duration::from_secs(300));
        assert_eq!(config.cache_max_entries, 1000);
        assert!(config.database_url.is_none());
    }

    #[test]
    fn the_old_env_names_are_honoured() {
        let config = Config::from_lookup(&vars(&[
            ("SCIENTIFIC_PROCESSOR_CACHE_TTL_SECONDS", "15"),
            ("SCIENTIFIC_PROCESSOR_CACHE_MAX_ENTRIES", "7"),
            ("SCIENTIFIC_PROCESSOR_LEAN4_PROVER_URL", "http://lean4"),
        ]))
        .expect("parses");
        assert_eq!(config.cache_ttl, Duration::from_secs(15));
        assert_eq!(config.cache_max_entries, 7);
        assert_eq!(config.lean4_prover_url.as_deref(), Some("http://lean4"));
    }

    #[test]
    fn a_bad_bind_is_refused_not_defaulted() {
        let error = Config::from_lookup(&vars(&[("BIND_ADDR", "not-an-address")]))
            .expect_err("must refuse");
        assert!(matches!(error, ConfigError::InvalidBind(_)));
    }
}
