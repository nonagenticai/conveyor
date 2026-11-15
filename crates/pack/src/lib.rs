#![forbid(unsafe_code)]
//! The public design pack: one data model, three consumers.
//!
//! - [`snapshot`] turns one tenant's rows into the deterministic snapshot files the CLI writes.
//! - [`outputs`] renders the publishable platform outputs — one page each plus the `outputs.js`
//!   index.
//! - [`live`] builds both in memory, in one read-only transaction, for the unauthenticated
//!   `/design` routes. The API crate calls it; `apps/server` only holds the CLI glue.
//!
//! The extraction exists because `crates/api` cannot depend on `apps/server` (the server already
//! depends on the API). One library keeps the live view and the CLI output byte-identical.

pub mod live;
pub mod outputs;
pub mod snapshot;
