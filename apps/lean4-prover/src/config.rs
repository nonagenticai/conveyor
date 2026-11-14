//! Environment-driven configuration.
//!
//! The variable names are the deployed contract: `gitops-production`
//! `clusters/strato/services/lean4-prover/configmap.yaml` sets exactly these,
//! and the multi-arch rule in `CLAUDE.md` is that every hardcoded path has a
//! Docker-compatible default here rather than a host-specific literal.
//!
//! A malformed value refuses to boot (the Python service's pydantic model did
//! the same). Silently falling back would mean a lane limit or timeout that
//! nobody set and nobody notices.
//!
//! Resolution goes through a lookup closure rather than `std::env` directly so
//! tests can supply a configuration without writing process-global state — an
//! env write in one test thread races the read in another.

use std::env;

/// Upper bound on any per-request Lean timeout, in seconds. A request over it
/// is rejected; the runner also clamps the configured default to it. Each Lean
/// run holds a `prove` admission slot for its whole duration.
pub const MAX_TIMEOUT_S: u64 = 120;

#[derive(Debug, Clone)]
pub struct Config {
    pub environment: String,
    pub enable_docs: bool,
    pub enable_auth: bool,
    pub license_introspect_url: String,
    /// Wall-clock budget for one licence introspection POST. 10 s is the fleet
    /// default (`LICENSE_INTROSPECT_TIMEOUT` in `w2v_common`): a tighter budget
    /// on a CPU-saturated host fires before the licences service replies and
    /// surfaces as a spurious 401.
    pub license_introspect_timeout_s: u64,
    pub cors_allow_origins: Vec<String>,
    pub version: String,
    pub service_name: String,
    pub bind_host: String,
    pub bind_port: u16,

    pub lean_binary: String,
    pub lake_binary: String,
    pub project_dir: String,
    pub timeout_s: u64,
    pub max_memory_mb: u64,
    pub elan_home: String,
    pub subprocess_path: String,

    pub lane_max_active: usize,
    pub lane_max_waiting: usize,
    pub lane_max_wait_s: f64,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Build a configuration from an arbitrary lookup. Production passes the
    /// environment; tests pass a map.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let raw = |key: &str| lookup(key).filter(|v| !v.trim().is_empty());
        let string = |key: &str, default: &str| {
            raw(key)
                .map(|v| v.trim().to_string())
                .unwrap_or_else(|| default.to_string())
        };
        let number = |key: &str, default: f64| -> Result<f64, String> {
            match raw(key) {
                None => Ok(default),
                Some(v) => v
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("{key}: {v:?} is not a number")),
            }
        };
        let bounded = |key: &str, default: u64, min: u64, max: u64| -> Result<u64, String> {
            let v = number(key, default as f64)?;
            if !v.is_finite() || v < min as f64 || v > max as f64 {
                return Err(format!("{key}: {v} is outside {min}..={max}"));
            }
            Ok(v.round() as u64)
        };

        let lane_max_wait_s = number("GPU_LANE_PROVE_MAX_WAIT_S", 30.0)?;
        if !lane_max_wait_s.is_finite() || !(0.0..=3600.0).contains(&lane_max_wait_s) {
            return Err(format!(
                "GPU_LANE_PROVE_MAX_WAIT_S: {lane_max_wait_s} is outside 0..=3600"
            ));
        }

        let cfg = Self {
            environment: string("ENVIRONMENT", "production"),
            enable_docs: parse_bool("ENABLE_DOCS", &string("ENABLE_DOCS", "false"))?,
            enable_auth: parse_bool("ENABLE_AUTH", &string("ENABLE_AUTH", "true"))?,
            license_introspect_url: string("LICENSE_INTROSPECT_URL", ""),
            // Parsed as a float: the fleet's value has historically been written
            // "10.0", and an integer parse would refuse it.
            license_introspect_timeout_s: bounded("LICENSE_INTROSPECT_TIMEOUT", 10, 1, 120)?,
            cors_allow_origins: string("CORS_ALLOW_ORIGINS", "")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            version: string("LEAN4_PROVER_VERSION", "0.1.0"),
            service_name: string("LEAN4_PROVER_SERVICE_NAME", "lean4-prover"),
            bind_host: string("LEAN4_PROVER_BIND_HOST", "0.0.0.0"),
            bind_port: bounded("LEAN4_PROVER_BIND_PORT", 8812, 1, 65535)? as u16,

            // Absolute defaults, not bare `lean`/`lake`: the child environment
            // is replaced wholesale, and whether a bare name resolves against
            // the CHILD's PATH or the server's is exactly the ambiguity that
            // works locally and fails in the image. /opt/elan/bin is the Docker
            // layout, so this matches the multi-arch contract.
            lean_binary: string("LEAN4_PROVER_LEAN_BINARY", "/opt/elan/bin/lean"),
            lake_binary: string("LEAN4_PROVER_LAKE_BINARY", "/opt/elan/bin/lake"),
            project_dir: string("LEAN4_PROVER_PROJECT_DIR", "/lean-project"),
            timeout_s: bounded("LEAN4_PROVER_TIMEOUT", 120, 1, 600)?,
            max_memory_mb: bounded("LEAN4_PROVER_MAX_MEMORY_MB", 4096, 256, u32::MAX as u64)?,
            elan_home: string("LEAN4_PROVER_ELAN_HOME", "/opt/elan"),
            subprocess_path: string(
                "LEAN4_PROVER_SUBPROCESS_PATH",
                "/usr/local/bin:/usr/bin:/bin:/opt/elan/bin",
            ),

            lane_max_active: bounded("GPU_LANE_PROVE_MAX_ACTIVE", 2, 1, 1024)? as usize,
            lane_max_waiting: bounded("GPU_LANE_PROVE_MAX_WAITING", 8, 0, 65_536)? as usize,
            lane_max_wait_s,
        };

        // The Python service refused to boot on this pair; keeping the refusal
        // means a missing licence gate is a failed rollout, never an open door.
        if cfg.enable_auth && cfg.license_introspect_url.trim().is_empty() {
            return Err(
                "ENABLE_AUTH=true but LICENSE_INTROSPECT_URL is empty: lean4-prover cannot \
                 verify licences"
                    .to_string(),
            );
        }

        Ok(cfg)
    }

    pub fn docs_enabled(&self) -> bool {
        self.environment == "development" || self.enable_docs
    }

    /// Effective per-request timeout: the caller's override, else the
    /// configured default, clamped to [`MAX_TIMEOUT_S`].
    pub fn effective_timeout(&self, requested: Option<u64>) -> u64 {
        clamp_timeout(self.timeout_s, requested)
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(format!("{key}: {other:?} is not a boolean")),
    }
}

/// The timeout rule, independent of the environment so it stays unit-testable
/// without a process-wide env write racing other tests.
pub fn clamp_timeout(default_s: u64, requested: Option<u64>) -> u64 {
    requested.unwrap_or(default_s).min(MAX_TIMEOUT_S)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg_from(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    #[test]
    fn effective_timeout_is_clamped_to_the_hard_ceiling() {
        assert_eq!(clamp_timeout(30, Some(999)), MAX_TIMEOUT_S);
        assert_eq!(clamp_timeout(30, Some(42)), 42);
        assert_eq!(clamp_timeout(600, None), MAX_TIMEOUT_S);
        assert_eq!(clamp_timeout(30, None), 30);
    }

    #[test]
    fn boolean_parses_the_accepted_spellings() {
        for truthy in ["1", "true", "TRUE", "yes", "on"] {
            assert!(parse_bool("X", truthy).unwrap(), "{truthy}");
        }
        for falsy in ["0", "false", "no", "off"] {
            assert!(!parse_bool("X", falsy).unwrap(), "{falsy}");
        }
        assert!(parse_bool("ENABLE_DOCS", "maybe").is_err());
    }

    #[test]
    fn auth_without_an_introspect_url_refuses_to_boot() {
        let err = cfg_from(&[("ENABLE_AUTH", "true")]).unwrap_err();
        assert!(err.contains("LICENSE_INTROSPECT_URL"), "{err}");
    }

    #[test]
    fn auth_can_be_disabled_explicitly() {
        let cfg = cfg_from(&[("ENABLE_AUTH", "false")]).expect("no URL needed");
        assert!(!cfg.enable_auth);
    }

    #[test]
    fn a_malformed_lane_limit_refuses_to_boot() {
        let err = cfg_from(&[
            ("ENABLE_AUTH", "false"),
            ("GPU_LANE_PROVE_MAX_ACTIVE", "lots"),
        ])
        .unwrap_err();
        assert!(err.contains("GPU_LANE_PROVE_MAX_ACTIVE"), "{err}");
    }

    #[test]
    fn defaults_match_the_docker_layout() {
        // The multi-arch contract: these are the image's paths, never a host's.
        let cfg = cfg_from(&[("ENABLE_AUTH", "false")]).expect("defaults parse");
        assert_eq!(cfg.project_dir, "/lean-project");
        assert_eq!(cfg.elan_home, "/opt/elan");
        assert_eq!(cfg.lake_binary, "/opt/elan/bin/lake");
        assert_eq!(cfg.bind_port, 8812);
        assert_eq!(cfg.lane_max_active, 2);
        assert_eq!(cfg.lane_max_waiting, 8);
        assert_eq!(cfg.lane_max_wait_s, 30.0);
    }
}
