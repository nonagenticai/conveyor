//! The wire shapes for `workflows.*` — one document per version, keyed and pageable.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use conveyor_store::workflow::workflow::WorkflowRecord;

use crate::views::PageIn;

/// A workflow definition as the API returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowOut {
    /// The row id this version lives at.
    pub workflow_id: Uuid,
    /// The stable name (`cap.x`).
    pub key: String,
    /// Display title.
    pub title: String,
    /// `draft` | `published` | `retired`.
    pub status: String,
    /// The row's dense version counter.
    pub version: i64,
    /// The whole graph document.
    pub definition: Value,
    /// The frozen revision of the last publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_revision_id: Option<Uuid>,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation.
    pub updated_at: DateTime<Utc>,
}

impl From<WorkflowRecord> for WorkflowOut {
    fn from(record: WorkflowRecord) -> Self {
        Self {
            workflow_id: record.id,
            key: record.key,
            title: record.title,
            status: record.status,
            version: record.version,
            definition: record.definition,
            published_revision_id: record.published_revision_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

/// `workflows.create` — a new key starts at v1, a reused key forks `max(version) + 1`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowCreateIn {
    /// The stable name.
    pub key: String,
    /// Display title.
    pub title: String,
    /// The graph document; absent means the empty graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
}

/// `workflows.update` — draft only; the document is replaced, never merged.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowUpdateIn {
    /// The row this edit targets.
    pub workflow_id: Uuid,
    /// A new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A replacement document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
}

/// `workflows.publish` and `workflows.retire` — one row, no other input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowRefIn {
    /// The row to act on.
    pub workflow_id: Uuid,
}

/// `workflows.get` — by id, or by key (optionally at a version).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowGetIn {
    /// The row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<Uuid>,
    /// The stable name, when the caller does not know the row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// With `key`: read this version instead of the highest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// `workflows.list` — filter and page.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowListIn {
    /// Only this status (`draft | published | retired`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The cursor and page size.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `workflows.publish` returns the frozen revision beside the row.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowPublishOut {
    /// The now-published row.
    pub workflow: WorkflowOut,
    /// The `revision` row this publish froze.
    pub revision_id: Uuid,
    /// The revision's sequence — the workflow's version at publish time.
    pub revision_seq: i32,
    /// The SHA-256 of the document's canonical JSON.
    pub content_hash: String,
}
