//! conveyor-scientific — the evidence tools the conveyor calls.
//!
//! The wire contract is fixed by `contract/openapi.lock.json`, captured from the old Python
//! service at commit `7d99059` and filtered by `contract/drop.jq`: the `/api/topology` and
//! `/api/explain` surfaces are **not** carried, because their only upstream is the graph adapter
//! and that store was destroyed on 2026-11-01. The lock is embedded and never regenerated from
//! this implementation — `--check-lock` builds the document from the router and compares it to the
//! capture, which is the only direction that can fail for the reason the check exists.
#![forbid(unsafe_code)]

pub mod auth;
pub mod cache;
pub mod clients;
pub mod config;
pub mod contract;
pub mod pipeline;
pub mod router;
pub mod state;
pub mod stats;

/// The service name reported by `/status` and the `service` log field.
pub const SERVICE_NAME: &str = "scientific";
