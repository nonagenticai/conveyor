#![forbid(unsafe_code)]
//! The procedure (SOP) surface's request and response documents.
//!
//! A procedure is the judge contract a process step is measured against (CONTRACT §21, SPEC D28):
//! `definition_of_ready`, `definition_of_done`, `acceptance_criteria`, `inspection` and `outputs`.
//! The three documents that decide whether a version may publish are carried as JSON, not as typed
//! structs, because their validation is `conveyor_domain::procedure::validate_publish` — one
//! validator, one error vocabulary, and a `422 procedure.inspection` that names every finding.
//!
//! `version` is the **family** version and `lock_version` is the optimistic-concurrency counter
//! `If-Match` compares; they are separate members for the same reason they are on a process
//! (migration `0033`): presenting the family version as a precondition would silently pass, because
//! an edit does not move it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::views::PageIn;

/// `procedures.create` input.
///
/// An absent member takes the column's own default — `{}` for the two definitions and the
/// inspection, `[]` for the criteria and the outputs — so a draft may be started before it is
/// judged. Publishing is where an empty `inspection` is refused.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureCreateIn {
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The definition of ready (inputs and preconditions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_of_ready: Option<Value>,
    /// The definition of done (outputs and checks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_of_done: Option<Value>,
    /// The criteria the judge reads; a non-empty array is required at publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<Value>,
    /// `{kind: laya|eval|checker, …}` — the judge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspection: Option<Value>,
    /// The user-facing outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Value>,
}

/// `procedures.update` input. Drafts only; an omitted member keeps its value.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureUpdateIn {
    /// The version to edit.
    pub procedure_id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The new definition of ready, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_of_ready: Option<Value>,
    /// The new definition of done, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_of_done: Option<Value>,
    /// The new acceptance criteria, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<Value>,
    /// The new inspection, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspection: Option<Value>,
    /// The new outputs, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Value>,
}

/// `procedures.get` input: exactly one of `procedure_id` or `key`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureGetIn {
    /// One version, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_id: Option<Uuid>,
    /// A version family, by key. Alone this resolves the published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// With `key`: an exact version. Not accepted with `procedure_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// `procedures.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureListIn {
    /// `draft | published | retired`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `procedures.publish` / `.retire` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureIdIn {
    /// The draft to publish, or the published version to retire.
    pub procedure_id: Uuid,
}

/// A procedure version as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProcedureOut {
    /// The row id.
    pub id: Uuid,
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// Free text.
    pub description: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The version within the family (never the `ETag`).
    pub version: i64,
    /// The optimistic-concurrency counter the `ETag` carries.
    pub lock_version: i64,
    /// The definition of ready.
    pub definition_of_ready: Value,
    /// The definition of done.
    pub definition_of_done: Value,
    /// The criteria the judge reads.
    pub acceptance_criteria: Value,
    /// `{kind: laya|eval|checker, …}`.
    pub inspection: Value,
    /// The user-facing outputs.
    pub outputs: Value,
}
