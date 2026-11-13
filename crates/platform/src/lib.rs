//! conveyor-platform — the shared service kit.
//!
//! Deliberately small: configuration, tracing, a `/health` router and a database/migration
//! helper. Domain logic belongs to the service crate, never here, and no feature flags exist
//! until a second consumer needs one.
#![forbid(unsafe_code)]

pub mod config;
pub mod db;
pub mod health;
pub mod telemetry;
