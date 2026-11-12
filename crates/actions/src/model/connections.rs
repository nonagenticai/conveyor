#![forbid(unsafe_code)]
//! `view_connections.*` — the connectors on a diagram (S08 Part B §5.9).
//!
//! A connector may draw a model relationship (`relationship_id`), and when it does that
//! relationship must belong to the **same model** as the view. Both endpoints are resolved by
//! the store and must be objects on the same view; a connector between two views is not a
//! thing this schema can express, and the store refuses it.

use conveyor_store as store;
use conveyor_store::view::{ConnectionPatch, ConnectionRecord, NewConnection, get_connection_tx};
use serde_json::Value;
use sqlx::PgConnection;

pub use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    ConnectionCreateIn, ConnectionOut, ConnectionUpdateIn, invalid, model_error,
    require_precondition_with, version_i64,
};

/// One connector as every action of this family returns it.
pub(crate) fn connection_out(record: &ConnectionRecord) -> ConnectionOut {
    ConnectionOut {
        id: record.id.to_string(),
        view_id: record.view_id.to_string(),
        relationship_id: record.relationship_id.map(|id| id.to_string()),
        source_object_id: record.source_object_id.to_string(),
        target_object_id: record.target_object_id.to_string(),
        bendpoints: record.bendpoints.clone(),
        style: record.style.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn connection_json(record: &ConnectionRecord) -> Value {
    serde_json::to_value(connection_out(record)).unwrap_or(Value::Null)
}

async fn read_connection(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ConnectionRecord, ActionError> {
    get_connection_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such view connection"))
}

/// Draw a connector.
#[crate::action(
    name = "view_connections.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_connections_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConnectionCreateIn,
) -> Result<HandlerOutput<ConnectionOut>, ActionError> {
    let action = ctx.action;
    let view = store::view::get_view_tx(&mut *tx, ctx.tenant, input.view_id)
        .await
        .map_err(|error| model_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such view"))?;

    if let Some(relationship_id) = input.relationship_id {
        let relationship = store::model::get_relationship_tx(&mut *tx, ctx.tenant, relationship_id)
            .await
            .map_err(|error| model_error(error, action))?
            .ok_or_else(|| ActionError::resource_not_found("such model relationship"))?;
        if relationship.model_id != view.model_id {
            return Err(invalid(
                "relationship_id",
                "invalid_value",
                "the relationship belongs to a different model than the view",
            ));
        }
    }

    let record = store::view::create_connection_tx(
        &mut *tx,
        ctx.tenant,
        &NewConnection {
            view_id: input.view_id,
            relationship_id: input.relationship_id,
            source_object_id: input.source_object_id,
            target_object_id: input.target_object_id,
            bendpoints: input.bendpoints.unwrap_or_else(|| Value::Array(Vec::new())),
            style: input
                .style
                .unwrap_or_else(|| Value::Object(Default::default())),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::created(connection_out(&record))
        .with_etag(super::etag_of(version_i64(record.version))))
}

/// Reroute or restyle a connector.
#[crate::action(
    name = "view_connections.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_connections_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConnectionUpdateIn,
) -> Result<HandlerOutput<ConnectionOut>, ActionError> {
    let action = ctx.action;
    let before = read_connection(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, connection_json(&before))?;

    let after = match store::view::update_connection_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        &ConnectionPatch {
            bendpoints: input.bendpoints,
            style: input.style,
        },
    )
    .await
    {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match store::view::get_connection_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    connection_json(&latest),
                    format!(
                        "{action}: the connection is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such view connection")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(connection_out(&after))
        .with_etag(super::etag_of(version_i64(after.version))))
}

/// Remove a connector.
#[crate::action(
    name = "view_connections.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn view_connections_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ConnectionOut>, ActionError> {
    let action = ctx.action;
    let before = read_connection(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, connection_json(&before))?;
    let removed = store::view::delete_connection_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(connection_out(&removed)))
}
