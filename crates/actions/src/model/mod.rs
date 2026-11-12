#![forbid(unsafe_code)]
//! `model.*` — the governance model surface: models, elements, relationships, views, objects,
//! connections and the four read-only architecture tools (S08 Part B §5.9).
//!
//! Every action is one `#[action]` declaration in a family module under this one; the
//! registry, the OpenAPI operation, the MCP tool and the invocation route are projections of
//! it (S04 §5.1). Nothing here speaks SQL directly — the queries live [`conveyor_store::model`]
//! and [`conveyor_store::view`] (CONTRACT §1).
//!
//! **One event pattern: the action layer emits.** Every model event is appended by
//! [`emit_model_event`] in the mutation's transaction, calling `store::outbox::append`
//! directly (S08 Part B decision 1). It deliberately does **not** go through
//! `crate::board::events::append_raw_event`, which also fans out webhook deliveries — a model
//! event has never created a delivery and must not start.
//!
//! **Profile enforcement is two instruments, not one.** A `kind` the model's profile does not
//! contain is refused at the door with a `422` ([`profile_violation`]); everything that can
//! only be judged once the whole graph is in view is a `model_validation_run` finding
//! (`models.validate`, post-commit — slice trap 6).
//!
//! **Revisions are written at publish, not per mutation** (decision 3): `revision.content` is
//! capped at 256 KiB, so `models.publish` freezes the graph and every other edit bumps
//! `version` only.

use conveyor_domain::Version;
use conveyor_domain::profile::{self, Profile};
use conveyor_store as store;
use conveyor_store::model::ModelStoreError;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod connections;
pub mod documents;
pub mod elements;
pub mod exchange;
pub mod model_views;
pub mod models;
pub mod objects;
pub mod relationships;
pub mod tools;

pub use documents::*;

/// Re-exported so every family module refuses a store failure the same way.
pub use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};

/// `model.created` — emitted by `models.create` (and by `models.import_xmi` in B2).
pub const MODEL_CREATED: &str = "model.created";

/// `model.published` — emitted by `models.publish`.
pub const MODEL_PUBLISHED: &str = "model.published";

/// `model.validated` — emitted by the `model.validate` worker.
pub const MODEL_VALIDATED: &str = "model.validated";

/// `model.exported` — emitted by `models.render` / `models.export_xmi` (B2).
pub const MODEL_EXPORTED: &str = "model.exported";

/// The row's `version` as the `i64` an output document carries.
pub fn version_i64(version: Version) -> i64 {
    i64::try_from(version.get()).unwrap_or(i64::MAX)
}

/// The profile a `(language, language_version)` pair names.
///
/// A model is created against exactly one profile and every later write is judged by it, so
/// an unknown pair is a `422` naming both values — never a default that would validate
/// against nothing.
pub fn profile_or_refuse(language: &str, version: &str) -> Result<&'static Profile, ActionError> {
    profile::for_language(language, version).map_err(|error| {
        ActionError::invalid_input(vec![FieldError {
            field: "language".to_owned(),
            code: "invalid_value".to_owned(),
            detail: error.to_string(),
        }])
    })
}

/// A `422` for a kind outside the model's profile.
///
/// The problem carries `{field, value, ruleset}` in `errors[]` so a client can tell which
/// value was refused and which profile version refused it.
pub fn profile_violation(
    field: &str,
    value: &str,
    profile: &'static Profile,
    what: &str,
) -> ActionError {
    ActionError::new(
        422,
        "model-profile-violation",
        format!(
            "{what} {value:?} is not in profile {} ({} {})",
            profile.ruleset,
            profile.language.as_str(),
            profile.language_version
        ),
    )
    .with_errors(vec![FieldError {
        field: field.to_owned(),
        code: "out_of_profile".to_owned(),
        detail: format!("{}; ruleset {}", profile.ruleset, profile.ruleset_version),
    }])
}

/// Append one model event in the caller's transaction.
///
/// Topic `<aggregate>:<id>` = `model:<uuid>`; the payload names what changed.
pub async fn emit_model_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    model_id: Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("model:{model_id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "model".to_owned(),
        aggregate_id: model_id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            // The outbox insert is part of the mutation's transaction, so this refusal rolls
            // the change back with it: a model row is never written unannounced.
            tracing::error!(%error, event_type, action = ctx.action, "append the model event");
            ActionError::internal()
        })?;
    Ok(())
}

/// A model store failure as the status the caller can act on.
pub fn model_error(error: ModelStoreError, action: &str) -> ActionError {
    match error {
        ModelStoreError::NotFound => ActionError::resource_not_found("such model row"),
        ModelStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the row is at version {}, not the version the caller supplied",
            current.get()
        )),
        ModelStoreError::BadCursor => invalid(
            "cursor",
            "invalid_value",
            "not a cursor this service issued",
        ),
        ModelStoreError::KindRefused { field, value } => invalid(
            field,
            "invalid_value",
            format!("{value:?} is not a value this column accepts"),
        ),
        ModelStoreError::HasChildren { count } => ActionError::new(
            409,
            "model-has-children",
            format!("the element still has {count} child element(s); delete those first"),
        ),
        ModelStoreError::RevisionTooLarge { bytes } => ActionError::new(
            422,
            "revision-too-large",
            format!(
                "the model serialises to {bytes} bytes; revision.content is capped at {} and is \
                 never truncated",
                conveyor_domain::model::REVISION_CONTENT_MAX_BYTES
            ),
        ),
        ModelStoreError::Parent(error) => parent_error(error, action),
        ModelStoreError::Database(error) => store_error(error, action),
    }
}

/// Read the model's profile from its stored `language`/`language_version`.
pub fn profile_of(language: &str, language_version: &str) -> Result<&'static Profile, ActionError> {
    profile_or_refuse(language, language_version)
}
