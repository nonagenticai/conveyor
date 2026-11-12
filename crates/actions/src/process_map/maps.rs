#![forbid(unsafe_code)]
//! `process_maps.get` · `.list` — the two reads the UI and the PM need (CONTRACT §21).
//!
//! Both are `processes:read`: a map is a view of the process model, and a caller who may read the
//! processes may read the map that lays them out. Both are pure reads — no event, no audit row, no
//! transaction of their own beyond the invocation's.

use conveyor_store as store;
use sqlx::PgConnection;

use crate::board::etag_of;
use crate::invoke::{ActionError, HandlerCtx};
use crate::process_map::{
    MapCreateIn, MapGetIn, MapIdIn, MapListIn, MapOut, MapUpdateIn, cursor_of, decode_cursor,
    invalid, map_error, map_out, not_found, parse_status,
};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// One map, by id or by key and project type.
#[crate::action(
    name = "process_maps.get",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapGetIn,
) -> Result<MapOut, ActionError> {
    match (input.map_id, input.key.as_deref()) {
        (Some(id), None) => {
            if input.project_type_id.is_some() {
                return Err(invalid(
                    "project_type_id",
                    "invalid_value",
                    "a map addressed by map_id carries its project type; do not name it again",
                ));
            }
            if input.version.is_some() {
                return Err(invalid(
                    "version",
                    "invalid_value",
                    "an exact version is addressed by key, not by map_id",
                ));
            }
            store::process_map::get_map_tx(&mut *tx, ctx.tenant, id)
                .await
                .map_err(|error| map_error(error, "process_maps.get"))?
                .map(|record| map_out(&record))
                .ok_or_else(not_found)
        }
        (None, Some(key)) => {
            let Some(project_type_id) = input.project_type_id else {
                return Err(invalid(
                    "project_type_id",
                    "required",
                    "a map is bound to a project type; name it with the key",
                ));
            };
            if key.trim().is_empty() {
                return Err(invalid("key", "required", "must not be empty"));
            }
            store::process_map::get_map_by_key_tx(
                &mut *tx,
                ctx.tenant,
                key,
                project_type_id,
                input.version,
            )
            .await
            .map_err(|error| map_error(error, "process_maps.get"))?
            .map(|record| map_out(&record))
            .ok_or_else(not_found)
        }
        (Some(_), Some(_)) => Err(invalid(
            "key",
            "invalid_value",
            "name either map_id or key, not both",
        )),
        (None, None) => Err(invalid("map_id", "required", "name a map_id or a key")),
    }
}

/// One page of maps, newest first.
#[crate::action(
    name = "process_maps.list",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapListIn,
) -> Result<Page<MapOut>, ActionError> {
    let status = input.status.as_deref().map(parse_status).transpose()?;
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);
    // One extra row decides whether a next page exists; it is not returned.
    let mut rows = store::process_map::list_maps_tx(
        &mut *tx,
        ctx.tenant,
        status,
        input.project_type_id,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| map_error(error, "process_maps.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(cursor_of)
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(map_out).collect(),
        next_cursor,
    })
}

/// Create the next version of a map key as a draft.
///
/// The project type is resolved in the caller's tenant first: the foreign key alone accepts
/// another tenant's id (FK checks bypass RLS), so a cross-tenant binding must be refused here.
#[crate::action(
    name = "process_maps.create",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapCreateIn,
) -> Result<HandlerOutput<MapOut>, ActionError> {
    let action = "process_maps.create";
    let key = input.key.trim();
    if key.is_empty() {
        return Err(invalid("key", "required", "must not be empty"));
    }
    if input.title.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    store::process_map::facts_for_project_type_tx(&mut *tx, ctx.tenant, input.project_type_id)
        .await
        .map_err(|error| map_error(error, action))?;
    let record = store::process_map::create_draft_tx(
        &mut *tx,
        ctx.tenant,
        &store::process_map::NewMap {
            key: key.to_owned(),
            title: input.title.trim().to_owned(),
            project_type_id: input.project_type_id,
            definition: input.definition,
        },
    )
    .await
    .map_err(|error| map_error(error, action))?;
    Ok(HandlerOutput::created(map_out(&record)).with_etag(etag_of(record.lock_version)))
}

/// Edit a draft. `If-Match` is the version the caller read (CONTRACT §3).
#[crate::action(
    name = "process_maps.update",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapUpdateIn,
) -> Result<HandlerOutput<MapOut>, ActionError> {
    let action = "process_maps.update";
    let current = store::process_map::get_map_tx(&mut *tx, ctx.tenant, input.map_id)
        .await
        .map_err(|error| map_error(error, action))?
        .ok_or_else(not_found)?;
    // Status before precondition: an edit of a published row cannot become legal whatever
    // `If-Match` says, so the answer tells the caller to create a new version.
    if current.status != "draft" {
        return Err(ActionError::new(
            409,
            "process_map.not_draft",
            format!(
                "the map is {}; only a draft is editable — create a new version from it",
                current.status
            ),
        ));
    }
    let before = map_out(&current);
    let expected = crate::board::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
    )?;
    if let Some(title) = input.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if let Some(project_type_id) = input.project_type_id {
        store::process_map::facts_for_project_type_tx(&mut *tx, ctx.tenant, project_type_id)
            .await
            .map_err(|error| map_error(error, action))?;
    }
    let record = store::process_map::update_draft_tx(
        &mut *tx,
        ctx.tenant,
        input.map_id,
        Some(i64::try_from(expected.get()).unwrap_or(i64::MAX)),
        &store::process_map::MapPatch {
            title: input.title.as_deref().map(|title| title.trim().to_owned()),
            project_type_id: input.project_type_id,
            definition: input.definition,
        },
    )
    .await
    .map_err(|error| match error {
        store::process_map::MapStoreError::VersionMismatch { current } => {
            ActionError::precondition_with(
                serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
                format!("the map moved to version {current} before this call"),
            )
        }
        other => map_error(other, action),
    })?;
    Ok(HandlerOutput::ok(map_out(&record)).with_etag(etag_of(record.lock_version)))
}

/// Publish a draft after validating it against the tenant's live facts.
///
/// This is the surface that makes "a map node that does not resolve" a publish refusal rather
/// than a document nobody checks (S13 §5.2, §5.6).
#[crate::action(
    name = "process_maps.publish",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapIdIn,
) -> Result<HandlerOutput<MapOut>, ActionError> {
    let action = "process_maps.publish";
    let current = store::process_map::get_map_tx(&mut *tx, ctx.tenant, input.map_id)
        .await
        .map_err(|error| map_error(error, action))?
        .ok_or_else(not_found)?;
    let facts = store::process_map::facts_for_project_type_tx(
        &mut *tx,
        ctx.tenant,
        current.project_type_id,
    )
    .await
    .map_err(|error| map_error(error, action))?;
    let document: conveyor_domain::process_map::MapDoc =
        serde_json::from_value(current.definition.clone()).map_err(|error| {
            invalid(
                "definition",
                "invalid_value",
                format!("the map is not a {{schema, nodes, edges}} document: {error}"),
            )
        })?;
    conveyor_domain::process_map::validate(&document, &facts).map_err(|errors| {
        ActionError::new(
            422,
            "process_map.invalid_map",
            format!("{action}: the map does not resolve against this tenant"),
        )
        .with_errors(
            errors
                .into_iter()
                .map(|error| crate::spec::FieldError {
                    field: "definition".to_owned(),
                    code: "unresolved_node".to_owned(),
                    detail: error.to_string(),
                })
                .collect(),
        )
    })?;
    let record = store::process_map::publish_tx(&mut *tx, ctx.tenant, input.map_id)
        .await
        .map_err(|error| map_error(error, action))?;
    Ok(HandlerOutput::ok(map_out(&record)).with_etag(etag_of(record.lock_version)))
}

/// Retire a published map.
#[crate::action(
    name = "process_maps.retire",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn process_maps_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MapIdIn,
) -> Result<HandlerOutput<MapOut>, ActionError> {
    let action = "process_maps.retire";
    let record = store::process_map::retire_tx(&mut *tx, ctx.tenant, input.map_id)
        .await
        .map_err(|error| map_error(error, action))?;
    Ok(HandlerOutput::ok(map_out(&record)).with_etag(etag_of(record.lock_version)))
}
