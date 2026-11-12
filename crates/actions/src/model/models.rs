#![forbid(unsafe_code)]
//! `models.*` — the model header, publish, compare and validate (S08 Part B §5.9).
//!
//! A model is created against **exactly one profile**: `language` and `language_version` are
//! resolved at create time and every later write is judged by that profile. An unknown pair
//! is a `422` (never a default), because a model whose profile does not exist would validate
//! against nothing.
//!
//! **Publish is the only writer of a revision** (Part B decision 3): the graph is snapshotted,
//! hashed and frozen in one transaction, and a graph over `revision.content`'s 256 KiB cap is
//! refused rather than truncated. Every other edit bumps `version` and nothing else.
//!
//! **Validate is post-commit by construction**: the action hashes the graph it read inside its
//! transaction and enqueues a `model.validate` job on the same connection; the worker walks
//! the graph afterwards, so a full check never holds the model's row lock (slice trap 6).

use conveyor_domain::compare;
use conveyor_domain::model::ModelSnapshot;
use conveyor_store as store;
use conveyor_store::model::{
    ModelPage, ModelPatch, ModelRecord, NewModel, get_model_tx, list_models_tx, update_model_tx,
};
use serde_json::Value;
use sqlx::PgConnection;

pub use crate::board::views::IdIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::{
    MODEL_CREATED, MODEL_PUBLISHED, ModelCompareIn, ModelCompareOut, ModelCreateIn, ModelListIn,
    ModelOut, ModelPublishIn, ModelPublishOut, ModelUpdateIn, ModelValidateIn, ModelValidateOut,
    ValidationRunOut, emit_model_event, etag_of, invalid, model_error, profile_or_refuse,
    require_precondition_with, version_i64,
};

/// One model as every action of this family returns it.
pub(crate) fn model_out(record: &ModelRecord) -> ModelOut {
    ModelOut {
        id: record.id.to_string(),
        project_id: record.project_id.map(|id| id.to_string()),
        language: record.language.clone(),
        language_version: record.language_version.clone(),
        title: record.title.clone(),
        description: record.description.clone(),
        head_revision: i64::from(record.head_revision),
        published_revision_id: record.published_revision_id.map(|id| id.to_string()),
        meta: record.meta.clone(),
        version: version_i64(record.version),
        created_at: record.created_at,
        updated_at: record.updated_at,
        latest_validation_run: None,
    }
}

/// The representation a `412` hands back.
fn model_json(record: &ModelRecord) -> Value {
    serde_json::to_value(model_out(record)).unwrap_or(Value::Null)
}

/// Read one model or `404`.
pub(crate) async fn read_model(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
) -> Result<ModelRecord, ActionError> {
    get_model_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("such model"))
}

/// Create a model against one profile.
#[crate::action(
    name = "models.create",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelCreateIn,
) -> Result<HandlerOutput<ModelOut>, ActionError> {
    let action = ctx.action;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    // A model's profile is fixed for its lifetime, so an unknown pair is refused here and
    // never re-defaulted later.
    profile_or_refuse(&input.language, &input.language_version)?;

    let record = store::model::create_model_tx(
        &mut *tx,
        ctx.tenant,
        &NewModel {
            project_id: input.project_id,
            language: input.language,
            language_version: input.language_version,
            title: title.to_owned(),
            description: input.description,
            meta: input
                .meta
                .unwrap_or_else(|| Value::Object(Default::default())),
        },
    )
    .await
    .map_err(|error| model_error(error, action))?;

    emit_model_event(
        &mut *tx,
        &ctx,
        MODEL_CREATED,
        record.id,
        serde_json::json!({
            "model_id": record.id,
            "language": record.language,
            "language_version": record.language_version,
        }),
    )
    .await?;

    Ok(HandlerOutput::created(model_out(&record)).with_etag(etag_of(version_i64(record.version))))
}

/// Read one model, with its newest validation run.
#[crate::action(
    name = "models.get",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ModelOut>, ActionError> {
    let record = read_model(&mut *tx, &ctx, input.id).await?;
    let run = store::validation::latest_run_tx(&mut *tx, ctx.tenant, record.id)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    let mut out = model_out(&record);
    out.latest_validation_run = run.map(|run| ValidationRunOut {
        id: run.id.to_string(),
        status: run.status,
        ruleset: run.ruleset,
        ruleset_version: run.ruleset_version,
        content_hash: run.content_hash,
        findings: run.findings,
        created_at: run.created_at,
    });
    let etag = etag_of(version_i64(record.version));
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

/// Edit a model; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "models.update",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelUpdateIn,
) -> Result<HandlerOutput<ModelOut>, ActionError> {
    let action = ctx.action;
    let before = read_model(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, model_json(&before))?;

    let title = match input.title {
        Some(title) => {
            let title = title.trim();
            if title.is_empty() {
                return Err(invalid("title", "required", "must not be empty"));
            }
            Some(title.to_owned())
        }
        None => None,
    };
    let patch = ModelPatch {
        title,
        description: input.description,
        project_id: input.project_id,
        meta: input.meta,
    };

    let after = match update_model_tx(&mut *tx, ctx.tenant, input.id, expected, &patch).await {
        Ok(record) => record,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            return match get_model_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    model_json(&latest),
                    format!(
                        "{action}: the model is at version {}, not the version the caller supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such model")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    Ok(HandlerOutput::ok(model_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// Delete a model and everything it owns.
///
/// No event: §5.11 declares four model event types and deletion is not one of them, so a
/// subscriber learns nothing it could act on. The row, its graph and its revisions are gone.
#[crate::action(
    name = "models.delete",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ModelOut>, ActionError> {
    let action = ctx.action;
    let before = read_model(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, model_json(&before))?;
    let removed = store::model::delete_model_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| model_error(error, action))?;
    Ok(HandlerOutput::ok(model_out(&removed)))
}

/// One page of models, newest `updated_at` first.
#[crate::action(
    name = "models.list",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelListIn,
) -> Result<Page<ModelOut>, ActionError> {
    let page = ModelPage {
        cursor: input.page.cursor,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let (rows, next_cursor) = list_models_tx(&mut *tx, ctx.tenant, &page, input.project_id)
        .await
        .map_err(|error| model_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(model_out).collect(),
        next_cursor,
    })
}

/// Freeze the current graph into a new published revision.
#[crate::action(
    name = "models.publish",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelPublishIn,
) -> Result<HandlerOutput<ModelPublishOut>, ActionError> {
    let action = ctx.action;
    let before = read_model(&mut *tx, &ctx, input.id).await?;
    let expected = require_precondition_with(&ctx, action, model_json(&before))?;

    let outcome = match store::model::publish_model_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        Some(ctx.actor_id),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(store::model::ModelStoreError::VersionMismatch { current }) => {
            // Nothing was written; re-read so the 412 still carries the current
            // representation (CONTRACT §3), exactly as `models.update` does.
            return match get_model_tx(&mut *tx, ctx.tenant, input.id)
                .await
                .map_err(|error| model_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    model_json(&latest),
                    format!(
                        "{action}: the model is at version {}, not the version the caller supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("such model")),
            };
        }
        Err(error) => return Err(model_error(error, action)),
    };

    emit_model_event(
        &mut *tx,
        &ctx,
        MODEL_PUBLISHED,
        outcome.model.id,
        serde_json::json!({
            "model_id": outcome.model.id,
            "revision_id": outcome.revision_id,
            "seq": outcome.seq,
            "content_hash": outcome.content_hash,
        }),
    )
    .await?;

    let etag = etag_of(version_i64(outcome.model.version));
    Ok(HandlerOutput::created(ModelPublishOut {
        model: model_out(&outcome.model),
        revision_id: outcome.revision_id.to_string(),
        seq: i64::from(outcome.seq),
        content_hash: outcome.content_hash,
    })
    .with_etag(etag))
}

/// Compare any two revisions of one model.
///
/// Both ids are resolved in the caller's tenant; the statuses are not consulted, so a draft
/// written by `models.import_xmi` can be compared with the published one — which is the point
/// of importing: you want to see what the document would change.
#[crate::action(
    name = "models.compare",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_compare(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelCompareIn,
) -> Result<HandlerOutput<ModelCompareOut>, ActionError> {
    let action = ctx.action;
    let from = store::model::revision_content_tx(
        &mut *tx,
        ctx.tenant,
        input.model_id,
        input.from_revision_id,
    )
    .await
    .map_err(|error| model_error(error, action))?
    .ok_or_else(|| ActionError::resource_not_found("such revision on this model"))?;
    let to = store::model::revision_content_tx(
        &mut *tx,
        ctx.tenant,
        input.model_id,
        input.to_revision_id,
    )
    .await
    .map_err(|error| model_error(error, action))?
    .ok_or_else(|| ActionError::resource_not_found("such revision on this model"))?;

    // `compare` matches array members by their string `id`, so an element or relationship is
    // reported as changed rather than as a removal plus an addition.
    let diff = compare::diff(&from, &to);
    let diff_value = serde_json::to_value(&diff).unwrap_or(Value::Null);

    Ok(HandlerOutput::ok(ModelCompareOut {
        from_revision_id: input.from_revision_id.to_string(),
        to_revision_id: input.to_revision_id.to_string(),
        from_content_hash: compare::content_hash(&from),
        to_content_hash: compare::content_hash(&to),
        diff: diff_value,
    }))
}

/// Queue a validation run for the model's current graph.
#[crate::action(
    name = "models.validate",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_validate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ModelValidateIn,
) -> Result<HandlerOutput<ModelValidateOut>, ActionError> {
    let action = ctx.action;
    let record = read_model(&mut *tx, &ctx, input.id).await?;
    // The hash is taken from the graph this transaction reads, and it is the job's dedupe
    // key: a second request for an unchanged graph joins the live job instead of adding one.
    let snapshot: ModelSnapshot = store::model::load_snapshot_tx(&mut *tx, ctx.tenant, record.id)
        .await
        .map_err(|error| model_error(error, action))?;
    let content_hash = snapshot.content_hash();
    let job = store::model::enqueue_validate_tx(&mut *tx, ctx.tenant, record.id, &content_hash)
        .await
        .map_err(|error| model_error(error, action))?;

    Ok(HandlerOutput::created(ModelValidateOut {
        queued: job.is_some(),
        job_id: job.map(|id| id.to_string()),
        content_hash,
    }))
}
