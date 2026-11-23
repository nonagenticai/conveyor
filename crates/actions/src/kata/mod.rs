#![forbid(unsafe_code)]
//! T5 (#77, SPEC_V2 §6.3, §9.3–§9.4, §12 T5;
//! `docs/plans/inception/T5/02-architecture-design.md`): the kata loop — the pre-registered
//! improvement hypothesis, its evidence set, the candidates, the held-out measurement, the
//! human adoption gate, per-target propagation and the revert outcome.
//!
//! Ten actions over [`conveyor_store::kata`], the family's only SQL (CONTRACT §1):
//! `kata.open` pins an instrument, the held-out split with the sha that locks it and the
//! prediction before anything is measured; `kata.observe` and `kata.analyse` work the six Gets;
//! `kata.propose` records a candidate with its prediction; `kata.measure` records an immutable
//! reading from the held-out instrument; `kata.adopt` records another person's adoption
//! decision; `kata.propagate` registers one target per approval; `kata.close` records the
//! outcome, including the restore a revert names; `kata.get` and `kata.list` read the family.
//!
//! The §6.3 / §12 T5 rules are enforced by the store, never restated here: an unmeasurable kata
//! is never opened, one open kata per signature, the prediction precedes the measurement, only
//! the held-out split on the pinned instrument version and sha measures, a human other than the
//! kata's principals adopts, a reverted close names its restore and every target carries its own
//! approval. This module is the projection — the input schema, the output document and the typed
//! refusal each store error becomes.
//!
//! `kata.adopt` OPENS nothing: the approval is requested separately through `approvals.request`
//! with gate [`conveyor_store::kata::ADOPT_GATE`] and `meta.target` = the kata id, and this
//! action reads that already-decided gate. The same split holds for propagation: the row
//! `kata.propagate` registers is published later by the target tenant's own action under its
//! own `kata.propagate` gate (D-T5-6), never from here.

use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{etag_of, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

use store::kata::{
    CandidateRecord, KataError, KataRecord, KataStoreError, MeasurementRecord, PropagationRecord,
};

/// A store failure as the status and problem code the caller can act on.
fn kata_error(error: KataStoreError, action: &str) -> ActionError {
    match error {
        KataStoreError::NotFound(what) => ActionError::resource_not_found(what),
        KataStoreError::Illegal(illegal) => kata_illegal(illegal),
        KataStoreError::Database(error) => store_error(error, action),
    }
}

/// A §6.3 / §12 T5 rule refusal with its stable problem code.
fn kata_illegal(error: KataError) -> ActionError {
    match error {
        // The refusal names the field the store refused, so a caller can fix the request.
        KataError::Unmeasurable { missing } => ActionError::new(
            422,
            "kata_unmeasurable",
            format!("the kata cannot be measured: {missing} is missing or invalid"),
        ),
        KataError::SignatureOpen => ActionError::new(
            409,
            "kata_signature_open",
            "an open kata already exists for this signature",
        ),
        KataError::PredictionMissing => ActionError::new(
            422,
            "kata_prediction_missing",
            "the candidate carries no pre-registered prediction; the prediction is recorded \
             before the measurement",
        ),
        KataError::MeasurementNotHeldOut { split } => ActionError::new(
            422,
            "kata_measurement_not_held_out",
            format!(
                "the measurement is not held out (split={split:?}); only 'test' on the kata's \
                 pinned instrument version and split sha measures"
            ),
        ),
        KataError::RevertMissing => ActionError::new(
            422,
            "kata_revert_missing",
            "a reverted kata must name the version it restored",
        ),
        KataError::NotOpen { status } => ActionError::new(
            409,
            "kata_not_open",
            format!("the kata is {status:?}, not open"),
        ),
        KataError::CandidateNotMeasured => ActionError::new(
            409,
            "kata_candidate_not_measured",
            "the candidate has no held-out measurement to adopt",
        ),
        KataError::SelfApproval => ActionError::new(
            403,
            "kata_self_approval",
            "the kata has no independent human decision for this gate; the kata's opener, its \
             measurer and a machine-only decision cannot adopt it",
        ),
        KataError::PropagationApprovalMissing { approval_id } => ActionError::new(
            422,
            "kata_propagation_approval_missing",
            format!(
                "approval {approval_id} is not an approved human kata.propagate decision for \
                 this propagation"
            ),
        ),
        KataError::ObservationWithoutEvidence => ActionError::new(
            422,
            "kata_observation_needs_evidence",
            "the observation carries no evidence reference; the six-Gets set is evidence, never \
             an assertion",
        ),
        KataError::ProblemWithoutData => ActionError::new(
            422,
            "kata_problem_needs_data",
            "the problem statement carries no measurement data",
        ),
    }
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// One kata as every `kata.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataOut {
    /// The row id.
    pub id: String,
    /// The kata project (a kata is a project type, D-T5-1).
    pub project_id: String,
    /// The repeat signature; at most one OPEN row per signature.
    pub signature: String,
    /// `eval` or `benchmark`.
    pub instrument_kind: String,
    /// The instrument pinned at open.
    pub instrument_id: String,
    /// The instrument version pinned at open.
    pub instrument_version: String,
    /// The held-out split pinned at open.
    pub split: String,
    /// The sha that locks the split.
    pub split_sha256: String,
    /// The pre-registered hypothesis.
    pub prediction: Value,
    /// The target version the kata is measured against.
    pub base_ref: Value,
    /// `open` or `closed`.
    pub status: String,
    /// `kept`, `reverted` or `inconclusive`; absent while no outcome is recorded.
    pub outcome: Option<String>,
    /// Why the outcome was recorded.
    pub outcome_reason: String,
    /// The version a revert restored; absent for every other outcome.
    pub restored_ref: Option<Value>,
    /// The six-Gets evidence set, in the order the loop worked it.
    pub observations: Value,
    /// The data-backed problem statement, once analysed.
    pub problem: Option<Value>,
    /// The If-Match version.
    pub version: i64,
}

fn kata_out(record: &KataRecord) -> KataOut {
    KataOut {
        id: record.id.to_string(),
        project_id: record.project_id.to_string(),
        signature: record.signature.clone(),
        instrument_kind: record.instrument_kind.clone(),
        instrument_id: record.instrument_id.to_string(),
        instrument_version: record.instrument_version.clone(),
        split: record.split.clone(),
        split_sha256: record.split_sha256.clone(),
        prediction: record.prediction.clone(),
        base_ref: record.base_ref.clone(),
        status: record.status.clone(),
        outcome: record.outcome.clone(),
        outcome_reason: record.outcome_reason.clone(),
        restored_ref: record.restored_ref.clone(),
        observations: record.observations.clone(),
        problem: record.problem.clone(),
        version: record.version,
    }
}

/// One proposed candidate as `kata.propose` and `kata.adopt` return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CandidateOut {
    /// The row id.
    pub id: String,
    /// The owning kata.
    pub kata_id: String,
    /// Position in the kata's proposal order (1-based).
    pub ordinal: i32,
    /// The candidate's title.
    pub title: String,
    /// Why the candidate is expected to win.
    pub rationale: String,
    /// The target the candidate would change.
    pub target_ref: Value,
    /// The pre-registered prediction recorded with the candidate.
    pub prediction: Option<Value>,
    /// `proposed`, `measured`, `adopted`, `reverted` or `inconclusive`.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

fn candidate_out(record: &CandidateRecord) -> CandidateOut {
    CandidateOut {
        id: record.id.to_string(),
        kata_id: record.kata_id.to_string(),
        ordinal: record.ordinal,
        title: record.title.clone(),
        rationale: record.rationale.clone(),
        target_ref: record.target_ref.clone(),
        prediction: record.prediction.clone(),
        status: record.status.clone(),
        version: record.version,
    }
}

/// One immutable instrument reading as `kata.measure` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeasurementOut {
    /// The row id.
    pub id: String,
    /// The owning kata.
    pub kata_id: String,
    /// The measured candidate.
    pub candidate_id: String,
    /// `eval` or `benchmark`, copied from the kata's pin.
    pub instrument_kind: String,
    /// The instrument id, copied from the kata's pin.
    pub instrument_id: String,
    /// The instrument version the run used (the kata's pin).
    pub instrument_version: String,
    /// The split the run used (`test`).
    pub split: String,
    /// The split's locking sha (the kata's pin).
    pub split_sha256: String,
    /// The instrument run this reading came from.
    pub run_id: String,
    /// The measured metric.
    pub metric: Value,
    /// The measurement noise band.
    pub noise_band: Value,
}

fn measurement_out(record: &MeasurementRecord) -> MeasurementOut {
    MeasurementOut {
        id: record.id.to_string(),
        kata_id: record.kata_id.to_string(),
        candidate_id: record.candidate_id.to_string(),
        instrument_kind: record.instrument_kind.clone(),
        instrument_id: record.instrument_id.to_string(),
        instrument_version: record.instrument_version.clone(),
        split: record.split.clone(),
        split_sha256: record.split_sha256.clone(),
        run_id: record.run_id.to_string(),
        metric: record.metric.clone(),
        noise_band: record.noise_band.clone(),
    }
}

/// One propagation target as `kata.propagate` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PropagationOut {
    /// The row id.
    pub id: String,
    /// The owning kata.
    pub kata_id: String,
    /// The target tenant's `(kind, id, base_version)`.
    pub target_ref: Value,
    /// The `kata.propagate` approval that authorised the target's publish, once decided.
    pub approval_id: Option<String>,
    /// `pending`, `approved`, `rejected` or `published`.
    pub status: String,
    /// The If-Match version.
    pub version: i64,
}

fn propagation_out(record: &PropagationRecord) -> PropagationOut {
    PropagationOut {
        id: record.id.to_string(),
        kata_id: record.kata_id.to_string(),
        target_ref: record.target_ref.clone(),
        approval_id: record.approval_id.map(|id| id.to_string()),
        status: record.status.clone(),
        version: record.version,
    }
}

// ── inputs ──────────────────────────────────────────────────────────────────────────────

/// `kata.open` — the measurement contract, pinned before anything is measured.
///
/// The instrument fields, the split sha and the prediction are defaulted rather than required
/// by the schema so an absent one reaches the store and is refused as the typed
/// `422 kata_unmeasurable` naming the field, never the schema's generic refusal. The store is
/// the first lock (and the migration the second), so the action layer does not restate it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataOpenIn {
    /// The kata project the hypothesis belongs to.
    pub project_id: Uuid,
    /// The repeat signature; at most one OPEN kata per signature.
    pub signature: String,
    /// `eval` or `benchmark`.
    #[serde(default)]
    pub instrument_kind: String,
    /// The instrument to pin.
    #[serde(default)]
    pub instrument_id: Option<Uuid>,
    /// The instrument version to pin.
    #[serde(default)]
    pub instrument_version: String,
    /// The held-out split to pin.
    #[serde(default)]
    pub split: String,
    /// The sha that locks the split.
    #[serde(default)]
    pub split_sha256: String,
    /// The pre-registered hypothesis: a non-empty object.
    #[serde(default)]
    pub prediction: Value,
    /// The target version the kata is measured against.
    pub base_ref: Value,
}

/// `kata.observe` — append one evidence-backed observation.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataObserveIn {
    /// The kata being worked.
    pub kata_id: Uuid,
    /// The observation and its evidence reference(s).
    pub evidence: Value,
}

/// `kata.analyse` — record the data-backed problem statement.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataAnalyseIn {
    /// The kata being worked.
    pub kata_id: Uuid,
    /// The problem statement and its measurement data.
    pub problem: Value,
}

/// `kata.propose` — one candidate with its pre-registered prediction.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataProposeIn {
    /// The kata the candidate answers.
    pub kata_id: Uuid,
    /// The candidate's title.
    pub title: String,
    /// Why the candidate is expected to win.
    pub rationale: String,
    /// The target the candidate would change.
    pub target_ref: Value,
    /// The pre-registered prediction: recorded before the measurement, never after it.
    #[serde(default)]
    pub prediction: Value,
}

/// `kata.measure` — one reading from the held-out instrument run.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataMeasureIn {
    /// The kata the measurement belongs to.
    pub kata_id: Uuid,
    /// The candidate that was measured.
    pub candidate_id: Uuid,
    /// The instrument version the run used; must equal the kata's pin.
    pub instrument_version: String,
    /// The split the run used; only `test` (the held-out split) measures.
    pub split: String,
    /// The split's sha; must equal the kata's pin.
    pub split_sha256: String,
    /// The instrument run this reading came from.
    pub run_id: Uuid,
    /// The measured metric.
    pub metric: Value,
    /// The measurement noise band.
    pub noise_band: Value,
}

/// `kata.adopt` — read an independently decided human adoption gate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataAdoptIn {
    /// The kata whose candidate is adopted.
    pub kata_id: Uuid,
    /// The measured candidate to adopt.
    pub candidate_id: Uuid,
    /// The approved `kata.adopt` gate naming this kata, decided by a person.
    pub approval_id: Uuid,
}

/// `kata.propagate` — register one target, under its own later approval.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataPropagateIn {
    /// The kata whose improvement propagates.
    pub kata_id: Uuid,
    /// The target tenant's `(kind, id, base_version)`.
    pub target_ref: Value,
}

/// `kata.close` — the outcome, and the restore a revert names.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataCloseIn {
    /// The kata to close.
    pub kata_id: Uuid,
    /// `kept`, `reverted` or `inconclusive`.
    pub outcome: String,
    /// Why the outcome was recorded.
    pub reason: String,
    /// The version a revert restored; required when `outcome` is `reverted`.
    #[serde(default)]
    pub restored_ref: Option<Value>,
}

/// `kata.get`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KataIdIn {
    /// The kata id, resolved under the caller's tenant.
    pub id: Uuid,
}

/// `kata.list` — the tenant's katas, optionally filtered by exact status.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct KataListIn {
    /// Exact-match status filter.
    #[serde(default)]
    pub status: Option<String>,
    /// The shared pagination envelope; the list is returned whole.
    #[serde(flatten)]
    pub page: PageIn,
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Open one kata: pin the instrument, the held-out split and the pre-registered prediction.
#[crate::action(
    name = "kata.open",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
pub(crate) async fn kata_open(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataOpenIn,
) -> Result<HandlerOutput<KataOut>, ActionError> {
    let record = store::kata::kata_open_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        input.project_id,
        &input.signature,
        &input.instrument_kind,
        input.instrument_id,
        &input.instrument_version,
        &input.split,
        &input.split_sha256,
        &input.prediction,
        &input.base_ref,
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::created(kata_out(&record)).with_etag(etag_of(record.version)))
}

/// Append one observation to the six-Gets evidence set.
#[crate::action(
    name = "kata.observe",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
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
) -> Result<HandlerOutput<KataOut>, ActionError> {
    let record = store::kata::kata_observe_tx(&mut *tx, ctx.tenant, input.kata_id, &input.evidence)
        .await
        .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(kata_out(&record)).with_etag(etag_of(record.version)))
}

/// Record the data-backed problem statement on an open kata.
#[crate::action(
    name = "kata.analyse",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
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
) -> Result<HandlerOutput<KataOut>, ActionError> {
    let record = store::kata::kata_analyse_tx(&mut *tx, ctx.tenant, input.kata_id, &input.problem)
        .await
        .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(kata_out(&record)).with_etag(etag_of(record.version)))
}

/// Propose one candidate; its prediction is pre-registered, never optional.
#[crate::action(
    name = "kata.propose",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
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
) -> Result<HandlerOutput<CandidateOut>, ActionError> {
    let record = store::kata::kata_propose_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        input.kata_id,
        &input.title,
        &input.rationale,
        &input.target_ref,
        &input.prediction,
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::created(candidate_out(&record)).with_etag(etag_of(record.version)))
}

/// Record one held-out measurement of a candidate.
#[crate::action(
    name = "kata.measure",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
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
) -> Result<HandlerOutput<MeasurementOut>, ActionError> {
    let record = store::kata::kata_measure_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        input.kata_id,
        input.candidate_id,
        &input.instrument_version,
        &input.split,
        &input.split_sha256,
        input.run_id,
        &input.metric,
        &input.noise_band,
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    // The measurement is immutable and carries no version, so it answers no ETag.
    Ok(HandlerOutput::created(measurement_out(&record)))
}

/// Adopt a measured candidate under the independently decided human gate.
///
/// The approval is opened and decided elsewhere (`approvals.request` / `approvals.decide` with
/// gate `kata.adopt` and `meta.target` = the kata id); this action only reads the decision and
/// refuses the kata's own principals and any machine-only decision (D-T5-4).
#[crate::action(
    name = "kata.adopt",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_adopt(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataAdoptIn,
) -> Result<HandlerOutput<CandidateOut>, ActionError> {
    let record = store::kata::kata_adopt_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        input.candidate_id,
        input.approval_id,
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(candidate_out(&record)).with_etag(etag_of(record.version)))
}

/// Register one propagation target; each target carries its own later approval (D-T5-6).
#[crate::action(
    name = "kata.propagate",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_propagate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataPropagateIn,
) -> Result<HandlerOutput<PropagationOut>, ActionError> {
    let record = store::kata::kata_propagate_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        input.kata_id,
        &input.target_ref,
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::created(propagation_out(&record)).with_etag(etag_of(record.version)))
}

/// Close an open kata with its outcome; a revert must name the restore (D-T5-5).
#[crate::action(
    name = "kata.close",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataCloseIn,
) -> Result<HandlerOutput<KataOut>, ActionError> {
    let record = store::kata::kata_close_tx(
        &mut *tx,
        ctx.tenant,
        input.kata_id,
        &input.outcome,
        &input.reason,
        input.restored_ref.as_ref(),
    )
    .await
    .map_err(|error| kata_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(kata_out(&record)).with_etag(etag_of(record.version)))
}

/// Read one kata inside the caller's tenant.
#[crate::action(
    name = "kata.get",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
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
) -> Result<HandlerOutput<KataOut>, ActionError> {
    let record = store::kata::kata_get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| kata_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("kata"))?;
    Ok(HandlerOutput::ok(kata_out(&record)).with_etag(etag_of(record.version)))
}

/// List the tenant's katas, newest `updated_at` first, optionally filtered by status.
#[crate::action(
    name = "kata.list",
    version = 1,
    kind = "internal",
    scopes = ["portfolio:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn kata_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KataListIn,
) -> Result<Page<KataOut>, ActionError> {
    let records = store::kata::kata_list_tx(&mut *tx, ctx.tenant, input.status.as_deref())
        .await
        .map_err(|error| kata_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(kata_out).collect(),
        next_cursor: None,
    })
}
