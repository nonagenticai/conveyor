//! The six `workflows.*` actions — create, edit, publish, retire, read one, list.
//!
//! Every mutation appends its event in the same transaction and returns the row with an `ETag`
//! over its version, so a caller's next edit carries the `If-Match` the store checks (CONTRACT §3).

use chrono::{DateTime, Utc};
use conveyor_store::workflow::workflow::{
    NewWorkflow, WorkflowPatch, WorkflowRecord, create_draft_tx, get_by_id_tx, get_by_key_tx,
    list_tx, publish_tx, retire_tx, update_draft_tx,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::views::{
    WorkflowCreateIn, WorkflowGetIn, WorkflowListIn, WorkflowOut, WorkflowPublishOut,
    WorkflowRefIn, WorkflowUpdateIn,
};
use super::{
    WORKFLOW_CREATED, WORKFLOW_PUBLISHED, WORKFLOW_RETIRED, WORKFLOW_UPDATED, emit_workflow_event,
    invalid, workflow_error,
};
use crate::board::{etag_of, row_version};
use crate::control::require_precondition_with;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// The statuses a list may filter by.
const STATUSES: [&str; 3] = ["draft", "published", "retired"];

// ── workflows.create ─────────────────────────────────────────────────────────────────────

/// Create the next version of a key as a draft.
#[crate::action(
    name = "workflows.create",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowCreateIn,
) -> Result<HandlerOutput<WorkflowOut>, ActionError> {
    if input.key.trim().is_empty() {
        return Err(invalid("key", "blank", "a workflow key must not be blank"));
    }
    if input.title.trim().is_empty() {
        return Err(invalid(
            "title",
            "blank",
            "a workflow title must not be blank",
        ));
    }
    let created = create_draft_tx(
        &mut *tx,
        ctx.tenant,
        NewWorkflow {
            key: input.key,
            title: input.title,
            definition: input.definition,
        },
    )
    .await
    .map_err(|error| workflow_error(error, "workflows.create"))?;
    let out = WorkflowOut::from(created);
    emit_workflow_event(
        &mut *tx,
        &ctx,
        WORKFLOW_CREATED,
        out.workflow_id,
        json!({"id": out.workflow_id, "key": out.key, "version": out.version}),
    )
    .await?;
    let etag = etag_of(out.version);
    Ok(HandlerOutput::created(out).with_etag(etag))
}

// ── workflows.update ─────────────────────────────────────────────────────────────────────

/// Replace a draft's title and/or document. `If-Match` is required.
#[crate::action(
    name = "workflows.update",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowUpdateIn,
) -> Result<HandlerOutput<WorkflowOut>, ActionError> {
    let action = "workflows.update";
    if input.title.is_none() && input.definition.is_none() {
        return Err(invalid(
            "title",
            "nothing_to_update",
            "provide a title, a definition, or both",
        ));
    }
    if let Some(title) = input.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(invalid(
            "title",
            "blank",
            "a workflow title must not be blank",
        ));
    }
    let current = get_by_id_tx(&mut *tx, ctx.tenant, input.workflow_id)
        .await
        .map_err(|error| workflow_error(error, action))?;
    let expected = require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(WorkflowOut::from(current)).unwrap_or(Value::Null),
    )?;
    let updated = update_draft_tx(
        &mut *tx,
        ctx.tenant,
        input.workflow_id,
        WorkflowPatch {
            expected: Some(expected),
            title: input.title.as_deref(),
            definition: input.definition.as_ref(),
        },
    )
    .await
    .map_err(|error| workflow_error(error, action))?;
    let out = WorkflowOut::from(updated);
    emit_workflow_event(
        &mut *tx,
        &ctx,
        WORKFLOW_UPDATED,
        out.workflow_id,
        json!({"id": out.workflow_id, "key": out.key, "version": out.version}),
    )
    .await?;
    let etag = etag_of(out.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

// ── workflows.publish ────────────────────────────────────────────────────────────────────

/// Publish a draft: freeze a revision and retire the key's previous published version.
#[crate::action(
    name = "workflows.publish",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowRefIn,
) -> Result<HandlerOutput<WorkflowPublishOut>, ActionError> {
    let action = "workflows.publish";
    let expected = ctx.version.map(row_version);
    let outcome = publish_tx(
        &mut *tx,
        ctx.tenant,
        input.workflow_id,
        expected,
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| workflow_error(error, action))?;
    let out = WorkflowPublishOut {
        workflow: WorkflowOut::from(outcome.workflow),
        revision_id: outcome.revision_id,
        revision_seq: outcome.seq,
        content_hash: outcome.content_hash.clone(),
    };
    emit_workflow_event(
        &mut *tx,
        &ctx,
        WORKFLOW_PUBLISHED,
        out.workflow.workflow_id,
        json!({
            "id": out.workflow.workflow_id,
            "key": out.workflow.key,
            "version": out.workflow.version,
            "revision_id": out.revision_id,
            "content_hash": out.content_hash,
        }),
    )
    .await?;
    let etag = etag_of(out.workflow.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

// ── workflows.retire ────────────────────────────────────────────────────────────────────

/// Retire a published version. Live runs are unaffected.
#[crate::action(
    name = "workflows.retire",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowRefIn,
) -> Result<HandlerOutput<WorkflowOut>, ActionError> {
    let action = "workflows.retire";
    let expected = ctx.version.map(row_version);
    let retired = retire_tx(&mut *tx, ctx.tenant, input.workflow_id, expected)
        .await
        .map_err(|error| workflow_error(error, action))?;
    let out = WorkflowOut::from(retired);
    emit_workflow_event(
        &mut *tx,
        &ctx,
        WORKFLOW_RETIRED,
        out.workflow_id,
        json!({"id": out.workflow_id, "key": out.key}),
    )
    .await?;
    let etag = etag_of(out.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

// ── workflows.get ───────────────────────────────────────────────────────────────────────

/// Read one version by id, or by key (highest version unless `version` names one).
#[crate::action(
    name = "workflows.get",
    version = 1,
    kind = "internal",
    scopes = ["workflows:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowGetIn,
) -> Result<HandlerOutput<WorkflowOut>, ActionError> {
    let action = "workflows.get";
    let record: WorkflowRecord = match (input.workflow_id, input.key.as_deref()) {
        (Some(id), None) => get_by_id_tx(&mut *tx, ctx.tenant, id)
            .await
            .map_err(|error| workflow_error(error, action))?,
        (None, Some(key)) => get_by_key_tx(&mut *tx, ctx.tenant, key, input.version)
            .await
            .map_err(|error| workflow_error(error, action))?,
        _ => {
            return Err(invalid(
                "workflow_id",
                "ambiguous",
                "provide exactly one of workflow_id or key",
            ));
        }
    };
    let etag = etag_of(record.version);
    Ok(HandlerOutput::ok(WorkflowOut::from(record)).with_etag(etag))
}

// ── workflows.list ──────────────────────────────────────────────────────────────────────

/// List the tenant's workflow versions, newest first.
#[crate::action(
    name = "workflows.list",
    version = 1,
    kind = "internal",
    scopes = ["workflows:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workflows_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkflowListIn,
) -> Result<HandlerOutput<Page<WorkflowOut>>, ActionError> {
    let action = "workflows.list";
    if let Some(status) = input.status.as_deref()
        && !STATUSES.contains(&status)
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "must be draft, published or retired",
        ));
    }
    let limit = i64::from(input.page.limit.unwrap_or(50).clamp(1, 200));
    let before = input
        .page
        .cursor
        .as_deref()
        .map(decode_cursor)
        .transpose()?;
    let mut rows = list_tx(
        &mut *tx,
        ctx.tenant,
        input.status.as_deref(),
        before,
        limit + 1,
    )
    .await
    .map_err(|error| workflow_error(error, action))?;
    let more = i64::try_from(rows.len()).unwrap_or(0) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_cursor = if more {
        rows.last().map(encode_cursor)
    } else {
        None
    };
    Ok(HandlerOutput::ok(Page {
        items: rows.into_iter().map(WorkflowOut::from).collect(),
        next_cursor,
    }))
}

/// `"<rfc3339>|<uuid>"` — the list's keyset, opaque to the caller.
fn encode_cursor(record: &WorkflowRecord) -> String {
    format!("{}|{}", record.created_at.to_rfc3339(), record.id)
}

/// Parse a cursor; a malformed one is a `422` on `cursor`, never an empty page.
fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), ActionError> {
    let (at, id) = cursor.split_once('|').ok_or_else(|| {
        invalid(
            "cursor",
            "invalid_value",
            "a cursor is the value a previous page returned",
        )
    })?;
    let at = DateTime::parse_from_rfc3339(at)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_value",
                "the cursor's timestamp is not RFC 3339",
            )
        })?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id)
        .map_err(|_| invalid("cursor", "invalid_value", "the cursor's id is not a uuid"))?;
    Ok((at, id))
}
