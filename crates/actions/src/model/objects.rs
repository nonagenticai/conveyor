#![forbid(unsafe_code)]
//! `view_objects.*` — the boxes on a diagram (S08 Part B §5.9).
//!
//! An object may draw an element (`element_id`), and when it does that element must belong to
//! the **same model** as the view: the store resolves the id in the caller's tenant, and this
//! layer is where "same model" is checked. `bounds` is geometry and is never filtered on — it
//! is written and read whole (slice trap 3).

use conveyor_store as store;
use conveyor_store::view::{NewObject, ObjectPatch, ObjectRecord, get_object_tx};
use serde_json::Value;
use sqlx::PgConnection;

pub use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    ObjectCreateIn, ObjectOut, ObjectUpdateIn, invalid, model_error, require_precondition_with,
    version_i64,
};

/// One object as every action of this family returns it.
pub(crate) fn object_out(record: &ObjectRecord) -> ObjectOut {
    ObjectOut {
        id: record.id.to_string(),
        view_id: record.view_id.to_string(),
        element_id: record.element_id.map(|id| id.to_string()),
        parent_object_id: record.parent_object_id.map(|id| id.to_string()),
        kind: record.kind.clone(),
        bounds: record.bounds.clone(),
        style: record.style.clone(),
        label: record.label.clone(),
        position: record.position,
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn object_json(record: &ObjectRecord) -> Value {
    serde_json::to_value(object_out(record)).unwrap_or(Value::Null)
}

async fn read_object(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ObjectRecord, ActionError> {
    get_object_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such view object"))
}

/// Place an object on a view.
#[crate::action(
    name = "view_objects.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_objects_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ObjectCreateIn,
) -> Result<HandlerOutput<ObjectOut>, ActionError> {
    let action = ctx.action;
    let view = store::view::get_view_tx(&mut *tx, ctx.tenant, input.view_id)
        .await
        .map_err(|error| model_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such view"))?;

    if let Some(element_id) = input.element_id {
        let element = store::model::get_element_tx(&mut *tx, ctx.tenant, element_id)
            .await
            .map_err(|error| model_error(error, action))?
            .ok_or_else(|| ActionError::resource_not_found("such element"))?;
        if element.model_id != view.model_id {
            return Err(invalid(
                "element_id",
                "invalid_value",
                "the element belongs to a different model than the view",
            ));
        }
    }

    let record = store::view::create_object_tx(
        &mut *tx,
        ctx.tenant,
        &NewObject {
            view_id: input.view_id,
            element_id: input.element_id,
            parent_object_id: input.parent_object_id,
            kind: input.kind.unwrap_or_else(|| "element".to_owned()),
            bounds: input
                .bounds
                .unwrap_or_else(|| Value::Object(Default::default())),
            style: input
                .style
                .unwrap_or_else(|| Value::Object(Default::default())),
            label: input.label,
            position: input.position.unwrap_or(0.0),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::created(object_out(&record))
        .with_etag(super::etag_of(version_i64(record.version))))
}

/// Move or restyle an object.
#[crate::action(
    name = "view_objects.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_objects_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ObjectUpdateIn,
) -> Result<HandlerOutput<ObjectOut>, ActionError> {
    let action = ctx.action;
    let before = read_object(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, object_json(&before))?;

    let after = match store::view::update_object_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        &ObjectPatch {
            bounds: input.bounds,
            style: input.style,
            label: input.label,
            position: input.position,
        },
    )
    .await
    {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match store::view::get_object_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    object_json(&latest),
                    format!(
                        "{action}: the object is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such view object")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(object_out(&after)).with_etag(super::etag_of(version_i64(after.version))))
}

/// Remove an object and every connector that names it.
#[crate::action(
    name = "view_objects.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_objects_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ObjectOut>, ActionError> {
    let action = ctx.action;
    let before = read_object(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, object_json(&before))?;
    let removed = store::view::delete_object_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(object_out(&removed)))
}
