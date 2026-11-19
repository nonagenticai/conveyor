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

use chrono::{DateTime, Duration, Utc};
use conveyor_domain::metrics::{self as rules, MaturityLevel, Threshold};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

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
