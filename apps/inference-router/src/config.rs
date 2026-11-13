//! Configuration from the environment.
//!
//! The names are the live service's (`INFERENCE_ROUTER_*`), deliberately: the deployment reuses
//! the existing ConfigMap mount and Secret, so the cutover is a deployment, not a config rewrite.
//! `from_lookup` exists so the parser is testable without mutating process-global environment
//! variables (tests run in parallel in one process).

use std::net::{Ipv4Addr, SocketAddr};

use crate::auth::Mode;
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
    /// `INFERENCE_ROUTER_INTROSPECT_MODE` (`off` | `observe` | `enforce`); when unset, `enforce`
    /// iff `INFERENCE_ROUTER_REQUIRE_CALLER_LICENSE` is true, else `off`.
    pub introspect_mode: Mode,
    /// `INFERENCE_ROUTER_LICENSE_INTROSPECT_URL` — the licences `/license/introspect` endpoint.
    pub introspect_url: String,
    /// `INFERENCE_ROUTER_LICENSE_INTROSPECT_TIMEOUT` (seconds, default 2).
    pub introspect_timeout_seconds: f64,
    /// `INFERENCE_ROUTER_LICENSE_INTROSPECT_FAIL_OPEN` (default true): an outage is not a verdict.
    pub introspect_fail_open: bool,
    /// `INFERENCE_ROUTER_METERING_REDIS_URL` — the cost bus. Absent means metering is off, and the
    /// boot log says so; it is never a silent no-op.
    pub metering_redis_url: Option<String>,
    /// `INFERENCE_ROUTER_METERING_COSTS_ENABLED` (the reference defaults to true).
    pub metering_costs_enabled: bool,
    /// `INFERENCE_ROUTER_METERING_COSTS_STREAM` (the reference default; the live deployment sets
    /// `billing:costs`).
    pub metering_costs_stream: String,
    /// `INFERENCE_ROUTER_BUDGET_GATE_MODE` — `off` makes no call at all.
    pub budget_gate_mode: GateMode,
    /// `INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL` — billing's `/api/budget/reserve` origin.
    pub budget_gate_billing_base_url: String,
    /// `INFERENCE_ROUTER_BUDGET_GATE_TIMEOUT_SECONDS` — the WHOLE reserve call, no retries.
    pub budget_gate_timeout_seconds: f64,
    /// `INFERENCE_ROUTER_BUDGET_GATE_RESERVE_TTL_SECONDS` — billing accepts 5..=600.
    pub budget_gate_reserve_ttl_seconds: i64,
    /// `INFERENCE_ROUTER_POLICY_MODE` — `store` runs the store and admin API but routes globally.
    pub policy_mode: PolicyMode,
    /// `INFERENCE_ROUTER_POLICY_CACHE_TTL_SECONDS` — bounds staleness with the epoch poll.
    pub policy_cache_ttl_seconds: f64,
    /// `INFERENCE_ROUTER_POLICY_EPOCH_POLL_SECONDS` — no LISTEN/NOTIFY: pgbouncer pools.
    pub policy_epoch_poll_seconds: f64,
    /// `INFERENCE_ROUTER_INTERNAL_SECRET` — the six `/api/internal/*` reads' shared secret.
    pub private_api_secret: Option<String>,
    /// `INFERENCE_ROUTER_INTERNAL_PUBLIC_HOST_SUFFIXES` — Host suffixes that mean the request
    /// came through the PUBLIC ingress; `/api/internal/*` answers 404 for them.
    pub public_host_suffixes: Vec<String>,
    /// `INFERENCE_ROUTER_KEYCLOAK_JWKS_URL` — verifies the service-account JWT on `/api/admin/*`.
    /// Unset makes the admin API answer 503 (fail closed).
    pub keycloak_jwks_url: Option<String>,
    /// `INFERENCE_ROUTER_PORTAL_JWKS_URL` — verifies the forwarded portal JWT. Unset falls back
    /// to `keycloak_jwks_url` (the reference's rule).
    pub portal_jwks_url: Option<String>,
    /// `INFERENCE_ROUTER_ADMIN_AZP_ALLOWLIST` — client ids (`azp`) whose service-account JWT may
    /// call `/api/admin/*`. Empty makes the admin API answer 503.
    pub admin_azp_allowlist: Vec<String>,
    /// `INFERENCE_ROUTER_ADMIN_JWT_ISSUERS` — optional `iss` allowlist for `/api/admin/*`.
    /// Empty means the issuer is not checked (the JWKS pins the realm).
    pub admin_jwt_issuers: Vec<String>,
    /// `INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE` — per client host, per pod.
    pub whoami_rate_limit_per_minute: u32,
    /// `INFERENCE_CAPTURE_ALLOWED_CALLERS` — deliberately UNPREFIXED (the reference aliases it).
    /// `X-App-Id` values allowed to read a captured body; empty means every capture read is 403,
    /// which is the live deployment's state and the captured golden.
    pub inference_capture_allowed_callers: Vec<String>,
    /// `COMMERCIAL_PLATFORM_TENANT_IDS` — deliberately UNPREFIXED, matching the live manifest.
    pub commercial_platform_tenant_ids: Vec<String>,
    /// `AUTH_DELEGATING_SUBJECTS` — the FLEET's allowlist of subjects permitted to name another
    /// tenant via `X-Effective-Tenant-Id`. Deliberately UNPREFIXED: the same value is read by
    /// the modality gateway and mlx_backend, so one list serves every hop.
    pub auth_delegating_subjects: Vec<String>,
}

/// `INFERENCE_ROUTER_BUDGET_GATE_MODE`, exactly the reference's three states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateMode {
    Off,
    Shadow,
    Enforce,
}

impl GateMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "shadow" => Some(Self::Shadow),
            "enforce" => Some(Self::Enforce),
            _ => None,
        }
    }
}

/// `INFERENCE_ROUTER_POLICY_MODE`. `store` is the rollback position: the policy store and the
/// admin API are live, routing ignores policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyMode {
    Off,
    Store,
    On,
}

impl PolicyMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "store" => Some(Self::Store),
            "on" => Some(Self::On),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR is not a socket address: {0}")]
    InvalidBind(String),
    #[error("INFERENCE_ROUTER_PORT is not a port number: {0}")]
    InvalidPort(String),
    #[error("INFERENCE_ROUTER_PROVIDER_SLOT_WAIT_TIMEOUT is not a positive number of seconds: {0}")]
    InvalidSlotWait(String),
    #[error("INFERENCE_ROUTER_INTROSPECT_MODE is not one of off/observe/enforce: {0}")]
    InvalidIntrospectMode(String),
    #[error("INFERENCE_ROUTER_LICENSE_INTROSPECT_TIMEOUT is not a number of seconds: {0}")]
    InvalidIntrospectTimeout(String),
    #[error("INFERENCE_ROUTER_DATABASE_URL (or DATABASE_URL) is required to serve")]
    MissingDatabaseUrl,
    #[error("INFERENCE_ROUTER_BUDGET_GATE_MODE is not one of off/shadow/enforce: {0}")]
    InvalidBudgetGateMode(String),
    #[error("INFERENCE_ROUTER_POLICY_MODE is not one of off/store/on: {0}")]
    InvalidPolicyMode(String),
    #[error(
        "INFERENCE_ROUTER_BUDGET_GATE_TIMEOUT_SECONDS is not a positive number of seconds: {0}"
    )]
    InvalidBudgetTimeout(String),
    #[error("INFERENCE_ROUTER_BUDGET_GATE_RESERVE_TTL_SECONDS is not 5..=600: {0}")]
    InvalidReserveTtl(String),
    #[error("INFERENCE_ROUTER_POLICY_CACHE_TTL_SECONDS is not a number in (0, 60]: {0}")]
    InvalidPolicyCacheTtl(String),
    #[error("INFERENCE_ROUTER_POLICY_EPOCH_POLL_SECONDS is not a number in (0, 5]: {0}")]
    InvalidPolicyEpochPoll(String),
    #[error("INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE must be at least 1: {0}")]
    InvalidWhoamiRateLimit(String),
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

        let require_caller_license = non_empty(lookup("INFERENCE_ROUTER_REQUIRE_CALLER_LICENSE"))
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        let introspect_mode = match non_empty(lookup("INFERENCE_ROUTER_INTROSPECT_MODE")) {
            Some(raw) => Mode::parse(&raw).ok_or(ConfigError::InvalidIntrospectMode(raw))?,
            None if require_caller_license => Mode::Enforce,
            None => Mode::Off,
        };
        let introspect_url =
            non_empty(lookup("INFERENCE_ROUTER_LICENSE_INTROSPECT_URL")).unwrap_or_default();
        let introspect_timeout_seconds: f64 =
            match non_empty(lookup("INFERENCE_ROUTER_LICENSE_INTROSPECT_TIMEOUT")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidIntrospectTimeout(raw.clone()))?,
                None => 2.0,
            };
        let introspect_fail_open =
            non_empty(lookup("INFERENCE_ROUTER_LICENSE_INTROSPECT_FAIL_OPEN"))
                .is_none_or(|value| value.eq_ignore_ascii_case("true"));

        let metering_redis_url = non_empty(lookup("INFERENCE_ROUTER_METERING_REDIS_URL"));
        let metering_costs_enabled = non_empty(lookup("INFERENCE_ROUTER_METERING_COSTS_ENABLED"))
            .is_none_or(|value| value.eq_ignore_ascii_case("true"));
        // The reference default; the live deployment overrides it with `billing:costs`. A fresh
        // deploy must never write to the retired `metering:costs` name.
        let metering_costs_stream = non_empty(lookup("INFERENCE_ROUTER_METERING_COSTS_STREAM"))
            .unwrap_or_else(|| "improvement-loop:costs".to_owned());

        let budget_gate_mode = match non_empty(lookup("INFERENCE_ROUTER_BUDGET_GATE_MODE")) {
            Some(raw) => GateMode::parse(&raw).ok_or(ConfigError::InvalidBudgetGateMode(raw))?,
            None => GateMode::Off,
        };
        let budget_gate_billing_base_url = non_empty(lookup(
            "INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL",
        ))
        .unwrap_or_else(|| "http://billing-service.billing.svc.cluster.local:8157".to_owned());
        let budget_gate_timeout_seconds: f64 =
            match non_empty(lookup("INFERENCE_ROUTER_BUDGET_GATE_TIMEOUT_SECONDS")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidBudgetTimeout(raw.clone()))?,
                None => 0.75,
            };
        if budget_gate_timeout_seconds <= 0.0 || !budget_gate_timeout_seconds.is_finite() {
            return Err(ConfigError::InvalidBudgetTimeout(
                budget_gate_timeout_seconds.to_string(),
            ));
        }
        let budget_gate_reserve_ttl_seconds: i64 =
            match non_empty(lookup("INFERENCE_ROUTER_BUDGET_GATE_RESERVE_TTL_SECONDS")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidReserveTtl(raw.clone()))?,
                None => 60,
            };
        if !(5..=600).contains(&budget_gate_reserve_ttl_seconds) {
            return Err(ConfigError::InvalidReserveTtl(
                budget_gate_reserve_ttl_seconds.to_string(),
            ));
        }

        let policy_mode = match non_empty(lookup("INFERENCE_ROUTER_POLICY_MODE")) {
            Some(raw) => PolicyMode::parse(&raw).ok_or(ConfigError::InvalidPolicyMode(raw))?,
            None => PolicyMode::Off,
        };
        let policy_cache_ttl_seconds: f64 =
            match non_empty(lookup("INFERENCE_ROUTER_POLICY_CACHE_TTL_SECONDS")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidPolicyCacheTtl(raw.clone()))?,
                None => 15.0,
            };
        if policy_cache_ttl_seconds <= 0.0 || policy_cache_ttl_seconds > 60.0 {
            return Err(ConfigError::InvalidPolicyCacheTtl(
                policy_cache_ttl_seconds.to_string(),
            ));
        }
        let policy_epoch_poll_seconds: f64 =
            match non_empty(lookup("INFERENCE_ROUTER_POLICY_EPOCH_POLL_SECONDS")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidPolicyEpochPoll(raw.clone()))?,
                None => 5.0,
            };
        if policy_epoch_poll_seconds <= 0.0 || policy_epoch_poll_seconds > 5.0 {
            return Err(ConfigError::InvalidPolicyEpochPoll(
                policy_epoch_poll_seconds.to_string(),
            ));
        }

        let private_api_secret = non_empty(lookup("INFERENCE_ROUTER_INTERNAL_SECRET"));
        let public_host_suffixes =
            match non_empty(lookup("INFERENCE_ROUTER_INTERNAL_PUBLIC_HOST_SUFFIXES")) {
                Some(raw) => comma_list(Some(raw)),
                None => vec!["nonagentic.ai".to_owned()],
            };
        // The admin API's two token slots and its allowlists. The names are the live service's
        // (`Settings` uses `INFERENCE_ROUTER_` as the pydantic prefix), and the 503 body names
        // the FIELD names, exactly as the capture does.
        let keycloak_jwks_url = non_empty(lookup("INFERENCE_ROUTER_KEYCLOAK_JWKS_URL"));
        let portal_jwks_url = non_empty(lookup("INFERENCE_ROUTER_PORTAL_JWKS_URL"));
        let admin_azp_allowlist = comma_list(lookup("INFERENCE_ROUTER_ADMIN_AZP_ALLOWLIST"));
        let admin_jwt_issuers = comma_list(lookup("INFERENCE_ROUTER_ADMIN_JWT_ISSUERS"));
        let whoami_rate_limit_per_minute: u32 =
            match non_empty(lookup("INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE")) {
                Some(raw) => raw
                    .parse()
                    .map_err(|_| ConfigError::InvalidWhoamiRateLimit(raw.clone()))?,
                None => 60,
            };
        if whoami_rate_limit_per_minute < 1 {
            return Err(ConfigError::InvalidWhoamiRateLimit(
                whoami_rate_limit_per_minute.to_string(),
            ));
        }
        let inference_capture_allowed_callers =
            comma_list(lookup("INFERENCE_CAPTURE_ALLOWED_CALLERS"));
        // UNPREFIXED in the live manifest (it is shared with the workers deployment), so it is
        // read unprefixed here too.
        let commercial_platform_tenant_ids = comma_list(lookup("COMMERCIAL_PLATFORM_TENANT_IDS"));
        // The reference normalises the allowlist to lower case and does NOT validate the entries
        // as UUIDs at boot: an unparsable entry simply never matches.
        let auth_delegating_subjects = comma_list(lookup("AUTH_DELEGATING_SUBJECTS"))
            .into_iter()
            .map(|subject| subject.to_ascii_lowercase())
            .collect();

        Ok(Self {
            bind,
            database_url,
            providers_config_path,
            environment,
            docs_enabled,
            provider_slot_wait_seconds,
            introspect_mode,
            introspect_url,
            introspect_timeout_seconds,
            introspect_fail_open,
            metering_redis_url,
            metering_costs_enabled,
            metering_costs_stream,
            budget_gate_mode,
            budget_gate_billing_base_url,
            budget_gate_timeout_seconds,
            budget_gate_reserve_ttl_seconds,
            policy_mode,
            policy_cache_ttl_seconds,
            policy_epoch_poll_seconds,
            private_api_secret,
            public_host_suffixes,
            keycloak_jwks_url,
            portal_jwks_url,
            admin_azp_allowlist,
            admin_jwt_issuers,
            whoami_rate_limit_per_minute,
            inference_capture_allowed_callers,
            commercial_platform_tenant_ids,
            auth_delegating_subjects,
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

/// A comma-separated list, trimmed with blanks dropped — the reference's normalisation.
fn comma_list(value: Option<String>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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
    fn the_delegating_subject_allowlist_is_unprefixed_and_normalised() {
        assert!(
            parse(&[])
                .expect("defaults")
                .auth_delegating_subjects
                .is_empty()
        );
        let config = parse(&[(
            "AUTH_DELEGATING_SUBJECTS",
            " 8382F0B3-C082-4F97-8D47-120CE5A18E05 ,, 01a0dd02-d324-77bb-acd8-7c31fb6e95ec ",
        )])
        .expect("allowlist parses");
        assert_eq!(
            config.auth_delegating_subjects,
            vec![
                "8382f0b3-c082-4f97-8d47-120ce5a18e05".to_owned(),
                "01a0dd02-d324-77bb-acd8-7c31fb6e95ec".to_owned(),
            ]
        );
        // A prefixed alias is not read: the deployment sets the fleet name only.
        assert!(
            parse(&[(
                "INFERENCE_ROUTER_AUTH_DELEGATING_SUBJECTS",
                "8382f0b3-c082-4f97-8d47-120ce5a18e05"
            )])
            .expect("prefixed name is simply unknown")
            .auth_delegating_subjects
            .is_empty()
        );
    }

    #[test]
    fn the_internal_guard_config_matches_the_live_defaults() {
        let config = parse(&[]).expect("defaults");
        assert_eq!(
            config.public_host_suffixes,
            vec!["nonagentic.ai".to_owned()]
        );
        assert_eq!(config.whoami_rate_limit_per_minute, 60);
        assert!(config.inference_capture_allowed_callers.is_empty());

        let configured = parse(&[
            (
                "INFERENCE_ROUTER_INTERNAL_PUBLIC_HOST_SUFFIXES",
                ".nonagentic.ai, merqi.nl",
            ),
            ("INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE", "5"),
            ("INFERENCE_CAPTURE_ALLOWED_CALLERS", " fr , coding "),
        ])
        .expect("configured");
        assert_eq!(
            configured.public_host_suffixes,
            vec![".nonagentic.ai".to_owned(), "merqi.nl".to_owned()]
        );
        assert_eq!(configured.whoami_rate_limit_per_minute, 5);
        assert_eq!(
            configured.inference_capture_allowed_callers,
            vec!["fr".to_owned(), "coding".to_owned()]
        );

        assert_eq!(
            parse(&[("INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE", "0")]),
            Err(ConfigError::InvalidWhoamiRateLimit("0".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_WHOAMI_RATE_LIMIT_PER_MINUTE", "soon")]),
            Err(ConfigError::InvalidWhoamiRateLimit("soon".to_owned()))
        );
    }

    #[test]
    fn the_admin_auth_settings_use_the_live_prefixed_names() {
        let defaults = parse(&[]).expect("defaults");
        assert_eq!(defaults.keycloak_jwks_url, None);
        assert_eq!(defaults.portal_jwks_url, None);
        assert!(defaults.admin_azp_allowlist.is_empty());
        assert!(defaults.admin_jwt_issuers.is_empty());

        let configured = parse(&[
            (
                "INFERENCE_ROUTER_KEYCLOAK_JWKS_URL",
                "https://auth.example/realms/portal/certs",
            ),
            (
                "INFERENCE_ROUTER_PORTAL_JWKS_URL",
                "https://auth.example/realms/portal/certs",
            ),
            (
                "INFERENCE_ROUTER_ADMIN_AZP_ALLOWLIST",
                "sa-a, sa-b ,,provisioning-sa",
            ),
            (
                "INFERENCE_ROUTER_ADMIN_JWT_ISSUERS",
                "https://auth.example/realms/portal",
            ),
        ])
        .expect("admin settings parse");
        assert_eq!(
            configured.keycloak_jwks_url.as_deref(),
            Some("https://auth.example/realms/portal/certs")
        );
        assert_eq!(
            configured.portal_jwks_url.as_deref(),
            Some("https://auth.example/realms/portal/certs")
        );
        assert_eq!(
            configured.admin_azp_allowlist,
            vec!["sa-a", "sa-b", "provisioning-sa"]
        );
        assert_eq!(
            configured.admin_jwt_issuers,
            vec!["https://auth.example/realms/portal"]
        );

        // The unprefixed FIELD name is not an environment variable: the live deployment sets the
        // prefixed name only (pydantic's `env_prefix`).
        assert!(
            parse(&[("KEYCLOAK_JWKS_URL", "https://auth.example/certs")])
                .expect("unknown names are ignored")
                .keycloak_jwks_url
                .is_none()
        );
    }

    #[test]
    fn the_introspection_mode_follows_the_live_rules() {
        assert_eq!(parse(&[]).expect("defaults").introspect_mode, Mode::Off);
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_REQUIRE_CALLER_LICENSE", "true")])
                .expect("legacy flag")
                .introspect_mode,
            Mode::Enforce
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_INTROSPECT_MODE", "observe")])
                .expect("mode")
                .introspect_mode,
            Mode::Observe
        );
        // The explicit mode wins over the legacy flag.
        assert_eq!(
            parse(&[
                ("INFERENCE_ROUTER_INTROSPECT_MODE", "off"),
                ("INFERENCE_ROUTER_REQUIRE_CALLER_LICENSE", "true")
            ])
            .expect("mode wins")
            .introspect_mode,
            Mode::Off
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_INTROSPECT_MODE", "sometimes")]),
            Err(ConfigError::InvalidIntrospectMode("sometimes".to_owned()))
        );
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

    #[test]
    fn write_path_defaults_match_the_reference() {
        let config = parse(&[]).expect("defaults parse");
        assert_eq!(config.metering_redis_url, None);
        assert!(config.metering_costs_enabled);
        assert_eq!(config.metering_costs_stream, "improvement-loop:costs");
        assert_eq!(config.budget_gate_mode, GateMode::Off);
        assert_eq!(
            config.budget_gate_billing_base_url,
            "http://billing-service.billing.svc.cluster.local:8157"
        );
        assert_eq!(config.budget_gate_timeout_seconds, 0.75);
        assert_eq!(config.budget_gate_reserve_ttl_seconds, 60);
        assert_eq!(config.policy_mode, PolicyMode::Off);
        assert_eq!(config.policy_cache_ttl_seconds, 15.0);
        assert_eq!(config.policy_epoch_poll_seconds, 5.0);
        assert_eq!(config.private_api_secret, None);
        assert!(config.commercial_platform_tenant_ids.is_empty());
    }

    #[test]
    fn the_live_deployments_write_path_values_parse() {
        let config = parse(&[
            (
                "INFERENCE_ROUTER_METERING_REDIS_URL",
                "redis://redis:6379/0",
            ),
            ("INFERENCE_ROUTER_METERING_COSTS_ENABLED", "false"),
            ("INFERENCE_ROUTER_METERING_COSTS_STREAM", "billing:costs"),
            ("INFERENCE_ROUTER_BUDGET_GATE_MODE", "enforce"),
            (
                "INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL",
                "http://billing:8157",
            ),
            ("INFERENCE_ROUTER_POLICY_MODE", "on"),
            ("INFERENCE_ROUTER_INTERNAL_SECRET", "synthetic"),
            ("COMMERCIAL_PLATFORM_TENANT_IDS", "a, b ,,c"),
        ])
        .expect("live values parse");
        assert_eq!(
            config.metering_redis_url.as_deref(),
            Some("redis://redis:6379/0")
        );
        assert!(!config.metering_costs_enabled);
        assert_eq!(config.metering_costs_stream, "billing:costs");
        assert_eq!(config.budget_gate_mode, GateMode::Enforce);
        assert_eq!(config.budget_gate_billing_base_url, "http://billing:8157");
        assert_eq!(config.policy_mode, PolicyMode::On);
        assert_eq!(config.private_api_secret.as_deref(), Some("synthetic"));
        // Blank and empty entries are dropped, spaces trimmed.
        assert_eq!(config.commercial_platform_tenant_ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn the_write_path_options_fail_closed() {
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_BUDGET_GATE_MODE", "sometimes")]),
            Err(ConfigError::InvalidBudgetGateMode("sometimes".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_POLICY_MODE", "half")]),
            Err(ConfigError::InvalidPolicyMode("half".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_BUDGET_GATE_TIMEOUT_SECONDS", "0")]),
            Err(ConfigError::InvalidBudgetTimeout("0".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_BUDGET_GATE_RESERVE_TTL_SECONDS", "4")]),
            Err(ConfigError::InvalidReserveTtl("4".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_BUDGET_GATE_RESERVE_TTL_SECONDS", "601")]),
            Err(ConfigError::InvalidReserveTtl("601".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_POLICY_CACHE_TTL_SECONDS", "61")]),
            Err(ConfigError::InvalidPolicyCacheTtl("61".to_owned()))
        );
        assert_eq!(
            parse(&[("INFERENCE_ROUTER_POLICY_EPOCH_POLL_SECONDS", "6")]),
            Err(ConfigError::InvalidPolicyEpochPoll("6".to_owned()))
        );
        assert!(parse(&[("INFERENCE_ROUTER_BUDGET_GATE_MODE", "shadow")]).is_ok());
        assert!(parse(&[("INFERENCE_ROUTER_POLICY_MODE", "store")]).is_ok());
    }
}
