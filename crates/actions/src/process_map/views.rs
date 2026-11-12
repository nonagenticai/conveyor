#![forbid(unsafe_code)]
//! The process map surface's request and response documents.
//!
//! The map is **read-only here** (CONTRACT §21): it is a declarative document the UI renders and the
//! PM plans from, and nothing in this slice authors or executes it. `definition` is therefore carried
//! whole as JSON — its shape is `conveyor_domain::process_map::MapDoc`
//! (`{schema, nodes, edges}`), and re-declaring it as typed structs would give the API a second copy
//! of a contract the validator already owns.
//!
//! A map is bound to a project type, and the binding is part of its identity: the same key may exist
//! for two project types, so `get` by key needs the project type as well as the key.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::views::PageIn;

/// `process_maps.get` input: one map by id, or by key and project type.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MapGetIn {
    /// One row, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_id: Option<Uuid>,
    /// The version family, by key. Needs `project_type_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The project type the map was drawn for. Needed with `key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_id: Option<Uuid>,
    /// With `key`: an exact version. Not accepted with `map_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// `process_maps.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct MapListIn {
    /// `draft | published | retired`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Only maps drawn for this project type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_id: Option<Uuid>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// A process map as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MapOut {
    /// The row id.
    pub id: Uuid,
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// `draft | published | retired`.
    pub status: String,
    /// The version within the family (never the `ETag`).
    pub version: i64,
    /// The optimistic-concurrency counter a future edit's `ETag` carries.
    pub lock_version: i64,
    /// The `catalogue_entry(kind='project_type')` the map is drawn for.
    pub project_type_id: Uuid,
    /// The `{schema, nodes, edges}` document.
    pub definition: Value,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// `process_maps.create` input: the next version of a key as a draft.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MapCreateIn {
    /// The version-family key.
    pub key: String,
    /// The display title.
    pub title: String,
    /// The project type the map is drawn for.
    pub project_type_id: Uuid,
    /// The `{schema, nodes, edges}` document.
    pub definition: Value,
}

/// `process_maps.update` input. Drafts only; `If-Match` carries the version.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MapUpdateIn {
    /// The version to edit.
    pub map_id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A new project type binding (a draft may be re-bound before it is published).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_id: Option<Uuid>,
    /// The replacement document, whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Value>,
}

/// `process_maps.publish` · `.retire` input: the draft or published row to move.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MapIdIn {
    /// The row.
    pub map_id: Uuid,
}
