#![forbid(unsafe_code)]
//! `model_relationships.*` — the edges of a model's graph (S08 Part B §5.9).
//!
//! A relationship lives inside **one** model, and both of its ends must be elements of that
//! same model. The store resolves each end in the caller's tenant — so another tenant's row is
//! unreachable — but deliberately does not check model membership: a cross-model edge is a
//! modelling error the caller made, and it is refused here with a `422` rather than left for
//! the post-commit validator to report. `source_id == target_id` is refused the same way,
//! naming `target_id`; the table's CHECK would otherwise surface it as a `500`.
//!
//! The `kind` is judged by the model's profile exactly as an element's is, on create and on
//! every kind-changing update.
//!
//! **No events.** §5.11 declares no relationship event type, so a mutation writes no outbox
//! row.

use conveyor_store as store;
use conveyor_store::model::{
    ModelPage, NewRelationship, RelationshipPatch, RelationshipRecord, create_relationship_tx,
    delete_relationship_tx, get_element_tx, get_relationship_tx, list_relationships_tx,
    update_relationship_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::models::read_model;
use super::{
    RelationshipCreateIn, RelationshipListIn, RelationshipOut, RelationshipUpdateIn, etag_of,
    invalid, model_error, profile_of, profile_violation, require_precondition_with, version_i64,
};

/// One relationship row as every action of this family returns it.
pub(crate) fn relationship_out(record: &RelationshipRecord) -> RelationshipOut {
    RelationshipOut {
        id: record.id.to_string(),
        model_id: record.model_id.to_string(),
        kind: record.kind.clone(),
        source_id: record.source_id.to_string(),
        target_id: record.target_id.to_string(),
        meta: record.meta.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back.
fn relationship_json(record: &RelationshipRecord) -> Value {
    serde_json::to_value(relationship_out(record)).unwrap_or(Value::Null)
}

/// Read one relationship or `404` (the update/delete precondition needs it).
async fn read_relationship(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<RelationshipRecord, ActionError> {
    get_relationship_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such relationship"))
}

/// Refuse an end that is not an element of the relationship's own model.
///
/// Both ends are resolved by the store inside the caller's tenant, but tenancy is not model
/// membership: an element of another model in the same tenant is exactly the modelling error
/// this check exists for.
async fn ensure_end_in_model(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    field: &str,
    model_id: uuid::Uuid,
    element_id: uuid::Uuid,
) -> Result<(), ActionError> {
    let end = get_element_tx(&mut *conn, ctx.tenant, element_id)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    match end {
        Some(element) if element.model_id == model_id => Ok(()),
        _ => Err(invalid(
            field,
            "invalid_value",
            "the element is missing or belongs to another model",
        )),
    }
}

/// Add a relationship to a model, judged by that model's profile.
#[crate::action(
    name = "model_relationships.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_relationships_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RelationshipCreateIn,
) -> Result<HandlerOutput<RelationshipOut>, ActionError> {
    let action = ctx.action;
    // The model is the only authority on which kinds exist here.
    let model = read_model(&mut *tx, &ctx, input.model_id).await?;
    let profile = profile_of(&model.language, &model.language_version)?;
    if !profile.has_relationship_kind(&input.kind) {
        return Err(profile_violation(
            "kind",
            &input.kind,
            profile,
            "relationship kind",
        ));
    }
    if input.source_id == input.target_id {
        return Err(invalid(
            "target_id",
            "invalid_value",
            "a relationship cannot relate an element to itself",
        ));
    }
    ensure_end_in_model(&mut *tx, &ctx, "source_id", input.model_id, input.source_id).await?;
    ensure_end_in_model(&mut *tx, &ctx, "target_id", input.model_id, input.target_id).await?;

    let record = create_relationship_tx(
        &mut *tx,
        ctx.tenant,
        &NewRelationship {
            model_id: input.model_id,
            kind: input.kind,
            source_id: input.source_id,
            target_id: input.target_id,
            meta: input
                .meta
                .unwrap_or_else(|| Value::Object(Default::default())),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::created(relationship_out(&record))
        .with_etag(etag_of(version_i64(record.version))))
}

/// Edit a relationship; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "model_relationships.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_relationships_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RelationshipUpdateIn,
) -> Result<HandlerOutput<RelationshipOut>, ActionError> {
    let action = ctx.action;
    let before = read_relationship(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, relationship_json(&before))?;

    // A kind change is judged by the profile of the model the relationship already belongs to.
    if let Some(kind) = &input.kind {
        let model = read_model(&mut *tx, &ctx, before.model_id).await?;
        let profile = profile_of(&model.language, &model.language_version)?;
        if !profile.has_relationship_kind(kind) {
            return Err(profile_violation(
                "kind",
                kind,
                profile,
                "relationship kind",
            ));
        }
    }

    let patch = RelationshipPatch {
        kind: input.kind,
        meta: input.meta,
    };

    let after = match update_relationship_tx(&mut *tx, ctx.tenant, input.id, expected, &patch).await
    {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match get_relationship_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    relationship_json(&latest),
                    format!(
                        "{action}: the relationship is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such relationship")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(relationship_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// Remove a relationship; `If-Match` is required and the removed row is returned.
#[crate::action(
    name = "model_relationships.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_relationships_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<RelationshipOut>, ActionError> {
    let action = ctx.action;
    let before = read_relationship(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, relationship_json(&before))?;
    let removed = delete_relationship_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(relationship_out(&removed)))
}

/// One page of a model's relationships, newest `created_at` first.
#[crate::action(
    name = "model_relationships.list",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_relationships_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RelationshipListIn,
) -> Result<Page<RelationshipOut>, ActionError> {
    let page = ModelPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let (rows, next_cursor) = list_relationships_tx(&mut *tx, ctx.tenant, input.model_id, &page)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(relationship_out).collect(),
        next_cursor,
    })
}
