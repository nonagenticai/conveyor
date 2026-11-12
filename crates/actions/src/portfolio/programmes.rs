#![forbid(unsafe_code)]
//! `programmes.*` — S03's `programme` table (S08 Part A §5.2).
//!
//! Programmes are the one portfolio family with no table of their own: the rows already exist
//! for `projects.create` to roll up to, and this surface is their read and write path. There is
//! **no delete** — a programme with projects is not removed (S08 §5.2 lists four actions).
//!
//! `programme.name` is unique per tenant (`programme_tenant_name_key`), so a duplicate create
//! is the store's `409`, never a second row.

use conveyor_store as store;
use conveyor_store::portfolio::{
    NewProgrammeRow, PortfolioPage, ProgrammePatch, ProgrammeRecord, get_programme_tx,
    list_programmes_tx, update_programme_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    IdIn, Page, ProgrammeCreateIn, ProgrammeListIn, ProgrammeOut, ProgrammeUpdateIn,
    emit_portfolio_event, etag_of, invalid, portfolio_error, require_precondition_with, trimmed,
    version_i64,
};

/// One programme as every action of this family returns it.
fn programme_out(record: &ProgrammeRecord) -> ProgrammeOut {
    ProgrammeOut {
        id: record.id.to_string(),
        name: record.name.clone(),
        status: record.status.clone(),
        owner_user_id: record.owner_user_id.map(|id| id.to_string()),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back.
fn programme_json(record: &ProgrammeRecord) -> Value {
    serde_json::to_value(programme_out(record)).unwrap_or(Value::Null)
}

/// Read one programme or `404`.
async fn read_programme(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ProgrammeRecord, ActionError> {
    get_programme_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| portfolio_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such programme"))
}

/// Create a programme.
#[crate::action(
    name = "programmes.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn programmes_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProgrammeCreateIn,
) -> Result<HandlerOutput<ProgrammeOut>, ActionError> {
    let action = ctx.action;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let record = store::portfolio::create_programme_tx(
        &mut *tx,
        ctx.tenant,
        &NewProgrammeRow {
            name: name.to_owned(),
            status: trimmed(input.status),
            owner_user_id: input.owner_user_id,
        },
    )
    .await
    .map_err(|error| portfolio_error(error, action))?;
    emit_portfolio_event(
        &mut *tx,
        &ctx,
        "programme",
        "programme",
        record.id,
        "created",
    )
    .await?;

    Ok(HandlerOutput::created(programme_out(&record))
        .with_etag(etag_of(version_i64(record.version))))
}

/// Read one programme.
#[crate::action(
    name = "programmes.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn programmes_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ProgrammeOut>, ActionError> {
    let record = read_programme(&mut *tx, &ctx, input.id).await?;
    Ok(HandlerOutput::ok(programme_out(&record)).with_etag(etag_of(version_i64(record.version))))
}

/// Edit a programme; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "programmes.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn programmes_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProgrammeUpdateIn,
) -> Result<HandlerOutput<ProgrammeOut>, ActionError> {
    let action = ctx.action;
    let before = read_programme(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, programme_json(&before))?;

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
    let patch = ProgrammePatch {
        name,
        status: trimmed(input.status),
        owner_user_id: input.owner_user_id,
    };

    let after = match update_programme_tx(&mut *tx, ctx.tenant, input.id, expected, &patch).await {
        Ok(record) => record,
        Err(store::portfolio::PortfolioStoreError::VersionMismatch { current }) => {
            // The row moved between our read and the store's lock. Nothing was written; re-read
            // so the `412` still carries the current representation (CONTRACT §3).
            return match get_programme_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| portfolio_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    programme_json(&latest),
                    format!(
                        "{action}: the programme is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such programme")),
            };
        }
        Err(error) => return Err(portfolio_error(error, action)),
    };
    emit_portfolio_event(
        &mut *tx,
        &ctx,
        "programme",
        "programme",
        after.id,
        "updated",
    )
    .await?;

    Ok(HandlerOutput::ok(programme_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// One page of programmes, newest `updated_at` first.
#[crate::action(
    name = "programmes.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn programmes_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProgrammeListIn,
) -> Result<Page<ProgrammeOut>, ActionError> {
    let page = PortfolioPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let (rows, next_cursor) = list_programmes_tx(&mut *tx, ctx.tenant, &page)
        .await
        .map_err(|error| portfolio_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(programme_out).collect(),
        next_cursor,
    })
}
