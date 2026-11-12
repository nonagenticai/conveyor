#![forbid(unsafe_code)]
//! `model_elements.*` — the vertices of a model's graph (S08 Part B §5.9).
//!
//! An element's `kind` is judged by **its model's profile**, never by a default: the model is
//! read before the write, its `(language, language_version)` is resolved with
//! [`super::profile_of`] and a kind that profile does not contain is refused with a `422`
//! whose problem type is `model-profile-violation`. The same check runs whenever an update
//! changes `kind`, because the column is what validation and rendering branch on.
//!
//! **No events.** §5.11 declares four model event types; an element mutation is none of them,
//! so nothing is appended to the outbox.
//!
//! `delete` answers with the row it removed and requires `If-Match`; a stale precondition
//! writes nothing. An element that still has children is a `409 model-has-children` — the
//! store refuses to orphan them silently.

use conveyor_store as store;
use conveyor_store::model::{
    ElementPatch, ElementRecord, ModelPage, NewElement, create_element_tx, delete_element_tx,
    get_element_tx, list_elements_tx, update_element_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::models::read_model;
use super::{
    ElementCreateIn, ElementListIn, ElementOut, ElementUpdateIn, etag_of, invalid, model_error,
    profile_of, profile_violation, require_precondition_with, version_i64,
};

/// One element row as every action of this family returns it.
pub(crate) fn element_out(record: &ElementRecord) -> ElementOut {
    ElementOut {
        id: record.id.to_string(),
        model_id: record.model_id.to_string(),
        kind: record.kind.clone(),
        name: record.name.clone(),
        documentation: record.documentation.clone(),
        parent_element_id: record.parent_element_id.map(|id| id.to_string()),
        meta: record.meta.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back.
fn element_json(record: &ElementRecord) -> Value {
    serde_json::to_value(element_out(record)).unwrap_or(Value::Null)
}

/// Read one element or `404` (the update/delete precondition needs it).
async fn read_element(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ElementRecord, ActionError> {
    get_element_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such element"))
}

/// Add an element to a model, judged by that model's profile.
#[crate::action(
    name = "model_elements.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_elements_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ElementCreateIn,
) -> Result<HandlerOutput<ElementOut>, ActionError> {
    let action = ctx.action;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    // The model is the only authority on which kinds exist here. Reading it first makes an
    // out-of-profile kind a `422` at the door instead of a row the validator rejects later.
    let model = read_model(&mut *tx, &ctx, input.model_id).await?;
    let profile = profile_of(&model.language, &model.language_version)?;
    if !profile.has_element_kind(&input.kind) {
        return Err(profile_violation(
            "kind",
            &input.kind,
            profile,
            "element kind",
        ));
    }

    let record = create_element_tx(
        &mut *tx,
        ctx.tenant,
        &NewElement {
            model_id: input.model_id,
            kind: input.kind,
            name: name.to_owned(),
            documentation: input.documentation,
            parent_element_id: input.parent_element_id,
            meta: input
                .meta
                .unwrap_or_else(|| Value::Object(Default::default())),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(
        HandlerOutput::created(element_out(&record))
            .with_etag(etag_of(version_i64(record.version))),
    )
}

/// Edit an element; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "model_elements.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_elements_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ElementUpdateIn,
) -> Result<HandlerOutput<ElementOut>, ActionError> {
    let action = ctx.action;
    let before = read_element(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, element_json(&before))?;

    // A kind change is judged by the profile of the model the element already belongs to,
    // not by the caller's idea of one.
    if let Some(kind) = &input.kind {
        let model = read_model(&mut *tx, &ctx, before.model_id).await?;
        let profile = profile_of(&model.language, &model.language_version)?;
        if !profile.has_element_kind(kind) {
            return Err(profile_violation("kind", kind, profile, "element kind"));
        }
    }

    let name = match input.name {
        Some(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(invalid("name", "required", "must not be empty"));
            }
            Some(name.to_owned())
        }
        None => None,
    };
    let patch = ElementPatch {
        kind: input.kind,
        name,
        documentation: input.documentation,
        parent_element_id: input.parent_element_id,
        meta: input.meta,
    };

    let after = match update_element_tx(&mut *tx, ctx.tenant, input.id, expected, &patch).await {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match get_element_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    element_json(&latest),
                    format!(
                        "{action}: the element is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such element")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(element_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// Remove an element; `If-Match` is required and the removed row is returned.
#[crate::action(
    name = "model_elements.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_elements_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ElementOut>, ActionError> {
    let action = ctx.action;
    let before = read_element(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, element_json(&before))?;
    let removed = delete_element_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(element_out(&removed)))
}

/// One page of a model's elements, newest `created_at` first.
#[crate::action(
    name = "model_elements.list",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_elements_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ElementListIn,
) -> Result<Page<ElementOut>, ActionError> {
    let page = ModelPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let (rows, next_cursor) = list_elements_tx(
        &mut *tx,
        ctx.tenant,
        input.model_id,
        input.kind.as_deref(),
        &page,
    )
    .await
    .map_err(|error| model_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(element_out).collect(),
        next_cursor,
    })
}
