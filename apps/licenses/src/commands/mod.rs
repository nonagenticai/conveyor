//! One-shot operator commands: run once, print one JSON line, exit with a code.
//!
//! These replace the pinned Python service's `scripts/*.py` CronJob entries. The payload keys and
//! the exit codes are the contract — the service has no Prometheus client and no audit table, so
//! `kubectl logs` IS the audit trail. Each command therefore keeps the script's key names rather
//! than inventing Rust-shaped ones.

pub mod expire_stale;
pub mod revoke_expired_tokens;

/// A command's verdict: the process exit code and the one object printed to stdout.
#[derive(Debug)]
pub struct RunOutcome {
    pub code: i32,
    pub payload: serde_json::Value,
}
