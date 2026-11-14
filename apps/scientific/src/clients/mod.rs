//! The three outbound dependencies the surviving routes need.
//!
//! Every one of them is a client this service owns: the old service reached the same three hosts
//! through the same shapes, and `contract/openapi.lock.json` is where the answers are pinned.
//!
//! - [`lean4`] — the prover, with its `503 gpu_busy` + `Retry-After` back-pressure contract.
//! - [`inference`] — the model router, used for translation and for the RAGAS judgements.
//! - [`objects`] — the MinIO/S3 bucket the proofs and the verification history live in.
//!
//! A client that is not configured is `None` at construction, not an error at call time: the old
//! service answered empty for an unconfigured store and failed loudly for an unconfigured prover,
//! and both behaviours are part of the contract.

pub mod embeddings;
pub mod inference;
pub mod lean4;
pub mod objects;
