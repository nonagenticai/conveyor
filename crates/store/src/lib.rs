#![forbid(unsafe_code)]
//! The only crate that speaks SQL (CONTRACT §1).
//!
//! Tenant scoping comes in two shapes, and the shape is visible in the signature:
//!
//! - **pool-first**: the function takes `&PgPool` and a [`TenantId`], and opens its own
//!   transaction with [`begin_scoped`] ([`jobs`], [`prune`], [`outbox::replay`],
//!   [`schedule`], [`licence`]). It commits before returning.
//! - **`_tx`**: the function takes an open `&mut PgConnection`, which the caller has already
//!   scoped with [`begin_scoped`] (or [`set_tenant`]), plus the [`TenantId`] it binds
//!   explicitly. The write and its outbox event therefore share the caller's transaction
//!   (CONTRACT §7) — the S03 domain modules are all this shape.
//!
//! Two reads are deliberately unscoped and say so in their own module docs: [`tenants`]
//! (`tenant` has no `tenant_id` and no RLS) and [`identity::resolve_identity`] (`app_user`
//! is global; the membership read runs under the SELECT-only `membership_self_read` policy).
//!
//! Every write that names a parent id resolves it in the caller's scope first — see
//! [`parent`]. A single-column FK does not enforce tenancy (CONTRACT §2).
//!
//! Queries are runtime-checked (`sqlx::query` / `query_as` with binds). The compile-time
//! `query!` macros need a live database (or an offline cache) at build time and are not
//! used anywhere in this workspace.
//!
//! Modules: [`outbox`] (event + doorbell), [`bus`] (the relay's unpublished-page cursor),
//! [`listen`] (the dedicated LISTEN connection), [`jobs`] (claim/fence/reclaim), [`schedule`] (cron → jobs under the advisory lock),
//! [`prune`] (retention, per tenant), [`audit`] (append-only), [`idempotency`] (POST replay),
//! [`licence`] (`app_licence`), [`tenants`] (the unscoped enumeration the worker needs),
//! [`seed`] (the `migrate --dev-seed` fixtures), [`parent`] (tenant-scoped parent resolution).
//!
//! S03 adds the domain query modules: [`project`], [`board`], [`list`], [`card`], [`run`],
//! [`approval`], [`governance`], [`catalogue`], [`revision`], [`andon`].
//!
//! S05a adds [`position`] (exact midpoints, appends and the board advisory lock) and the
//! board satellites: [`comment`], [`task`] (checklists and items), [`custom_field`] (fields
//! and their per-card values) and [`label`] (labels and the `card_label` join).
//!
//! S06 adds [`channel`] (bindings and per-user preferences) and [`delivery`] (messages, the
//! per-recipient deliveries and the drainer's claim/mark path).
//!
//! S07 adds [`project_request`] (the CP→conveyor intake), [`quality`] (the append-only score
//! history and the rubric's trigram read), [`trace_link`] (the single writer of the
//! polymorphic trace edge) and [`search`] (the three ranked arms, in one statement).
//!
//! S08 Part A adds [`portfolio`] (the four portfolio registers) and [`toc`] (the TOC tree over
//! S07's `outline` / `outline_node`). Both ship as skeletons in wave A and hold no items yet.
//!
//! S19 adds [`eval`] — the instrument's store: the eval and its versions, the recomputed
//! instrument digest and the one-shot split lock, the write-only answer key ([`eval::cases`]),
//! the metrics ([`eval::metrics`]) and the driver's two writes ([`eval::results`]). The two
//! sign-off gates are ordinary `approval` rows carrying `eval_id`, so [`approval`] changes by one
//! nullable column and the readiness predicate lives in [`eval::evals`].

pub mod actions;
pub mod andon;
pub mod approval;
pub mod audit;
pub mod benchmark;
pub mod board;
// S13/CONTRACT §7: the bus cursor — the unpublished `event` rows and the mark that records a
// publication. The relay in `apps/server` is its only caller.
pub mod bus;
pub mod capability;
pub mod card;
pub mod catalogue;
pub mod channel;
pub mod comment;
pub mod copilot;
pub mod custom_field;
pub mod delivery;
pub mod document;
pub mod eval;
pub mod governance;
pub mod idempotency;
pub mod identity;
pub mod interaction;
// S16: the inference ledger — the engine's write, the keyset list and the usage aggregate.
pub mod inference;
pub mod invite;
pub mod jobs;
pub mod label;
pub mod licence;
pub mod list;
pub mod listen;
// S15: work-package memory — the hot rows and the derived digest.
pub mod memory;
pub mod migrate;
pub mod model;
pub mod outbox;
pub mod parent;
pub mod platform;
pub mod pool;
pub mod portfolio;
pub mod position;
pub mod process;
// The judge/park writes the step executor needs (0033's run_step.judge and the run tree).
pub mod process_judge;
// SPEC D29: the default process catalogue — `seed/processes/**`, parsed and applied.
pub mod process_seed;
// SPEC D28 / CONTRACT §21: the procedure (SOP) version family.
pub mod procedure;
// SPEC D30 / CONTRACT §21: the process map — the declarative DAG bound to a project type.
pub mod process_map;
pub mod project;
pub mod project_request;
// SPEC D27: the prompt catalogue — the recipe read by pin, and the facts a compile draws from.
pub mod prompt;
// SPEC D27: the built-in prompt texts, seed material for the catalogue.
pub mod prompts;
// S16: the prompt audit's rows, decisions and cadence helpers.
pub mod prompt_audit;
pub mod prune;
pub mod quality;
pub mod rbac;
pub mod repository;
pub mod retire;
pub mod revision;
// T0 (SPEC_V2 §4, §10.3): industry, job catalogue, immutable release, binding and the
// `organisation` snapshot section.
pub mod organisation;
pub mod run;
pub mod schedule;
pub mod scope;
pub mod search;
pub mod secret;
pub mod seed;
pub mod seed_benchmark;
pub mod seed_organisation;
pub mod seed_programme;
pub mod snapshot;
pub mod task;
pub mod tenants;
pub mod trace_link;
pub mod validation;
pub mod view;
pub mod webhook;
// S13: the workflow engine's rows — one document, its publish freeze, and the one-published-per-key
// rule the database owns. `run_step`'s trace writes land with the executor.
pub mod workflow;

pub use actions::{
    AuditRow, DepartmentRow, EventRowOut, InvocationRecord, JobRow, MemberRow, RegistryRow,
    ScheduleRowOut, TenantRow, VersionMismatch,
};
pub use andon::{AndonState, AndonStoreError};
pub use approval::{
    ApprovalStoreError, NewApproval, expire_due_tx, insert_approval_tx, record_decision_tx,
};
pub use audit::{AuditEntry, AuditError, record};
pub use board::{BoardMemberStoreError, BoardStoreError, NewBoard, NewCardType};
pub use card::{CardStoreError, ClaimGranted, NewCard};
pub use catalogue::{CatalogueStoreError, NewCatalogueEntry};
pub use channel::{
    CHANNEL_KINDS, ChannelBindingRecord, ChannelPreferenceRecord, ChannelStoreError,
    NewChannelBinding,
};
pub use comment::{CommentStoreError, NewComment};
pub use custom_field::{CustomFieldStoreError, NewCustomField};
pub use delivery::{
    DeliveryStoreError, InsertedDelivery, MAX_DELIVERY_BATCH, MessageRecord, NewDelivery,
    NewMessage, RecipientDelivery,
};
// `delivery::DELIVERY_MAX_ATTEMPTS` (S06's channel budget, 5) is deliberately NOT re-exported
// here: `webhook::DELIVERY_MAX_ATTEMPTS` (S05's webhook retry budget, 3) already owns the
// crate-root name, and two budgets under one name is how the wrong one gets used.
pub use governance::{GovernanceStoreError, NewAsset};
pub use idempotency::{Lookup, request_hash};
pub use identity::{Identity, MembershipRow, resolve_identity};
pub use jobs::{Job, claim, enqueue, fail, finish_dead, finish_succeeded};
pub use label::{LabelStoreError, NewLabel};
pub use licence::{LicenceRow, LicenceUpsert};
pub use list::{ListStoreError, NewList};
pub use listen::{EventListen, ListenError, probe_direct};
pub use migrate::{
    MIGRATOR, RUNTIME_GRANTS_SQL, RUNTIME_ROLE, StoreMigrateError, apply_runtime_grants,
    assert_runtime_role, migrate,
};
// The model store functions are called through `store::model::`; the import path's
// head-only writer is re-exported here too because it is the SECOND public freeze entry
// point beside `model::publish_model_tx` (S08 Part B2).
pub use model::write_head_revision_tx;
pub use outbox::{EventRow, NewEvent, OutboxError, ReplayScope, append, bounds, max_seq, replay};
pub use parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};
pub use pool::{ACQUIRE_TIMEOUT, MAX_CONNECTIONS, connect, connect_lazy, ping};
pub use project::{NewProgramme, NewProject, ProjectStoreError};
pub use revision::RevisionStoreError;
pub use run::{NewRun, RunStoreError};
pub use schedule::{
    Cron, CronError, DEFAULT_SCHEDULES, ScheduleRow, materialise, materialise_all, seed_defaults,
};
pub use scope::{begin_scoped, begin_scoped_read_only, set_tenant};
pub use search::{
    KEY_EXACT_BONUS, SearchArm, SearchHit, SearchRequest, SearchStoreError, W_RANK, W_TRGM,
    WORD_SIMILARITY_THRESHOLD,
};
pub use seed::{DEV_TENANTS, DEV_USERS, SeedError};
pub use task::{NewTask, NewTaskList, TaskStoreError};
pub use webhook::{
    DELIVER_KIND, DELIVERY_MAX_ATTEMPTS, DeliveryTarget, NewWebhook, WebhookRecord,
    WebhookStoreError, WebhookUpdate,
};

pub use conveyor_domain::TenantId;

use conveyor_domain::Version;

/// A row's `version` is `bigint NOT NULL DEFAULT 1`; the clamp keeps a read total even if a
/// row were hand-edited to 0, which no store path can produce.
pub(crate) fn version_of(raw: i64) -> Version {
    Version::new(raw.max(1) as u64).unwrap_or(Version::INITIAL)
}
