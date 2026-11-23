#![forbid(unsafe_code)]
//! T5 (#77) — the kata loop's queries (SPEC_V2 §6.3, §9.3–§9.4, §12 T5;
//! `docs/plans/inception/T5/02-architecture-design.md`).
//!
//! The family is one lifecycle over four tables (`0069_kata.sql`):
//!
//! ```text
//! kata_open ──► kata_observe ──► kata_analyse ──► kata_propose ──► kata_measure
//!                                                       │                │
//!                                                       │                ▼
//!                                                       └──► kata_adopt (human gate)
//!                                                                        │
//!                                       kata_propagate (per-target gate) ─┤
//!                                                                        ▼
//!                                                                  kata_close
//! ```
//!
//! # The rules this module is the first lock for
//!
//! - **No unmeasurable kata is opened.** Every instrument field, the split with its sha and a
//!   non-empty prediction object are required at [`kata_open_tx`]; a missing one is
//!   [`KataError::Unmeasurable`] and no row is written (acceptance 4).
//! - **One open kata per signature.** A pre-check refuses the duplicate typedly and
//!   `uq_kata_open_signature` is the concurrency guarantee, mapped to
//!   [`KataError::SignatureOpen`] (D-T5-7).
//! - **The prediction precedes the measurement.** A candidate is proposed with its prediction;
//!   [`kata_measure_tx`] refuses a candidate without one ([`KataError::PredictionMissing`]).
//! - **The measurement is held out.** [`kata_measure_tx`] accepts only `split='test'` on the
//!   kata's pinned `instrument_version` and `split_sha256`; anything else is
//!   [`KataError::MeasurementNotHeldOut`] (D-T5-3). The reading is an immutable row.
//! - **A person adopts, never the kata itself.** [`kata_adopt_tx`] requires an approved
//!   `kata.adopt` gate naming the kata whose latest HUMAN decision is `approve`, and refuses the
//!   kata's opener or measurer as decider ([`KataError::SelfApproval`]) — a machine-only
//!   decision is no adoption (D-T5-4).
//! - **A revert names its restore.** `kata.close{outcome:'reverted'}` without `restored_ref` is
//!   [`KataError::RevertMissing`] (D-T5-5).
//!
//! The actions (`kata.open|observe|analyse|propose|measure|adopt|propagate|close`) live in
//! `conveyor-actions`; this module is the only SQL they may reach. `kata_propose_tx` and
//! `kata_propagate_tx` take the acting user for the action layer's outbox event; neither row has
//! an actor column (the migration's column set is the design's four-table list).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

/// The instrument kinds a kata may pin (§12 T5: an eval or a benchmark).
pub const INSTRUMENT_KINDS: [&str; 2] = ["eval", "benchmark"];

/// The closed set of close outcomes (D-T5-5: revert is an explicit outcome).
pub const KATA_OUTCOMES: [&str; 3] = ["kept", "reverted", "inconclusive"];

/// The gate a human adoption must cite.
pub const ADOPT_GATE: &str = "kata.adopt";

/// The gate each propagation target must cite.
pub const PROPAGATE_GATE: &str = "kata.propagate";

/// The columns every kata read returns.
const KATA_COLUMNS: &str = "id, project_id, signature, instrument_kind, instrument_id, \
    instrument_version, split, split_sha256, prediction, base_ref, observations, problem, status, \
    outcome, outcome_reason, restored_ref, opened_by, version, created_at, updated_at";

/// The columns every candidate read returns.
const CANDIDATE_COLUMNS: &str = "id, kata_id, ordinal, title, rationale, target_ref, prediction, \
    status, version, created_at, updated_at";

/// The columns every measurement read returns.
const MEASUREMENT_COLUMNS: &str = "id, kata_id, candidate_id, instrument_kind, instrument_id, \
    instrument_version, split, split_sha256, run_id, metric, noise_band, measured_by, created_at";

/// The columns every propagation read returns.
const PROPAGATION_COLUMNS: &str =
    "id, kata_id, target_ref, approval_id, status, version, created_at, updated_at";

/// One kata, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataRecord {
    /// The row id.
    pub id: Uuid,
    /// The kata project (D-T5-1: a kata is a project type).
    pub project_id: Uuid,
    /// The repeat signature; at most one OPEN row per signature.
    pub signature: String,
    /// `eval` or `benchmark`.
    pub instrument_kind: String,
    /// The instrument pinned at open.
    pub instrument_id: Uuid,
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
    /// The six-Gets evidence set, appended one observation at a time.
    pub observations: Value,
    /// The data-backed problem statement, once analysed.
    pub problem: Option<Value>,
    /// `open` or `closed`.
    pub status: String,
    /// `kept`, `reverted` or `inconclusive`; NULL while no outcome is recorded.
    pub outcome: Option<String>,
    /// Why the outcome was recorded.
    pub outcome_reason: String,
    /// The version a revert restored; NULL for every other outcome.
    pub restored_ref: Option<Value>,
    /// Who opened the kata (the principal adoption may not be decided by).
    pub opened_by: Uuid,
    /// The change counter.
    pub version: i64,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One proposed candidate, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CandidateRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning kata.
    pub kata_id: Uuid,
    /// Position in the kata's proposal order (1-based).
    pub ordinal: i32,
    /// The candidate's title.
    pub title: String,
    /// Why the candidate is expected to win.
    pub rationale: String,
    /// The target the candidate would change.
    pub target_ref: Value,
    /// The pre-registered prediction. `None` only in a hand-written row; measure refuses it.
    pub prediction: Option<Value>,
    /// `proposed`, `measured`, `adopted`, `reverted` or `inconclusive`.
    pub status: String,
    /// The change counter.
    pub version: i64,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One immutable instrument reading, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MeasurementRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning kata.
    pub kata_id: Uuid,
    /// The measured candidate.
    pub candidate_id: Uuid,
    /// `eval` or `benchmark`, copied from the kata's pin.
    pub instrument_kind: String,
    /// The instrument id, copied from the kata's pin.
    pub instrument_id: Uuid,
    /// The instrument version the run used (must equal the kata's pin).
    pub instrument_version: String,
    /// The split the run used (must be `test`).
    pub split: String,
    /// The split's locking sha (must equal the kata's pin).
    pub split_sha256: String,
    /// The instrument run this reading came from.
    pub run_id: Uuid,
    /// The measured metric.
    pub metric: Value,
    /// The measurement noise band.
    pub noise_band: Value,
    /// Who ran the measurement.
    pub measured_by: Uuid,
    /// When the reading was recorded.
    pub created_at: DateTime<Utc>,
}

/// One propagation target, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PropagationRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning kata.
    pub kata_id: Uuid,
    /// The target tenant's `(kind, id, base_version)`.
    pub target_ref: Value,
    /// The `kata.propagate` approval that authorised the target's publish.
    pub approval_id: Option<Uuid>,
    /// `pending`, `approved`, `rejected` or `published`.
    pub status: String,
    /// The change counter.
    pub version: i64,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// Why a kata request breaks a §6.3 / §12 T5 rule.
#[derive(Clone, Debug, thiserror::Error)]
pub enum KataError {
    /// An instrument field, the split or the prediction is absent (or the kind is unknown).
    #[error("the kata cannot be measured: {missing} is missing or invalid")]
    Unmeasurable {
        /// The refused field.
        missing: &'static str,
    },
    /// A second open kata for one signature.
    #[error("an open kata already exists for this signature")]
    SignatureOpen,
    /// A candidate has no pre-registered prediction.
    #[error("the candidate carries no pre-registered prediction")]
    PredictionMissing,
    /// The measurement is not on the held-out split.
    #[error("the measurement is not held out (split={split:?}); only 'test' is accepted")]
    MeasurementNotHeldOut {
        /// The split the caller supplied.
        split: String,
    },
    /// A reverted close without the version it restored.
    #[error("a reverted kata must name the version it restored")]
    RevertMissing,
    /// The kata is not open.
    #[error("the kata is {status:?}, not open")]
    NotOpen {
        /// The row's current status.
        status: String,
    },
    /// The candidate has no held-out measurement yet.
    #[error("the candidate has no held-out measurement to adopt")]
    CandidateNotMeasured,
    /// The adoption or propagation gate is not another person's decision.
    #[error("the kata has no independent human decision for this gate")]
    SelfApproval,
    /// A propagation target has no approved human `kata.propagate` decision.
    #[error(
        "approval {approval_id} is not an approved human kata.propagate decision for this propagation"
    )]
    PropagationApprovalMissing {
        /// The approval the caller cited.
        approval_id: Uuid,
    },
    /// An observation carries no evidence reference.
    #[error("the observation carries no evidence reference")]
    ObservationWithoutEvidence,
    /// A problem statement carries no measurement data.
    #[error("the problem statement carries no measurement data")]
    ProblemWithoutData,
}

/// Why a kata read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum KataStoreError {
    /// No such row in this tenant.
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// The request breaks a §6.3 / §12 T5 rule.
    #[error(transparent)]
    Illegal(#[from] KataError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// True when a prediction is a pre-registered hypothesis: a non-empty JSON object.
fn has_prediction(prediction: Option<&Value>) -> bool {
    prediction
        .and_then(Value::as_object)
        .is_some_and(|object| !object.is_empty())
}

/// True when an observation names at least one evidence reference: a non-empty array, or an
/// object with a non-empty `refs`/`evidence` array (the six-Gets evidence set).
fn has_evidence_ref(evidence: &Value) -> bool {
    match evidence {
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => ["refs", "evidence"].iter().any(|key| {
            object
                .get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        }),
        _ => false,
    }
}

/// True when a problem statement carries measurement data: a non-empty object with a non-empty
/// `data` or `refs` array (step 2 of the loop: a problem with data, never an assertion).
fn has_measurement_data(problem: &Value) -> bool {
    let Some(object) = problem.as_object() else {
        return false;
    };
    !object.is_empty()
        && ["data", "refs"].iter().any(|key| {
            object
                .get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        })
}

/// A unique-index violation is how a concurrent opener loses the one-open-kata race; Postgres
/// reports the constraint name, so the refusal can stay typed.
fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
        .is_some_and(|name| name == constraint)
}

/// Lock one kata row in the caller's tenant, or [`KataStoreError::NotFound`].
async fn lock_kata_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<KataRecord, KataStoreError> {
    let sql = format!(
        "SELECT {KATA_COLUMNS} FROM kata
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound("kata"))
}

/// Refuse a write on a kata that is not open.
fn ensure_open(kata: &KataRecord) -> Result<(), KataStoreError> {
    if kata.status != "open" {
        return Err(KataError::NotOpen {
            status: kata.status.clone(),
        }
        .into());
    }
    Ok(())
}

/// Open one kata: pin the instrument, the held-out split and the pre-registered prediction.
///
/// Every instrument field, the split with its sha and a non-empty prediction object are
/// required; a missing one is [`KataError::Unmeasurable`] and nothing is written (acceptance 4).
/// The project must exist in the tenant. A second open kata for the same signature is
/// [`KataError::SignatureOpen`] — checked first, then guaranteed by
/// `uq_kata_open_signature` for concurrent openers (D-T5-7).
#[allow(clippy::too_many_arguments)] // one argument per column of the row it validates and writes
pub async fn kata_open_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: Uuid,
    project_id: Uuid,
    signature: &str,
    instrument_kind: &str,
    instrument_id: Option<Uuid>,
    instrument_version: &str,
    split: &str,
    split_sha256: &str,
    prediction: &Value,
    base_ref: &Value,
) -> Result<KataRecord, KataStoreError> {
    let signature = signature.trim();
    if signature.is_empty() {
        return Err(KataError::Unmeasurable {
            missing: "signature",
        }
        .into());
    }
    let instrument_kind = instrument_kind.trim();
    if !INSTRUMENT_KINDS.contains(&instrument_kind) {
        return Err(KataError::Unmeasurable {
            missing: "instrument_kind",
        }
        .into());
    }
    let Some(instrument_id) = instrument_id else {
        return Err(KataError::Unmeasurable {
            missing: "instrument_id",
        }
        .into());
    };
    let instrument_version = instrument_version.trim();
    if instrument_version.is_empty() {
        return Err(KataError::Unmeasurable {
            missing: "instrument_version",
        }
        .into());
    }
    let split = split.trim();
    if split.is_empty() {
        return Err(KataError::Unmeasurable { missing: "split" }.into());
    }
    let split_sha256 = split_sha256.trim();
    if split_sha256.is_empty() {
        return Err(KataError::Unmeasurable {
            missing: "split_sha256",
        }
        .into());
    }
    if !has_prediction(Some(prediction)) {
        return Err(KataError::Unmeasurable {
            missing: "prediction",
        }
        .into());
    }
    let project: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM project WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_one(&mut *conn)
    .await?;
    if !project {
        return Err(KataStoreError::NotFound("project"));
    }
    let already_open: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM kata
             WHERE tenant_id = $1 AND signature = $2 AND status = 'open'
         )",
    )
    .bind(tenant.as_uuid())
    .bind(signature)
    .fetch_one(&mut *conn)
    .await?;
    if already_open {
        return Err(KataError::SignatureOpen.into());
    }
    let sql = format!(
        "INSERT INTO kata
             (tenant_id, project_id, signature, instrument_kind, instrument_id,
              instrument_version, split, split_sha256, prediction, base_ref, opened_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING {KATA_COLUMNS}"
    );
    let outcome: Result<KataRecord, sqlx::Error> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(signature)
        .bind(instrument_kind)
        .bind(instrument_id)
        .bind(instrument_version)
        .bind(split)
        .bind(split_sha256)
        .bind(prediction)
        .bind(base_ref)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await;
    match outcome {
        Ok(row) => Ok(row),
        // A concurrent opener lost the partial unique index; the refusal stays typed.
        Err(error) if is_unique_violation(&error, "uq_kata_open_signature") => {
            Err(KataError::SignatureOpen.into())
        }
        Err(error) => Err(error.into()),
    }
}

/// Append one observation to an open kata's six-Gets evidence set.
///
/// The evidence must name at least one reference — a non-empty array, or an object with a
/// non-empty `refs`/`evidence` array; anything else is
/// [`KataError::ObservationWithoutEvidence`]. The observation is stored verbatim in
/// `observations`, so the set reads back in the order the loop worked it.
pub async fn kata_observe_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    evidence: &Value,
) -> Result<KataRecord, KataStoreError> {
    let current = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&current)?;
    if !has_evidence_ref(evidence) {
        return Err(KataError::ObservationWithoutEvidence.into());
    }
    let sql = format!(
        "UPDATE kata
            SET observations = observations || jsonb_build_array($3::jsonb), version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'open'
          RETURNING {KATA_COLUMNS}"
    );
    let row: Option<KataRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(evidence)
        .fetch_optional(&mut *conn)
        .await?;
    // The row was locked above, so this is unreachable while the lock is held; a changed row
    // is still refused rather than reported as a successful observation.
    row.ok_or_else(|| {
        KataError::NotOpen {
            status: "changed".to_owned(),
        }
        .into()
    })
}

/// Record the data-backed problem statement on an open kata.
///
/// The statement must be a non-empty object carrying at least one measurement-data reference
/// (a non-empty `data` or `refs` array); an assertion without data is
/// [`KataError::ProblemWithoutData`] (step 2 of the loop, §6.3).
pub async fn kata_analyse_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    problem: &Value,
) -> Result<KataRecord, KataStoreError> {
    let current = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&current)?;
    if !has_measurement_data(problem) {
        return Err(KataError::ProblemWithoutData.into());
    }
    let sql = format!(
        "UPDATE kata
            SET problem = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'open'
          RETURNING {KATA_COLUMNS}"
    );
    let row: Option<KataRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(problem)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or_else(|| {
        KataError::NotOpen {
            status: "changed".to_owned(),
        }
        .into()
    })
}

/// Propose one candidate on an open kata.
///
/// The prediction is pre-registered, never optional: a missing or empty one is
/// [`KataError::PredictionMissing`] (encouragement 5 in the action catalogue, §6.3). The
/// ordinal is `max + 1` under the kata's row lock, so concurrent proposers cannot collide.
/// `_actor` is the acting user the action layer names in its outbox event; the row has no
/// actor column.
#[allow(clippy::too_many_arguments)] // one argument per field of the row it writes
pub async fn kata_propose_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    _actor: Uuid,
    kata_id: Uuid,
    title: &str,
    rationale: &str,
    target_ref: &Value,
    prediction: &Value,
) -> Result<CandidateRecord, KataStoreError> {
    let kata = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&kata)?;
    if !has_prediction(Some(prediction)) {
        return Err(KataError::PredictionMissing.into());
    }
    let ordinal: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(ordinal), 0) + 1 FROM kata_candidate
          WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "INSERT INTO kata_candidate
             (tenant_id, kata_id, ordinal, title, rationale, target_ref, prediction)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {CANDIDATE_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(ordinal)
        .bind(title.trim())
        .bind(rationale)
        .bind(target_ref)
        .bind(prediction)
        .fetch_one(&mut *conn)
        .await?)
}

/// Record one held-out measurement of a candidate.
///
/// The kata must be open and the candidate must belong to it and carry a prediction
/// ([`KataError::PredictionMissing`]). The reading is accepted only when the instrument
/// version and split sha equal the kata's pins and the split is `test`
/// ([`KataError::MeasurementNotHeldOut`], D-T5-3): a train split or a re-drawn split proves
/// nothing. The measurement row is immutable; the candidate advances to `measured`.
#[allow(clippy::too_many_arguments)] // one argument per field of the row it writes
pub async fn kata_measure_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: Uuid,
    kata_id: Uuid,
    candidate_id: Uuid,
    instrument_version: &str,
    split: &str,
    split_sha256: &str,
    run_id: Uuid,
    metric: &Value,
    noise_band: &Value,
) -> Result<MeasurementRecord, KataStoreError> {
    let kata = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&kata)?;
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} FROM kata_candidate
          WHERE tenant_id = $1 AND id = $2 AND kata_id = $3 FOR UPDATE"
    );
    let candidate: CandidateRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(candidate_id)
        .bind(kata_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound("kata candidate"))?;
    if !has_prediction(candidate.prediction.as_ref()) {
        return Err(KataError::PredictionMissing.into());
    }
    let instrument_version = instrument_version.trim();
    let split = split.trim();
    let split_sha256 = split_sha256.trim();
    if split != "test"
        || instrument_version != kata.instrument_version
        || split_sha256 != kata.split_sha256
    {
        return Err(KataError::MeasurementNotHeldOut {
            split: split.to_owned(),
        }
        .into());
    }
    let sql = format!(
        "INSERT INTO kata_measurement
             (tenant_id, kata_id, candidate_id, instrument_kind, instrument_id,
              instrument_version, split, split_sha256, run_id, metric, noise_band, measured_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING {MEASUREMENT_COLUMNS}"
    );
    let measurement: MeasurementRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(candidate_id)
        .bind(&kata.instrument_kind)
        .bind(kata.instrument_id)
        .bind(&kata.instrument_version)
        .bind(&kata.split)
        .bind(&kata.split_sha256)
        .bind(run_id)
        .bind(metric)
        .bind(noise_band)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await?;
    // `proposed` → `measured`; a repeat reading against the same locked split keeps the status
    // (the candidate has no editable fields either way).
    sqlx::query(
        "UPDATE kata_candidate
            SET status = 'measured', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'proposed'",
    )
    .bind(tenant.as_uuid())
    .bind(candidate_id)
    .execute(&mut *conn)
    .await?;
    Ok(measurement)
}

/// Adopt a measured candidate under an independent human decision.
///
/// The candidate must be `measured` ([`KataError::CandidateNotMeasured`]). The cited approval
/// must be an approved `kata.adopt` gate naming this kata and carrying a HUMAN `approve`
/// decision whose decider is neither the kata's opener nor the candidate's measurer — anything
/// else (pending, machine-only, self-decided) is [`KataError::SelfApproval`] (D-T5-4,
/// acceptance 3). The candidate becomes `adopted` and the kata records `kept`; the kata stays
/// open until [`kata_close_tx`].
pub async fn kata_adopt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    candidate_id: Uuid,
    approval_id: Uuid,
) -> Result<CandidateRecord, KataStoreError> {
    let kata = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&kata)?;
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} FROM kata_candidate
          WHERE tenant_id = $1 AND id = $2 AND kata_id = $3 FOR UPDATE"
    );
    let candidate: CandidateRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(candidate_id)
        .bind(kata_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound("kata candidate"))?;
    if candidate.status != "measured" {
        return Err(KataError::CandidateNotMeasured.into());
    }
    let decider: Option<Uuid> = sqlx::query_scalar(
        "SELECT d.approver_id
           FROM approval a
           JOIN approval_decision d
             ON d.tenant_id = a.tenant_id AND d.approval_id = a.id
          WHERE a.tenant_id = $1 AND a.id = $2 AND a.gate = $3 AND a.status = 'approved'
            AND a.meta->>'target' = $4
            AND d.decision = 'approve' AND d.decided_by_kind = 'human'
          ORDER BY d.created_at DESC, d.id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(ADOPT_GATE)
    .bind(kata_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    let Some(decider) = decider else {
        // No human approval: a pending or machine-only decision adopts nothing.
        return Err(KataError::SelfApproval.into());
    };
    // The executor never approves its own work (P4): the opener and the measurer are the
    // kata's principals, and neither may be the person who adopts.
    let measured_by_decider: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM kata_measurement
             WHERE tenant_id = $1 AND kata_id = $2 AND candidate_id = $3 AND measured_by = $4
         )",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .bind(candidate_id)
    .bind(decider)
    .fetch_one(&mut *conn)
    .await?;
    if decider == kata.opened_by || measured_by_decider {
        return Err(KataError::SelfApproval.into());
    }
    let sql = format!(
        "UPDATE kata_candidate
            SET status = 'adopted', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'measured'
          RETURNING {CANDIDATE_COLUMNS}"
    );
    let adopted: Option<CandidateRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(candidate_id)
        .fetch_optional(&mut *conn)
        .await?;
    let adopted = adopted.ok_or(KataError::CandidateNotMeasured)?;
    // The kata records the adoption but stays open: only `kata_close_tx` closes it.
    sqlx::query(
        "UPDATE kata
            SET outcome = 'kept', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .execute(&mut *conn)
    .await?;
    Ok(adopted)
}

/// Register one propagation target on an open kata.
///
/// The row starts `pending`: each target carries its own approval (D-T5-6) and is never
/// published from here. `_actor` is the acting user the action layer names in its outbox event;
/// the row has no actor column.
pub async fn kata_propagate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    _actor: Uuid,
    kata_id: Uuid,
    target_ref: &Value,
) -> Result<PropagationRecord, KataStoreError> {
    let kata = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&kata)?;
    let sql = format!(
        "INSERT INTO kata_propagation (tenant_id, kata_id, target_ref)
         VALUES ($1, $2, $3)
         RETURNING {PROPAGATION_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(target_ref)
        .fetch_one(&mut *conn)
        .await?)
}

/// Publish one propagation target under its own approved human gate.
///
/// The cited approval must be an approved `kata.propagate` gate naming this propagation row and
/// carrying a HUMAN `approve` decision; anything else is
/// [`KataError::PropagationApprovalMissing`]. The row becomes `published` — the target's own
/// publish action remains the write that reaches production (D-T5-6).
pub async fn kata_propagation_approve_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    propagation_id: Uuid,
    approval_id: Uuid,
) -> Result<PropagationRecord, KataStoreError> {
    let sql = format!(
        "SELECT {PROPAGATION_COLUMNS} FROM kata_propagation
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let current: PropagationRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(propagation_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound("kata propagation"))?;
    let decider: Option<Uuid> = sqlx::query_scalar(
        "SELECT d.approver_id
           FROM approval a
           JOIN approval_decision d
             ON d.tenant_id = a.tenant_id AND d.approval_id = a.id
          WHERE a.tenant_id = $1 AND a.id = $2 AND a.gate = $3 AND a.status = 'approved'
            AND a.meta->>'target' = $4
            AND d.decision = 'approve' AND d.decided_by_kind = 'human'
          ORDER BY d.created_at DESC, d.id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(PROPAGATE_GATE)
    .bind(propagation_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    if decider.is_none() {
        return Err(KataError::PropagationApprovalMissing { approval_id }.into());
    }
    let sql = format!(
        "UPDATE kata_propagation
            SET status = 'published', approval_id = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {PROPAGATION_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(current.id)
        .bind(approval_id)
        .fetch_one(&mut *conn)
        .await?)
}

/// Close an open kata with its outcome.
///
/// `outcome` is `kept`, `reverted` or `inconclusive` (the migration repeats the closed set in a
/// CHECK). A `reverted` close must name the version it restored — `restored_ref` NULL or JSON
/// null is [`KataError::RevertMissing`] (D-T5-5, acceptance 2). The kata becomes `closed`; a
/// second close is [`KataError::NotOpen`].
pub async fn kata_close_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    outcome: &str,
    reason: &str,
    restored_ref: Option<&Value>,
) -> Result<KataRecord, KataStoreError> {
    let current = lock_kata_tx(&mut *conn, tenant, kata_id).await?;
    ensure_open(&current)?;
    let restored = if outcome == "reverted" {
        match restored_ref {
            Some(value) if !value.is_null() => Some(value),
            _ => return Err(KataError::RevertMissing.into()),
        }
    } else {
        None
    };
    let sql = format!(
        "UPDATE kata
            SET status = 'closed', outcome = $3, outcome_reason = $4, restored_ref = $5,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'open'
          RETURNING {KATA_COLUMNS}"
    );
    let row: Option<KataRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(outcome)
        .bind(reason.trim())
        .bind(restored)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or_else(|| {
        KataError::NotOpen {
            status: "changed".to_owned(),
        }
        .into()
    })
}

/// Read one kata inside the caller's tenant.
pub async fn kata_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Option<KataRecord>, KataStoreError> {
    let sql = format!("SELECT {KATA_COLUMNS} FROM kata WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List the tenant's katas, newest `updated_at` first, optionally filtered by exact status.
pub async fn kata_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
) -> Result<Vec<KataRecord>, KataStoreError> {
    let sql = format!(
        "SELECT {KATA_COLUMNS} FROM kata
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
          ORDER BY updated_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .fetch_all(&mut *conn)
        .await?)
}
