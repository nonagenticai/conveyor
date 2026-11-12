#![forbid(unsafe_code)]
//! The eval instrument's action surface — Part A: define, inputs, split, sign-off
//! (S19 steps 4–7; SPEC D25/D26, CONTRACT §20).
//!
//! Six families, one per table group, mirroring `crates/store/src/eval`:
//!
//! - [`evals`] — `evals.create` · `.get` · `.list` · `.update` · `.retire` and
//!   `evals.draw_split`.
//! - [`versions`] — `eval_versions.create` · `.get` · `.list`; a version is frozen by copy, so
//!   there is no update and no delete.
//! - [`cases`] — `eval_cases.create` · `.update` · `.delete` · `.list` · `.import`.
//! - [`metrics`] — `eval_metrics.create` · `.update` · `.delete`.
//! - [`signoffs`] — `eval_signoffs.request` · `.decide`: the instrument's two literal pauses,
//!   carried on the one `approval` primitive with `eval_id` set.
//! - [`grades`] — `eval_grades.create` · `.get`: the one-shot per-output grade an
//!   `inspection.kind = 'eval'` SOP uses (migration 0034).
//!
//! ⛔ **The answer key never reaches a view.** `expected` exists only on the create/update/
//! import *input* types; [`CaseView`] has no such field, so the isolation is the type rather
//! than a promise (CONTRACT §20 trap 6).
//!
//! ⛔ **A sign-off is a human act.** `evals:approve` is granted to `Owner | Admin` only and
//! `eval_signoffs.decide` refuses any principal whose `actor_kind` is not `user`; neither eval
//! action is in `AGENT_TOOLS` (trap 1).
//!
//! Every mutation writes exactly one outbox row in its own transaction (CONTRACT §7) on the
//! `eval:<eval_id>` topic, with `aggregate_type = "eval"` for every emitter — a case or a
//! metric changes the instrument, and the instrument is the aggregate a subscriber watches.
//! The payload carries only `{id, status}`; the row itself rides `data`, as the webhook
//! envelope expects.

use conveyor_store as store;
use conveyor_store::eval::EvalStoreError;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{self, RawEvent};
use crate::control::invalid;
use crate::invoke::{ActionError, HandlerCtx};

pub use grades::{
    GRADE_JOB_KIND, GradeCreateIn, GradeCreatedOut, GradeIdIn, GradeView, case_input_text,
};
pub use results::{ResultDetailOut, ResultGetIn, ResultLabelIn, ResultListIn, ResultView};
pub use runs::{RegradeIn, RegradeOut, RunIdIn, RunListIn, RunStartIn, RunStartOut, RunView};

pub mod audits;
pub mod cases;
pub mod climbs;
pub mod evals;
pub mod export;
pub mod graders;
pub mod grades;
pub mod metrics;
pub mod report;
pub mod results;
pub mod run_core;
pub mod runs;
pub mod signoffs;
pub mod versions;

pub use audits::*;
pub use cases::*;
pub use evals::*;
pub use export::*;
pub use metrics::*;
pub use report::*;
pub use signoffs::*;
pub use versions::*;

/// An eval was born (`evals.create`).
pub const EVAL_CREATED: &str = "eval.created";
/// An eval or its instrument changed (`evals.update`, `evals.retire`, `eval_versions.create`,
/// `eval_metrics.*`, `evals.draw_split`).
pub const EVAL_CHANGED: &str = "eval.changed";
/// A case was created, changed, deleted or imported (`eval_cases.*`).
pub const EVAL_CASE_CHANGED: &str = "eval_case.changed";
/// A run of the instrument changed (`eval_runs.*`, Part B).
pub const EVAL_RUN_CHANGED: &str = "eval_run.changed";
/// One `(case, rep)` result was recorded (`eval.case.run`, Part B).
pub const EVAL_RESULT_RECORDED: &str = "eval_result.recorded";
/// An audit finished (`eval_audits.start`, Part C).
pub const EVAL_AUDIT_COMPLETED: &str = "eval_audit.completed";
/// A finding was decided (`eval_findings.decide`, Part C).
pub const EVAL_FINDING_DECIDED: &str = "eval_finding.decided";
/// A climb's loop state changed — started, a round created, a climb stopped (`eval_climbs.*`,
/// S20).
pub const EVAL_CLIMB_CHANGED: &str = "eval_climb.changed";
/// One round was settled: kept, or reverted with its reason (`eval_climbs.round`, S20).
pub const EVAL_CLIMB_ROUND_COMPLETED: &str = "eval_climb.round_completed";

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One eval as every eval action returns it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalView {
    /// The eval id.
    pub id: String,
    /// The display name (non-empty).
    pub name: String,
    /// What the eval is for.
    pub purpose: String,
    /// `completions | copilot | process | external | agent_run`.
    pub subject_kind: String,
    /// The registered action name a run enters through.
    pub entry_action: String,
    /// The model the subject runs against, when it names one.
    pub model_ref: Option<String>,
    /// The prompt the subject runs, when it names one.
    pub prompt_ref: Option<String>,
    /// The JSON schema a case input must satisfy.
    pub inputs_schema: Value,
    /// `synthesized | conversation_turn | run_step`.
    pub trace_source: String,
    /// `draft | running | climbing | retired`.
    pub status: String,
    /// The version a run of this eval uses; `null` until the first version exists.
    pub current_version_id: Option<String>,
    /// Who created the eval.
    pub created_by: Option<String>,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One eval version, frozen as created.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VersionView {
    /// The version id.
    pub id: String,
    /// The eval it belongs to.
    pub eval_id: String,
    /// The version number inside the eval, from 1.
    pub version: i32,
    /// The instrument digest **as frozen when the version was created**.
    ///
    /// A sign-off is bound to the digest recomputed from the current rows, not this value; the
    /// two agree until the instrument is edited.
    pub digest: String,
    /// The hash of the locked split; `null` until a draw is locked.
    pub split_sha256: Option<String>,
    /// When the split was locked.
    pub split_locked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The price table the cost estimate uses.
    pub prices: Value,
    /// The harness files the run needs, in order.
    pub harness_paths: Vec<String>,
    /// When the version was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `eval_versions.get` — the frozen version plus the metric definitions it froze.
///
/// The cases are deliberately NOT embedded: `eval_cases.list` already pages a version and a
/// silently truncated instrument would be a lie. Metrics have no list action, so this read is
/// the authoring screen's only way to see (and therefore update or delete) them.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VersionDetailView {
    /// The version row itself, at the top level.
    #[serde(flatten)]
    pub version: VersionView,
    /// The metrics in this version, in ordinal order.
    pub metrics: Vec<MetricView>,
}

/// One eval case as every case action returns it.
///
/// ⛔ There is no `expected` field and there must never be one. The answer key is write-only:
/// it appears on the input types and on no output document, list row or MCP tool.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaseView {
    /// The case id.
    pub id: String,
    /// The version it belongs to.
    pub version_id: String,
    /// Its stable position inside the version, from 0.
    pub ordinal: i32,
    /// The input a subject is shown.
    pub input: String,
    /// The case's tags; the first one is the split's stratum.
    pub tags: Vec<String>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: Value,
    /// `production | ticket | handwritten | synthesized`.
    pub source: String,
    /// `human_written | human_verified | model`.
    pub gold_provenance: String,
    /// The model that produced a `model`-provenance key.
    pub gold_model: Option<String>,
    /// `train | test | none` — written only by the split lock.
    pub split: String,
    /// Free-form case metadata.
    pub meta: Value,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One eval metric.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MetricView {
    /// The metric id.
    pub id: String,
    /// The version it belongs to.
    pub version_id: String,
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
    pub unit: Option<String>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: String,
    /// The grader's configuration object.
    pub grader: Value,
    /// The rubric text, required by `rubric` and `pairwise` graders.
    pub rubric: Option<String>,
    /// The metric's stable position in the version.
    pub ordinal: i32,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// One stratum of a drawn split.
///
/// A local twin of [`conveyor_domain::eval::TagCounts`], which carries no `JsonSchema` derive;
/// the domain value is the one the draw computed and this is only its wire projection.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct TagCountOut {
    /// The stratum (the cases' first tag, or `""` for the untagged group).
    pub tag: String,
    /// Cases in the stratum.
    pub total: i64,
    /// Cases assigned to `train`.
    pub train: i64,
    /// Cases assigned to `test`.
    pub test: i64,
}

/// `evals.draw_split` — the locked version and the per-stratum counts.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SplitOut {
    /// The version the split was locked on.
    pub version: VersionView,
    /// The per-stratum counts, sorted by tag.
    pub per_tag: Vec<TagCountOut>,
    /// Warnings about the draw. Part A has no baseline data, so this is empty by contract; a
    /// train/test mean mismatch becomes an audit finding in Part C and never unlocks a re-draw.
    pub warnings: Vec<String>,
}

// ── mappers ─────────────────────────────────────────────────────────────────────────────

/// Map a stored eval onto the wire document.
pub fn eval_view(eval: &store::eval::Eval) -> EvalView {
    EvalView {
        id: eval.id.to_string(),
        name: eval.name.clone(),
        purpose: eval.purpose.clone(),
        subject_kind: eval.subject_kind.as_str().to_owned(),
        entry_action: eval.entry_action.clone(),
        model_ref: eval.model_ref.clone(),
        prompt_ref: eval.prompt_ref.clone(),
        inputs_schema: eval.inputs_schema.clone(),
        trace_source: eval.trace_source.clone(),
        status: eval.status.as_str().to_owned(),
        current_version_id: eval.current_version_id.map(|id| id.to_string()),
        created_by: eval.created_by.map(|id| id.to_string()),
        created_at: eval.created_at,
        updated_at: eval.updated_at,
    }
}

/// Map a stored version onto the wire document.
pub fn version_view(version: &store::eval::EvalVersion) -> VersionView {
    VersionView {
        id: version.id.to_string(),
        eval_id: version.eval_id.to_string(),
        version: version.version,
        digest: version.digest.clone(),
        split_sha256: version.split_sha256.clone(),
        split_locked_at: version.split_locked_at,
        prices: version.prices.clone(),
        harness_paths: version.harness_paths.clone(),
        created_at: version.created_at,
    }
}

/// Map a stored case onto the wire document — **without** its answer key.
pub fn case_view(case: &store::eval::EvalCase) -> CaseView {
    CaseView {
        id: case.id.to_string(),
        version_id: case.version_id.to_string(),
        ordinal: case.ordinal,
        input: case.input.clone(),
        tags: case.tags.clone(),
        attachments: case.attachments.clone(),
        source: case.source.clone(),
        gold_provenance: case.gold_provenance.clone(),
        gold_model: case.gold_model.clone(),
        split: case.split.clone(),
        meta: case.meta.clone(),
        created_at: case.created_at,
        updated_at: case.updated_at,
    }
}

/// Map a stored metric onto the wire document.
pub fn metric_view(metric: &store::eval::EvalMetric) -> MetricView {
    MetricView {
        id: metric.id.to_string(),
        version_id: metric.version_id.to_string(),
        key: metric.key.clone(),
        role: metric.role.clone(),
        kind: metric.kind.clone(),
        better: metric.better.clone(),
        label: metric.label.clone(),
        unit: metric.unit.clone(),
        grader_kind: metric.grader_kind.clone(),
        grader: metric.grader.clone(),
        rubric: metric.rubric.clone(),
        ordinal: metric.ordinal,
        created_at: metric.created_at,
    }
}

// ── error mapping ───────────────────────────────────────────────────────────────────────

/// The one place an [`EvalStoreError`] becomes an RFC 9457 problem.
///
/// A missing row is `404`; a unique or check refusal is `409`/`422`; a refused domain
/// transition is `409` (the state moved underneath the caller); a stored value the domain does
/// not know, or a database failure, is a scrubbed `500` — never a quietly passed-through 4xx.
pub fn eval_error(error: EvalStoreError, action: &str) -> ActionError {
    match error {
        EvalStoreError::NotFound { what } => ActionError::resource_not_found(what),
        EvalStoreError::Conflict { constraint } => ActionError::new(
            409,
            "conflict",
            format!("{action:?}: the write conflicts ({constraint})"),
        ),
        EvalStoreError::Invalid { constraint } => invalid(
            "value",
            "invalid_value",
            format!("the database refused the value ({constraint})"),
        ),
        EvalStoreError::SplitLocked => ActionError::new(
            409,
            "split_locked",
            "the split is already locked for this eval version",
        ),
        EvalStoreError::Illegal(illegal) => ActionError::new(409, "conflict", illegal.to_string()),
        EvalStoreError::Label(refusal) => match refusal {
            // Only a `pending_label` row is labelled; scoring an already-scored result would
            // overwrite the automated number the report's calibration rests on.
            conveyor_store::eval::LabelRefusal::NotPending => ActionError::new(
                409,
                "not_pending_label",
                format!("{action:?}: the result is not awaiting a human label"),
            ),
        },
        // An audit runs once and a finding is decided once; a second write is refused rather
        // than allowed to rewrite a finished judgement.
        EvalStoreError::NotOpen { what } => ActionError::new(
            409,
            "not_open",
            format!("{action:?}: the {what} is not open for this write"),
        ),
        EvalStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored eval value is outside the domain");
            ActionError::internal()
        }
        EvalStoreError::Database(error) => {
            tracing::error!(%error, action, "eval store failure");
            ActionError::internal()
        }
    }
}

/// `409 eval_not_ready` — the eval has no version, so it has no instrument to act on.
pub fn not_ready(action: &str, detail: &str) -> ActionError {
    ActionError::new(409, "eval_not_ready", format!("{action}: {detail}"))
}

// ── the outbox row ──────────────────────────────────────────────────────────────────────

/// Append one eval event in the caller's transaction (CONTRACT §7).
///
/// The topic is the eval's aggregate topic and the aggregate is always the eval, even when the
/// row that changed is a case or a metric: the instrument is what a subscriber watches, and a
/// case's own id would make every consumer join through the version to find the topic.
///
/// `payload_id` is the row that changed (the eval, the case, the metric, or the version for a
/// batch import) and `status` is its salient state; neither carries a body. The full row rides
/// `data`, which is what the webhook envelope forwards.
pub(crate) async fn emit(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    eval_id: Uuid,
    payload_id: Uuid,
    status: &str,
    event_type: &'static str,
    data: Value,
) -> Result<(), ActionError> {
    events::append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: format!("eval:{eval_id}"),
            board_id: None,
            aggregate_type: "eval",
            aggregate_id: eval_id,
            event_type,
            payload: json!({ "id": payload_id, "status": status }),
            data,
            prev_data: Value::Null,
        },
    )
    .await
}
