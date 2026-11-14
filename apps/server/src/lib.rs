#![forbid(unsafe_code)]
//! The composition root. `main.rs` is a thin wrapper so integration tests can build the
//! exact state and router the binary serves.

pub mod admin;
pub mod benchmark_improvement;
pub mod benchmark_producer;
pub mod benchmark_worker;
// S13/CONTRACT §7: the outbox relay — unpublished `event` rows to Redis Streams.
pub mod bus_relay;
pub mod eval_audit;
pub mod eval_runner;
pub mod eval_worker;
pub mod export;
pub mod memory_consolidate;
pub mod prompt_audit_run;
pub mod publish;
pub mod publish_outputs;
pub mod sandbox;
pub mod seed;
pub mod shutdown;
pub mod snapshot;
pub mod telemetry;
pub mod worker;

pub use conveyor_api::state::AppStateError;
pub use conveyor_api::{ApiError, AppState, Config, ConfigError, Metrics, app, openapi};
