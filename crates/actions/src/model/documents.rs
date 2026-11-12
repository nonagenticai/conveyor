#![forbid(unsafe_code)]
//! The request and response documents of the `model.*` action set (S08 Part B §5.9).
//!
//! Every type here derives `serde` **and** `schemars::JsonSchema` from the same fields, so the
//! OpenAPI operation, the MCP tool and the runtime validator cannot disagree about a member
//! (S04's rule). Ids are `Uuid` on inputs and `String` on outputs, matching the rest of the
//! workspace: an input is validated by `serde`, and an output is a document a client reads.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use crate::board::views::IdIn;
use crate::views::PageIn;

/// The shared single-id input, re-exported so callers import one name from this family.
pub use crate::board::views::IdIn as ModelIdIn;

// ── model ───────────────────────────────────────────────────────────────────────────────

/// Create a model.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelCreateIn {
    /// The display title.
    pub title: String,
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The description.
    #[serde(default)]
    pub description: String,
    /// Attach the model to a project in this tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Free-form metadata, read whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// One validation run as `models.get` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ValidationRunOut {
    /// The run id.
    pub id: String,
    /// `ok`, `findings` or `error`.
    pub status: String,
    /// The profile's ruleset id.
    pub ruleset: String,
    /// The ruleset's own version.
    pub ruleset_version: String,
    /// The hash of the graph the run saw.
    pub content_hash: String,
    /// The whole findings document.
    pub findings: Value,
    /// When the run finished.
    pub created_at: DateTime<Utc>,
}

/// One model as every `models.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelOut {
    /// The model id.
    pub id: String,
    /// The owning project, when attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The display title.
    pub title: String,
    /// The description.
    pub description: String,
    /// The last published revision's `seq`; `0` before the first publish.
    pub head_revision: i64,
    /// The revision `models.publish` froze last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_revision_id: Option<String>,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
    /// The newest validation run — populated by `models.get`, absent elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_validation_run: Option<ValidationRunOut>,
}

/// Edit a model.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelUpdateIn {
    /// The model to edit.
    pub id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Attach the model to a project. Absent leaves the column unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Replace the metadata document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List models.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ModelListIn {
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
    /// Narrow to one project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
}

/// Publish the current graph as a new revision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelPublishIn {
    /// The model to publish.
    pub id: Uuid,
}

/// What a publish produced.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelPublishOut {
    /// The updated header, at its new version.
    pub model: ModelOut,
    /// The frozen revision.
    pub revision_id: String,
    /// The revision's `seq`.
    pub seq: i64,
    /// `revision.content_hash`.
    pub content_hash: String,
}

/// Compare two published revisions.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelCompareIn {
    /// The model both revisions belong to.
    pub model_id: Uuid,
    /// The older revision.
    pub from_revision_id: Uuid,
    /// The newer revision.
    pub to_revision_id: Uuid,
}

/// The difference between two revisions.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelCompareOut {
    /// The older revision.
    pub from_revision_id: String,
    /// The newer revision.
    pub to_revision_id: String,
    /// The older revision's content hash.
    pub from_content_hash: String,
    /// The newer revision's content hash.
    pub to_content_hash: String,
    /// The field-level diff: `added`, `removed`, `changed`, matched by id.
    pub diff: Value,
}

/// Queue a validation run for a model's current graph.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelValidateIn {
    /// The model to validate.
    pub id: Uuid,
}

/// What a validation request queued.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelValidateOut {
    /// False when an identical job was already live.
    pub queued: bool,
    /// The job id, when a new one was written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// The hash of the graph the job will validate.
    pub content_hash: String,
}

// ── element ─────────────────────────────────────────────────────────────────────────────

/// Create an element.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ElementCreateIn {
    /// The model it belongs to.
    pub model_id: Uuid,
    /// The profile kind, e.g. `application_component`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The long description.
    #[serde(default)]
    pub documentation: String,
    /// The containment parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_element_id: Option<Uuid>,
    /// Free-form metadata; `governed_asset_id` links to S03's asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// One element as every `model_elements.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ElementOut {
    /// The element id.
    pub id: String,
    /// The model it belongs to.
    pub model_id: String,
    /// The profile kind.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The long description.
    pub documentation: String,
    /// The containment parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_element_id: Option<String>,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Edit an element.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ElementUpdateIn {
    /// The element to edit.
    pub id: Uuid,
    /// The new kind — re-validated against the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The new documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Re-parent the element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_element_id: Option<Uuid>,
    /// Replace the metadata document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List a model's elements.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ElementListIn {
    /// The model whose elements to list.
    pub model_id: Uuid,
    /// Only elements of this kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

// ── relationship ────────────────────────────────────────────────────────────────────────

/// Create a relationship.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipCreateIn {
    /// The model it belongs to.
    pub model_id: Uuid,
    /// The profile relationship kind, e.g. `serving`.
    pub kind: String,
    /// The source element.
    pub source_id: Uuid,
    /// The target element.
    pub target_id: Uuid,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// One relationship as every `model_relationships.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipOut {
    /// The relationship id.
    pub id: String,
    /// The model it belongs to.
    pub model_id: String,
    /// The profile relationship kind.
    pub kind: String,
    /// The source element.
    pub source_id: String,
    /// The target element.
    pub target_id: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Edit a relationship.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipUpdateIn {
    /// The relationship to edit.
    pub id: Uuid,
    /// The new kind — re-validated against the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Replace the metadata document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List a model's relationships.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipListIn {
    /// The model whose relationships to list.
    pub model_id: Uuid,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

// ── view ────────────────────────────────────────────────────────────────────────────────

/// Create a view.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewCreateIn {
    /// The model it belongs to.
    pub model_id: Uuid,
    /// `diagram`, `archimate_view` or `uml_diagram`; `diagram` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The display name.
    pub name: String,
    /// Free-form metadata; `diagram_kind` names the UML diagram family.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// One view as every `model_views.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewOut {
    /// The view id.
    pub id: String,
    /// The model it belongs to.
    pub model_id: String,
    /// The view kind.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Edit a view.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewUpdateIn {
    /// The view to edit.
    pub id: Uuid,
    /// The new kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Replace the metadata document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List a model's views.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewListIn {
    /// The model whose views to list.
    pub model_id: Uuid,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// Lay out every element of a model onto one view.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewGenerateIn {
    /// The view to fill.
    pub view_id: Uuid,
}

/// What a generate produced.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ViewGenerateOut {
    /// The view, at its new version.
    pub view: ViewOut,
    /// How many objects were written.
    pub objects: i64,
}

// ── view object ─────────────────────────────────────────────────────────────────────────

/// Place an object on a view.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ObjectCreateIn {
    /// The view to place it on.
    pub view_id: Uuid,
    /// The element it draws.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_id: Option<Uuid>,
    /// The containing object, when nested on the canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_object_id: Option<Uuid>,
    /// The object kind; `element` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// `{x, y, w, h}` in canvas units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Value>,
    /// Free-form styling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Value>,
    /// A label override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The z-order / left-to-right order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
}

/// One object as every `view_objects.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ObjectOut {
    /// The object id.
    pub id: String,
    /// The view it is on.
    pub view_id: String,
    /// The element it draws.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// The containing object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_object_id: Option<String>,
    /// The object kind.
    pub kind: String,
    /// `{x, y, w, h}`.
    pub bounds: Value,
    /// Free-form styling.
    pub style: Value,
    /// A label override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The z-order.
    pub position: f64,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Move or restyle an object.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ObjectUpdateIn {
    /// The object to edit.
    pub id: Uuid,
    /// The new bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Value>,
    /// The new styling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Value>,
    /// The new label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The new z-order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
}

// ── view connection ─────────────────────────────────────────────────────────────────────

/// Draw a connector.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionCreateIn {
    /// The view to draw it on.
    pub view_id: Uuid,
    /// The relationship it draws, when it draws one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<Uuid>,
    /// The starting object.
    pub source_object_id: Uuid,
    /// The ending object.
    pub target_object_id: Uuid,
    /// The route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bendpoints: Option<Value>,
    /// Free-form styling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Value>,
}

/// One connector as every `view_connections.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionOut {
    /// The connection id.
    pub id: String,
    /// The view it is on.
    pub view_id: String,
    /// The relationship it draws.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    /// The starting object.
    pub source_object_id: String,
    /// The ending object.
    pub target_object_id: String,
    /// The route.
    pub bendpoints: Value,
    /// Free-form styling.
    pub style: Value,
    /// The optimistic-concurrency version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Reroute or restyle a connector.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionUpdateIn {
    /// The connection to edit.
    pub id: Uuid,
    /// The new route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bendpoints: Option<Value>,
    /// The new styling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Value>,
}

// ── the four read-only tools ────────────────────────────────────────────────────────────

/// Ask for a model's traceability report.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TraceabilityIn {
    /// The model to report on.
    pub model_id: Uuid,
}

/// One element's trace-link coverage.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ElementTraceOut {
    /// The element id.
    pub element_id: String,
    /// The element kind.
    pub kind: String,
    /// The element name.
    pub name: String,
    /// The governed asset it represents, when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governed_asset_id: Option<String>,
    /// Outgoing trace links.
    pub outgoing: i64,
    /// Incoming trace links.
    pub incoming: i64,
}

/// A model's traceability report.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TraceabilityOut {
    /// The model.
    pub model_id: String,
    /// Every element, ordered by id.
    pub elements: Vec<ElementTraceOut>,
    /// Elements with no trace link in either direction.
    pub untraced: i64,
}

/// Ask for a model's gaps.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GapsIn {
    /// The model to report on.
    pub model_id: Uuid,
}

/// One gap.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GapOut {
    /// Why it is a gap.
    pub code: String,
    /// What it is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// A human sentence.
    pub message: String,
}

/// A model's gaps.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GapsOut {
    /// The model.
    pub model_id: String,
    /// Every gap found.
    pub gaps: Vec<GapOut>,
}

/// Ask what an element's change would reach.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImpactIn {
    /// The model to walk.
    pub model_id: Uuid,
    /// The element to start from.
    pub element_id: Uuid,
}

/// One element reached by the walk.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImpactedOut {
    /// The element id.
    pub element_id: String,
    /// The element kind.
    pub kind: String,
    /// The element name.
    pub name: String,
    /// How many hops from the start (1 = directly related).
    pub distance: i64,
}

/// An element's impact set.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImpactOut {
    /// The model.
    pub model_id: String,
    /// The element the walk started from.
    pub element_id: String,
    /// Everything reached, nearest first, then by id.
    pub impacted: Vec<ImpactedOut>,
}

/// Ask for a model's completeness.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompletenessIn {
    /// The model to report on.
    pub model_id: Uuid,
}

/// A model's completeness report.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompletenessOut {
    /// The model.
    pub model_id: String,
    /// Elements in the model.
    pub elements: i64,
    /// Relationships in the model.
    pub relationships: i64,
    /// Elements with no relationship at either end.
    pub isolated: i64,
    /// Elements with no view object drawing them.
    pub undrawn: i64,
    /// Views with no objects.
    pub empty_views: i64,
    /// `0.0`–`1.0`: the share of elements that are related and drawn.
    pub score: f64,
}

/// The shared single-id input, kept for family modules that name it explicitly.
pub type ObjectIdIn = IdIn;
