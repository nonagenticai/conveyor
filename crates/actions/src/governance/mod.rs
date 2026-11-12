#![forbid(unsafe_code)]
//! The governance action surface: requirements, principles, decisions, catalogues, trace
//! links, project requests, quality scoring and search (S07).
//!
//! One lifecycle, three declarations: [`assets`] implements create/get/update/transition/list
//! **once** and binds it to `kind = requirement | principle | adr` at declaration, so the kind
//! is never an input and `requirements.*` cannot reach a principle (S07 §5.2). Every mutation
//! writes exactly one `revision` row in the same transaction (S07 §5.3) and its outbox event
//! through [`crate::board::events::append_raw_event`].
//!
//! Nothing here speaks SQL directly — the queries live in `crates/store` (CONTRACT §1) — and
//! every id is resolved in the caller's tenant before a write, because a single-column FK does
//! not enforce tenancy (CONTRACT §2).

pub mod assets;
pub mod catalogues;
pub mod quality;
pub mod requests;
pub mod search;
pub mod trace;
pub mod views;

pub use views::*;

/// The problem mappers live in `control` and are re-exported rather than copied: one place
/// decides how a store failure, a parent miss or a refused precondition renders, so governance
/// refuses exactly like the control plane (CONTRACT §14 — one rule, not two).
pub use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};
