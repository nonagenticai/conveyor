//! JSON logs to stdout (the estate's log pipeline parses JSON lines). Level from
//! `RUST_LOG`, default `info`.

use tracing_subscriber::EnvFilter;

/// Install the global subscriber. Idempotent enough for a process that calls it once.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(false)
        .try_init();
}
