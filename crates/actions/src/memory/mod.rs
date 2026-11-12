#![forbid(unsafe_code)]
//! S15's work-package memory actions.
//!
//! [`remember`] is the **only action** that writes memory; [`recall`] returns the hot rows (the
//! record) first, then the semantic digest hits derived from them. Consolidation is a job, not an
//! action — see `apps/server/src/memory_consolidate.rs`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use chrono::{DateTime, Utc};
use conveyor_domain::memory::{MemoryKind, MemorySource};
use conveyor_store::memory::MemoryRecord;

pub mod recall;
pub mod remember;

/// One memory row as an action returns it. Never carries a digest body: that is read from strata.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryOut {
    /// The row id.
    pub id: Uuid,
    /// The work package.
    pub card_id: Uuid,
    /// The run that learned it, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The capability the run executed under, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<Uuid>,
    /// `fact | attempt | decision | blocker | digest`.
    pub kind: String,
    /// `agent | human | system`.
    pub source: String,
    /// The text.
    pub body: String,
    /// The row this one supersedes, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<Uuid>,
    /// When a consolidation covered it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consolidated_at: Option<DateTime<Utc>>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

impl From<MemoryRecord> for MemoryOut {
    fn from(record: MemoryRecord) -> Self {
        Self {
            id: record.id,
            card_id: record.card_id,
            run_id: record.run_id,
            capability_id: record.capability_id,
            kind: record.kind.as_str().to_owned(),
            source: record.source.as_str().to_owned(),
            body: record.body,
            supersedes_id: record.supersedes_id,
            consolidated_at: record.consolidated_at,
            created_at: record.created_at,
        }
    }
}

/// The source a row records when the caller names none: the principal's kind, not a guess.
pub fn source_from_actor(actor_kind: &str) -> MemorySource {
    match actor_kind {
        "agent" => MemorySource::Agent,
        "service" => MemorySource::System,
        _ => MemorySource::Human,
    }
}

/// Parse an explicitly supplied kind. `None` is the caller's default, handled by the action.
pub fn parse_kind(value: Option<&str>) -> Result<Option<MemoryKind>, String> {
    value
        .map(|text| {
            MemoryKind::parse(text).ok_or_else(|| {
                format!("unknown memory kind {text:?}; expected fact|attempt|decision|blocker")
            })
        })
        .transpose()
}

/// Parse an explicitly supplied source.
pub fn parse_source(value: Option<&str>) -> Result<Option<MemorySource>, String> {
    value
        .map(|text| {
            MemorySource::parse(text).ok_or_else(|| {
                format!("unknown memory source {text:?}; expected agent|human|system")
            })
        })
        .transpose()
}

/// A free-form metadata object, or `{}`.
pub fn meta_or_empty(meta: Option<Value>) -> Value {
    match meta {
        Some(value) if value.is_object() => value,
        _ => Value::Object(serde_json::Map::new()),
    }
}
