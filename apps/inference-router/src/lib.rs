//! conveyor-inference-router — the workspace's OpenAI-compatible inference router.
//!
//! The wire contract is fixed by `contract/openapi.lock.json`, captured from the live service at
//! image `sha256:723671a9…` (26 paths, 35 schemas). The lock is embedded and **never regenerated
//! from this implementation**: `--check-lock` builds the document from the router and compares it
//! to the capture, so the check can fail for the reason it exists.
#![forbid(unsafe_code)]

pub mod config;
pub mod contract;
pub mod router;

/// The service name reported by `/` and the `service` log field.
pub const SERVICE_NAME: &str = "inference-router";
