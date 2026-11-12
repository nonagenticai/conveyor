#![forbid(unsafe_code)]
//! `procedures.create` · `.get` · `.list` · `.update` · `.publish` · `.retire`.
//!
//! The version family is the model (CONTRACT §21): `create` is the only way a version appears and it
//! always produces a **draft**; `update` edits a draft only; `publish` validates the judge contract
//! (`conveyor_domain::procedure::validate_publish`) and flips the status, retiring the key's previous
//! published row in the same transaction.
//!
//! Preconditions follow CONTRACT §3 exactly: an **edit** (`update`) requires `If-Match` and answers a
//! stale one with `412` carrying the current representation; a **state transition** (`publish`,
//! `retire`) is status-guarded and takes no precondition.
//!
//! A second publish of an already-published version returns the SAME row rather than a `409`: the
//! caller asked for a state, and the state already holds (§5.2 "re-publish returns the same").

use conveyor_store as store;
use sqlx::PgConnection;

use crate::board::{etag_of, require_precondition_with};
use crate::invoke::{ActionError, HandlerCtx};
use crate::procedure::{
    PROCEDURE_PUBLISHED, PROCEDURE_RETIRED, ProcedureCreateIn, ProcedureGetIn, ProcedureIdIn,
    ProcedureListIn, ProcedureOut, ProcedureUpdateIn, cursor_of, decode_cursor,
    emit_procedure_event, invalid, new_sop, not_found, parse_status, patch_of, procedure_error,
    procedure_out, validated,
};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// Create a draft: the next version of a key.
///
/// The draft starts with the members the caller supplied and the column defaults for the rest, so a
/// caller may start a SOP before it has a judge. `publish` is where a missing judge is refused.
#[crate::action(
    name = "procedures.create",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureCreateIn,
) -> Result<HandlerOutput<ProcedureOut>, ActionError> {
    if input.key.trim().is_empty() {
        return Err(invalid("key", "required", "must not be empty"));
    }
    if input.title.trim().is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let record = store::procedure::create_draft_tx(&mut *tx, ctx.tenant, &new_sop(input))
        .await
        .map_err(|error| procedure_error(error, "procedures.create"))?;
    Ok(HandlerOutput::created(procedure_out(&record)).with_etag(etag_of(record.lock_version)))
}

/// One version, by id or by key.
#[crate::action(
    name = "procedures.get",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureGetIn,
) -> Result<ProcedureOut, ActionError> {
    let record = match (input.procedure_id, input.key.as_deref()) {
        (Some(id), None) => {
            if input.version.is_some() {
                return Err(invalid(
                    "version",
                    "invalid_value",
                    "an exact version is addressed by key, not by procedure_id",
                ));
            }
            store::procedure::get_sop_tx(&mut *tx, ctx.tenant, id)
                .await
                .map_err(|error| procedure_error(error, "procedures.get"))?
                .ok_or_else(not_found)?
        }
        (None, Some(key)) => match input.version {
            Some(version) => {
                store::procedure::get_sop_by_key_tx(&mut *tx, ctx.tenant, key, Some(version))
                    .await
                    .map_err(|error| procedure_error(error, "procedures.get"))?
                    .ok_or_else(not_found)?
            }
            // No version: the key's PUBLISHED version is what a caller means by "the procedure".
            None => store::procedure::published_by_key_tx(&mut *tx, ctx.tenant, key)
                .await
                .map_err(|error| procedure_error(error, "procedures.get"))?
                .ok_or_else(not_found)?,
        },
        (Some(_), Some(_)) => {
            return Err(invalid(
                "key",
                "invalid_value",
                "name either procedure_id or key, not both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "procedure_id",
                "required",
                "name a procedure_id or a key",
            ));
        }
    };
    Ok(procedure_out(&record))
}

/// One page of procedures, newest first.
#[crate::action(
    name = "procedures.list",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureListIn,
) -> Result<Page<ProcedureOut>, ActionError> {
    let status = input.status.as_deref().map(parse_status).transpose()?;
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);
    // One extra row decides whether a next page exists; it is not returned.
    let mut rows = store::procedure::list_sops_tx(&mut *tx, ctx.tenant, status, cursor, limit + 1)
        .await
        .map_err(|error| procedure_error(error, "procedures.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(cursor_of)
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(procedure_out).collect(),
        next_cursor,
    })
}

/// Change a draft's title, description or any of its five documents.
#[crate::action(
    name = "procedures.update",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureUpdateIn,
) -> Result<HandlerOutput<ProcedureOut>, ActionError> {
    let current = store::procedure::get_sop_tx(&mut *tx, ctx.tenant, input.procedure_id)
        .await
        .map_err(|error| procedure_error(error, "procedures.update"))?
        .ok_or_else(not_found)?;
    // Status before precondition: an edit of a published version cannot become legal whatever
    // `If-Match` says, so `409 not_draft` is the answer that tells the caller to create a new
    // version; answering `412` would send them round a GET-and-retry loop for a request that can
    // never succeed.
    if current.status != "draft" {
        return Err(ActionError::new(
            409,
            "procedure.not_draft",
            format!(
                "the procedure is {}; only a draft is editable — create a new version from it",
                current.status
            ),
        ));
    }
    if let Some(title) = input.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let before = procedure_out(&current);
    let expected = require_precondition_with(
        &ctx,
        "procedures.update",
        serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
    )?;
    let expected = i64::try_from(expected.get()).map_err(|error| {
        tracing::error!(%error, "the If-Match version is out of i64 range");
        ActionError::internal()
    })?;
    let record = store::procedure::update_draft_tx(
        &mut *tx,
        ctx.tenant,
        current.id,
        Some(expected),
        &patch_of(&input),
    )
    .await
    .map_err(|error| match error {
        // A stale `If-Match` must answer with the representation the caller should now be looking
        // at (CONTRACT §3), so a client can retry without a second GET.
        store::procedure::SopStoreError::VersionMismatch { current } => {
            ActionError::precondition_slug_with(
                "procedure.version_mismatch",
                serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
                format!("the procedure moved to version {current} before this call"),
            )
        }
        other => procedure_error(other, "procedures.update"),
    })?;
    Ok(HandlerOutput::ok(procedure_out(&record)).with_etag(etag_of(record.lock_version)))
}

/// Freeze a draft: validate its judge contract, publish it and append the event in one transaction.
#[crate::action(
    name = "procedures.publish",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureIdIn,
) -> Result<HandlerOutput<ProcedureOut>, ActionError> {
    let current = store::procedure::get_sop_tx(&mut *tx, ctx.tenant, input.procedure_id)
        .await
        .map_err(|error| procedure_error(error, "procedures.publish"))?
        .ok_or_else(not_found)?;
    if current.status == "published" {
        return Ok(
            HandlerOutput::ok(procedure_out(&current)).with_etag(etag_of(current.lock_version))
        );
    }
    if current.status != "draft" {
        return Err(ActionError::new(
            409,
            "procedure.not_draft",
            format!("the procedure is {}, not a draft", current.status),
        ));
    }
    validated(&current)?;
    let published = store::procedure::publish_tx(&mut *tx, ctx.tenant, current.id)
        .await
        .map_err(|error| procedure_error(error, "procedures.publish"))?;
    emit_procedure_event(&mut *tx, &ctx, PROCEDURE_PUBLISHED, &published).await?;
    Ok(HandlerOutput::ok(procedure_out(&published)).with_etag(etag_of(published.lock_version)))
}

/// Retire a published SOP. A step already judged against it is unaffected — it resolved the version
/// it was measured on.
#[crate::action(
    name = "procedures.retire",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn procedures_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcedureIdIn,
) -> Result<HandlerOutput<ProcedureOut>, ActionError> {
    let retired = store::procedure::retire_tx(&mut *tx, ctx.tenant, input.procedure_id)
        .await
        .map_err(|error| procedure_error(error, "procedures.retire"))?;
    emit_procedure_event(&mut *tx, &ctx, PROCEDURE_RETIRED, &retired).await?;
    Ok(HandlerOutput::ok(procedure_out(&retired)).with_etag(etag_of(retired.lock_version)))
}
