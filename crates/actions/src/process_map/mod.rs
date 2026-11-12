#![forbid(unsafe_code)]
//! The process map surface: `process_maps.get` · `.list`.
//!
//! Two **read** actions and nothing else. The map is the user-facing DAG (SPEC D30, CONTRACT §21):
//! declarative, never executed, never able to reorder a process step. Authoring is deliberately
//! absent from this slice — the store (`conveyor_store::process_map`) already holds the draft/publish
//! state machine the authoring actions will call, and no client can reach it until those actions
//! exist.
//!
//! A missing row answers the shared `not_found`: that is what every other read family answers and
//! what a generic client expects.

use chrono::{DateTime, Utc};
use conveyor_store::process_map::{MapRecord, MapStoreError};
use uuid::Uuid;

use crate::invoke::ActionError;
use crate::spec::FieldError;

pub mod maps;
pub mod views;

pub use views::*;

/// One map as every read returns it.
pub(crate) fn map_out(record: &MapRecord) -> MapOut {
    MapOut {
        id: record.id,
        key: record.key.clone(),
        title: record.title.clone(),
        status: record.status.clone(),
        version: record.version,
        lock_version: record.lock_version,
        project_type_id: record.project_type_id,
        definition: record.definition.clone(),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// A `422` on one field of the caller's document.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// A row that does not exist in the caller's tenant.
pub(crate) fn not_found() -> ActionError {
    ActionError::resource_not_found("process map")
}

/// Parse a `process_map.status` from the wire.
pub(crate) fn parse_status(raw: &str) -> Result<&'static str, ActionError> {
    match raw {
        "draft" => Ok("draft"),
        "published" => Ok("published"),
        "retired" => Ok("retired"),
        _ => Err(invalid(
            "status",
            "invalid_value",
            "must be one of draft, published, retired",
        )),
    }
}

/// A map-store failure.
pub(crate) fn map_error(error: MapStoreError, action: &str) -> ActionError {
    match error {
        MapStoreError::NotFound => not_found(),
        // Unreachable through the two read actions; mapped rather than panicked so a future write
        // action cannot inherit a silent arm.
        MapStoreError::DraftExists => ActionError::new(
            409,
            "process_map.draft_exists",
            "this key already has a draft for this project type",
        ),
        MapStoreError::NotDraft(status) => ActionError::new(
            409,
            "process_map.not_draft",
            format!("the process map is {status}, not a draft"),
        ),
        MapStoreError::NotPublished => ActionError::new(
            409,
            "process_map.not_published",
            "only a published process map can be retired",
        ),
        MapStoreError::ProjectTypeNotFound => invalid(
            "project_type_id",
            "invalid_value",
            "the referenced project type does not exist in this tenant",
        ),
        MapStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "the process map moved to version {current} before this call"
        )),
        MapStoreError::Database(error) => {
            tracing::error!(%error, action, "process map store failure");
            ActionError::internal()
        }
    }
}

/// The cursor a `process_maps.list` page hands back: `["<created_at>", "<id>"]`.
///
/// The store's sort key is `(created_at DESC, id DESC)`, so the pair — not the timestamp alone — is
/// what makes the cursor stable when two rows share a millisecond.
pub(crate) fn cursor_of(record: &MapRecord) -> String {
    serde_json::to_string(&(record.created_at, record.id)).unwrap_or_else(|_| String::from("[]"))
}

/// Decode a `process_maps.list` cursor.
pub(crate) fn decode_cursor(
    cursor: Option<&str>,
) -> Result<Option<(DateTime<Utc>, Uuid)>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<(DateTime<Utc>, Uuid)>(raw)
        .map(Some)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_cursor",
                "a process_maps cursor is the [created_at, id] pair the last page returned",
            )
        })
}
