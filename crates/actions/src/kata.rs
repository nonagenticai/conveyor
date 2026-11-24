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
//!   the result names the split and the instrument version; P2 compares the directed delta with
//!   the threshold recorded at open, and a regression past it restores the pre-kata procedure
//!   version and appends the revert;
//! - `kata.adopt` — P3's person's gate: a human holding `evals:approve` (Admin literally, never
//!   the machine `KataExecutor`) adopts a measured improvement;
//! - `kata.get` — the record, its Gets, its measurement and (when they exist) the revert and the
//!   adoption.
//!
//! P4's open-time gate lives in `kata.open`: a kata without a named instrument and a success
//! threshold is never opened, and a second live kata for the same signature is refused by name.
//!
//! Writes declare `kata:write`; reads declare `kata:read`. Neither is `evals:approve` — a
//! measurement returns the aggregate, never a held-out case row, so the held-out wall
//! (`machine_forbidden`) is untouched.

use chrono::{DateTime, Utc};
use conveyor_domain::kata::{Get, HELD_OUT_SPLIT, Surface, Verdict, movement, verdict};
use conveyor_store as store;
use conveyor_store::kata::{
    KataAdoptionRow, KataGetRow, KataMeasurementRow, KataRevertRow, KataRow, KataStoreError,
    NewKata, NewMeasurement, NewRevert,
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
    /// The delta direction-corrected so positive always means better.
    pub directed_delta: f64,
    /// The regression threshold recorded when the kata opened.
    pub threshold: Option<f64>,
    /// `improved | flat | regressed` (P2's threshold comparison).
    pub verdict: String,
    /// When it was written.
    pub recorded_at: DateTime<Utc>,
}

/// P2's append-only revert: the reason and the two procedure versions.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataRevertView {
    /// Row id.
    pub id: String,
    /// Why the candidate was reverted — the measured movement against the threshold.
    pub reason: String,
    /// The pre-kata procedure version the surface was put back to.
    pub baseline_version_id: Option<String>,
    /// The candidate procedure version that was retired.
    pub candidate_version_id: Option<String>,
    /// Whether the target surface was actually restored.
    pub restored: bool,
    /// The person or principal that recorded the revert.
    pub reverted_by: Option<String>,
    /// When the revert was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// P3's adoption decision: the person and their note.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataAdoptionView {
    /// Row id.
    pub id: String,
    /// The adopter's note.
    pub note: String,
    /// The human who adopted the change.
    pub adopted_by: String,
    /// When the decision was recorded.
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
    /// The success threshold (P4 requires it at open; P2 reads it at measure).
    pub threshold: Option<f64>,
    /// The pre-kata procedure version the kata pinned at open (P2's restore target).
    pub baseline_version_id: Option<String>,
    /// The Gets recorded so far, in procedure order.
    pub gets: Vec<KataGetView>,
    /// The measurement, once it exists.
    pub measurement: Option<KataMeasurementView>,
    /// The revert, when the measurement regressed past the threshold (P2).
    pub revert: Option<KataRevertView>,
    /// The adoption decision, once a person has made it (P3).
    pub adoption: Option<KataAdoptionView>,
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
        directed_delta: row
            .result
            .get("directed_delta")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        threshold: row.result.get("threshold").and_then(Value::as_f64),
        verdict: row
            .result
            .get("verdict")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        recorded_at: row.created_at,
    }
}

fn revert_view(row: &KataRevertRow) -> KataRevertView {
    KataRevertView {
        id: hex(row.id),
        reason: row.reason.clone(),
        baseline_version_id: row.baseline_version_id.map(hex),
        candidate_version_id: row.candidate_version_id.map(hex),
        restored: row.restored,
        reverted_by: row.reverted_by.map(hex),
        recorded_at: row.created_at,
    }
}

fn adoption_view(row: &KataAdoptionRow) -> KataAdoptionView {
    KataAdoptionView {
        id: hex(row.id),
        note: row.note.clone(),
        adopted_by: hex(row.adopted_by),
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
    let revert = store::kata::revert_of_tx(&mut *conn, tenant, row.id)
        .await
        .map_err(|error| kata_error(error, "kata.get"))?;
    let adoption = store::kata::adoption_of_tx(&mut *conn, tenant, row.id)
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
        baseline_version_id: row.baseline_version_id.map(hex),
        gets: gets.iter().map(get_view).collect(),
        measurement: measurement.as_ref().map(measurement_view),
        revert: revert.as_ref().map(revert_view),
        adoption: adoption.as_ref().map(adoption_view),
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
        KataStoreError::MissingPrevious(get) => ActionError::new(
            409,
            "get_out_of_order",
            format!("{action}: the {get} Get must be recorded before this step"),
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
        KataStoreError::RevertTarget(detail) => ActionError::new(
            422,
            "revert_target_unresolved",
            format!("{action}: a measured regression cannot be reverted: {detail}"),
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
    /// The instrument that will measure the change (P4: without one, no kata opens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument_version_id: Option<Uuid>,
    /// The success threshold (P4: without one, no kata opens; P2 reads it at measure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// `kata.adopt` — a person's adoption of a measured improvement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataAdoptIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The adopter's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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

/// `kata.standardise` — Get to Standardization.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataStandardiseIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The standard the change was folded into (a procedure/prompt/knowledge/job reference).
    pub standard_ref: String,
    /// Optional description of the update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `kata.sustain` — Get to Sustainability.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataSustainIn {
    /// The kata.
    pub kata_id: Uuid,
    /// How the change is monitored.
    pub monitoring: String,
    /// The guard that catches a regression.
    pub regression_guard: String,
}

/// `kata.stretch` — Get to Stretch (yokoten).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataStretchIn {
    /// The kata.
    pub kata_id: Uuid,
    /// The jobs or departments the change is propagated to.
    pub targets: Vec<String>,
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

    // P4: a kata that cannot name an instrument and a success threshold is not opened. The
    // refusal names every missing piece, because "unmeasurable" is the whole answer.
    let mut missing = Vec::new();
    if input.instrument_version_id.is_none() {
        missing.push("instrument_version_id (the named instrument)");
    }
    if input.threshold.is_none() {
        missing.push("threshold (the success threshold)");
    }
    if !missing.is_empty() {
        return Err(ActionError::new(
            422,
            "kata_not_measurable",
            format!(
                "kata.open: a kata opens only with a named instrument and a success threshold; \
                 missing {}",
                missing.join(" and ")
            ),
        ));
    }
    let instrument_version_id = input.instrument_version_id.expect("checked above");
    let threshold = input.threshold.expect("checked above");
    if !(threshold.is_finite() && threshold >= 0.0) {
        return Err(invalid(
            "threshold",
            "invalid_value",
            "must be a finite number >= 0",
        ));
    }
    // The named instrument must exist and its split must already be locked: the kata opens
    // against a held-out instrument, never a split that can still be drawn.
    store::kata::instrument_tx(&mut *tx, ctx.tenant, instrument_version_id, None)
        .await
        .map_err(|error| kata_error(error, "kata.open"))?;

    // P2: pin the procedure version that is live now, so a measured regression restores
    // exactly the pre-kata state. A non-procedure surface (or a free-form target reference)
    // has no procedure row to pin; `target_ref` stays the evidence.
    let baseline_version_id = if surface == Surface::Procedure {
        match Uuid::parse_str(target_ref) {
            Ok(id) => {
                let sop = store::procedure::get_sop_tx(&mut *tx, ctx.tenant, id)
                    .await
                    .map_err(|error| match error {
                        store::procedure::SopStoreError::Database(error) => {
                            store_error(error, "kata.open")
                        }
                        _ => ActionError::internal(),
                    })?;
                match sop {
                    Some(sop) => Some(sop.id),
                    None => {
                        return Err(invalid(
                            "target_ref",
                            "not_found",
                            format!("no procedure {id} in this tenant"),
                        ));
                    }
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

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
            threshold: Some(threshold),
            instrument_version_id: Some(instrument_version_id),
            baseline_version_id,
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
    // P4 pinned the instrument at open. The hypothesis may only confirm it: an instrument
    // chosen after the kata opened is not the one the kata was opened to test.
    let kata = store::kata::get_tx(&mut *tx, ctx.tenant, input.kata_id)
        .await
        .map_err(|error| kata_error(error, "kata.hypothesise"))?;
    if let Some(pinned) = kata.instrument_version_id
        && pinned != input.instrument_version_id
    {
        return Err(ActionError::new(
            409,
            "instrument_mismatch",
            format!(
                "kata.hypothesise: the kata opened against instrument {pinned}, but the \
                 hypothesis names {}; the instrument is the one the kata was opened with",
                input.instrument_version_id
            ),
        ));
    }
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

/// Get to Standardization: record the update to the procedure, prompt, knowledge or job.
#[crate::action(
    name = "kata.standardise",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_standardise(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataStandardiseIn,
) -> Result<KataView, ActionError> {
    let standard_ref = input.standard_ref.trim();
    if standard_ref.is_empty() {
        return Err(invalid("standard_ref", "required", "must not be empty"));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Standardisation,
        json!({
            "standard_ref": standard_ref,
            "description": input.description,
        }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.standardise"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Get to Sustainability: record the monitoring and the regression guard.
#[crate::action(
    name = "kata.sustain",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_sustain(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataSustainIn,
) -> Result<KataView, ActionError> {
    let monitoring = input.monitoring.trim();
    if monitoring.is_empty() {
        return Err(invalid("monitoring", "required", "must not be empty"));
    }
    let regression_guard = input.regression_guard.trim();
    if regression_guard.is_empty() {
        return Err(invalid("regression_guard", "required", "must not be empty"));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Sustainability,
        json!({
            "monitoring": monitoring,
            "regression_guard": regression_guard,
        }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.sustain"))?;
    view_tx(&mut *tx, ctx.tenant, row).await
}

/// Get to Stretch (yokoten): record the jobs or departments the change is propagated to.
#[crate::action(
    name = "kata.stretch",
    version = 1,
    kind = "internal",
    scopes = ["kata:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_stretch(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataStretchIn,
) -> Result<KataView, ActionError> {
    let targets: Vec<String> = input
        .targets
        .iter()
        .map(|target| target.trim().to_owned())
        .filter(|target| !target.is_empty())
        .collect();
    if targets.is_empty() {
        return Err(invalid(
            "targets",
            "required",
            "name at least one job or department the change is propagated to",
        ));
    }
    let row = store::kata::record_get_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        Get::Stretch,
        json!({ "targets": targets }),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| kata_error(error, "kata.stretch"))?;
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
    let Some(version_id) = kata
        .instrument_version_id
        .filter(|_| kata.prediction.is_some())
    else {
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
    // P4 requires the threshold at open; a row without one is not measurable (the synthetic
    // fixtures predate the gate, so this is a 409 rather than an unwrap).
    let Some(threshold) = kata.threshold else {
        return Err(ActionError::new(
            409,
            "threshold_missing",
            format!(
                "kata.measure: kata {} has no success threshold; a kata without one is not \
                 measurable",
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

    // P2's comparison: the delta, direction-corrected by the metric's `better`, against the
    // threshold recorded when the kata opened.
    let directed_delta = movement.directed_delta(&instrument.better);
    let verdict = verdict(directed_delta, threshold);

    let result = json!({
        "split": HELD_OUT_SPLIT,
        "instrument_version_id": version_id,
        "metric_key": instrument.metric_key.clone(),
        "better": instrument.better.clone(),
        "threshold": threshold,
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
        "directed_delta": directed_delta,
        "verdict": verdict.as_str(),
    });

    let (row, _measurement) = store::kata::insert_measurement_tx(
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

    if verdict != Verdict::Regressed {
        let view = view_tx(&mut *tx, ctx.tenant, row).await?;
        return Ok(HandlerOutput::ok(view));
    }

    // P2: a regression past the threshold reverts the candidate. For a procedure surface the
    // pre-kata version pinned at open is put back; a surface with no restorable version is
    // refused rather than recorded as if it had been reverted.
    let reason = format!(
        "the held-out {} moved {:+.4} against the recorded threshold {:.4} (raw delta {:+.4}, \
         better={}); the candidate regressed past the threshold and was reverted",
        instrument.metric_key, directed_delta, threshold, movement.delta, instrument.better
    );
    let evidence = json!({
        "metric_key": instrument.metric_key,
        "better": instrument.better,
        "delta": movement.delta,
        "directed_delta": directed_delta,
        "threshold": threshold,
        "candidate_score": movement.candidate.mean,
        "baseline_score": movement.baseline.mean,
    });
    let mut baseline_version_id = None;
    let mut candidate_version_id = None;
    let mut restored = false;
    if kata.target_surface == Surface::Procedure.as_str() {
        let candidate_ref = store::kata::candidate_ref_tx(&mut *tx, ctx.tenant, kata.id)
            .await
            .map_err(|error| kata_error(error, "kata.measure"))?;
        let candidate = candidate_ref
            .as_deref()
            .and_then(|raw| Uuid::parse_str(raw).ok());
        match (kata.baseline_version_id, candidate) {
            (Some(baseline), Some(candidate)) => {
                store::kata::restore_procedure_tx(&mut *tx, ctx.tenant, baseline, candidate)
                    .await
                    .map_err(|error| kata_error(error, "kata.measure"))?;
                baseline_version_id = Some(baseline);
                candidate_version_id = Some(candidate);
                restored = true;
            }
            _ => {
                return Err(kata_error(
                    KataStoreError::RevertTarget(format!(
                        "the procedure surface needs the pre-kata version pinned at open and a \
                         candidate version in the solution Get; kata baseline={:?}, \
                         candidate_ref={:?}",
                        kata.baseline_version_id, candidate_ref
                    )),
                    "kata.measure",
                ));
            }
        }
    }
    let (row, _revert) = store::kata::revert_tx(
        &mut *tx,
        ctx.tenant,
        kata.id,
        NewRevert {
            reason,
            baseline_version_id,
            candidate_version_id,
            restored,
            evidence,
            reverted_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| kata_error(error, "kata.measure"))?;
    let view = view_tx(&mut *tx, ctx.tenant, row).await?;
    Ok(HandlerOutput::ok(view))
}

/// Adopt a measured improvement — a person's gate (P3, §9.3, D31/D45).
///
/// ⛔ This action declares `evals:approve`, which `Scope::granted_to` gives to `Role::Admin`
/// literally and never to the machine role `KataExecutor`. The handler adds the second half of
/// the rule: the principal must be a `user`, because a scope a machine holds cannot be a
/// person's sign-off (CONTRACT §20).
#[crate::action(
    name = "kata.adopt",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_adopt(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataAdoptIn,
) -> Result<HandlerOutput<KataView>, ActionError> {
    if ctx.actor_kind != "user" {
        return Err(ActionError::new(
            403,
            "kata_adopt_requires_human",
            format!(
                "kata.adopt: adoption is a person's gate, not a {} principal's",
                ctx.actor_kind
            ),
        ));
    }
    let kata = store::kata::get_tx(&mut *tx, ctx.tenant, input.kata_id)
        .await
        .map_err(|error| kata_error(error, "kata.adopt"))?;
    if kata.status != "measured" {
        return Err(kata_error(
            KataStoreError::GetOutOfOrder {
                current: kata.status,
                required: "measured".to_owned(),
            },
            "kata.adopt",
        ));
    }
    let measurement = store::kata::measurement_tx(&mut *tx, ctx.tenant, kata.id)
        .await
        .map_err(|error| kata_error(error, "kata.adopt"))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "kata_not_adoptable",
                "kata.adopt: the kata has no measurement; adopt nothing",
            )
        })?;
    let verdict = measurement
        .result
        .get("verdict")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if verdict != Verdict::Improved.as_str() {
        return Err(ActionError::new(
            409,
            "kata_not_adoptable",
            format!(
                "kata.adopt: the measurement's verdict is {verdict}; only a measured improvement \
                 can be adopted — a flat, incomparable or reverted result adopts nothing"
            ),
        ));
    }
    let note = input.note.as_deref().unwrap_or("").trim();
    let (row, _adoption) = store::kata::adopt_tx(&mut *tx, ctx.tenant, kata.id, note, ctx.actor_id)
        .await
        .map_err(|error| kata_error(error, "kata.adopt"))?;
    let view = view_tx(&mut *tx, ctx.tenant, row).await?;
    Ok(HandlerOutput::created(view))
}
