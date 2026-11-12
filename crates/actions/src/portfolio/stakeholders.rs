#![forbid(unsafe_code)]
//! `stakeholders.*` — `project_stakeholder` (S08 Part A §5.2).
//!
//! `project_id` is resolved in the caller's tenant before the row is written (CONTRACT §2), so
//! naming another tenant's project is a `404`, never a row the FK would have accepted. The
//! linked `user_id` is resolved the same way and is optional: a stakeholder is often recorded
//! before they have an `app_user`.
//!
//! `delete` is a real `DELETE` — this table is not append-only — guarded by `If-Match`
//! (CONTRACT §3) so a concurrent edit is a `412`, not a lost update.

use conveyor_store as store;
use conveyor_store::portfolio::{
    NewStakeholder, PortfolioPage, ProjectStakeholderRecord, StakeholderFilter, StakeholderPatch,
    delete_stakeholder_tx, get_stakeholder_tx, list_stakeholders_tx, update_stakeholder_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    IdIn, Page, StakeholderCreateIn, StakeholderListIn, StakeholderOut, StakeholderUpdateIn,
    emit_portfolio_event, etag_of, invalid, portfolio_error, require_precondition_with, trimmed,
    version_i64,
};

/// One stakeholder as every action of this family returns it.
fn stakeholder_out(record: &ProjectStakeholderRecord) -> StakeholderOut {
    StakeholderOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        user_id: record.user_id.map(|id| id.to_string()),
        name: record.name.clone(),
        email: record.email.clone(),
        role: record.role.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back.
fn stakeholder_json(record: &ProjectStakeholderRecord) -> Value {
    serde_json::to_value(stakeholder_out(record)).unwrap_or(Value::Null)
}

/// Read one stakeholder or `404`.
async fn read_stakeholder(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ProjectStakeholderRecord, ActionError> {
    get_stakeholder_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| portfolio_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such stakeholder"))
}

/// Create a stakeholder on a project.
#[crate::action(
    name = "stakeholders.create",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderCreateIn,
) -> Result<HandlerOutput<StakeholderOut>, ActionError> {
    let action = ctx.action;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "required", "must not be empty"));
    }
    let record = store::portfolio::create_stakeholder_tx(
        &mut *tx,
        ctx.tenant,
        &NewStakeholder {
            project_id: input.project_id,
            user_id: input.user_id,
            name: name.to_owned(),
            email: trimmed(input.email),
            role: trimmed(input.role).unwrap_or_default(),
        },
    )
    .await
    .map_err(|error| portfolio_error(error, action))?;
    emit_portfolio_event(
        &mut *tx,
        &ctx,
        "stakeholder",
        "project_stakeholder",
        record.id,
        "created",
    )
    .await?;

    Ok(HandlerOutput::created(stakeholder_out(&record))
        .with_etag(etag_of(version_i64(record.version))))
}

/// Read one stakeholder.
#[crate::action(
    name = "stakeholders.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<StakeholderOut>, ActionError> {
    let record = read_stakeholder(&mut *tx, &ctx, input.id).await?;
    Ok(HandlerOutput::ok(stakeholder_out(&record)).with_etag(etag_of(version_i64(record.version))))
}

/// Edit a stakeholder; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "stakeholders.update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderUpdateIn,
) -> Result<HandlerOutput<StakeholderOut>, ActionError> {
    let action = ctx.action;
    let before = read_stakeholder(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, stakeholder_json(&before))?;

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
    let patch = StakeholderPatch {
        user_id: input.user_id,
        name,
        email: trimmed(input.email),
        role: trimmed(input.role),
    };

    let after = match update_stakeholder_tx(&mut *tx, ctx.tenant, input.id, expected, &patch).await
    {
        Ok(record) => record,
        Err(store::portfolio::PortfolioStoreError::VersionMismatch { current }) => {
            return match get_stakeholder_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| portfolio_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    stakeholder_json(&latest),
                    format!(
                        "{action}: the stakeholder is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such stakeholder")),
            };
        }
        Err(error) => return Err(portfolio_error(error, action)),
    };
    emit_portfolio_event(
        &mut *tx,
        &ctx,
        "stakeholder",
        "project_stakeholder",
        after.id,
        "updated",
    )
    .await?;

    Ok(HandlerOutput::ok(stakeholder_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// Delete a stakeholder; `If-Match` is required and a missing row is a `404`.
#[crate::action(
    name = "stakeholders.delete",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let action = ctx.action;
    let before = read_stakeholder(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, stakeholder_json(&before))?;

    match delete_stakeholder_tx(&mut *tx, ctx.tenant, input.id, expected).await {
        Ok(record) => {
            emit_portfolio_event(
                &mut *tx,
                &ctx,
                "stakeholder",
                "project_stakeholder",
                record.id,
                "deleted",
            )
            .await?;
        }
        Err(store::portfolio::PortfolioStoreError::VersionMismatch { current }) => {
            return match get_stakeholder_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| portfolio_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    stakeholder_json(&latest),
                    format!(
                        "{action}: the stakeholder is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such stakeholder")),
            };
        }
        Err(error) => return Err(portfolio_error(error, action)),
    }

    Ok(HandlerOutput::no_content(()))
}

/// One page of stakeholders, newest `updated_at` first.
#[crate::action(
    name = "stakeholders.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn stakeholders_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: StakeholderListIn,
) -> Result<Page<StakeholderOut>, ActionError> {
    let page = PortfolioPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let filter = StakeholderFilter {
        project_id: input.project_id,
    };
    let (rows, next_cursor) = list_stakeholders_tx(&mut *tx, ctx.tenant, &filter, &page)
        .await
        .map_err(|error| portfolio_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(stakeholder_out).collect(),
        next_cursor,
    })
}
