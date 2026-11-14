#![forbid(unsafe_code)]
//! The licensing service, absorbed: the captured contract, the Rust service and its database.
//!
//! The wire contract is fixed by `contract/openapi.lock.json`, captured from the live Python
//! service on 2026-11-14 (image `sha256:13093d39…`). `--check-lock` compares the served document
//! to that capture; this module tree never rewrites the capture from the implementation.
//!
//! The schema is `migrations/0001_baseline.sql`, the live `licenses` database's
//! `pg_dump --schema-only`; `contract/schema-diff.sh` applies it to a throwaway Postgres 18.6 and
//! diffs the result against `contract/schema.lock.sql`.

pub mod auth;
pub mod billing;
pub mod config;
pub mod contract;
pub mod db;
pub mod error;
pub mod router;
pub mod state;

/// The name the live service reports (and the captured `HealthResponse.service` carries).
pub const SERVICE_NAME: &str = "licensing-service";

/// The live service's defaults, reproduced when the environment does not override them.
pub const DEFAULT_VERSION: &str = "1.0.0";
pub const DEFAULT_ENVIRONMENT: &str = "production";
/// The container port the cluster Service targets.
pub const DEFAULT_PORT: u16 = 8544;
