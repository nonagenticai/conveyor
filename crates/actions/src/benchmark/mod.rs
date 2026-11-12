#![forbid(unsafe_code)]
//! The benchmark actions (S17 §5): suites, runs and cases.
//!
//! **Answers are grader-only.** No view in this module carries `gold_patch`, `test_patch`,
//! `fail_to_pass`, `pass_to_pass` or `eval_script` (CONTRACT §19). The answers are reachable only
//! through the store's `grader_instance_tx`, which no action calls.
//!
//! **Compare is exact and refuses.** `benchmark_runs.compare` derives both arms from their
//! labels and asks the domain's McNemar; a suite/oracle/prompt mismatch is `comparable:false`
//! with a reason, never a p-value.
//!
//! **The sandbox is fail-closed at the door.** `benchmark_runs.start` refuses
//! `503 benchmark_sandbox_not_configured` when `SANDBOX_NAMESPACE` is unset, so a run cannot be
//! created that no driver could ever execute.

pub mod cases;
pub mod runs;
pub mod trend;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::benchmark::arm::Arm;
use conveyor_domain::benchmark::mcnemar;
use conveyor_store::benchmark::{BenchmarkStoreError, Case, Run, Suite, graded_cases_tx};
use rust_decimal::prelude::ToPrimitive;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;
use crate::spec::FieldError;
use crate::views::PageIn;

/// A 422 with one field-level error.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Map a store error from another module (capability, procedure) onto the action layer's error.
///
/// Those reads cannot refuse in a way the caller can act on differently — the subject is resolved
/// inside this transaction and a database fault is never the caller's fault — so it is an internal
/// error, logged with the action that hit it.
pub(crate) fn subject_read_error(error: impl std::fmt::Display, action: &str) -> ActionError {
    tracing::error!(%error, action, "benchmark subject read");
    ActionError::internal()
}

/// Map a store refusal onto the action layer's typed error.
pub(crate) fn benchmark_error(error: BenchmarkStoreError, action: &str) -> ActionError {
    match error {
        BenchmarkStoreError::NotFound { what } => ActionError::resource_not_found(what),
        BenchmarkStoreError::SeedConflict { slug } => ActionError::new(
            409,
            "suite_seed_conflict",
            format!("suite {slug:?} already exists with a different seed; a suite is immutable"),
        ),
        BenchmarkStoreError::Conflict { constraint } => ActionError::new(
            409,
            "benchmark_conflict",
            format!("the {constraint} constraint refused the write"),
        ),
        BenchmarkStoreError::Invalid { constraint } => ActionError::new(
            422,
            "benchmark_invalid",
            format!("the {constraint} constraint refused the row"),
        ),
        BenchmarkStoreError::NotCancellable { status } => ActionError::new(
            409,
            "run_not_cancellable",
            format!("the run is {status} and cannot be cancelled"),
        ),
        BenchmarkStoreError::PatchTooLarge { len } => ActionError::new(
            422,
            "patch_too_large",
            format!(
                "the patch is {len} bytes, over the {}-byte cap",
                conveyor_store::benchmark::MAX_PATCH_BYTES
            ),
        ),
        other => {
            tracing::error!(error = %other, action, "benchmark store refusal");
            ActionError::internal()
        }
    }
}

/// The keyset cursor `(created_at, id)`, the same JSON shape the process actions use.
pub(crate) fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    serde_json::to_string(&(created_at, id)).unwrap_or_else(|_| "[]".to_owned())
}

/// Decode a benchmark cursor; a malformed one is a 422 naming the field.
pub(crate) fn decode_cursor(
    cursor: Option<&str>,
) -> Result<Option<(DateTime<Utc>, Uuid)>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<(DateTime<Utc>, Uuid)>(raw)
        .map(Some)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_cursor",
                "a benchmark cursor is the [created_at, id] pair the previous page returned",
            )
        })
}

// ── views ────────────────────────────────────────────────────────────────────────────────────

/// One suite, as a caller may see it (hashes and versions, never an instance).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SuiteView {
    /// The suite id.
    pub id: Uuid,
    /// The stable slug.
    pub slug: String,
    /// The dataset identity.
    pub dataset: String,
    /// The HF revision the seed was cut from.
    pub dataset_revision: String,
    /// The seed artifact hash.
    pub seed_sha256: String,
    /// The image-lock hash.
    pub image_lock_sha256: String,
    /// The swebench semantics the grader reproduces.
    pub swebench_version: String,
    /// The parser set's version.
    pub parser_version: String,
    /// How many instances the suite carries.
    pub instance_count: i64,
    /// The promoted baseline run, when one exists.
    pub baseline_run_id: Option<Uuid>,
    /// When the suite was seeded.
    pub created_at: DateTime<Utc>,
    /// When the suite last changed.
    pub updated_at: DateTime<Utc>,
}

/// One run.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunView {
    /// The run id.
    pub id: Uuid,
    /// The suite measured.
    pub suite_id: Uuid,
    /// `gold | empty | baseline | conveyor`.
    pub arm: String,
    /// `suite | capability_agent | process`.
    pub subject_kind: String,
    /// The capability agent, when the subject names one.
    pub capability_agent_id: Option<Uuid>,
    /// The process, when the subject names one.
    pub process_id: Option<Uuid>,
    /// The subject's version when the run started (S18 §3).
    pub subject_version: Option<i64>,
    /// `keep | revert | inconclusive`, when this run was compared against a baseline.
    pub improvement_verdict: Option<String>,
    /// The baseline the verdict was measured against.
    pub improvement_of_run_id: Option<Uuid>,
    /// Why the verdict came out that way.
    pub improvement_note: Option<String>,
    /// The predictions label.
    pub label: String,
    /// The model the router served, recorded from the response.
    pub model: Option<String>,
    /// `queued | running | succeeded | failed | cancelled`.
    pub status: String,
    /// The number of cases in the run.
    pub n: i32,
    /// Cases that produced a patch.
    pub n_graded: i32,
    /// Cases the grader resolved.
    pub n_resolved: i32,
    /// Cases withheld from the denominator.
    pub n_no_attempt: i32,
    /// The full verdict histogram.
    pub counts: Value,
    /// `None` for non-control arms; `false` for a control that missed.
    pub control_ok: Option<bool>,
    /// When a cancel was requested.
    pub cancel_requested_at: Option<DateTime<Utc>>,
    /// The driver's heartbeat.
    pub last_polled_at: Option<DateTime<Utc>>,
    /// When the driver started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run reached a terminal status.
    pub finished_at: Option<DateTime<Utc>>,
    /// The terminal error, when the run failed.
    pub error: Option<String>,
    /// The run options.
    pub options: Value,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// When the run last changed.
    pub updated_at: DateTime<Utc>,
}

/// One case. `patch` is the producer's bounded output; the log is an S09 document.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CaseView {
    /// The case id.
    pub id: Uuid,
    /// The run this case belongs to.
    pub benchmark_run_id: Uuid,
    /// The instance id.
    pub instance_id: String,
    /// `pending | ok | fail | no_attempt`.
    pub produce_status: String,
    /// How many produce attempts have been made.
    pub attempts: i32,
    /// The produced patch.
    pub patch: Option<String>,
    /// The patch hash.
    pub patch_sha256: Option<String>,
    /// `pending | resolved | unresolved | apply_failed | eval_timeout | infra_error`.
    pub verdict: String,
    /// The eval duration.
    pub seconds: Option<f64>,
    /// The sandbox Job name.
    pub sandbox_ref: Option<String>,
    /// The S09 document holding the log.
    pub log_document_id: Option<Uuid>,
    /// The grader's detail record.
    pub detail: Value,
    /// When the case was created.
    pub created_at: DateTime<Utc>,
    /// When the case last changed.
    pub updated_at: DateTime<Utc>,
}

/// The result of `benchmark_runs.compare`; `comparable:false` carries a reason and no p-value.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompareView {
    /// Whether the two runs measured the same task.
    pub comparable: bool,
    /// Why not, when `comparable` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Instances both runs resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub both: Option<usize>,
    /// Instances neither run resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neither: Option<usize>,
    /// Resolved by `a` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_only: Option<usize>,
    /// Resolved by `b` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_only: Option<usize>,
    /// `a_only + b_only`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discordant: Option<usize>,
    /// The exact two-sided McNemar p-value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
    /// Instances graded in exactly one run (reported, never dropped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_in_one: Option<usize>,
}

impl CompareView {
    /// A refusal: `200` with the reason and no statistics (CONTRACT §19).
    pub(crate) fn refused(reason: impl Into<String>) -> Self {
        Self {
            comparable: false,
            reason: Some(reason.into()),
            both: None,
            neither: None,
            a_only: None,
            b_only: None,
            discordant: None,
            p_value: None,
            missing_in_one: None,
        }
    }
}

/// Compare two runs that are already loaded — **the one implementation** of the comparison.
///
/// `benchmark_runs.compare` and `benchmark_runs.trend` both call this, so a trend point can never
/// disagree with what a person reading `compare` gets. The rules are CONTRACT §19's, in order:
/// different suites refuse, a control that missed refuses, and the arms must be comparable
/// (the oracle and the prompt agree — the *pipeline* is what is being compared, so it never
/// takes part in the refusal).
pub(crate) async fn compare_two_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    a: &Run,
    b: &Run,
    action: &str,
) -> Result<CompareView, ActionError> {
    if a.suite_id != b.suite_id {
        return Ok(CompareView::refused("the runs measured different suites"));
    }
    if a.control_ok == Some(false) || b.control_ok == Some(false) {
        return Ok(CompareView::refused(
            "a control missed; its number is not a score and cannot be compared",
        ));
    }
    let a_arm = Arm::derive(&a.label);
    let b_arm = Arm::derive(&b.label);
    if !a_arm.comparable(&b_arm) {
        return Ok(CompareView::refused(format!(
            "the arms are not comparable (a: {a_arm:?}, b: {b_arm:?}); the oracle and the \
             prompt must agree"
        )));
    }

    let a_map: BTreeMap<String, bool> = graded_cases_tx(&mut *conn, tenant, a.id)
        .await
        .map_err(|error| benchmark_error(error, action))?
        .into_iter()
        .map(|(instance, verdict)| (instance, verdict == "resolved"))
        .collect();
    let b_map: BTreeMap<String, bool> = graded_cases_tx(&mut *conn, tenant, b.id)
        .await
        .map_err(|error| benchmark_error(error, action))?
        .into_iter()
        .map(|(instance, verdict)| (instance, verdict == "resolved"))
        .collect();

    match mcnemar::compare(a_arm, b_arm, &a_map, &b_map) {
        Ok(result) => Ok(CompareView {
            comparable: true,
            reason: None,
            both: Some(result.counts.both),
            neither: Some(result.counts.neither),
            a_only: Some(result.counts.a_only),
            b_only: Some(result.counts.b_only),
            discordant: Some(result.counts.a_only + result.counts.b_only),
            p_value: Some(result.p_value),
            missing_in_one: Some(result.counts.missing_in_one),
        }),
        Err(error) => Ok(CompareView::refused(error.to_string())),
    }
}

pub(crate) fn suite_view(suite: &Suite) -> SuiteView {
    SuiteView {
        id: suite.id,
        slug: suite.slug.clone(),
        dataset: suite.dataset.clone(),
        dataset_revision: suite.dataset_revision.clone(),
        seed_sha256: suite.seed_sha256.clone(),
        image_lock_sha256: suite.image_lock_sha256.clone(),
        swebench_version: suite.swebench_version.clone(),
        parser_version: suite.parser_version.clone(),
        instance_count: suite.instance_count,
        baseline_run_id: suite.baseline_run_id,
        created_at: suite.created_at,
        updated_at: suite.updated_at,
    }
}

pub(crate) fn run_view(run: &Run) -> RunView {
    RunView {
        id: run.id,
        suite_id: run.suite_id,
        arm: run.arm.clone(),
        subject_kind: run.subject_kind.clone(),
        capability_agent_id: run.capability_agent_id,
        process_id: run.process_id,
        subject_version: run.subject_version,
        improvement_verdict: run.improvement_verdict.clone(),
        improvement_of_run_id: run.improvement_of_run_id,
        improvement_note: run.improvement_note.clone(),
        label: run.label.clone(),
        model: run.model.clone(),
        status: run.status.clone(),
        n: run.n,
        n_graded: run.n_graded,
        n_resolved: run.n_resolved,
        n_no_attempt: run.n_no_attempt,
        counts: run.counts.clone(),
        control_ok: run.control_ok,
        cancel_requested_at: run.cancel_requested_at,
        last_polled_at: run.last_polled_at,
        started_at: run.started_at,
        finished_at: run.finished_at,
        error: run.error.clone(),
        options: run.options.clone(),
        created_at: run.created_at,
        updated_at: run.updated_at,
    }
}

pub(crate) fn case_view(case: &Case) -> CaseView {
    CaseView {
        id: case.id,
        benchmark_run_id: case.benchmark_run_id,
        instance_id: case.instance_id.clone(),
        produce_status: case.produce_status.clone(),
        attempts: case.attempts,
        patch: case.patch.clone(),
        patch_sha256: case.patch_sha256.clone(),
        verdict: case.verdict.clone(),
        seconds: case.seconds.and_then(|seconds| seconds.to_f64()),
        sandbox_ref: case.sandbox_ref.clone(),
        log_document_id: case.log_document_id,
        detail: case.detail.clone(),
        created_at: case.created_at,
        updated_at: case.updated_at,
    }
}

// ── inputs ───────────────────────────────────────────────────────────────────────────────────

/// `benchmark_suites.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SuitesListIn {
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `benchmark_runs.start` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunStartIn {
    /// The suite to measure.
    pub suite_id: Uuid,
    /// `gold | empty | baseline | conveyor`.
    pub arm: String,
    /// `suite | capability_agent | process`; defaults to `suite`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_kind: Option<String>,
    /// The capability agent, when the subject names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// The process, when the subject names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<Uuid>,
    /// The predictions label (the arm identity is derived from it).
    pub label: String,
    /// How many sandbox Jobs one tick may start (1–10, default 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
    /// Measure only these instances.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    /// Whether the producer is told the failing test id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_failing_test: Option<bool>,
    /// Start this candidate against a baseline (S18 step 3): the run is enqueued for the
    /// improvement job, which records `keep | revert | inconclusive` on the candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_run_id: Option<Uuid>,
}

/// `benchmark_runs.get` / `.cancel` / `.promote` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunIdIn {
    /// The run id.
    pub id: Uuid,
}

/// `benchmark_runs.list` input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct RunListIn {
    /// Only runs of this suite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite_id: Option<Uuid>,
    /// Only this arm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arm: Option<String>,
    /// Only this status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `benchmark_runs.compare` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunCompareIn {
    /// The first run.
    pub a_run_id: Uuid,
    /// The second run.
    pub b_run_id: Uuid,
}

/// `benchmark_cases.list` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CasesListIn {
    /// The run.
    pub run_id: Uuid,
    /// Only this verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// The verdict vocabulary the database enforces.
pub(crate) const VERDICTS: [&str; 6] = [
    "pending",
    "resolved",
    "unresolved",
    "apply_failed",
    "eval_timeout",
    "infra_error",
];
