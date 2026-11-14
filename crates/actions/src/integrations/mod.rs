#![forbid(unsafe_code)]
//! S09's boundary to things the conveyor does not own (S09 §5.1).
//!
//! [`objects`] — MinIO, the one place bytes live. The conveyor is the sole authorizer of every byte it
//! serves back; the store credential never reaches a client.
//!
//! [`ingest`] — the `strata_ingest` handoff behind `documents.ingest`. A transport, not an external action:
//! the job id it returns becomes a `document` column, which no passthrough action could write.
//!
//! External actions (workers, artifacts, billing, coding, strata, inference-router, licences) are declared as
//! hand-written `ActionSpec`s with `HandlerRef::External` — the derive macro has no external form, and the
//! engine's `invoke_external` performs the HTTP call with the spec's own timeout.
//!
//! [`inference`] — `completions.create` (S10 §5.5). The copilot's model, declared the same way: the engine
//! owns the timeout, the breaker and the audit row, so there is no second HTTP client.
//!
//! [`strata`] — `strata_rag` behind `knowledge.query` and `memory.consolidate` (S15): query, document
//! ingest, source prune and platform-pack leaves. A transport, not an external action.

pub mod external;
pub mod github;
pub mod inference;
pub mod ingest;
pub mod objects;
pub mod strata;
