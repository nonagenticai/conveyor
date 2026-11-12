#![forbid(unsafe_code)]
//! `model_views.*` — the diagrams a model owns (S08 Part B §5.9).
//!
//! A view kind is checked against the **model's** profile, not a global list: an
//! `archimate_view` on a UML model is the same class of error as an out-of-profile element
//! kind, and it is refused the same way. `meta.diagram_kind`, when present, must be one of
//! the profile's named diagram families.
//!
//! `model_views.generate` replaces a view's objects with the deterministic layout from
//! [`conveyor_domain::model::layout`] — the same function the renderer draws from, so a
//! generated view and a rendered one cannot disagree about where a box is. It is a replacement
//! of a representation, so it takes `If-Match` like an update (CONTRACT §3).

use conveyor_store as store;
use conveyor_store::view::{
    NewView, ViewPatch, ViewRecord, get_view_tx, list_views_tx, update_view_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

pub use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::{
    ViewCreateIn, ViewGenerateIn, ViewGenerateOut, ViewListIn, ViewOut, ViewUpdateIn, invalid,
    model_error, models::read_model, profile_of, require_precondition_with, version_i64,
};
use conveyor_store::model::ModelPage;

/// One view as every action of this family returns it.
pub(crate) fn view_out(record: &ViewRecord) -> ViewOut {
    ViewOut {
        id: record.id.to_string(),
        model_id: record.model_id.to_string(),
        kind: record.kind.clone(),
        name: record.name.clone(),
        meta: record.meta.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn view_json(record: &ViewRecord) -> Value {
    serde_json::to_value(view_out(record)).unwrap_or(Value::Null)
}

/// Read one view or `404`.
async fn read_view(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ViewRecord, ActionError> {
    get_view_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such view"))
}

/// The model's profile, resolved from the model the view belongs to.
async fn profile_of_model(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    model_id: uuid::Uuid,
) -> Result<&'static conveyor_domain::profile::Profile, ActionError> {
    let model = read_model(&mut *conn, ctx, model_id).await?;
    profile_of(&model.language, &model.language_version)
}

/// Refuse a view kind the model's profile does not accept.
fn check_view_kind(
    profile: &'static conveyor_domain::profile::Profile,
    kind: &str,
    meta: Option<&Value>,
) -> Result<(), ActionError> {
    if !profile.has_view_kind(kind) {
        return Err(super::profile_violation("kind", kind, profile, "view kind"));
    }
    if let Some(diagram) = meta
        .and_then(|meta| meta.get("diagram_kind"))
        .and_then(Value::as_str)
        && !profile.has_diagram_kind(diagram)
    {
        return Err(super::profile_violation(
            "meta.diagram_kind",
            diagram,
            profile,
            "diagram kind",
        ));
    }
    Ok(())
}

/// Create a view.
#[crate::action(
    name = "model_views.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ViewCreateIn,
) -> Result<HandlerOutput<ViewOut>, ActionError> {
    let action = ctx.action;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let profile = profile_of_model(&mut *tx, &ctx, input.model_id).await?;
    let kind = input.kind.unwrap_or_else(|| "diagram".to_owned());
    check_view_kind(profile, &kind, input.meta.as_ref())?;

    let record = store::view::create_view_tx(
        &mut *tx,
        ctx.tenant,
        &NewView {
            model_id: input.model_id,
            kind,
            name: name.to_owned(),
            meta: input
                .meta
                .unwrap_or_else(|| Value::Object(Default::default())),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::created(view_out(&record))
        .with_etag(super::etag_of(version_i64(record.version))))
}

/// Read one view.
#[crate::action(
    name = "model_views.get",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ViewOut>, ActionError> {
    let record = read_view(&mut *tx, &ctx, input.id).await?;
    Ok(HandlerOutput::ok(view_out(&record)).with_etag(super::etag_of(version_i64(record.version))))
}

/// Edit a view; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "model_views.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ViewUpdateIn,
) -> Result<HandlerOutput<ViewOut>, ActionError> {
    let action = ctx.action;
    let before = read_view(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, view_json(&before))?;

    if let Some(kind) = input.kind.as_deref() {
        let profile = profile_of_model(&mut *tx, &ctx, before.model_id).await?;
        check_view_kind(profile, kind, input.meta.as_ref())?;
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

    let after = match update_view_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        &ViewPatch {
            kind: input.kind,
            name,
            meta: input.meta,
        },
    )
    .await
    {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match get_view_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    view_json(&latest),
                    format!(
                        "{action}: the view is at version {}, not the version the caller supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such view")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(view_out(&after)).with_etag(super::etag_of(version_i64(after.version))))
}

/// Delete a view and its objects and connectors.
#[crate::action(
    name = "model_views.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ViewOut>, ActionError> {
    let action = ctx.action;
    let before = read_view(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, view_json(&before))?;
    let removed = store::view::delete_view_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(view_out(&removed)))
}

/// One page of a model's views.
#[crate::action(
    name = "model_views.list",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ViewListIn,
) -> Result<Page<ViewOut>, ActionError> {
    read_model(&mut *tx, &ctx, input.model_id).await?;
    let page = ModelPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let (rows, next_cursor) = list_views_tx(&mut *tx, ctx.tenant, input.model_id, &page)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(view_out).collect(),
        next_cursor,
    })
}

/// Lay every element of the model onto the view, replacing what was there.
#[crate::action(
    name = "model_views.generate",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn model_views_generate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ViewGenerateIn,
) -> Result<HandlerOutput<ViewGenerateOut>, ActionError> {
    let action = ctx.action;
    let before = read_view(&mut *tx, &ctx, input.view_id).await?;
    let expected = require_precondition_with(&ctx, action, view_json(&before))?;

    let snapshot = store::model::load_snapshot_tx(&mut *tx, ctx.tenant, before.model_id)
        .await
        .map_err(|error| model_error(error, action))?;
    let boxes = conveyor_domain::model::layout(&snapshot.elements);
    let objects: Vec<(uuid::Uuid, Value, f64)> = boxes
        .iter()
        .enumerate()
        .map(|(index, placed)| {
            (
                placed.element_id,
                serde_json::json!({
                    "x": placed.x,
                    "y": placed.y,
                    "w": placed.w,
                    "h": placed.h,
                }),
                index as f64,
            )
        })
        .collect();

    let written = store::view::replace_objects_tx(&mut *tx, ctx.tenant, input.view_id, &objects)
        .await
        .map_err(|error| model_error(error, action))?;

    // Replacing the contents is an edit of the view, so the view's version advances and a
    // concurrent `generate` or `update` refuses rather than silently overwriting.
    let after = update_view_tx(
        &mut *tx,
        ctx.tenant,
        input.view_id,
        expected,
        &ViewPatch::default(),
    )
    .await
    .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::ok(ViewGenerateOut {
        view: view_out(&after),
        objects: i64::try_from(written).unwrap_or(i64::MAX),
    })
    .with_etag(super::etag_of(version_i64(after.version))))
}
