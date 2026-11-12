#![forbid(unsafe_code)]
//! S15's capability-bound retrieval. `knowledge.query` is the only path a run uses; S09's
//! `retrieval.query` is the raw strata passthrough and is deliberately distinct.

pub mod query;
