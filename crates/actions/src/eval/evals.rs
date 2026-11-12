#![forbid(unsafe_code)]
//! `evals.*` — the eval's own lifecycle and the split draw (S19 steps 4 and 6).
//!
//! - `evals.create` validates the instrument's *subject* — the kind, the trace source, and that
//!   the entry action is a registered action. `agent_run` is the one exception: the vocabulary
//!   is stable but no action executes one yet, so it is accepted here and refused at run start
//!   (Part B), where the refusal can name what is missing.
//! - `evals.update` carries only the mutable content fields. `subject_kind` and `entry_action`
//!   are not on the wire at all, so "one eval is one flow" cannot be broken by an update —
//!   the store has no statement that could change them either.
//! - `evals.retire` goes through the domain's transition table; an illegal edge is a `409`.
//! - `evals.draw_split` draws the stratified train/test split over the current version and
//!   locks it once. The lock is the store's conditional `UPDATE`; the pre-check here exists so
//!   the ordinary retry gets a clear `409 split_locked` rather than a database race.
//!
//! The split's seed is the caller's `seed` when supplied, otherwise the version's stored
//! digest: the same instrument therefore draws the same split on any machine, and a caller who
//! wants a different draw must say so.

use std::str::FromStr;

use conveyor_domain::eval::{EvalStatus, SplitCase, SubjectKind, draw_split};
use conveyor_store as store;
use conveyor_store::eval::{NewEval, UpdateEval};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::registry::registry;
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

use super::{
    EVAL_CHANGED, EVAL_CREATED, EvalView, SplitOut, TagCountOut, emit, eval_error, eval_view,
    not_ready, version_view,
};

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `evals.create` — the instrument's subject and its first content fields.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalCreateIn {
    /// The display name (non-empty).
    pub name: String,
    /// What the eval is for; `""` is legal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// `completions | copilot | process | external | agent_run`.
    pub subject_kind: String,
    /// The registered action name a run enters through.
    pub entry_action: String,
    /// The model the subject runs against, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    /// The prompt the subject runs, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// The JSON schema a case input must satisfy; `{}` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs_schema: Option<serde_json::Value>,
    /// `synthesized | conversation_turn | run_step`; defaults to `synthesized`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_source: Option<String>,
}

/// `evals.get` / `evals.retire` — one eval by id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalIdIn {
    /// The eval id, resolved in the caller's tenant.
    pub id: Uuid,
}

/// `evals.list` — the status filter plus the shared page envelope.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalListIn {
    /// Only evals in this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// `evals.update` — the mutable content fields only.
///
/// `subject_kind` and `entry_action` are deliberately absent: an eval measures one flow, and
/// reinterpreting every stored version by changing what it measures is refused rather than
/// supported. An absent field keeps the stored value; an **empty** `model_ref`/`prompt_ref`
/// clears it (there is no other way to unset a nullable field on a patch).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalUpdateIn {
    /// The eval to patch.
    pub id: Uuid,
    /// The display name (non-empty); absent keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the eval is for; absent keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// The model reference; absent keeps it, `""` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    /// The prompt reference; absent keeps it, `""` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// The JSON schema a case input must satisfy; absent keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs_schema: Option<serde_json::Value>,
    /// `synthesized | conversation_turn | run_step`; absent keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_source: Option<String>,
}

/// `evals.draw_split` — the ratio and, optionally, the seed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DrawSplitIn {
    /// The eval whose current version is being split.
    pub eval_id: Uuid,
    /// The fraction of each stratum assigned to `train`; strictly between 0 and 1.
    pub train_ratio: f64,
    /// The draw's seed. Absent means the version's stored digest, which makes the draw a
    /// property of the instrument rather than of the moment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Create the eval and announce it on its own topic.
#[crate::action(
    name = "evals.create",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvalCreateIn,
) -> Result<HandlerOutput<EvalView>, ActionError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("name", "missing", "a name is required"));
    }

    let subject_kind = SubjectKind::from_str(input.subject_kind.trim()).map_err(|_| {
        invalid(
            "subject_kind",
            "invalid_value",
            "must be completions, copilot, process, external or agent_run",
        )
    })?;

    let trace_source = input
        .trace_source
        .as_deref()
        .map(str::trim)
        .unwrap_or("synthesized");
    if !matches!(
        trace_source,
        "synthesized" | "conversation_turn" | "run_step"
    ) {
        return Err(invalid(
            "trace_source",
            "invalid_value",
            "must be synthesized, conversation_turn or run_step",
        ));
    }

    let entry_action = input.entry_action.trim();
    if entry_action.is_empty() {
        return Err(invalid(
            "entry_action",
            "missing",
            "an entry action is required",
        ));
    }
    // `agent_run` is declared but not runnable: no registered action executes one yet, so the
    // name is accepted here and refused at run start (Part B), where the refusal can say which
    // action is missing. Every other kind must name an action that exists NOW, because the
    // eval is unbuildable otherwise and the caller can be told at the cheapest moment.
    if subject_kind != SubjectKind::AgentRun && registry().get(entry_action).is_none() {
        return Err(invalid(
            "entry_action",
            "unknown_action",
            format!("no action {entry_action:?} is registered"),
        ));
    }

    let empty_schema = serde_json::json!({});
    let new = NewEval {
        name,
        purpose: input.purpose.as_deref().unwrap_or(""),
        subject_kind: subject_kind.as_str(),
        entry_action,
        model_ref: blank_to_none(input.model_ref.as_deref()),
        prompt_ref: blank_to_none(input.prompt_ref.as_deref()),
        inputs_schema: input.inputs_schema.as_ref().unwrap_or(&empty_schema),
        trace_source,
        created_by: Some(ctx.actor_id),
    };
    let eval = store::eval::insert_eval_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let view = eval_view(&eval);
    emit(
        &mut *tx,
        &ctx,
        eval.id,
        eval.id,
        eval.status.as_str(),
        EVAL_CREATED,
        serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(view))
}

/// One eval by id.
#[crate::action(
    name = "evals.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvalIdIn,
) -> Result<EvalView, ActionError> {
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(eval_view(&eval))
}

/// One page of evals, newest first.
#[crate::action(
    name = "evals.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvalListIn,
) -> Result<Page<EvalView>, ActionError> {
    let status = match input.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(
            EvalStatus::from_str(raw)
                .map_err(|_| {
                    invalid(
                        "status",
                        "invalid_value",
                        "must be draft, running, climbing or retired",
                    )
                })?
                .as_str(),
        ),
    };
    let cursor = decode_time_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);

    // One extra row decides whether a next page exists; it is never returned.
    let mut rows = store::eval::list_evals_tx(&mut *tx, ctx.tenant, status, cursor, limit + 1)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(eval_view).collect(),
        next_cursor,
    })
}

/// Patch the eval's content fields.
#[crate::action(
    name = "evals.update",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvalUpdateIn,
) -> Result<EvalView, ActionError> {
    let current = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    // Merge: an absent field keeps the stored value, so a patch cannot accidentally blank a
    // column it did not mention. An explicitly empty model/prompt reference clears it.
    let name = match input.name.as_deref() {
        Some(raw) => raw.trim().to_owned(),
        None => current.name.clone(),
    };
    if name.is_empty() {
        return Err(invalid("name", "missing", "a name is required"));
    }
    let purpose = input.purpose.clone().unwrap_or(current.purpose.clone());
    let model_ref = match input.model_ref.as_deref() {
        Some(raw) => blank_to_none(Some(raw)).map(str::to_owned),
        None => current.model_ref.clone(),
    };
    let prompt_ref = match input.prompt_ref.as_deref() {
        Some(raw) => blank_to_none(Some(raw)).map(str::to_owned),
        None => current.prompt_ref.clone(),
    };
    let inputs_schema = input
        .inputs_schema
        .clone()
        .unwrap_or_else(|| current.inputs_schema.clone());
    let trace_source = input
        .trace_source
        .as_deref()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_else(|| current.trace_source.clone());
    if !matches!(
        trace_source.as_str(),
        "synthesized" | "conversation_turn" | "run_step"
    ) {
        return Err(invalid(
            "trace_source",
            "invalid_value",
            "must be synthesized, conversation_turn or run_step",
        ));
    }

    let patch = UpdateEval {
        name: &name,
        purpose: &purpose,
        model_ref: model_ref.as_deref(),
        prompt_ref: prompt_ref.as_deref(),
        inputs_schema: &inputs_schema,
        trace_source: &trace_source,
    };
    let eval = store::eval::update_eval_tx(&mut *tx, ctx.tenant, input.id, &patch)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let view = eval_view(&eval);
    emit(
        &mut *tx,
        &ctx,
        eval.id,
        eval.id,
        eval.status.as_str(),
        EVAL_CHANGED,
        serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(view)
}

/// Withdraw the eval: `draft | running | climbing → retired`.
#[crate::action(
    name = "evals.retire",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvalIdIn,
) -> Result<EvalView, ActionError> {
    let eval = store::eval::transition_eval_tx(&mut *tx, ctx.tenant, input.id, EvalStatus::Retired)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let view = eval_view(&eval);
    emit(
        &mut *tx,
        &ctx,
        eval.id,
        eval.id,
        eval.status.as_str(),
        EVAL_CHANGED,
        serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
    )
    .await?;
    Ok(view)
}

/// Draw a stratified train/test split over the current version and lock it.
///
/// The draw sees ids and tags only ([`conveyor_domain::eval::draw_split`]'s signature), so it
/// cannot be chosen by a baseline score. The lock is one-shot: the version's `split_sha256`
/// being set is a `409`, and the store's conditional `UPDATE` is the guarantee behind this
/// pre-check.
#[crate::action(
    name = "evals.draw_split",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn evals_draw_split(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DrawSplitIn,
) -> Result<SplitOut, ActionError> {
    if !input.train_ratio.is_finite() || input.train_ratio <= 0.0 || input.train_ratio >= 1.0 {
        return Err(invalid(
            "train_ratio",
            "invalid_value",
            "must be a finite number strictly between 0 and 1",
        ));
    }

    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let Some(version_id) = eval.current_version_id else {
        return Err(not_ready(
            ctx.action,
            "the eval has no version; create one before drawing a split",
        ));
    };
    let version = store::eval::get_version_tx(&mut *tx, ctx.tenant, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    if version.split_sha256.is_some() {
        return Err(ActionError::new(
            409,
            "split_locked",
            format!(
                "{}: version {} already has a locked split",
                ctx.action, version.version
            ),
        ));
    }

    let rows = store::eval::cases_for_split_tx(&mut *tx, ctx.tenant, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    if rows.len() < 2 {
        return Err(ActionError::new(
            422,
            "split_too_small",
            format!(
                "{}: a split needs at least 2 cases, the version has {}",
                ctx.action,
                rows.len()
            ),
        ));
    }

    let cases: Vec<SplitCase<'_>> = rows
        .iter()
        .map(|(id, tags)| SplitCase {
            id: *id,
            tags: tags.as_slice(),
        })
        .collect();
    // The seed is the caller's, or the instrument's own digest: the same version therefore
    // draws the same split everywhere, and a different draw must be asked for.
    let seed = match input.seed.as_deref() {
        Some(seed) => seed.as_bytes().to_vec(),
        None => version.digest.as_bytes().to_vec(),
    };
    let assignment = draw_split(&cases, input.train_ratio, &seed).map_err(|error| match error {
        conveyor_domain::eval::EvalError::Ratio => invalid(
            "train_ratio",
            "invalid_value",
            "must be a finite number strictly between 0 and 1",
        ),
        conveyor_domain::eval::EvalError::NoCases => ActionError::new(
            422,
            "split_too_small",
            format!("{}: a split needs at least 2 cases", ctx.action),
        ),
        other => ActionError::new(409, "conflict", other.to_string()),
    })?;

    let locked = store::eval::lock_split_tx(
        &mut *tx,
        ctx.tenant,
        version_id,
        &assignment.split_sha256,
        &assignment.per_case,
    )
    .await
    .map_err(|error| eval_error(error, ctx.action))?;

    // Locking the split is an instrument change: the assignment is now fixed and a re-draw
    // is refused, so a subscriber watching `eval:{id}` must see it like any other edit.
    let view = version_view(&locked);
    emit(
        &mut *tx,
        &ctx,
        version.eval_id,
        locked.id,
        "split_locked",
        EVAL_CHANGED,
        serde_json::to_value(&view).unwrap_or(serde_json::Value::Null),
    )
    .await?;

    Ok(SplitOut {
        version: view,
        per_tag: assignment
            .per_tag
            .iter()
            .map(|counts| TagCountOut {
                tag: counts.tag.clone(),
                total: i64::try_from(counts.total).unwrap_or(i64::MAX),
                train: i64::try_from(counts.train).unwrap_or(i64::MAX),
                test: i64::try_from(counts.test).unwrap_or(i64::MAX),
            })
            .collect(),
        // Part A has no baseline data to compare train and test means over; the hard rule is
        // the sign-off gate, and a mismatch is a Part C audit finding, never a re-draw.
        warnings: Vec::new(),
    })
}

// ── shared helpers ─────────────────────────────────────────────────────────────────────

/// `None` for an absent or blank reference, so `""` clears a nullable text field.
pub(crate) fn blank_to_none(value: Option<&str>) -> Option<&str> {
    value.filter(|raw| !raw.trim().is_empty())
}

/// Decode the shared `(created_at, id)` page cursor, `422` when it was not issued here.
pub(crate) fn decode_time_cursor(
    raw: Option<&str>,
) -> Result<Option<(chrono::DateTime<chrono::Utc>, Uuid)>, ActionError> {
    match raw {
        None => Ok(None),
        Some(raw) => store::actions::decode_time_cursor(raw)
            .map(Some)
            .ok_or_else(|| {
                invalid(
                    "cursor",
                    "invalid_cursor",
                    "not a cursor this endpoint issued",
                )
            }),
    }
}
