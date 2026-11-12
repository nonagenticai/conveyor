#![forbid(unsafe_code)]
//! The capability aggregate's action surface: `capabilities.*` and `capability_agents.*`.
//!
//! `capability_agent` is **configuration, not a loop** (CONTRACT §17): these actions persist and
//! read the executor spec; the loop that consumes it is S13's `agent.run`, fed by S15 step 11.
//! Validation runs in two places on purpose — the domain refuses a bad body with a typed `422`
//! before the database's CHECK can surface it as a `500`.

use conveyor_domain::capability::{CapabilityError, CapabilityStatus};
use conveyor_store::capability::{
    AgentRecord, CapabilityRecord, CapabilityStoreError, KnowledgeRecord,
};
use conveyor_store::parent::ParentError;
use rust_decimal::prelude::ToPrimitive;

use crate::invoke::ActionError;
use crate::spec::FieldError;

pub mod agents;
pub mod capabilities;
pub(crate) mod readiness;
pub mod views;

pub use views::*;

/// The iteration ceiling a spec gets when the caller names none (CONTRACT §17: default 8, max 20).
pub(crate) const DEFAULT_ITERATIONS: i32 = 8;

/// One capability row as the wire returns it.
pub(crate) fn capability_out(record: &CapabilityRecord) -> CapabilityOut {
    CapabilityOut {
        id: record.id,
        code: record.code.clone(),
        label: record.label.clone(),
        description: record.description.clone(),
        status: record.status.clone(),
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// One knowledge binding as the wire returns it.
pub(crate) fn knowledge_out(record: &KnowledgeRecord) -> KnowledgeOut {
    KnowledgeOut {
        id: record.id,
        capability_id: record.capability_id,
        pack_key: record.pack_key.clone(),
        pack_version: record.pack_version,
        min_score: record.min_score.and_then(|score| score.to_f64()),
        required: record.required,
    }
}

/// One executor spec as the wire returns it.
pub(crate) fn agent_out(record: &AgentRecord) -> AgentOut {
    AgentOut {
        id: record.id,
        capability_id: record.capability_id,
        key: record.key.clone(),
        status: record.status.clone(),
        model_ref: record.model_ref.clone(),
        prompt_ref: record.prompt_ref.clone(),
        params: record.params.clone(),
        allowed_actions: record.allowed_actions.clone(),
        max_iterations: record.max_iterations,
        timeout_secs: record.timeout_secs,
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// A `422` naming one field.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Parse a status against the one three-value vocabulary.
pub(crate) fn parse_status(raw: &str) -> Result<CapabilityStatus, ActionError> {
    CapabilityStatus::parse(raw).ok_or_else(|| {
        invalid(
            "status",
            "invalid_value",
            "status must be draft, active or retired",
        )
    })
}

/// A domain refusal is the caller's input, never a `500`.
pub(crate) fn config_error(error: CapabilityError) -> ActionError {
    let field = match &error {
        CapabilityError::BlankModelRef => "model_ref",
        CapabilityError::IterationsOutOfRange(_) => "max_iterations",
        CapabilityError::TimeoutOutOfRange(_) => "timeout_secs",
        CapabilityError::BadPromptRef(_) => "prompt_ref",
        CapabilityError::BadActionName(_) | CapabilityError::DuplicateAction(_) => {
            "allowed_actions"
        }
        _ => "capability_agent",
    };
    invalid(field, "invalid_value", error.to_string())
}

/// A stale `If-Match` on an agent: the caller gets the row it should now be looking at.
pub(crate) fn agent_conflict(current: &AgentOut, moved: i64) -> ActionError {
    ActionError::precondition_with(
        serde_json::to_value(current).unwrap_or(serde_json::Value::Null),
        format!("the capability agent moved to version {moved} before this call"),
    )
}

/// Map a store refusal onto the wire. A `VersionMismatch` is special-cased by the one caller that
/// already holds the current representation; the fallback here still answers `412`, never `500`.
pub(crate) fn store_error(error: CapabilityStoreError, action: &str, what: &str) -> ActionError {
    match error {
        CapabilityStoreError::NotFound => ActionError::resource_not_found(what),
        CapabilityStoreError::Parent(ParentError::NotFound { .. }) => {
            ActionError::resource_not_found("capability")
        }
        CapabilityStoreError::DuplicateKey => ActionError::new(
            409,
            "capability_agent.duplicate_key",
            "that key already names another capability agent in this tenant",
        ),
        CapabilityStoreError::DuplicateCode => ActionError::new(
            409,
            "capability.duplicate_code",
            "that code already names another capability in this tenant",
        ),
        CapabilityStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "the capability agent moved to version {current} before this call"
        )),
        CapabilityStoreError::UnknownStatus(raw) => invalid(
            "status",
            "invalid_value",
            format!("{raw:?} is not a capability status"),
        ),
        CapabilityStoreError::Blank(field) => invalid(field, "required", "must not be empty"),
        CapabilityStoreError::BadPackVersion(version) => invalid(
            "pack_version",
            "invalid_value",
            format!("pack_version must be positive, got {version}"),
        ),
        other => {
            tracing::error!(%other, action, "capability store error");
            ActionError::internal()
        }
    }
}

/// Encode a list cursor: the code (or key) of the last row returned, as JSON.
pub(crate) fn key_cursor(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_else(|_| String::from("\"\""))
}

/// Decode a cursor this module produced; anything else is a `422`, not an ignored page boundary.
pub(crate) fn decode_key_cursor(
    cursor: Option<&str>,
    what: &str,
) -> Result<Option<String>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<String>(raw).map(Some).map_err(|_| {
        invalid(
            "cursor",
            "invalid_cursor",
            format!("a {what} cursor is the last row's key, as the previous page returned it"),
        )
    })
}

/// A `min_score` threshold as an exact decimal; a NaN or infinite number is refused, not rounded.
pub(crate) fn parse_min_score(raw: f64) -> Result<rust_decimal::Decimal, ActionError> {
    rust_decimal::Decimal::try_from(raw).map_err(|_| {
        invalid(
            "min_score",
            "invalid_value",
            "min_score must be a finite decimal number",
        )
    })
}
