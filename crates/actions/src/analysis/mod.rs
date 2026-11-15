#![forbid(unsafe_code)]
//! `analyse.*` — the workspace analysis library, exposed as invocable actions.
//!
//! The arithmetic and the checks live in `crates/analysis`, which is a pure library: it opens no
//! connection, mints no id and reads no clock. This module is the persistence adapter and nothing
//! more — it loads a document's verified bytes through the existing `documents` path, hands them
//! to the library with the page's URL, and returns the report.
//!
//! `analyse.site` is `effect = read` and `idempotent = true`: it writes no row and emits no event,
//! so a replay returns the same report and the engine's idempotency claim is not needed to make
//! that true. Fetching is not done here either — `apps/research` owns the fetch contract, and a
//! caller that wants a live page scrapes it into a document first.

pub mod actions;
