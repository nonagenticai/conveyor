#![forbid(unsafe_code)]
//! T11 — metrics, metering and maturity (SPEC_V2 §7.5, §11.9, §15.1 D37/D50/D52; issue #84).
//!
//! Every figure is a read-time aggregate over the ledgers §7.5 names; nothing here stores a
//! metric. The two rows this family writes are configuration and a verdict: `metric_threshold`
//! (a versioned tenant row, D50) and `metric_reconciliation` (the billing comparison, P2).
//!
//! **A gate that never refused is flagged** (P4): [`ProgrammeOut::never_refused`] lists every gate
//! with decided approvals and no rejection in the window. **A missing figure is never zero**
//! (P1.4/P5.4): an unmeasured benefit reports `unmeasured`, and a threshold naming a metric with
//! no evidence is unmet.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use conveyor_domain::metrics::{self as rules, MaturityLevel, Threshold};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::views::CardCreateIn;
use crate::invoke::{ActionError, HandlerCtx};

/// The default evidence window when a caller names none: the last 90 days.
const DEFAULT_WINDOW_DAYS: i64 = 90;

fn metrics_error(error: store::metrics::MetricsStoreError, action: &str) -> ActionError {
    match error {
        store::metrics::MetricsStoreError::Conflict => ActionError::new(
            409,
            "threshold_version_conflict",
            "the threshold configuration changed concurrently; re-read and retry",
        ),
        store::metrics::MetricsStoreError::Database(error) => {
            tracing::error!(%error, action, "metrics: store failure");
            ActionError::internal()
        }
    }
}

fn store_internal(error: impl std::fmt::Display) -> ActionError {
    tracing::error!(%error, "metrics: store read failed");
    ActionError::internal()
}

fn window(
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let until = until.unwrap_or_else(Utc::now);
    let since = since.unwrap_or_else(|| until - Duration::days(DEFAULT_WINDOW_DAYS));
    (since, until)
}

fn parse_metrics(value: &Value) -> Result<BTreeMap<String, f64>, ActionError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        tracing::error!(%error, "metrics: stored threshold is not a metric map");
        ActionError::internal()
    })
}

fn parse_level(value: &str) -> Result<MaturityLevel, ActionError> {
    MaturityLevel::parse(value).ok_or_else(|| {
        tracing::error!(level = value, "metrics: stored level does not parse");
        ActionError::internal()
    })
}

// ── threshold configuration (P3) ─────────────────────────────────────────────────────────

/// One stored threshold version as `metrics.threshold_get` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThresholdOut {
    pub level: String,
    pub version: i64,
    pub metrics: BTreeMap<String, f64>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// `metrics.threshold_get` — the latest version of every configured level.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThresholdGetIn {}

/// The configured levels, C0–C5 order.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThresholdGetOut {
    pub levels: Vec<ThresholdOut>,
}

#[crate::action(
    name = "metrics.threshold_get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_threshold_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: ThresholdGetIn,
) -> Result<ThresholdGetOut, ActionError> {
    let rows = store::metrics::latest_thresholds_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let mut levels = Vec::with_capacity(rows.len());
    for row in rows {
        levels.push(ThresholdOut {
            level: row.level,
            version: row.version,
            metrics: parse_metrics(&row.metrics)?,
            note: row.note,
            created_at: row.created_at,
        });
    }
    levels.sort_by_key(|level| level.level.clone());
    Ok(ThresholdGetOut { levels })
}

/// `metrics.threshold_update` — write the next version of one level.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ThresholdUpdateIn {
    /// `C0` … `C5`.
    pub level: String,
    /// The named evidence metric → the minimum value that meets this level.
    pub metrics: BTreeMap<String, f64>,
    /// Why this version exists (recorded on the row).
    #[serde(default)]
    pub note: String,
}

#[crate::action(
    name = "metrics.threshold_update",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_threshold_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ThresholdUpdateIn,
) -> Result<ThresholdOut, ActionError> {
    let level = rules::validate_threshold(&input.level, &input.metrics)
        .map_err(|message| ActionError::new(422, "invalid_threshold", message))?;
    let metrics = serde_json::to_value(&input.metrics).map_err(|error| {
        tracing::error!(%error, "metrics: encode threshold");
        ActionError::internal()
    })?;
    let row = store::metrics::insert_threshold_tx(
        &mut *tx,
        ctx.tenant,
        level.as_str(),
        &metrics,
        &input.note,
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(ThresholdOut {
        level: row.level,
        version: row.version,
        metrics: parse_metrics(&row.metrics)?,
        note: row.note,
        created_at: row.created_at,
    })
}

// ── maturity (P3) ────────────────────────────────────────────────────────────────────────

/// `metrics.maturity` — the level the recorded evidence supports.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityIn {
    /// The programme whose benefit figures join the evidence; absent means benefits are absent.
    pub programme_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One configured level's verdict.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LevelOut {
    pub level: String,
    pub version: i64,
    pub met: bool,
    /// The metrics missing from the evidence or below their minimum.
    pub unmet: Vec<String>,
}

/// The computed level, the threshold version it used and the evidence behind it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaturityOut {
    /// `C0` … `C5`, or null when no contiguous level is met.
    pub level: Option<String>,
    /// The threshold version the reported level used.
    pub threshold_version: Option<i64>,
    /// Every named evidence figure, so a caller can see why a level did or did not move.
    pub evidence: BTreeMap<String, f64>,
    /// Every configured level, in order.
    pub levels: Vec<LevelOut>,
}

#[crate::action(
    name = "metrics.maturity",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_maturity(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MaturityIn,
) -> Result<MaturityOut, ActionError> {
    let (since, until) = window(input.since, input.until);
    let evidence = evidence_tx(&mut *tx, ctx.tenant, input.programme_id, since, until).await?;
    let rows = store::metrics::latest_thresholds_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let mut thresholds = Vec::with_capacity(rows.len());
    for row in rows {
        thresholds.push(Threshold {
            level: parse_level(&row.level)?,
            version: row.version,
            metrics: parse_metrics(&row.metrics)?,
        });
    }
    let states = rules::levels(&thresholds, &evidence);
    let current = rules::current_level(&states);
    Ok(MaturityOut {
        level: current.map(|(level, _)| level.as_str().to_owned()),
        threshold_version: current.map(|(_, version)| version),
        evidence,
        levels: states
            .into_iter()
            .map(|state| LevelOut {
                level: state.level.as_str().to_owned(),
                version: state.version,
                met: state.met,
                unmet: state.unmet,
            })
            .collect(),
    })
}

/// The evidence map for the closed metric set, all derived from rows in the window.
async fn evidence_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    programme_id: Option<Uuid>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<BTreeMap<String, f64>, ActionError> {
    let gates = store::metrics::gate_counts_tx(&mut *conn, tenant, since, until)
        .await
        .map_err(store_internal)?;
    let mut approved = 0_i64;
    let mut rejected = 0_i64;
    let mut decided = 0_i64;
    let mut with_evidence = 0_i64;
    for gate in &gates {
        approved += gate.approved;
        rejected += gate.rejected;
        decided += gate.decided;
        with_evidence += gate.with_evidence;
    }
    let runs = store::metrics::run_totals_tx(&mut *conn, tenant, since, until)
        .await
        .map_err(store_internal)?;
    let mut evidence = BTreeMap::new();
    evidence.insert("approvals.decided".to_owned(), decided as f64);
    evidence.insert("approvals.approved".to_owned(), approved as f64);
    evidence.insert("approvals.rejected".to_owned(), rejected as f64);
    evidence.insert(
        "approvals.approval_rate".to_owned(),
        if decided == 0 {
            0.0
        } else {
            approved as f64 / decided as f64
        },
    );
    evidence.insert(
        "approvals.evidence_completeness".to_owned(),
        if decided == 0 {
            0.0
        } else {
            with_evidence as f64 / decided as f64
        },
    );
    evidence.insert("runs.total".to_owned(), runs.total as f64);
    evidence.insert("runs.failed".to_owned(), runs.failed as f64);
    if let Some(programme_id) = programme_id {
        let benefits = store::metrics::benefit_rows_tx(&mut *conn, tenant, programme_id)
            .await
            .map_err(store_internal)?;
        let (realised, planned) = benefit_totals(&benefits);
        evidence.insert("benefits.realised".to_owned(), realised as f64);
        evidence.insert("benefits.planned".to_owned(), planned as f64);
    }
    Ok(evidence)
}

// ── the programme's MSP and governance metrics (P4/P5) ───────────────────────────────────

/// `metrics.programme` — the programme's benefits, risk, tranches and gate health.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeIn {
    pub programme_id: Uuid,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One benefit profile's measured state.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitMetricOut {
    pub reference: String,
    pub status: String,
    /// `measured` when a baseline value is recorded, else `unmeasured`.
    pub baseline_state: String,
    pub baseline: Option<Value>,
    pub target: Option<Value>,
    pub post_measured_at: Option<DateTime<Utc>>,
    /// A profile with a target and a post-measurement transition is realised.
    pub realised: bool,
}

/// One benefit put at risk by a dependency slip.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BenefitAtRiskOut {
    pub reference: String,
    pub via_project: String,
}

/// One tranche and its latest review decision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrancheOut {
    pub number: i32,
    pub state: String,
    pub decision: Option<String>,
}

/// One gate's governance figures.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GateOut {
    pub gate: String,
    pub approved: i64,
    pub rejected: i64,
    pub decided: i64,
    pub with_evidence: i64,
    pub latency_seconds: Option<f64>,
    /// Decided approvals with no rejection: a stamp, not health.
    pub never_refused: bool,
}

/// The MSP programme metric set and the governance section.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProgrammeOut {
    pub benefits: Vec<BenefitMetricOut>,
    /// Profiles with a target and a post-measurement transition.
    pub realised: i64,
    /// Profiles with a target.
    pub planned: i64,
    /// `realised / planned`, null when nothing is planned.
    pub benefit_ratio: Option<f64>,
    pub benefit_at_risk: Vec<BenefitAtRiskOut>,
    pub tranches: Vec<TrancheOut>,
    pub gates: Vec<GateOut>,
    /// Every gate in the window with decided approvals and no rejection.
    pub never_refused: Vec<String>,
    pub tokens: i64,
    pub runs: i64,
}

#[crate::action(
    name = "metrics.programme",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_programme(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProgrammeIn,
) -> Result<ProgrammeOut, ActionError> {
    let (since, until) = window(input.since, input.until);
    let rows = store::metrics::benefit_rows_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let (realised, planned) = benefit_totals(&rows);
    let benefits = rows
        .into_iter()
        .map(|row| BenefitMetricOut {
            reference: row.reference,
            status: row.status,
            baseline_state: if row.baseline.is_some() {
                "measured"
            } else {
                "unmeasured"
            }
            .to_owned(),
            baseline: row.baseline,
            target: row.target.clone(),
            post_measured_at: row.post_measured_at,
            realised: row.target.is_some() && row.post_measured_at.is_some(),
        })
        .collect();
    let at_risk = store::metrics::benefit_at_risk_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let tranches = store::metrics::tranche_rows_tx(&mut *tx, ctx.tenant, input.programme_id)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let gate_rows = store::metrics::gate_counts_tx(&mut *tx, ctx.tenant, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let mut gates = Vec::with_capacity(gate_rows.len());
    let mut never_refused = Vec::new();
    for gate in gate_rows {
        let never = gate.decided > 0 && gate.rejected == 0;
        if never {
            never_refused.push(gate.gate.clone());
        }
        gates.push(GateOut {
            gate: gate.gate,
            approved: gate.approved,
            rejected: gate.rejected,
            decided: gate.decided,
            with_evidence: gate.with_evidence,
            latency_seconds: gate.latency_seconds,
            never_refused: never,
        });
    }
    let tokens = store::metrics::token_totals_tx(&mut *tx, ctx.tenant, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?
        .tokens;
    let runs = store::metrics::run_totals_tx(&mut *tx, ctx.tenant, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?
        .total;
    Ok(ProgrammeOut {
        benefit_ratio: if planned == 0 {
            None
        } else {
            Some(realised as f64 / planned as f64)
        },
        benefits,
        realised,
        planned,
        benefit_at_risk: at_risk
            .into_iter()
            .map(|row| BenefitAtRiskOut {
                reference: row.reference,
                via_project: row.via_project,
            })
            .collect(),
        tranches: tranches
            .into_iter()
            .map(|row| TrancheOut {
                number: row.number,
                state: row.state,
                decision: row.decision,
            })
            .collect(),
        gates,
        never_refused,
        tokens,
        runs,
    })
}

/// `realised` counts profiles with a target AND a post-measurement transition; `planned` counts
/// profiles with a target. An unmeasured baseline never counts as zero improvement (P5.4).
fn benefit_totals(rows: &[store::metrics::BenefitRow]) -> (i64, i64) {
    let mut realised = 0;
    let mut planned = 0;
    for row in rows {
        if row.target.is_some() {
            planned += 1;
            if row.post_measured_at.is_some() {
                realised += 1;
            }
        }
    }
    (realised, planned)
}

// Keep the reconciliation enum referenced until the P2 action lands on this branch; the state
// constant is the stored vocabulary and must not drift.
#[allow(dead_code)]
fn reconciliation_verdict(
    conveyor_usd: f64,
    billing_usd: f64,
) -> (rules::ReconciliationState, f64) {
    rules::reconcile(conveyor_usd, billing_usd)
}

// ── the metering levels (P1.2) ───────────────────────────────────────────────────────────

/// `metrics.usage` — one metering level, recomputed from the ledgers.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UsageIn {
    /// `tenant | department | programme | project | job | run | step | call`.
    pub level: String,
    /// The row to scope to, where the level supports one.
    pub id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One metering row.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UsageRowOut {
    pub key: String,
    pub calls: i64,
    pub tool_calls: i64,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
    pub priced_calls: i64,
    pub unpriced_calls: i64,
    pub runs: i64,
}

/// The level's rows; the sum of the child level equals its parent.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UsageOut {
    pub level: String,
    pub rows: Vec<UsageRowOut>,
}

#[crate::action(
    name = "metrics.usage",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_usage(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: UsageIn,
) -> Result<UsageOut, ActionError> {
    let level = store::metrics::MeteringLevel::parse(&input.level).ok_or_else(|| {
        ActionError::new(
            422,
            "unknown_metering_level",
            format!(
                "unknown level {:?}; expected tenant, department, programme, project, job, run, step or call",
                input.level
            ),
        )
    })?;
    let (since, until) = window(input.since, input.until);
    let rows =
        store::metrics::usage_by_level_tx(&mut *tx, ctx.tenant, level, since, until, input.id)
            .await
            .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(UsageOut {
        level: level.as_str().to_owned(),
        rows: rows.into_iter().map(usage_row_out).collect(),
    })
}

fn usage_row_out(row: store::metrics::UsageRow) -> UsageRowOut {
    UsageRowOut {
        key: row.key,
        calls: row.calls,
        tool_calls: row.tool_calls,
        tokens: row.tokens,
        cost_usd: row.cost_usd,
        tool_cost_usd: row.tool_cost_usd,
        priced_calls: row.priced_calls,
        unpriced_calls: row.unpriced_calls,
        runs: row.runs,
    }
}

// ── flow (P1.1/P1.4) ────────────────────────────────────────────────────────────────────

/// `metrics.flow` — the flow family over the board's own rows.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FlowIn {
    pub project_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// A figure whose source ledger does not exist yet: unavailable with the reason, never zero.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UnavailableOut {
    pub metric: String,
    pub reason: String,
}

/// The flow metrics and the figures this slice cannot compute.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FlowOut {
    pub wip: i64,
    pub throughput: i64,
    pub lead_time_seconds: Option<f64>,
    pub cycle_time_seconds: Option<f64>,
    pub queue_time_seconds: Option<f64>,
    pub block_time_seconds: Option<f64>,
    pub flow_efficiency: Option<f64>,
    pub unavailable: Vec<UnavailableOut>,
}

#[crate::action(
    name = "metrics.flow",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_flow(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: FlowIn,
) -> Result<FlowOut, ActionError> {
    let (since, until) = window(input.since, input.until);
    let flow = store::metrics::flow_tx(&mut *tx, ctx.tenant, input.project_id, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(FlowOut {
        wip: flow.wip,
        throughput: flow.throughput,
        lead_time_seconds: flow.lead_time_seconds,
        cycle_time_seconds: flow.cycle_time_seconds,
        queue_time_seconds: flow.queue_time_seconds,
        block_time_seconds: flow.block_time_seconds,
        flow_efficiency: flow.flow_efficiency,
        unavailable: vec![
            UnavailableOut {
                metric: "knowledge.lesson_promotion_rate".to_owned(),
                reason: "the lesson ledger (T8) is not in the schema".to_owned(),
            },
            UnavailableOut {
                metric: "knowledge.pack_hit_rate".to_owned(),
                reason: "no knowledge-retrieval ledger exists; inference_call records the prompt, not the pack hit".to_owned(),
            },
        ],
    })
}

// ── the job and the run drill-down (P1.3) ───────────────────────────────────────────────

/// `metrics.job` — the job's metering and, with a run id, its step/call drill-down.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobIn {
    pub job_binding_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One step's spend.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StepOut {
    pub step_id: Option<Uuid>,
    pub action: Option<String>,
    pub position: Option<i32>,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
}

/// One call at the drill-down grain.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CallOut {
    pub id: Uuid,
    pub model: String,
    pub tokens: i64,
    pub cost_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// One tool call at the drill-down grain.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallOut {
    pub id: Uuid,
    pub tool_key: String,
    pub cost_usd: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// The run total and the rows that sum to it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunOut {
    pub run_id: Uuid,
    pub tokens: i64,
    pub cost_usd: f64,
    pub tool_cost_usd: f64,
    pub steps: Vec<StepOut>,
    pub calls: Vec<CallOut>,
    pub tool_calls: Vec<ToolCallOut>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobOut {
    pub levels: Vec<UsageRowOut>,
    pub run: Option<RunOut>,
}

#[crate::action(
    name = "metrics.job",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_job(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: JobIn,
) -> Result<JobOut, ActionError> {
    if input.job_binding_id.is_none() && input.run_id.is_none() {
        return Err(ActionError::new(
            422,
            "missing_scope",
            "name a job_binding_id or a run_id",
        ));
    }
    let (since, until) = window(input.since, input.until);
    let mut levels = Vec::new();
    if let Some(job_binding_id) = input.job_binding_id {
        levels = store::metrics::usage_by_level_tx(
            &mut *tx,
            ctx.tenant,
            store::metrics::MeteringLevel::Job,
            since,
            until,
            Some(job_binding_id),
        )
        .await
        .map_err(|error| metrics_error(error, ctx.action))?
        .into_iter()
        .map(usage_row_out)
        .collect();
    }
    let run = match input.run_id {
        Some(run_id) => {
            let drill = store::metrics::run_drilldown_tx(&mut *tx, ctx.tenant, run_id)
                .await
                .map_err(|error| metrics_error(error, ctx.action))?;
            Some(RunOut {
                run_id: drill.run_id,
                tokens: drill.tokens,
                cost_usd: drill.cost_usd,
                tool_cost_usd: drill.tool_cost_usd,
                steps: drill
                    .steps
                    .into_iter()
                    .map(|step| StepOut {
                        step_id: step.step_id,
                        action: step.action,
                        position: step.position,
                        tokens: step.tokens,
                        cost_usd: step.cost_usd,
                        tool_cost_usd: step.tool_cost_usd,
                    })
                    .collect(),
                calls: drill
                    .calls
                    .into_iter()
                    .map(|call| CallOut {
                        id: call.id,
                        model: call.model,
                        tokens: call.tokens,
                        cost_usd: call.cost_usd,
                        created_at: call.created_at,
                    })
                    .collect(),
                tool_calls: drill
                    .tool_calls
                    .into_iter()
                    .map(|call| ToolCallOut {
                        id: call.id,
                        tool_key: call.tool_key,
                        cost_usd: call.cost_usd,
                        created_at: call.created_at,
                    })
                    .collect(),
            })
        }
        None => None,
    };
    Ok(JobOut { levels, run })
}

// ── billing.summary — the ledger figure (issue #450) ────────────────────────────────────

/// The window `billing.summary` reports over.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct BillingSummaryIn {
    /// Days back from now; default 30, bounded `1..=365`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_days: Option<i64>,
}

/// The conveyor's own usage for one window.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BillingSummaryOut {
    /// The window's start (inclusive).
    pub from: DateTime<Utc>,
    /// The window's end (exclusive, `now` at read time).
    pub to: DateTime<Utc>,
    /// Derived spend: inference from the price catalogue plus the ledger's tool cost.
    pub total_usd: f64,
    /// Model calls in the window.
    pub inference_calls: i64,
    /// Tool calls in the window.
    pub tool_calls: i64,
}

/// The cost/budget overview, derived from the conveyor's own ledger.
///
/// V1 read this from a separate Python billing service. #46 (owner, 2026-11-16) ruled billing is
/// the GPU-surface usage figure derived at read time from `inference_call`/`tool_call`; this
/// action is that figure under the name the console already calls. An empty window is a zero
/// with zero calls — never an upstream error.
#[crate::action(
    name = "billing.summary",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn billing_summary(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BillingSummaryIn,
) -> Result<BillingSummaryOut, ActionError> {
    let days = input.window_days.unwrap_or(30).clamp(1, 365);
    let until = Utc::now();
    let since = until - Duration::days(days);
    let summary = store::metrics::usage_summary_tx(&mut *tx, ctx.tenant, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(BillingSummaryOut {
        from: since,
        to: until,
        total_usd: summary.total_usd,
        inference_calls: summary.inference_calls,
        tool_calls: summary.tool_calls,
    })
}

// ── the billing reconciliation (P2) ─────────────────────────────────────────────────────

/// The billing figure is read by the **declared** `billing.summary` read action, never by a
/// handler: the engine owns that path's timeout, breaker, idempotency and audit. This action
/// consumes the attestation — the total, or the failure when that action could not read.
/// `metrics.reconcile` — the conveyor's metering against billing's roll-up for one period.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReconcileIn {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    /// The billing roll-up total, as returned by the declared `billing.summary` action.
    pub billing_total_usd: Option<f64>,
    /// The upstream error when `billing.summary` could not read the service.
    pub billing_error: Option<String>,
    /// Where an incident card is opened when the totals differ.
    pub board_id: Option<Uuid>,
    pub list_id: Option<Uuid>,
}

/// The verdict and its figures. An unreachable billing service is `unavailable`, never a zero.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReconcileOut {
    pub state: String,
    pub conveyor_usd: Option<f64>,
    pub billing_usd: Option<f64>,
    pub delta_usd: Option<f64>,
    pub incident_card_id: Option<Uuid>,
    pub upstream_error: Option<String>,
}

#[crate::action(
    name = "metrics.reconcile",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn metrics_reconcile(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReconcileIn,
) -> Result<ReconcileOut, ActionError> {
    if input.period_end < input.period_start {
        return Err(ActionError::new(
            422,
            "invalid_period",
            "period_end must be on or after period_start",
        ));
    }
    let since = input
        .period_start
        .and_hms_opt(0, 0, 0)
        .map(|value| value.and_utc())
        .ok_or_else(ActionError::internal)?;
    let until = (input.period_end + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .map(|value| value.and_utc())
        .ok_or_else(ActionError::internal)?;
    let conveyor_usd = store::metrics::conveyor_cost_tx(&mut *tx, ctx.tenant, since, until)
        .await
        .map_err(|error| metrics_error(error, ctx.action))?;
    let (state, billing_usd, delta) = match (
        input.billing_total_usd,
        input.billing_error.as_deref(),
    ) {
        (Some(total), None) if total.is_finite() => {
            let (state, delta) = rules::reconcile(conveyor_usd, total);
            (state, total, delta)
        }
        (None, Some(error)) if !error.trim().is_empty() => {
            return record_unavailable(&mut *tx, ctx, &input, error).await;
        }
        _ => {
            return Err(ActionError::new(
                422,
                "billing_figure_required",
                "name exactly one of billing_total_usd (from the declared billing.summary action) or billing_error (that action's upstream failure)",
            ));
        }
    };
    let mut incident_card_id = None;
    if state == rules::ReconciliationState::Difference {
        let (Some(board_id), Some(list_id)) = (input.board_id, input.list_id) else {
            return Err(ActionError::new(
                422,
                "incident_target_required",
                format!(
                    "the totals differ (conveyor {conveyor_usd}, billing {billing_usd}); name board_id and list_id so the incident card can be opened"
                ),
            ));
        };
        let name = format!(
            "Billing reconciliation {}..{}",
            input.period_start, input.period_end
        );
        let card = crate::board::cards::cards_create(
            &mut *tx,
            ctx.clone(),
            CardCreateIn {
                board_id,
                list_id,
                name,
                description: Some(format!(
                    "conveyor {conveyor_usd} vs billing {billing_usd}; delta {delta}"
                )),
                card_type_code: "incident".to_owned(),
                due_at: None,
                parent_card_id: None,
                job_binding_id: None,
                meta: Some(serde_json::json!({
                    "period_start": input.period_start.to_string(),
                    "period_end": input.period_end.to_string(),
                    "conveyor_usd": conveyor_usd,
                    "billing_usd": billing_usd,
                    "delta_usd": delta,
                })),
            },
        )
        .await?
        .value;
        incident_card_id = Some(Uuid::parse_str(&card.id).map_err(|error| {
            tracing::error!(%error, "metrics.reconcile: the incident card id is not a uuid");
            ActionError::internal()
        })?);
    }
    store::metrics::record_reconciliation_tx(
        &mut *tx,
        ctx.tenant,
        store::metrics::NewReconciliation {
            period_start: input.period_start,
            period_end: input.period_end,
            conveyor_usd: Some(conveyor_usd),
            billing_usd: Some(billing_usd),
            delta_usd: Some(delta),
            state: state.as_str(),
            upstream_error: None,
            incident_card_id,
            actor: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(ReconcileOut {
        state: state.as_str().to_owned(),
        conveyor_usd: Some(conveyor_usd),
        billing_usd: Some(billing_usd),
        delta_usd: Some(delta),
        incident_card_id,
        upstream_error: None,
    })
}

async fn record_unavailable(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: &ReconcileIn,
    reason: &str,
) -> Result<ReconcileOut, ActionError> {
    store::metrics::record_reconciliation_tx(
        tx,
        ctx.tenant,
        store::metrics::NewReconciliation {
            period_start: input.period_start,
            period_end: input.period_end,
            conveyor_usd: None,
            billing_usd: None,
            delta_usd: None,
            state: rules::ReconciliationState::Unavailable.as_str(),
            upstream_error: Some(reason),
            incident_card_id: None,
            actor: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| metrics_error(error, ctx.action))?;
    Ok(ReconcileOut {
        state: rules::ReconciliationState::Unavailable.as_str().to_owned(),
        conveyor_usd: None,
        billing_usd: None,
        delta_usd: None,
        incident_card_id: None,
        upstream_error: Some(reason.to_owned()),
    })
}
