//! The kata action family (SPEC_V2 §6.3, §9.3–§9.4, §12 T5; issue #77).
//!
//! Six Gets, in the procedure's order, each one immutable evidence:
//!
//! - `kata.open` — the record, on an **active V2 job binding**; the binding's pinned
//!   `release_hash` is copied onto the kata, so the kata is an ordinary job on the spine (§10.3,
//!   D31) and a retired binding can never carry one (D38);
//! - `kata.observe` — Go to See;
//! - `kata.analyse` — Grasp the Situation;
//! - `kata.hypothesise` — the **pre-registration**: the prediction and the instrument version are
//!   written before any candidate or measurement exists (P1.1);
//! - `kata.propose` — Get to Solution: the candidate change, after the hypothesis;
//! - `kata.measure` — the held-out measurement (P1.2): only the locked `test` split is read, and
//!   the result names the split and the instrument version;
//! - `kata.get` — the record, its Gets and its measurement.
//!
//! What is deliberately absent: adoption (P3), the revert verdict (P2) and the open-time
//! instrument/threshold gate (P4). The status machine already has their vocabulary; they add
//! behaviour, not schema.
//!
//! Writes declare `kata:write`; reads declare `kata:read`. Neither is `evals:approve` — a
//! measurement returns the aggregate, never a held-out case row, so the held-out wall
//! (`machine_forbidden`) is untouched.

use chrono::{DateTime, Utc};
use conveyor_domain::kata::{Get, HELD_OUT_SPLIT, Surface, movement};
use conveyor_store as store;
use conveyor_store::kata::{
    KataGetRow, KataMeasurementRow, KataRow, KataStoreError, NewKata, NewMeasurement,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

// ── views ────────────────────────────────────────────────────────────────────────────────

/// One recorded Get.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataGetView {
    /// `observe | analyse | hypothesis | solution | …`.
    pub get: String,
    /// The Get's evidence, as recorded.
    pub content: Value,
    /// Who recorded it.
    pub recorded_by: Option<String>,
    /// When it was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// The kata's one held-out measurement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataMeasurementView {
    /// Row id.
    pub id: String,
    /// The pre-registered eval version.
    pub instrument_version_id: String,
    /// Always `test` — the held-out side of the locked split.
    pub split: String,
    /// The quality metric that moved.
    pub metric_key: String,
    /// The candidate run.
    pub candidate_run_id: String,
    /// The baseline run.
    pub baseline_run_id: String,
    /// Distinct held-out cases on the candidate side.
    pub candidate_cases: i64,
    /// Distinct held-out cases on the baseline side.
    pub baseline_cases: i64,
    /// The candidate's held-out mean.
    pub candidate_score: f64,
    /// The baseline's held-out mean.
    pub baseline_score: f64,
    /// `candidate_score - baseline_score` (raw).
    pub delta: f64,
    /// When it was written.
    pub recorded_at: DateTime<Utc>,
}

/// One kata: the record, its Gets and its measurement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataView {
    /// Row id.
    pub id: String,
    /// The ordinary job binding the kata runs on.
    pub job_binding_id: String,
    /// The binding's pinned release hash (copied at open).
    pub release_hash: String,
    /// The open signature (unique while the kata is live).
    pub signature: String,
    /// Display title.
    pub title: String,
    /// `procedure | prompt | knowledge | tool | job_config | code`.
    pub target_surface: String,
    /// What the change targets (a procedure version, a prompt ref, …).
    pub target_ref: String,
    /// `open | observed | analysed | hypothesised | proposed | measured | …`.
    pub status: String,
    /// The pre-registered prediction, once the hypothesis Get completes.
    pub prediction: Option<String>,
    /// The pre-registered instrument version.
    pub instrument_version_id: Option<String>,
    /// The success threshold (P4 reads it at open).
    pub threshold: Option<f64>,
    /// The Gets recorded so far, in procedure order.
    pub gets: Vec<KataGetView>,
    /// The measurement, once it exists.
    pub measurement: Option<KataMeasurementView>,
    /// The row version.
    pub version: i64,
    /// When the kata was opened.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

fn hex(id: Uuid) -> String {
    id.to_string()
}

fn get_view(row: &KataGetRow) -> KataGetView {
    KataGetView {
        get: row.get_key.clone(),
        content: row.content.clone(),
        recorded_by: row.recorded_by.map(hex),
        recorded_at: row.created_at,
    }
}

fn measurement_view(row: &KataMeasurementRow) -> KataMeasurementView {
    KataMeasurementView {
        id: hex(row.id),
        instrument_version_id: hex(row.instrument_version_id),
        split: row.split.clone(),
        metric_key: row.metric_key.clone(),
        candidate_run_id: hex(row.candidate_run_id),
        baseline_run_id: hex(row.baseline_run_id),
        candidate_cases: i64::from(row.candidate_cases),
        baseline_cases: i64::from(row.baseline_cases),
        candidate_score: row.candidate_score,
        baseline_score: row.baseline_score,
        delta: row.delta,
        recorded_at: row.created_at,
    }
}

async fn view_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    row: KataRow,
) -> Result<KataView, ActionError> {
    let gets = store::kata::gets_tx(&mut *conn, tenant, row.id)
        .await
        .map_err(|error| kata_error(error, "kata.get"))?;
    let measurement = store::kata::measurement_tx(&mut *conn, tenant, row.id)
        .await
        .map_err(|error| kata_error(error, "kata.get"))?;
    Ok(KataView {
        id: hex(row.id),
        job_binding_id: hex(row.job_binding_id),
        release_hash: row.release_hash,
        signature: row.signature,
        title: row.title,
        target_surface: row.target_surface,
        target_ref: row.target_ref,
        status: row.status,
        prediction: row.prediction,
        instrument_version_id: row.instrument_version_id.map(hex),
        threshold: row.threshold,
        gets: gets.iter().map(get_view).collect(),
        measurement: measurement.as_ref().map(measurement_view),
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ── errors ───────────────────────────────────────────────────────────────────────────────

fn kata_error(error: KataStoreError, action: &str) -> ActionError {
    match error {
        KataStoreError::Database(error) => store_error(error, action),
        KataStoreError::NotFound(id) => {
            ActionError::resource_not_found(&format!("such kata ({id})"))
        }
        KataStoreError::GetAlreadyRecorded(get) => ActionError::new(
            409,
            "get_already_recorded",
            format!("{action}: the {get} Get is already recorded; a Get is evidence"),
        ),
        KataStoreError::GetOutOfOrder { current, required } => ActionError::new(
            409,
            "get_out_of_order",
            format!("{action}: the kata is {current}; this step requires {required}"),
        ),
        KataStoreError::SignatureOpen(id) => ActionError::new(
            409,
            "kata_signature_open",
            format!("{action}: the signature is already owned by open kata {id}"),
        ),
        KataStoreError::InstrumentMissing(id) => invalid(
            "instrument_version_id",
            "not_found",
            format!("no eval version {id} in this tenant"),
        ),
        KataStoreError::InstrumentNotLocked(id) => ActionError::new(
            422,
            "instrument_not_locked",
            format!(
                "{action}: eval version {id} has no locked split; draw and lock one before \
                 pre-registering it"
            ),
        ),
        KataStoreError::MetricMissing { version_id, key } => invalid(
            "metric_key",
            "not_found",
            match key {
                Some(key) => format!("eval version {version_id} has no quality metric {key:?}"),
                None => format!("eval version {version_id} has no quality metric"),
            },
        ),
        KataStoreError::HypothesisMalformed(field) => invalid(
            field,
            "invalid_value",
            "the hypothesis is a non-empty prediction and an instrument version",
        ),
        KataStoreError::RunMissing(id) => {
            ActionError::resource_not_found(&format!("such eval run ({id})"))
        }
        KataStoreError::RunNotComplete { run_id, status } => ActionError::new(
            409,
            "run_not_complete",
            format!("{action}: run {run_id} is {status}; a measurement reads a complete run"),
        ),
        KataStoreError::InstrumentMismatch {
            run_id,
            expected,
            actual,
        } => ActionError::new(
            409,
            "instrument_mismatch",
            format!(
                "{action}: run {run_id} graded eval version {actual}, but the kata pre-registered \
                 {expected}"
            ),
        ),
        KataStoreError::NoHeldOutCases(run_id) => ActionError::new(
            422,
            "no_held_out_cases",
            format!("{action}: run {run_id} graded no case on the held-out split"),
        ),
    }
}

// ── inputs ───────────────────────────────────────────────────────────────────────────────

/// `kata.open`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataOpenIn {
    /// The active V2 job binding the kata runs on (its pin is copied onto the kata).
    pub job_binding_id: Uuid,
    /// The signature that stays unique among live katas (P4.2).
    pub signature: String,
    /// Display title.
    pub title: String,
    /// `procedure | prompt | knowledge | tool | job_config | code`.
    pub target_surface: String,
    /// What the change targets.
    pub target_ref: String,
    /// The success threshold (P4 reads it at open).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// One kata by id.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataIdIn {
    /// The kata id.
    pub id: Uuid,
}

/// `kata.observe` — Go to See.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataObserveIn {
    /// The kata.
    pub kata_id: Uuid,
    /// What the work and its evidence show.
    pub observation: String,
}

/// `kata.analyse` — Grasp the Situation.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataAnalyseIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The problem statement, with its data.
    pub problem_statement: String,
}

/// `kata.hypothesise` — the pre-registration, before the candidate exists.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataHypothesiseIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The falsifiable prediction the measurement will test.
    pub prediction: String,
    /// The locked eval version that will measure it.
    pub instrument_version_id: Uuid,
}

/// `kata.propose` — Get to Solution.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataProposeIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The candidate change's reference (a procedure version, prompt ref, …).
    pub candidate_ref: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `kata.measure` — the held-out measurement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataMeasureIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The candidate's complete eval run.
    pub candidate_run_id: Uuid,
    /// The baseline's complete eval run.
    pub baseline_run_id: Uuid,
    /// The quality metric to read; omitted means the instrument's first quality metric.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_key: Option<String>,
}

// ── actions ──────────────────────────────────────────────────────────────────────────────

/// Open one kata on an active binding.
#[crate::action(
    name = "kata.open",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_open(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataOpenIn,
) -> Result<HandlerOutput<KataView>, ActionError> {
    let signature = input.signature.trim();
    let title = input.title.trim();
    let target_ref = input.target_ref.trim();
    for (field, value) in [
        ("signature", signature),
        ("title", title),
        ("target_ref", target_ref),
    ] {
        if value.is_empty() {
            return Err(invalid(field, "required", "must not be empty"));
        }
    }
    let Some(surface) = Surface::parse(input.target_surface.trim()) else {
        return Err(invalid(
            "target_surface",
            "invalid_value",
            "must be one of procedure, prompt, knowledge, tool, job_config, code",
        ));
    };
    if let Some(threshold) = input.threshold
        && !(threshold.is_finite() && threshold >= 0.0)
    {
        return Err(invalid(
            "threshold",
            "invalid_value",
            "must be a finite number >= 0",
        ));
    }

    // The kata is an ordinary job on the V2 spine: it opens only on an ACTIVE binding, and it
    // copies the binding's pin. A retired binding is `409 job_binding_stale`, the same refusal a
    // run gets (D38).
    let binding =
        store::organisation::job_binding_get_tx(&mut *tx, ctx.tenant, input.job_binding_id)
            .await
            .map_err(|error| match error {
                store::organisation::OrganisationStoreError::Database(error) => {
                    store_error(error, "kata.open")
                }
                store::organisation::OrganisationStoreError::NotFound(_) => {
                    ActionError::resource_not_found("such job binding")
                }
                _ => ActionError::internal(),
            })?;
    if binding.status != "active" {
        return Err(ActionError::new(
            409,
            "job_binding_stale",
            format!(
                "kata.open: job binding {} is {}; a kata opens only on an active binding",
                binding.id, binding.status
            ),
        ));
    }

    let row = store::kata::open_tx(
        &mut *tx,
        ctx.tenant,
        NewKata {
            job_binding_id: binding.id,
            release_hash: binding.release_hash.clone(),
            signature: signature.to_owned(),
            title: title.to_owned(),
            target_surface: surface.as_str().to_owned(),
            target_ref: target_ref.to_owned(),
            threshold: input.threshold,
            created_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| kata_error(error, "kata.open"))?;
    let view = view_tx(&mut *tx, ctx.tenant, row).await?;
    Ok(HandlerOutput::created(view))
}

/// One kata, its Gets and its measurement.
#[crate::action(
    name = "kata.get",
    version = 1,
    kind = "internal",
    scopes = ["kata:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataIdIn,
) -> Result<KataView, ActionError> {
    let row = store::kata::get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| kata_error(error, "kata.get"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Go to See: record the observation.
#[crate::action(
    name = "kata.observe",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_observe(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataObserveIn,
) -> Result<KataView, ActionError> {
    let observation = input.observation.trim();
    if observation.is_empty() {
        return Err(invalid("observation", "required", "must not be empty"));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Observe,
        json!({ "observation": observation }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.observe"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Grasp the Situation: record the problem statement.
#[crate::action(
    name = "kata.analyse",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_analyse(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataAnalyseIn,
) -> Result<KataView, ActionError> {
    let statement = input.problem_statement.trim();
    if statement.is_empty() {
        return Err(invalid(
            "problem_statement",
            "required",
            "must not be empty",
        ));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Analyse,
        json!({ "problem_statement": statement }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.analyse"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Pre-register the prediction and the instrument version (P1.1).
#[crate::action(
    name = "kata.hypothesise",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_hypothesise(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataHypothesiseIn,
) -> Result<KataView, ActionError> {
    let prediction = input.prediction.trim();
    if prediction.is_empty() {
        return Err(invalid("prediction", "required", "must not be empty"));
    }
    // The instrument must exist AND its split must be locked: a pre-registration against a
    // split that can still be drawn is not a held-out pre-registration.
    store::kata::instrument_tx(&mut *tx, ctx.tenant, input.instrument_version_id, None)
        .await
        .map_err(|error| kata_error(error, "kata.hypothesise"))?;
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Hypothesis,
        json!({
            "prediction": prediction,
            "instrument_version_id": input.instrument_version_id,
        }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.hypothesise"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Get to Solution: record the candidate change, after the pre-registration.
#[crate::action(
    name = "kata.propose",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_propose(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataProposeIn,
) -> Result<KataView, ActionError> {
    let candidate_ref = input.candidate_ref.trim();
    if candidate_ref.is_empty() {
        return Err(invalid("candidate_ref", "required", "must not be empty"));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Solution,
        json!({
            "candidate_ref": candidate_ref,
            "description": input.description,
        }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.propose"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Measure the candidate on the pre-registered instrument's held-out split (P1.2).
#[crate::action(
    name = "kata.measure",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_measure(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataMeasureIn,
) -> Result<HandlerOutput<KataView>, ActionError> {
    let kata = store::kata::get_tx(&mut *tx, ctx.tenant, input.kata_id)
        .await
        .map_err(|error| kata_error(error, "kata.measure"))?;

    // P1.1's ordering, refused with its own code: there is no measurement without a
    // pre-registered prediction and instrument.
    let Some(version_id) = kata.instrument_version_id else {
        return Err(ActionError::new(
            409,
            "prediction_missing",
            format!(
                "kata.measure: kata {} has no pre-registered prediction and instrument version; \
                 complete the hypothesis Get first",
                kata.id
            ),
        ));
    };
    if kata.status != "proposed" {
        return Err(ActionError::new(
            409,
            "get_out_of_order",
            format!(
                "kata.measure: the kata is {}; a measurement requires a proposed candidate",
                kata.status
            ),
        ));
    }

    let instrument = store::kata::instrument_tx(
        &mut *tx,
        ctx.tenant,
        version_id,
        input.metric_key.as_deref(),
    )
    .await
    .map_err(|error| kata_error(error, "kata.measure"))?;

    for run_id in [input.candidate_run_id, input.baseline_run_id] {
        let run = store::kata::run_tx(&mut *tx, ctx.tenant, run_id)
            .await
            .map_err(|error| kata_error(error, "kata.measure"))?;
        if run.status != "complete" {
            return Err(kata_error(
                KataStoreError::RunNotComplete {
                    run_id,
                    status: run.status,
                },
                "kata.measure",
            ));
        }
        if run.version_id != instrument.version_id {
            return Err(kata_error(
                KataStoreError::InstrumentMismatch {
                    run_id,
                    expected: instrument.version_id,
                    actual: run.version_id,
                },
                "kata.measure",
            ));
        }
    }

    let candidate_scores = store::kata::held_out_scores_tx(
        &mut *tx,
        ctx.tenant,
        input.candidate_run_id,
        &instrument.metric_key,
    )
    .await
    .map_err(|error| kata_error(error, "kata.measure"))?;
    let baseline_scores = store::kata::held_out_scores_tx(
        &mut *tx,
        ctx.tenant,
        input.baseline_run_id,
        &instrument.metric_key,
    )
    .await
    .map_err(|error| kata_error(error, "kata.measure"))?;
    let movement = movement(&candidate_scores, &baseline_scores).ok_or_else(|| {
        ActionError::new(
            422,
            "no_held_out_cases",
            "kata.measure: the held-out split contributed no graded case",
        )
    })?;

    let result = json!({
        "split": HELD_OUT_SPLIT,
        "instrument_version_id": version_id,
        "metric_key": instrument.metric_key.clone(),
        "better": instrument.better.clone(),
        "candidate": {
            "run_id": input.candidate_run_id,
            "cases": movement.candidate.cases,
            "score": movement.candidate.mean,
        },
        "baseline": {
            "run_id": input.baseline_run_id,
            "cases": movement.baseline.cases,
            "score": movement.baseline.mean,
        },
        "delta": movement.delta,
        "directed_delta": movement.directed_delta(&instrument.better),
    });

    let (row, _) = store::kata::insert_measurement_tx(
        &mut *tx,
        ctx.tenant,
        NewMeasurement {
            kata_id: kata.id,
            instrument_version_id: instrument.version_id,
            metric_key: instrument.metric_key.clone(),
            candidate_run_id: input.candidate_run_id,
            baseline_run_id: input.baseline_run_id,
            candidate_cases: movement.candidate.cases as i32,
            baseline_cases: movement.baseline.cases as i32,
            candidate_score: movement.candidate.mean,
            baseline_score: movement.baseline.mean,
            delta: movement.delta,
            result,
        },
    )
    .await
    .map_err(|error| kata_error(error, "kata.measure"))?;
    let view = view_tx(&mut *tx, ctx.tenant, row).await?;
    Ok(HandlerOutput::ok(view))
}
