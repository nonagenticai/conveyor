#![forbid(unsafe_code)]
//! `eval_cases.*` — the instrument's inputs, and the one place an answer key is written
//! (S19 step 5; CONTRACT §20 trap 6).
//!
//! ⛔ **`expected` exists only on the input types.** It is on [`CaseIn`] and [`CaseUpdateIn`];
//! [`CaseView`] has no field for it and no list row carries it, so a response cannot leak a key
//! even by accident. The store enforces the same thing one layer down (`EvalCase` has no
//! `expected`).
//!
//! **The update is a replacement, not a merge.** The store's `UpdateEvalCase` is the case's
//! whole content document, so every content field is required here: a partial send would
//! silently blank the columns it omitted (a case's attachments and meta are load-bearing for
//! the digest). Only the two genuinely nullable fields — `expected` and `gold_model` — may be
//! omitted, which is how they are cleared.
//!
//! **Import copies; it never links.** Each entry becomes a fresh row under the target version,
//! with its own ordinal allocated under the version lock; two evals can therefore hold the
//! same cases without one edit reaching the other. The whole call is one transaction and one
//! event, capped at 200 entries.

use conveyor_store as store;
use conveyor_store::eval::{NewEvalCase, UpdateEvalCase};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

use super::{CaseView, EVAL_CASE_CHANGED, case_view, emit, eval_error};

/// The most cases one `eval_cases.import` call may copy.
pub const IMPORT_CAP: usize = 200;

/// The case source vocabulary the database's CHECK also carries.
const SOURCES: [&str; 4] = ["production", "ticket", "handwritten", "synthesized"];
/// How a case's answer key was produced.
const GOLD_PROVENANCE: [&str; 3] = ["human_written", "human_verified", "model"];

/// One case to append (shared by `create` and every `import` entry).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseIn {
    /// The input a subject is shown (non-empty).
    pub input: String,
    /// The case's tags; the first one is the split's stratum. Defaults to none.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The answer key. **Write-only**: it never comes back out of any read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Structured attachments (files, context) the case needs; defaults to `[]`.
    #[serde(default = "empty_array")]
    pub attachments: Value,
    /// `production | ticket | handwritten | synthesized`; defaults to `handwritten`.
    #[serde(default = "default_source")]
    pub source: String,
    /// `human_written | human_verified | model`; defaults to `human_written`.
    #[serde(default = "default_gold_provenance")]
    pub gold_provenance: String,
    /// The model that produced a `model`-provenance key; required exactly then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold_model: Option<String>,
    /// Free-form case metadata; defaults to `{}`.
    #[serde(default = "empty_object")]
    pub meta: Value,
}

/// `eval_cases.create` — the version plus the case.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseCreateIn {
    /// The version the case belongs to.
    pub version_id: Uuid,
    /// The case itself.
    #[serde(flatten)]
    pub case: CaseIn,
}

/// `eval_cases.update` — the case id plus its whole content document.
///
/// `split`, `version_id` and `ordinal` are not patchable: the side is written only by the split
/// lock, a case never moves between versions, and its position is allocated on insert.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseUpdateIn {
    /// The case to replace.
    pub id: Uuid,
    /// The input a subject is shown (non-empty).
    pub input: String,
    /// The case's tags; the first one is the split's stratum.
    pub tags: Vec<String>,
    /// The answer key; absent clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: Value,
    /// `production | ticket | handwritten | synthesized`.
    pub source: String,
    /// `human_written | human_verified | model`.
    pub gold_provenance: String,
    /// The model that produced a `model`-provenance key; absent clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold_model: Option<String>,
    /// Free-form case metadata.
    pub meta: Value,
}

/// `eval_cases.delete` — the version and the case.
///
/// The version is named so the event can be published on `eval:{eval_id}`: the store has no
/// case → version read (a delete is the one write that cannot return the row it removed), and
/// inventing a second query for it would be a second SQL path outside the store.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseDeleteIn {
    /// The version the case belongs to, resolved in the caller's tenant.
    pub version_id: Uuid,
    /// The case to delete.
    pub id: Uuid,
}

/// `eval_cases.delete` — what was removed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseDeleteOut {
    /// The deleted case id.
    pub id: String,
    /// Always `true`: an absent case is a `404`, never a silent success.
    pub deleted: bool,
}

/// `eval_cases.list` — one version's cases plus the shared page envelope.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseListIn {
    /// The version whose cases are listed.
    pub version_id: Uuid,
    /// The shared pagination envelope (cursor + limit).
    #[serde(flatten)]
    pub page: PageIn,
}

/// `eval_cases.import` — the target version and the cases to copy into it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseImportIn {
    /// The version the cases are copied into.
    pub version_id: Uuid,
    /// The cases; at most [`IMPORT_CAP`] per call.
    pub cases: Vec<CaseIn>,
}

/// `eval_cases.import` — one row per copied case, in the order they were sent.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseImportOut {
    /// The created cases.
    pub imported: Vec<CaseView>,
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Append one case to a version.
#[crate::action(
    name = "eval_cases.create",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_cases_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseCreateIn,
) -> Result<HandlerOutput<CaseView>, ActionError> {
    let eval_id = eval_for_version(&mut *tx, &ctx, input.version_id).await?;
    validate_case(&input.case, "case")?;
    let new = new_case(input.version_id, &input.case);
    let case = store::eval::insert_case_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let view = case_view(&case);
    emit(
        &mut *tx,
        &ctx,
        eval_id,
        case.id,
        &case.split,
        EVAL_CASE_CHANGED,
        serde_json::to_value(&view).unwrap_or(Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(view))
}

/// Replace one case's content.
#[crate::action(
    name = "eval_cases.update",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_cases_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseUpdateIn,
) -> Result<CaseView, ActionError> {
    validate_input_text(&input.input, "input")?;
    validate_source_at(input.source.trim(), "source")?;
    validate_gold_at(
        input.gold_provenance.trim(),
        input.gold_model.as_deref(),
        "gold_provenance",
        "gold_model",
    )?;

    let patch = UpdateEvalCase {
        input: input.input.trim(),
        tags: &input.tags,
        expected: input.expected.as_ref(),
        attachments: &input.attachments,
        source: input.source.trim(),
        gold_provenance: input.gold_provenance.trim(),
        gold_model: super::evals::blank_to_none(input.gold_model.as_deref()),
        meta: &input.meta,
    };
    let case = store::eval::update_case_tx(&mut *tx, ctx.tenant, input.id, &patch)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let eval_id = eval_for_version(&mut *tx, &ctx, case.version_id).await?;

    let view = case_view(&case);
    emit(
        &mut *tx,
        &ctx,
        eval_id,
        case.id,
        &case.split,
        EVAL_CASE_CHANGED,
        serde_json::to_value(&view).unwrap_or(Value::Null),
    )
    .await?;
    Ok(view)
}

/// Delete one case.
#[crate::action(
    name = "eval_cases.delete",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_cases_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseDeleteIn,
) -> Result<CaseDeleteOut, ActionError> {
    let eval_id = eval_for_version(&mut *tx, &ctx, input.version_id).await?;
    store::eval::delete_case_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    emit(
        &mut *tx,
        &ctx,
        eval_id,
        input.id,
        "deleted",
        EVAL_CASE_CHANGED,
        Value::Null,
    )
    .await?;
    Ok(CaseDeleteOut {
        id: input.id.to_string(),
        deleted: true,
    })
}

/// One page of a version's cases, in ordinal order — never including an answer key.
#[crate::action(
    name = "eval_cases.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_cases_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseListIn,
) -> Result<Page<CaseView>, ActionError> {
    let cursor = decode_case_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);

    let mut rows =
        store::eval::list_cases_tx(&mut *tx, ctx.tenant, input.version_id, cursor, limit + 1)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|row| format!("{}:{}", row.ordinal, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(case_view).collect(),
        next_cursor,
    })
}

/// Copy up to [`IMPORT_CAP`] cases into a version, all or nothing.
#[crate::action(
    name = "eval_cases.import",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_cases_import(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseImportIn,
) -> Result<HandlerOutput<CaseImportOut>, ActionError> {
    if input.cases.len() > IMPORT_CAP {
        return Err(invalid(
            "cases",
            "too_many",
            format!("at most {IMPORT_CAP} cases may be imported per call"),
        ));
    }
    let eval_id = eval_for_version(&mut *tx, &ctx, input.version_id).await?;

    // Validate every entry BEFORE the first insert: the transaction would roll the partial rows
    // back anyway, but a caller gets one refusal naming all the bad entries rather than
    // whichever one the loop reached last.
    let mut errors = Vec::new();
    for (index, case) in input.cases.iter().enumerate() {
        if let Err(error) = validate_case(case, &format!("cases.{index}")) {
            errors.extend(error.errors);
        }
    }
    if !errors.is_empty() {
        return Err(ActionError::invalid_input(errors));
    }

    let mut imported = Vec::with_capacity(input.cases.len());
    for case in &input.cases {
        let new = new_case(input.version_id, case);
        let stored = store::eval::insert_case_tx(&mut *tx, ctx.tenant, &new)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        imported.push(stored);
    }

    let views: Vec<CaseView> = imported.iter().map(case_view).collect();
    emit(
        &mut *tx,
        &ctx,
        eval_id,
        input.version_id,
        "imported",
        EVAL_CASE_CHANGED,
        serde_json::to_value(&views).unwrap_or(Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(CaseImportOut { imported: views }))
}

// ── serde defaults ─────────────────────────────────────────────────────────────────────

/// The `[]` a case's `attachments` defaults to (the column's own default).
fn empty_array() -> Value {
    Value::Array(Vec::new())
}

/// The `{}` a case's `meta` defaults to (the column's own default).
fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// `handwritten`, the column's default.
fn default_source() -> String {
    "handwritten".to_owned()
}

/// `human_written`, the column's default.
fn default_gold_provenance() -> String {
    "human_written".to_owned()
}

// ── helpers ────────────────────────────────────────────────────────────────────────────

/// The eval a version belongs to — the event topic, and the in-tenant proof the version exists.
async fn eval_for_version(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    version_id: Uuid,
) -> Result<Uuid, ActionError> {
    let version = store::eval::get_version_tx(conn, ctx.tenant, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(version.eval_id)
}

/// Build the store's insert document from a wire case.
fn new_case(version_id: Uuid, case: &CaseIn) -> NewEvalCase<'_> {
    NewEvalCase {
        version_id,
        input: case.input.trim(),
        tags: &case.tags,
        expected: case.expected.as_ref(),
        attachments: &case.attachments,
        source: case.source.trim(),
        gold_provenance: case.gold_provenance.trim(),
        gold_model: super::evals::blank_to_none(case.gold_model.as_deref()),
        meta: &case.meta,
    }
}

/// Validate one case: a non-empty input, then the two closed vocabularies.
fn validate_case(case: &CaseIn, prefix: &str) -> Result<(), ActionError> {
    validate_input_text(&case.input, &format!("{prefix}.input"))?;
    validate_source_at(case.source.trim(), &format!("{prefix}.source"))?;
    validate_gold_at(
        case.gold_provenance.trim(),
        case.gold_model.as_deref(),
        &format!("{prefix}.gold_provenance"),
        &format!("{prefix}.gold_model"),
    )
}

fn validate_input_text(raw: &str, field: &str) -> Result<(), ActionError> {
    if raw.trim().is_empty() {
        return Err(invalid(field, "missing", "a case input is required"));
    }
    Ok(())
}

fn validate_source_at(raw: &str, field: &str) -> Result<(), ActionError> {
    if SOURCES.contains(&raw) {
        Ok(())
    } else {
        Err(invalid(
            field,
            "invalid_value",
            "must be production, ticket, handwritten or synthesized",
        ))
    }
}

fn validate_gold_at(
    provenance: &str,
    model: Option<&str>,
    field: &str,
    model_field: &str,
) -> Result<(), ActionError> {
    if !GOLD_PROVENANCE.contains(&provenance) {
        return Err(invalid(
            field,
            "invalid_value",
            "must be human_written, human_verified or model",
        ));
    }
    if provenance == "model" && model.is_none_or(|raw| raw.trim().is_empty()) {
        return Err(invalid(
            model_field,
            "missing",
            "a model-derived answer key must name its model",
        ));
    }
    Ok(())
}

/// Decode the `"{ordinal}:{id}"` case cursor, `422` when it was not issued here.
fn decode_case_cursor(raw: Option<&str>) -> Result<Option<(i32, Uuid)>, ActionError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let bad = || {
        invalid(
            "cursor",
            "invalid_cursor",
            "not a cursor this endpoint issued",
        )
    };
    let (ordinal, id) = raw.split_once(':').ok_or_else(bad)?;
    let ordinal = ordinal.parse::<i32>().map_err(|_| bad())?;
    let id = Uuid::parse_str(id).map_err(|_| bad())?;
    Ok(Some((ordinal, id)))
}
