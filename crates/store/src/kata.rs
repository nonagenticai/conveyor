//! The kata store — the record, its six Gets and the held-out measurement (SPEC_V2 §6.3,
//! §9.3–§9.4; T5 P1, issue #77).
//!
//! Three guarantees live in this module, and none of them is the action layer's to re-check:
//!
//! 1. **The pre-registration is atomic.** [`record_get_tx`] sets the prediction and the
//!    instrument version in the same UPDATE, and the table's CHECK refuses one without the
//!    other. A measurement reads them from the row, never from the caller.
//! 2. **Only the held-out split is read.** [`held_out_scores_tx`] joins `eval_case` and filters
//!    `k.split = 'test'`; there is no filter argument that can widen it.
//! 3. **The measurement names its instrument.** The kata pins an `eval_version` at hypothesis
//!    time and each run must be on that version, so a measurement cannot silently grade a
//!    different instrument.
//!
//! Every function takes the caller's already-scoped transaction (`_tx` shape, CONTRACT §7).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::kata::{CaseScore, Get, HELD_OUT_SPLIT};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The kata columns every read returns, in one spelling.
const KATA_COLUMNS: &str = "id, tenant_id, job_binding_id, release_hash, signature, title, \
                            target_surface, target_ref, status, instrument_version_id, prediction, \
                            threshold, created_by, version, created_at, updated_at";

/// The statuses that still own a signature (the partial unique index's predicate).
const LIVE_STATUSES: &str =
    "'open', 'observed', 'analysed', 'hypothesised', 'proposed', 'measured'";

/// One `kata` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub job_binding_id: Uuid,
    /// The binding's pinned release hash, copied at open (§4.4, D38).
    pub release_hash: String,
    pub signature: String,
    pub title: String,
    pub target_surface: String,
    pub target_ref: String,
    pub status: String,
    /// The pre-registered instrument, set by the hypothesis Get.
    pub instrument_version_id: Option<Uuid>,
    /// The pre-registered prediction, set by the hypothesis Get.
    pub prediction: Option<String>,
    pub threshold: Option<f64>,
    pub created_by: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One immutable `kata_get` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataGetRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub get_key: String,
    pub content: Value,
    pub recorded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// One `kata_measurement` row — always a held-out number (`split = 'test'` by CHECK).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataMeasurementRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub instrument_version_id: Uuid,
    pub split: String,
    pub metric_key: String,
    pub candidate_run_id: Uuid,
    pub baseline_run_id: Uuid,
    pub candidate_cases: i32,
    pub baseline_cases: i32,
    pub candidate_score: f64,
    pub baseline_score: f64,
    pub delta: f64,
    pub result: Value,
    pub created_at: DateTime<Utc>,
}

/// The fields `kata.open` writes.
pub struct NewKata {
    pub job_binding_id: Uuid,
    pub release_hash: String,
    pub signature: String,
    pub title: String,
    pub target_surface: String,
    pub target_ref: String,
    pub threshold: Option<f64>,
    pub created_by: Option<Uuid>,
}

/// One instrument read at hypothesis or measurement time.
#[derive(Clone, Debug)]
pub struct Instrument {
    pub version_id: Uuid,
    pub eval_id: Uuid,
    pub metric_key: String,
    /// `higher | lower`.
    pub better: String,
}

/// The run fields a measurement checks.
#[derive(Clone, Debug)]
pub struct RunPin {
    pub version_id: Uuid,
    pub status: String,
}

/// What the kata store can refuse.
#[derive(Debug)]
pub enum KataStoreError {
    /// The database refused the statement.
    Database(sqlx::Error),
    /// No such kata in this tenant.
    NotFound(Uuid),
    /// A Get was recorded twice.
    GetAlreadyRecorded(String),
    /// A Get arrived out of order.
    GetOutOfOrder { current: String, required: String },
    /// An open kata already owns this signature (P4.2's rule).
    SignatureOpen(Uuid),
    /// The hypothesis's instrument version does not exist in this tenant.
    InstrumentMissing(Uuid),
    /// The instrument's split has not been drawn and locked; it is not held out yet.
    InstrumentNotLocked(Uuid),
    /// The instrument has no quality metric (or not the one the caller named).
    MetricMissing {
        version_id: Uuid,
        key: Option<String>,
    },
    /// The hypothesis content is not a prediction plus an instrument version.
    HypothesisMalformed(&'static str),
    /// The measurement's run does not exist in this tenant.
    RunMissing(Uuid),
    /// The run is not finished, so it has no number to read.
    RunNotComplete { run_id: Uuid, status: String },
    /// The run graded a different instrument version than the kata pre-registered.
    InstrumentMismatch {
        run_id: Uuid,
        expected: Uuid,
        actual: Uuid,
    },
    /// The held-out side contributed no graded case: no data, no improvement.
    NoHeldOutCases(Uuid),
}

impl From<sqlx::Error> for KataStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

fn unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
}

/// Open one kata on an active binding's pin.
pub async fn open_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: NewKata,
) -> Result<KataRow, KataStoreError> {
    let sql = format!(
        "INSERT INTO kata
             (tenant_id, job_binding_id, release_hash, signature, title, target_surface,
              target_ref, threshold, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING {KATA_COLUMNS}"
    );
    let result = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.job_binding_id)
        .bind(&new.release_hash)
        .bind(&new.signature)
        .bind(&new.title)
        .bind(&new.target_surface)
        .bind(&new.target_ref)
        .bind(new.threshold)
        .bind(new.created_by)
        .fetch_one(&mut *conn)
        .await;
    match result {
        Ok(row) => Ok(row),
        Err(error) if unique_violation(&error) => {
            let existing: Option<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT id FROM kata WHERE tenant_id = $1 AND signature = $2 \
                 AND status IN ({LIVE_STATUSES})"
            )))
            .bind(tenant.as_uuid())
            .bind(&new.signature)
            .fetch_optional(&mut *conn)
            .await?;
            match existing {
                Some(id) => Err(KataStoreError::SignatureOpen(id)),
                None => Err(KataStoreError::Database(error)),
            }
        }
        Err(error) => Err(KataStoreError::Database(error)),
    }
}

/// One kata by id.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<KataRow, KataStoreError> {
    let sql = format!("SELECT {KATA_COLUMNS} FROM kata WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound(id))
}

/// Every Get the kata has recorded, in the procedure's order.
pub async fn gets_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Vec<KataGetRow>, KataStoreError> {
    let rows = sqlx::query_as::<_, KataGetRow>(
        "SELECT id, kata_id, get_key, content, recorded_by, created_at
           FROM kata_get
          WHERE tenant_id = $1 AND kata_id = $2
          ORDER BY CASE get_key
                     WHEN 'observe' THEN 0 WHEN 'analyse' THEN 1 WHEN 'hypothesis' THEN 2
                     WHEN 'solution' THEN 3 WHEN 'standardisation' THEN 4
                     WHEN 'sustainability' THEN 5 ELSE 6 END, created_at, id",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The kata's measurement, when it has one.
pub async fn measurement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Option<KataMeasurementRow>, KataStoreError> {
    let row = sqlx::query_as::<_, KataMeasurementRow>(
        "SELECT id, kata_id, instrument_version_id, split, metric_key, candidate_run_id,
                baseline_run_id, candidate_cases, baseline_cases, candidate_score,
                baseline_score, delta, result, created_at
           FROM kata_measurement
          WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Record one Get and advance the kata to the status the Get completes.
///
/// The kata row is locked `FOR UPDATE`, so two racing Gets cannot both pass the status check.
/// The hypothesis content is read here — `prediction` and `instrument_version_id` — and written
/// to the readable columns in the same UPDATE as the evidence row.
pub async fn record_get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    get: Get,
    content: Value,
    recorded_by: Option<Uuid>,
) -> Result<KataRow, KataStoreError> {
    let current: Option<(String,)> =
        sqlx::query_as("SELECT status FROM kata WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
            .bind(tenant.as_uuid())
            .bind(kata_id)
            .fetch_optional(&mut *conn)
            .await?;
    let current = match current {
        Some((status,)) => status,
        None => return Err(KataStoreError::NotFound(kata_id)),
    };
    let required = get.requires_status().unwrap_or("open");
    if current != required {
        return Err(KataStoreError::GetOutOfOrder {
            current,
            required: required.to_owned(),
        });
    }

    let (prediction, instrument_version_id) = if get == Get::Hypothesis {
        let Some(prediction) = content.get("prediction").and_then(Value::as_str) else {
            return Err(KataStoreError::HypothesisMalformed("prediction"));
        };
        let Some(instrument) = content
            .get("instrument_version_id")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
        else {
            return Err(KataStoreError::HypothesisMalformed("instrument_version_id"));
        };
        if prediction.trim().is_empty() {
            return Err(KataStoreError::HypothesisMalformed("prediction"));
        }
        (Some(prediction.to_owned()), Some(instrument))
    } else {
        (None, None)
    };

    let inserted = sqlx::query(
        "INSERT INTO kata_get (tenant_id, kata_id, get_key, content, recorded_by)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .bind(get.key())
    .bind(&content)
    .bind(recorded_by)
    .execute(&mut *conn)
    .await;
    if let Err(error) = inserted {
        if unique_violation(&error) {
            return Err(KataStoreError::GetAlreadyRecorded(get.key().to_owned()));
        }
        return Err(KataStoreError::Database(error));
    }

    let completed = get.completes_status();
    let sql = format!(
        "UPDATE kata
            SET status = $3,
                prediction = coalesce($4, prediction),
                instrument_version_id = coalesce($5, instrument_version_id),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {KATA_COLUMNS}"
    );
    let row = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(completed)
        .bind(prediction)
        .bind(instrument_version_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The instrument a kata pre-registers. `metric_key = None` resolves the version's first
/// quality metric by ordinal — the same metric a report headlines.
pub async fn instrument_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    metric_key: Option<&str>,
) -> Result<Instrument, KataStoreError> {
    let version: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT eval_id, split_sha256 IS NOT NULL
           FROM eval_version WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (eval_id, locked) = version.ok_or(KataStoreError::InstrumentMissing(version_id))?;
    if !locked {
        return Err(KataStoreError::InstrumentNotLocked(version_id));
    }
    let metric: Option<(String, String)> = sqlx::query_as(
        "SELECT key, better
           FROM eval_metric
          WHERE tenant_id = $1 AND version_id = $2 AND role = 'quality'
            AND ($3::text IS NULL OR key = $3)
          ORDER BY ordinal, key
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .bind(metric_key)
    .fetch_optional(&mut *conn)
    .await?;
    let (metric_key, better) = metric.ok_or_else(|| KataStoreError::MetricMissing {
        version_id,
        key: metric_key.map(str::to_owned),
    })?;
    Ok(Instrument {
        version_id,
        eval_id,
        metric_key,
        better,
    })
}

/// One run's instrument version and state.
pub async fn run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<RunPin, KataStoreError> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT version_id, status FROM eval_run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (version_id, status) = row.ok_or(KataStoreError::RunMissing(run_id))?;
    Ok(RunPin { version_id, status })
}

/// A run's graded results on the held-out split only, one pair per repetition.
///
/// ⛔ The `k.split = 'test'` predicate is the wall (P1.2). It is not a parameter, and the grades
/// of a `train` case cannot enter this vector by any caller's choice.
pub async fn held_out_scores_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    metric_key: &str,
) -> Result<Vec<CaseScore>, KataStoreError> {
    let rows: Vec<(Uuid, f64)> = sqlx::query_as(
        "SELECT c.case_id, (c.grade ->> $3)::double precision
           FROM eval_case_result c
           JOIN eval_case k ON k.tenant_id = c.tenant_id AND k.id = c.case_id
          WHERE c.tenant_id = $1
            AND c.eval_run_id = $2
            AND c.status = 'ok'
            AND k.split = $4
            AND jsonb_typeof(c.grade -> $3) = 'number'
          ORDER BY c.case_id, c.rep",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(metric_key)
    .bind(HELD_OUT_SPLIT)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Err(KataStoreError::NoHeldOutCases(run_id));
    }
    Ok(rows
        .into_iter()
        .map(|(case_id, score)| CaseScore { case_id, score })
        .collect())
}

/// The fields the measurement row takes from the action's arithmetic.
pub struct NewMeasurement {
    pub kata_id: Uuid,
    pub instrument_version_id: Uuid,
    pub metric_key: String,
    pub candidate_run_id: Uuid,
    pub baseline_run_id: Uuid,
    pub candidate_cases: i32,
    pub baseline_cases: i32,
    pub candidate_score: f64,
    pub baseline_score: f64,
    pub delta: f64,
    pub result: Value,
}

/// Write the measurement and move the kata to `measured`, atomically.
///
/// The kata must be `proposed`; the status is re-read under the row lock so a racing second
/// measurement cannot write a second number.
pub async fn insert_measurement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: NewMeasurement,
) -> Result<(KataRow, KataMeasurementRow), KataStoreError> {
    let current: Option<(String,)> =
        sqlx::query_as("SELECT status FROM kata WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
            .bind(tenant.as_uuid())
            .bind(new.kata_id)
            .fetch_optional(&mut *conn)
            .await?;
    let current = match current {
        Some((status,)) => status,
        None => return Err(KataStoreError::NotFound(new.kata_id)),
    };
    if current != "proposed" {
        return Err(KataStoreError::GetOutOfOrder {
            current,
            required: "proposed".to_owned(),
        });
    }

    let measurement = sqlx::query_as::<_, KataMeasurementRow>(
        "INSERT INTO kata_measurement
             (tenant_id, kata_id, instrument_version_id, metric_key, candidate_run_id,
              baseline_run_id, candidate_cases, baseline_cases, candidate_score,
              baseline_score, delta, result)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, kata_id, instrument_version_id, split, metric_key, candidate_run_id,
                   baseline_run_id, candidate_cases, baseline_cases, candidate_score,
                   baseline_score, delta, result, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(new.kata_id)
    .bind(new.instrument_version_id)
    .bind(&new.metric_key)
    .bind(new.candidate_run_id)
    .bind(new.baseline_run_id)
    .bind(new.candidate_cases)
    .bind(new.baseline_cases)
    .bind(new.candidate_score)
    .bind(new.baseline_score)
    .bind(new.delta)
    .bind(&new.result)
    .fetch_one(&mut *conn)
    .await?;

    let sql = format!(
        "UPDATE kata
            SET status = 'measured', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {KATA_COLUMNS}"
    );
    let kata = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.kata_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok((kata, measurement))
}
