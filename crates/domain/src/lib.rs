#![forbid(unsafe_code)]
//! Pure domain types. No IO, no `sqlx`, no `axum`: `store` and `auth` depend on this crate,
//! never the other way round (CONTRACT §1).
//!
//! The identifiers are newtypes over [`uuid::Uuid`] with their invariant checked at
//! construction, so an invalid state cannot reach SQL or a JSON body. `TenantId` is
//! re-exported as `conveyor_auth::TenantId` (S2-D1); it lives here so `store` does not have
//! to depend on `auth` — that edge would invert the dependency chain.
//!
//! S03 adds the fourteen aggregates' state machines:
//!
//! - [`board`] — programme, project, board, list, card type, card and `may_move`.
//! - [`run`] — the run state machine; terminal wins.
//! - [`approval`] — dual-control decisions, expiry and voiding.
//! - [`governance`] — one lifecycle for requirements, principles and ADRs, plus revision
//!   retention.
//! - [`catalogue`] — the vocabulary table's kinds and the project-type matrix.
//!
//! S06 adds [`channel`] — the delivery state machine (`pending → sent → delivered | failed |
//! expired`) that `delivery.send` is driven by.
//!
//! S07 adds [`compare`] — canonical JSON, the revision content hash and the id-aware
//! `RevisionDiff` — and [`quality`] — rubric v1 over an immutable revision.
//!
//! S08 Part B adds [`model`] — the one model snapshot and the deterministic layout both
//! `model_views.generate` and the renderer use — plus [`profile`] (the curated, versioned
//! ArchiMate 3.2 / UML 2.5.1 element and relationship tables) and [`validate`] (the graph
//! checks `models.validate` runs in the worker).
//!
//! S15 adds [`capability`] — the aggregate's status machine, the executor spec's caps and
//! `prompt_ref` grammar, the `requires` relation graph and the readiness projection. It is
//! configuration only: the loop is S13's `agent.run`.
//!
//! S16 adds [`inference`] — the ledger's derived fields (the usage parse, the four-way cache
//! class and the report's arithmetic) — and [`prompt_audit`] — the audit's statuses, the
//! finding's decision machine and the tenant's cadence policy.
//!
//! Every transition returns a new value or an error; there is no mutating method. The store
//! layer writes what these functions agreed to, and the database refuses the vocabulary.

mod approval;
pub mod benchmark;
mod board;
// S15: the capability aggregate's status machine, executor config validation and relation graph.
pub mod capability;
mod catalogue;
pub mod channel;
pub mod compare;
pub mod copilot;
pub mod eval;
mod governance;
mod ids;
// S16: the inference ledger's derived fields and the usage report's arithmetic.
pub mod inference;
// S15: the work-package memory vocabularies and the digest a consolidation derives.
pub mod memory;
pub mod model;
// T0 (SPEC_V2 §4.5, D61): the internal Executor seam — the kind vocabulary, the two live
// implementations' validation contract, and the registry. Behaviour is T2's.
pub mod executor;
// T0 (SPEC_V2 §4.1–§4.4): the organisation spine's release hash and binding vocabulary.
pub mod organisation;
pub mod plan;
pub mod process;
// SPEC D28 / CONTRACT §21: the procedure (SOP) a step is judged against.
pub mod procedure;
pub mod process_map;
pub mod profile;
// SPEC D27: a prompt compiled at run time from the process, capability and project/card facts.
pub mod prompt;
// S16: the prompt audit's statuses, decisions, cadence policy and finding vocabulary.
pub mod prompt_audit;
pub mod quality;
mod run;
pub mod template;
pub mod validate;

pub use approval::{Approval, ApprovalError, ApprovalStatus, DeciderKind, Decision, DecisionKind};
pub use board::{
    ArchiveError, Board, Card, CardType, List, ListKind, Move, MoveError, Programme, Project,
    may_move,
};
pub use catalogue::{
    CardTypeRules, CatalogueEntry, CatalogueError, CatalogueKind, Code, CodeError,
};
pub use channel::{Delivery, DeliveryError, DeliveryState, DeliveryStateError};
pub use eval::{
    CaseMaterial, CaseSplit, EvalError, EvalGate, EvalStatus, Instrument, MetricMaterial,
    MissingGate, Readiness, ReadinessFacts, RetryPolicy, SplitAssignment, SplitCase, SubjectInput,
    SubjectKind, TagCounts, Variant, draw_split, instrument_digest, means_within_noise,
    split_sha256,
};
pub use governance::{
    AssetKind, AssetStatus, GovernanceError, GovernedAsset, Revision, RevisionPolicy,
    RevisionStatus,
};
pub use ids::{Id, IdError, TenantId, Version};
pub use run::{Run, RunError, RunStatus};

pub use chrono::{DateTime, Utc};
pub use uuid::Uuid;

/// Every timestamp in the domain is an absolute instant in UTC (`timestamptz`).
///
/// A lease test is always `lease_expires_at < now()` evaluated by Postgres, never against a
/// Rust clock (data-arch §8 trap 6); this type only carries the value.
pub type Timestamp = DateTime<Utc>;
