//! The evidence-review family: the review, its imports, screening, flow and sealed exports.
//!
//! Ported from `evidence_review_service` at revision `bac4f66` (captured in
//! `docs/evidence/evidence-review-capture-2026-11-18.md`). The schema is
//! `migrations/0001_evidence_review.sql`.
//!
//! Three invariants decide every handler:
//!
//! * **a machine cannot decide** — [`crate::actor::Actor::require_human`];
//! * **reviewer B cannot see reviewer A** — blinded reads, own-assignment queues and cursors,
//!   `404` never `403`, and no aggregate while the caller owes a decision;
//! * **history is append-only** — the database triggers refuse `UPDATE`/`DELETE`; a correction is a
//!   superseding row, never an edit.

use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::problem::Problem;
use crate::state::Shared;

pub mod corpus;
pub mod evidence;
pub mod flow;
pub mod imports;
pub mod jobs;
pub mod reviews;
pub mod screening;
pub mod snapshots;

/// The one identifier allocation rule: UUIDv7, so ids sort by creation time.
pub fn new_id() -> uuid::Uuid {
    uuid::Uuid::now_v7()
}

/// `sha256:<lowercase hex>` over raw bytes.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("sha256:{hex}")
}

/// The canonical (RFC 8785) hash of a JSON value, in the spelling the schema's `content_hash`
/// columns use. Reuses the workspace's one canonicaliser rather than a second answer.
pub fn hash_value(value: &Value) -> String {
    format!("sha256:{}", conveyor_domain::compare::content_hash(value))
}

/// The pool, or a refusal that names the missing variable. An unbound session makes every RLS
/// policy match nothing, and "no rows" is indistinguishable from "no data" at the call site.
pub fn pool(state: &Shared) -> Result<&PgPool, Problem> {
    state.pool.as_ref().ok_or_else(|| {
        Problem::unexpected(
            "the evidence-review store is not configured (DATABASE_URL); \
             this route cannot read or write review data",
        )
    })
}

/// The caller's review visibility, evaluated per row: the creator is the lead of their own
/// review, and any other caller must be able to see the review through its tenant scope.
pub const REVIEW_NOT_FOUND: &str = "no such review";
