#![forbid(unsafe_code)]
//! `eval_metrics.*` — what a run is graded and measured on (S19 step 5).
//!
//! A metric is **quality** (graded) or **perf** (measured, never graded). The database's three
//! CHECKs are the guarantee — `eval_metric_perf_ungraded`, `eval_metric_rubric_required`, the
//! key grammar and the 14-character label — and every one of them is pre-validated here so an
//! ordinary mistake is a `422` naming the field rather than a constraint name from the driver.
//!
//! Metrics are part of the instrument digest, so every write here reopens both sign-off gates
//! with no status write anywhere; that is why each one emits `eval.changed` on the eval's topic
//! rather than nothing.

use conveyor_store as store;
use conveyor_store::eval::{NewEvalMetric, UpdateEvalMetric};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{EVAL_CHANGED, MetricView, emit, eval_error, metric_view};

/// The metric key grammar — the database's `CHECK (key ~ '^[a-z][a-z0-9_]*$')`.
pub const KEY_GRAMMAR: &str = "^[a-z][a-z0-9_]*$";
/// The longest report legend the schema accepts.
pub const LABEL_MAX: usize = 14;

const ROLES: [&str; 2] = ["quality", "perf"];
const KINDS: [&str; 3] = ["binary", "judge", "float"];
const BETTER: [&str; 2] = ["higher", "lower"];
const GRADER_KINDS: [&str; 5] = ["programmatic", "pairwise", "rubric", "human", "none"];

/// `eval_metrics.create` — the version plus the metric.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricCreateIn {
    /// The version the metric belongs to.
    pub version_id: Uuid,
    /// The report's column key (`^[a-z][a-z0-9_]*$`).
    pub key: String,
    /// `quality | perf`.
    pub role: String,
    /// `binary | judge | float`.
    pub kind: String,
    /// `higher | lower`; defaults to `higher`.
    #[serde(default = "default_better")]
    pub better: String,
    /// The legend text, at most 14 characters.
    pub label: String,
    /// The unit shown next to the value, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: String,
    /// The grader's configuration object; defaults to `{}`.
    #[serde(default = "empty_object")]
    pub grader: Value,
    /// The rubric text; required by `rubric` and `pairwise` graders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric: Option<String>,
    /// The metric's position in the version; defaults to 0.
    #[serde(default)]
    pub ordinal: i32,
}

/// `eval_metrics.update` — the metric id plus its whole content document.
///
/// `version_id` is not patchable: a metric belongs to the version that defined it, and a new
/// version is minted by copying, never by moving rows between versions.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricUpdateIn {
    /// The metric to replace.
    pub id: Uuid,
    /// The report's column key (`^[a-z][a-z0-9_]*$`).
    pub key: String,
    /// `quality | perf`.
    pub role: String,
    /// `binary | judge | float`.
    pub kind: String,
    /// `higher | lower`.
    pub better: String,
    /// The legend text, at most 14 characters.
    pub label: String,
    /// The unit shown next to the value, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: String,
    /// The grader's configuration object.
    pub grader: Value,
    /// The rubric text; required by `rubric` and `pairwise` graders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric: Option<String>,
    /// The metric's position in the version.
    pub ordinal: i32,
}

/// `eval_metrics.delete` — the version and the metric.
///
/// The version is named for the same reason the case delete names it: it is how the event
/// finds `eval:{eval_id}`, and there is no metric → version read in the store.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricDeleteIn {
    /// The version the metric belongs to, resolved in the caller's tenant.
    pub version_id: Uuid,
    /// The metric to delete.
    pub id: Uuid,
}

/// `eval_metrics.delete` — what was removed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricDeleteOut {
    /// The deleted metric id.
    pub id: String,
    /// Always `true`: an absent metric is a `404`, never a silent success.
    pub deleted: bool,
}

/// Add a metric to a version.
#[crate::action(
    name = "eval_metrics.create",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_metrics_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MetricCreateIn,
) -> Result<HandlerOutput<MetricView>, ActionError> {
    validate_metric(
        &input.key,
        &input.role,
        &input.kind,
        &input.better,
        &input.label,
        &input.grader_kind,
        input.rubric.as_deref(),
    )?;

    let new = NewEvalMetric {
        version_id: input.version_id,
        key: input.key.trim(),
        role: input.role.trim(),
        kind: input.kind.trim(),
        better: input.better.trim(),
        label: input.label.trim(),
        unit: input
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|raw| !raw.is_empty()),
        grader_kind: input.grader_kind.trim(),
        grader: &input.grader,
        rubric: input.rubric.as_deref(),
        ordinal: input.ordinal,
    };
    let metric = store::eval::insert_metric_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let eval_id = eval_for_version(&mut *tx, &ctx, metric.version_id).await?;

    let view = metric_view(&metric);
    emit(
        &mut *tx,
        &ctx,
        eval_id,
        metric.id,
        &metric.role,
        EVAL_CHANGED,
        serde_json::to_value(&view).unwrap_or(Value::Null),
    )
    .await?;
    Ok(HandlerOutput::created(view))
}

/// Replace a metric's content.
#[crate::action(
    name = "eval_metrics.update",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_metrics_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MetricUpdateIn,
) -> Result<MetricView, ActionError> {
    validate_metric(
        &input.key,
        &input.role,
        &input.kind,
        &input.better,
        &input.label,
        &input.grader_kind,
        input.rubric.as_deref(),
    )?;

    let patch = UpdateEvalMetric {
        key: input.key.trim(),
        role: input.role.trim(),
        kind: input.kind.trim(),
        better: input.better.trim(),
        label: input.label.trim(),
        unit: input
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|raw| !raw.is_empty()),
        grader_kind: input.grader_kind.trim(),
        grader: &input.grader,
        rubric: input.rubric.as_deref(),
        ordinal: input.ordinal,
    };
    let metric = store::eval::update_metric_tx(&mut *tx, ctx.tenant, input.id, &patch)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let eval_id = eval_for_version(&mut *tx, &ctx, metric.version_id).await?;

    let view = metric_view(&metric);
    emit(
        &mut *tx,
        &ctx,
        eval_id,
        metric.id,
        &metric.role,
        EVAL_CHANGED,
        serde_json::to_value(&view).unwrap_or(Value::Null),
    )
    .await?;
    Ok(view)
}

/// Delete a metric.
#[crate::action(
    name = "eval_metrics.delete",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_metrics_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MetricDeleteIn,
) -> Result<MetricDeleteOut, ActionError> {
    let eval_id = eval_for_version(&mut *tx, &ctx, input.version_id).await?;
    store::eval::delete_metric_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    emit(
        &mut *tx,
        &ctx,
        eval_id,
        input.id,
        "deleted",
        EVAL_CHANGED,
        Value::Null,
    )
    .await?;
    Ok(MetricDeleteOut {
        id: input.id.to_string(),
        deleted: true,
    })
}

// ── helpers ────────────────────────────────────────────────────────────────────────────

/// `{}` — the grader document's default.
fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// `higher` — the column's default.
fn default_better() -> String {
    "higher".to_owned()
}

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

/// Pre-validate the four closed vocabularies, the key grammar, the label width and the two
/// cross-field rules the database also enforces.
fn validate_metric(
    key: &str,
    role: &str,
    kind: &str,
    better: &str,
    label: &str,
    grader_kind: &str,
    rubric: Option<&str>,
) -> Result<(), ActionError> {
    let key = key.trim();
    if !is_metric_key(key) {
        return Err(invalid(
            "key",
            "invalid_value",
            format!("a metric key must match {KEY_GRAMMAR}"),
        ));
    }
    if !ROLES.contains(&role.trim()) {
        return Err(invalid("role", "invalid_value", "must be quality or perf"));
    }
    if !KINDS.contains(&kind.trim()) {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be binary, judge or float",
        ));
    }
    if !BETTER.contains(&better.trim()) {
        return Err(invalid(
            "better",
            "invalid_value",
            "must be higher or lower",
        ));
    }
    if label.trim().chars().count() > LABEL_MAX {
        return Err(invalid(
            "label",
            "invalid_value",
            format!("a metric label is at most {LABEL_MAX} characters"),
        ));
    }
    let grader_kind = grader_kind.trim();
    if !GRADER_KINDS.contains(&grader_kind) {
        return Err(invalid(
            "grader_kind",
            "invalid_value",
            "must be programmatic, pairwise, rubric, human or none",
        ));
    }
    // A perf field is measured, never graded (`eval_metric_perf_ungraded`).
    if role.trim() == "perf" && grader_kind != "none" {
        return Err(invalid(
            "grader_kind",
            "invalid_value",
            "a perf metric is measured, so its grader_kind must be none",
        ));
    }
    // A rubric or pairwise grader always has its rubric text
    // (`eval_metric_rubric_required`).
    if matches!(grader_kind, "rubric" | "pairwise")
        && rubric.is_none_or(|raw| raw.trim().is_empty())
    {
        return Err(invalid(
            "rubric",
            "missing",
            "a rubric or pairwise grader requires the rubric text",
        ));
    }
    Ok(())
}

/// `^[a-z][a-z0-9_]*$`, spelled in bytes so no locale or regex dependency is involved.
fn is_metric_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}
