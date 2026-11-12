#![forbid(unsafe_code)]
//! The governance action surface's request and response documents (S07).
//!
//! Every type derives `serde` **and** `schemars::JsonSchema` from the same fields, so the
//! input validator, the OpenAPI operation and the MCP tool cannot disagree (S04 §5.1).
//!
//! The shared envelopes are CONTRACT §6's and are re-exported, never re-specified: [`PageIn`]
//! / [`Page`] from [`crate::views`], [`IdIn`] from the board surface that first shipped it.
//!
//! Ids on the wire are strings (an output document is a representation); ids in an input are
//! `Uuid`, so a malformed id is a schema error at the edge rather than a string the store has
//! to parse.
//!
//! **`kind` is never an input.** `requirements.*`, `principles.*` and `decisions.*` share
//! these documents and are bound to their `AssetKind` at declaration (S07 §5.2), so there is
//! no field here that could point a requirement action at a principle.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use crate::board::views::IdIn;
pub use crate::views::{Page, PageIn};

// ── governed assets: requirements, principles, ADRs ─────────────────────────────────────

/// `.create` — the fields a new asset may carry. `key` is derived from the title when absent.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AssetCreateIn {
    /// The stable key within the kind; derived from the title when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The title (required).
    pub title: String,
    /// The body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Acceptance criteria; an array of `{id, …}` objects is diffed by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<Value>,
    /// The owning project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The accountable user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<Uuid>,
    /// The delivery phase code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_code: Option<String>,
    /// Links to other entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Value>,
    /// The review lens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<Value>,
    /// Kind-specific fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `.update` — the editable fields. `If-Match` is the precondition; it is never a body field.
///
/// Every field is optional and an absent one is left alone; `Some(null)` is not expressible
/// for the nullable columns, which is deliberate — clearing `project_id` is a `transition`,
/// not an edit.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AssetUpdateIn {
    /// The asset to edit, resolved under the caller's tenant.
    pub id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The new acceptance criteria (a whole-array replacement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<Value>,
    /// The new links object (a whole-object replacement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Value>,
    /// The new review lens (a whole-object replacement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<Value>,
    /// The new kind-specific fields (a whole-object replacement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// The new owning project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The new accountable user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<Uuid>,
    /// The new delivery phase code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_code: Option<String>,
}

/// `.transition` — a status change. No `If-Match`: a transition is status-guarded (CONTRACT §3).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssetTransitionIn {
    /// The asset to move.
    pub id: Uuid,
    /// `draft | in_review | approved | superseded | archived`.
    pub to: String,
    /// The successor that replaces this asset; required when `to = superseded`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<Uuid>,
}

/// `.list` — the filters plus the shared page envelope. The kind is the action's, not a filter.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AssetListIn {
    /// Only assets in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only assets of this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// `requirements.compare` — two revision sequence numbers of one asset.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssetCompareIn {
    /// The asset both revisions belong to.
    pub id: Uuid,
    /// The older revision's `seq`.
    pub from: i32,
    /// The newer revision's `seq`.
    pub to: i32,
}

/// One governed asset as every asset action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssetOut {
    /// The asset id.
    pub id: String,
    /// `requirement | principle | adr`.
    pub kind: String,
    /// The stable key within the kind.
    pub key: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// `draft | in_review | approved | superseded | archived`.
    pub status: String,
    /// The optimistic-concurrency version; also the latest revision's `seq`.
    pub version: i64,
    /// The asset this one replaces.
    pub supersedes_id: Option<String>,
    /// The asset that replaced this one.
    pub superseded_by_id: Option<String>,
    /// The owning project.
    pub project_id: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<String>,
    /// The delivery phase code.
    pub phase_code: Option<String>,
    /// Acceptance criteria.
    pub acceptance: Value,
    /// Links to other entities.
    pub links: Value,
    /// The review lens.
    pub lens: Value,
    /// Kind-specific fields.
    pub meta: Value,
    /// When the asset was created.
    pub created_at: DateTime<Utc>,
    /// When the asset last changed.
    pub updated_at: DateTime<Utc>,
}

/// One revision, as `get` lists the asset's history.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RevisionMetaOut {
    /// The revision sequence; equal to the asset's version at that point.
    pub seq: i32,
    /// `created | updated | status:<to>`.
    pub label: String,
    /// `draft | published | superseded`.
    pub status: String,
    /// The author of the mutation that wrote it.
    pub author_id: Option<String>,
    /// SHA-256 of the JCS-canonical snapshot.
    pub content_hash: String,
    /// When the revision was written.
    pub created_at: DateTime<Utc>,
}

/// `.get` — the asset plus its revision history, newest first (capped; governed-asset
/// revisions are unbounded in the table, so the representation is not).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssetDetailOut {
    /// The asset's own representation.
    #[serde(flatten)]
    pub asset: AssetOut,
    /// The asset's revisions, newest first.
    pub revisions: Vec<RevisionMetaOut>,
}

/// One field-level change in a revision diff. `field` is `"acceptance.<id>"` for a criterion
/// matched by its id, never `"acceptance[2]"` — a reorder is not a change.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FieldChangeOut {
    /// The field's dotted path.
    pub field: String,
    /// The value before, when the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Value>,
    /// The value after, when the field still exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,
}

/// `.compare` — the three buckets of a diff.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct RevisionDiffOut {
    /// Fields present only in the newer revision.
    pub added: Vec<FieldChangeOut>,
    /// Fields present only in the older revision.
    pub removed: Vec<FieldChangeOut>,
    /// Fields present in both with a different value.
    pub changed: Vec<FieldChangeOut>,
}

// ── trace links ─────────────────────────────────────────────────────────────────────────

/// `trace_links.create` — both ends, resolved in the caller's tenant.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TraceLinkCreateIn {
    /// `governed_asset | card`.
    pub from_type: String,
    /// The id in that end's table.
    pub from_id: Uuid,
    /// `governed_asset | card`.
    pub to_type: String,
    /// The id in that end's table.
    pub to_id: Uuid,
    /// `traces | realises | derives | satisfies | conflicts`; `traces` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// `trace_links.list` — either end may be the filter, so one action serves both directions.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TraceLinkListIn {
    /// The start end's type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_type: Option<String>,
    /// The start end's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_id: Option<Uuid>,
    /// The pointed-at end's type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_type: Option<String>,
    /// The pointed-at end's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_id: Option<Uuid>,
    /// The edge's meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// One trace edge.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TraceLinkOut {
    /// The edge id.
    pub id: String,
    /// The start end's type.
    pub from_type: String,
    /// The start end's id.
    pub from_id: String,
    /// The pointed-at end's type.
    pub to_type: String,
    /// The pointed-at end's id.
    pub to_id: String,
    /// The edge's meaning.
    pub kind: String,
    /// When the edge was written.
    pub created_at: DateTime<Utc>,
}

// ── catalogues ──────────────────────────────────────────────────────────────────────────

/// `catalogues.list` — one kind at a time, or every kind.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CatalogueListIn {
    /// The catalogue kind (`project_type`, `phase`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Include rows with `is_active = false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_inactive: Option<bool>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `catalogues.import` — the checked-in export, or one kind of it, with an optional dry run.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CatalogueImportIn {
    /// Import only this kind; every kind when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Count what would change without writing anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// One catalogue row.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CatalogueEntryOut {
    /// The row id.
    pub id: String,
    /// Which vocabulary this row belongs to.
    pub kind: String,
    /// The stable code within the kind.
    pub code: String,
    /// The display label.
    pub label: String,
    /// The kind-specific payload.
    pub meta: Value,
    /// Display order within the kind.
    pub sort: i32,
    /// Whether the row is offered.
    pub is_active: bool,
    /// The row version.
    pub version: i64,
}

/// `catalogues.import` — what the run created, updated and skipped.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CatalogueImportOut {
    /// Rows that did not exist.
    pub created: u64,
    /// Rows that existed with at least one changed field.
    pub updated: u64,
    /// Rows that existed and were already identical.
    pub skipped: u64,
    /// True when nothing was written (the counts are what WOULD change).
    pub dry_run: bool,
}

// ── project requests ────────────────────────────────────────────────────────────────────

/// `project_requests.create` — idempotent on `(tenant, external_ref)`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestCreateIn {
    /// The bridge's stable id for this request; unique per tenant.
    pub external_ref: String,
    /// Which system sent it.
    pub external_origin: String,
    /// The whole forwarded body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// The repository the request nominates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// `project_requests.update` — an edit, guarded by `If-Match`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestUpdateIn {
    /// The request to edit.
    pub id: Uuid,
    /// The new origin system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_origin: Option<String>,
    /// The new forwarded body (a whole-object replacement).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// The new nominated repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
}

/// `project_requests.list` — the status filter plus the shared page envelope.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestListIn {
    /// Only requests in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `project_requests.reject` — an optional reason, recorded in the payload.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestRejectIn {
    /// The request to reject.
    pub id: Uuid,
    /// Why it was rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `project_requests.promote` — the project to create from an approved request.
///
/// Only the fields the bridge does not already carry: the request's `payload` supplies the
/// rest. `title` and `key` default from the request's payload and derived key.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestMappingIn {
    /// The approved request to promote.
    pub id: Uuid,
    /// The project's display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The project's key; derived from the title when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The project type code (see `catalogues.list`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_code: Option<String>,
    /// The board template code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_template_code: Option<String>,
    /// The project's description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One project request.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestOut {
    /// The request id.
    pub id: String,
    /// The bridge's stable id.
    pub external_ref: String,
    /// Which system sent it.
    pub external_origin: String,
    /// The forwarded body.
    pub payload: Value,
    /// `draft | approved | rejected | promoted`.
    pub status: String,
    /// The project this request became.
    pub project_id: Option<String>,
    /// The nominated repository.
    pub repository_url: Option<String>,
    /// The row version (`ETag`).
    pub version: i64,
    /// When the request first arrived.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// `project_requests.promote` — the promoted request and the project it created.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRequestMappingOut {
    /// The request, now `promoted`.
    pub request: ProjectRequestOut,
    /// The project that was created.
    pub project_id: String,
    /// Its key.
    pub project_key: String,
}

// ── quality ─────────────────────────────────────────────────────────────────────────────

/// `quality.score` — the revision to score. Scoring is advisory and runs in the worker.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct QualityScoreIn {
    /// The revision to score.
    pub revision_id: Uuid,
}

/// `quality.score` — the job that was enqueued for the revision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct QualityScoreOut {
    /// The revision the job will score.
    pub revision_id: String,
    /// The enqueued `quality.score` job; absent when an identical job was already pending.
    pub job_id: Option<String>,
    /// True when the row is queued (or was already queued).
    pub queued: bool,
}

/// One score evaluation, newest first.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct QualityEvaluationOut {
    /// The evaluation id.
    pub id: String,
    /// What the score is about (`revision` in S07).
    pub entity_type: String,
    /// The scored row's id.
    pub entity_id: String,
    /// The computed score (0–100); absent for a degraded evaluation.
    pub score: Option<f64>,
    /// The rubric version plus each raw and weighted value.
    pub criteria: Value,
    /// How the score was produced.
    pub evaluator_kind: String,
    /// When the score was recorded.
    pub created_at: DateTime<Utc>,
}

// ── search ───────────────────────────────────────────────────────────────────────────────

/// `search.query` — the query text, the arms to search and the governed-asset filters.
///
/// Only `limit` of the shared page envelope is honoured: ranked results carry no stable sort
/// key, so a cursor would be a lie and is refused rather than minted (see the action).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchQueryIn {
    /// The query text; trimmed, and 1–200 characters after trimming.
    pub q: String,
    /// Which arms to search — `governed_asset`, `card`, `catalogue_entry`; every arm when
    /// absent. A duplicate collapses; an empty array searches nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    /// Only governed assets of this kind (`requirement | principle | adr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Only governed assets in this status
    /// (`draft | in_review | approved | superseded | archived`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only governed assets of this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The shared pagination envelope; only `limit` is honoured.
    #[serde(flatten)]
    pub page: PageIn,
}

/// One ranked hit. `score` is comparable only within one response — the weights are the
/// contract, not the absolute values.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchHitOut {
    /// Which arm produced it: `governed_asset`, `card` or `catalogue_entry`.
    pub entity_type: String,
    /// The row id in that arm's table.
    pub id: String,
    /// The display text: an asset's title, a card's name, a catalogue entry's label.
    pub title: String,
    /// An asset's key or a catalogue entry's code; absent for a card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// An asset's kind or a catalogue entry's kind; absent for a card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// An asset's status, or `active` for a card or a catalogue entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The highlighted excerpt; absent for a catalogue entry, which has no body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// The ranking score; higher is better.
    pub score: f64,
}

// ── principle conflicts ─────────────────────────────────────────────────────────────────

/// `principles.list_conflicts` — the verdict filter plus the shared page envelope.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ConflictListIn {
    /// Only verdicts equal to this string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `principles.dismiss_conflict` — a dismissal is final; there is no un-dismiss.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConflictDismissIn {
    /// The conflict verdict to dismiss.
    pub id: Uuid,
    /// Why it was dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One persisted verdict between two principles.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConflictOut {
    /// The verdict id.
    pub id: String,
    /// The lower uuid of the pair.
    pub principle_a_id: String,
    /// The higher uuid of the pair.
    pub principle_b_id: String,
    /// The verdict text.
    pub verdict: String,
    /// Who dismissed it.
    pub dismissed_by: Option<String>,
    /// When it was dismissed.
    pub dismissed_at: Option<DateTime<Utc>>,
    /// When the verdict was recorded.
    pub created_at: DateTime<Utc>,
}
