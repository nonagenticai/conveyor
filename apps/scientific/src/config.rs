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
    /// This service's own licence, sent as the `Authorization` bearer on lean4-prover calls.
    ///
    /// lean4-prover rejects a tokenless call. The old service refused to boot without it; here an
    /// absent value is a refusal at the call site that names the variable, which is testable
    /// without a process-wide environment.
    pub downstream_license_token: Option<String>,
    /// The object store the proofs and the verification history live in. `None` means "not
    /// configured", and every route that reads it answers empty rather than failing — the old
    /// service's behaviour, kept because it is what makes the routes usable before the cutover.
    pub objects: Option<ObjectsConfig>,
    /// The licences service's introspection endpoint (`LICENSE_INTROSPECT_URL`). Unset means
    /// every credential is refused — the reference's fail-closed behaviour.
    pub license_introspect_url: Option<String>,
    /// The service `subject_id`s permitted to assert a tenant other than their licence's
    /// (`SCIENTIFIC_PROCESSOR_ONBEHALF_SUBJECTS`, CSV). Empty ⇒ no assertion is ever permitted.
    pub onbehalf_subjects: Vec<String>,
    /// Wall-clock budget for one introspection (`LICENSE_INTROSPECT_TIMEOUT`, seconds; the
    /// reference's 10 s default).
    pub license_introspect_timeout: Duration,
    /// The realm whose user tokens may name the human behind a decision
    /// (`SCIENTIFIC_PROCESSOR_ACTOR_ISSUER`). Unset ⇒ every `X-Actor-Token` is refused, which is
    /// the reference's fail-closed rule.
    pub actor_issuer: Option<String>,
    pub actor_audience: Option<String>,
    pub actor_jwks_url: Option<String>,
    /// `azp` values admitted for an actor token (CSV). Empty ⇒ any client of the realm.
    pub actor_azp: Vec<String>,
    /// Role names that confer the review lead (`SCIENTIFIC_PROCESSOR_REVIEW_LEAD_ROLES`, CSV).
    pub lead_roles: Vec<String>,
}

/// The reference's one lead role name, plus the second spelling the fleet has used.
pub const DEFAULT_LEAD_ROLES: [&str; 2] = ["review-lead", "evidence-review-lead"];

/// The MinIO/S3 endpoint and credentials, under the old service's variable names.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectsConfig {
    /// The endpoint as written in the environment. A bare `host:port` is read as `http://`; the
    /// HTTPS form is kept when the variable names it.
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
}

impl ObjectsConfig {
    fn from_lookup(vars: &[(String, String)]) -> Option<Self> {
        let endpoint = lookup(vars, "SCIENTIFIC_PROCESSOR_MINIO_ENDPOINT")?.to_owned();
        let access_key = lookup(vars, "SCIENTIFIC_PROCESSOR_MINIO_ACCESS_KEY")?.to_owned();
        if endpoint.is_empty() || access_key.is_empty() {
            return None;
        }
        Some(Self {
            endpoint,
            bucket: lookup(vars, "SCIENTIFIC_PROCESSOR_MINIO_BUCKET")
                .unwrap_or("scientific-processor")
                .to_owned(),
            access_key,
            secret_key: lookup(vars, "SCIENTIFIC_PROCESSOR_MINIO_SECRET_KEY")
                .unwrap_or_default()
                .to_owned(),
        })
    }

    /// The endpoint with a scheme, because `object_store` requires one and the old variable did
    /// not carry it.
    pub fn url(&self) -> String {
        if self.endpoint.contains("://") {
            self.endpoint.clone()
        } else {
            format!("http://{}", self.endpoint)
        }
    }
}

fn lookup<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn csv(value: Option<&str>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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
            downstream_license_token: lookup(vars, "SCIENTIFIC_PROCESSOR_DOWNSTREAM_LICENSE_TOKEN")
                .map(str::to_owned),
            objects: ObjectsConfig::from_lookup(vars),
            license_introspect_url: lookup(vars, "LICENSE_INTROSPECT_URL")
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .map(str::to_owned),
            onbehalf_subjects: lookup(vars, "SCIENTIFIC_PROCESSOR_ONBEHALF_SUBJECTS")
                .map(|raw| {
                    raw.split(',')
                        .map(str::trim)
                        .filter(|subject| !subject.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            license_introspect_timeout: Duration::from_secs_f64(
                lookup(vars, "LICENSE_INTROSPECT_TIMEOUT")
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|seconds| *seconds > 0.0)
                    .unwrap_or(10.0),
            ),
            actor_issuer: lookup(vars, "SCIENTIFIC_PROCESSOR_ACTOR_ISSUER")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            actor_audience: lookup(vars, "SCIENTIFIC_PROCESSOR_ACTOR_AUDIENCE")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            actor_jwks_url: lookup(vars, "SCIENTIFIC_PROCESSOR_ACTOR_JWKS_URL")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            actor_azp: csv(lookup(vars, "SCIENTIFIC_PROCESSOR_ACTOR_AZP")),
            lead_roles: {
                let configured = csv(lookup(vars, "SCIENTIFIC_PROCESSOR_REVIEW_LEAD_ROLES"));
                if configured.is_empty() {
                    DEFAULT_LEAD_ROLES
                        .iter()
                        .map(|role| (*role).to_owned())
                        .collect()
                } else {
                    configured
                }
            },
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
        assert!(config.actor_issuer.is_none());
        assert_eq!(
            config.lead_roles,
            vec!["review-lead".to_owned(), "evidence-review-lead".to_owned()]
        );
    }

    #[test]
    fn the_actor_rung_is_read_under_its_own_names() {
        let config = Config::from_lookup(&vars(&[
            (
                "SCIENTIFIC_PROCESSOR_ACTOR_ISSUER",
                "https://issuer/realms/x",
            ),
            ("SCIENTIFIC_PROCESSOR_ACTOR_AUDIENCE", "scientific"),
            (
                "SCIENTIFIC_PROCESSOR_ACTOR_JWKS_URL",
                "https://issuer/certs",
            ),
            ("SCIENTIFIC_PROCESSOR_ACTOR_AZP", "conveyor-web, other"),
            ("SCIENTIFIC_PROCESSOR_REVIEW_LEAD_ROLES", "head-reviewer"),
        ]))
        .expect("parses");
        assert_eq!(
            config.actor_issuer.as_deref(),
            Some("https://issuer/realms/x")
        );
        assert_eq!(config.actor_azp, vec!["conveyor-web", "other"]);
        assert_eq!(config.lead_roles, vec!["head-reviewer"]);
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

    #[test]
    fn the_object_store_is_read_under_the_old_names() {
        let config = Config::from_lookup(&vars(&[
            (
                "SCIENTIFIC_PROCESSOR_MINIO_ENDPOINT",
                "objects.example:9000",
            ),
            ("SCIENTIFIC_PROCESSOR_MINIO_ACCESS_KEY", "key"),
            ("SCIENTIFIC_PROCESSOR_MINIO_SECRET_KEY", "secret"),
        ]))
        .expect("parses");
        let objects = config.objects.expect("configured");
        assert_eq!(objects.bucket, "scientific-processor");
        assert_eq!(objects.url(), "http://objects.example:9000");
    }

    #[test]
    fn a_bare_endpoint_without_a_key_is_not_a_store() {
        let config = Config::from_lookup(&vars(&[(
            "SCIENTIFIC_PROCESSOR_MINIO_ENDPOINT",
            "objects.example:9000",
        )]))
        .expect("parses");
        assert!(config.objects.is_none());
    }
}
