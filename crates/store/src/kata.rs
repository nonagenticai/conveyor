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
                            threshold, baseline_version_id, created_by, version, created_at, \
                            updated_at, project_id, instrument_kind, instrument_id, \
                            instrument_version, split, split_sha256, prediction_payload, base_ref";

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
    /// The procedure version published when the kata opened — what P2's revert restores.
    pub baseline_version_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // ── the union's fields (0094): the lane's pre-registration and project link.
    pub project_id: Option<Uuid>,
    pub instrument_kind: Option<String>,
    pub instrument_id: Option<Uuid>,
    pub instrument_version: Option<String>,
    pub split: Option<String>,
    pub split_sha256: Option<String>,
    pub prediction_payload: Option<Value>,
    pub base_ref: Option<Value>,
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
///
/// The eval columns are `Option` because the union's second instrument shape is a benchmark
/// suite: its identity is `instrument_kind = 'benchmark'` plus `instrument_id`/`split_sha256`,
/// and its runs are `*_benchmark_run_id` (`F-T5-1`).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataMeasurementRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub instrument_version_id: Option<Uuid>,
    pub split: String,
    pub metric_key: Option<String>,
    pub candidate_run_id: Option<Uuid>,
    pub baseline_run_id: Option<Uuid>,
    pub candidate_cases: i32,
    pub baseline_cases: i32,
    pub candidate_score: f64,
    pub baseline_score: f64,
    pub delta: f64,
    pub result: Value,
    pub created_at: DateTime<Utc>,
    /// `eval` or `benchmark`; NULL on main's eval-only rows.
    pub instrument_kind: Option<String>,
    /// The pinned instrument id (a benchmark suite id).
    pub instrument_id: Option<Uuid>,
    /// The pinned instrument version (a benchmark suite's dataset revision).
    pub instrument_version: Option<String>,
    /// The split's locking sha (a benchmark suite's `seed_sha256`).
    pub split_sha256: Option<String>,
    /// The lane's metric document.
    pub metric: Option<Value>,
    /// The lane's noise band.
    pub noise_band: Option<Value>,
    /// The candidate's benchmark run.
    pub candidate_benchmark_run_id: Option<Uuid>,
    /// The baseline's benchmark run.
    pub baseline_benchmark_run_id: Option<Uuid>,
}

/// The measurement columns both readers and the writer select, in one place.
const MEASUREMENT_COLUMNS: &str = "id, kata_id, instrument_version_id, split, metric_key, \
     candidate_run_id, baseline_run_id, candidate_cases, baseline_cases, candidate_score, \
     baseline_score, delta, result, created_at, instrument_kind, instrument_id, \
     instrument_version, split_sha256, metric, noise_band, candidate_benchmark_run_id, \
     baseline_benchmark_run_id";

/// The fields `kata.open` writes.
pub struct NewKata {
    pub job_binding_id: Uuid,
    pub release_hash: String,
    pub signature: String,
    pub title: String,
    pub target_surface: String,
    pub target_ref: String,
    pub threshold: Option<f64>,
    /// P4: the named instrument; every measurable kata opens with one.
    pub instrument_version_id: Option<Uuid>,
    /// P2: the procedure version that is live when the kata opens.
    pub baseline_version_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    // ── The lane's union fields (0070): the coach's pre-registration and project link.
    /// The kata's project (the lane's design; the kata is a project type).
    pub project_id: Option<Uuid>,
    /// `eval` or `benchmark` (the lane's instrument kind; main's FK is eval-only).
    pub instrument_kind: Option<String>,
    /// The pinned instrument id (the lane's field; main's is instrument_version_id).
    pub instrument_id: Option<Uuid>,
    /// The pinned instrument version (the lane's field).
    pub instrument_version: Option<String>,
    /// The pinned held-out split.
    pub split: Option<String>,
    /// The split's locking sha.
    pub split_sha256: Option<String>,
    /// The lane's pre-registered prediction payload (main's text `prediction` stays free).
    pub prediction_payload: Option<Value>,
    /// The target version the kata is measured against (the lane's field).
    pub base_ref: Option<Value>,
}

/// One kata's append-only revert (P2).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataRevertRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub reason: String,
    pub baseline_version_id: Option<Uuid>,
    pub candidate_version_id: Option<Uuid>,
    /// Whether the target surface was actually put back at its pre-kata version.
    pub restored: bool,
    pub evidence: Value,
    pub reverted_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// One kata's adoption decision (P3).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct KataAdoptionRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub note: String,
    pub adopted_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// The fields a revert writes.
pub struct NewRevert {
    pub reason: String,
    pub baseline_version_id: Option<Uuid>,
    pub candidate_version_id: Option<Uuid>,
    pub restored: bool,
    pub evidence: Value,
    pub reverted_by: Option<Uuid>,
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
    /// A post-adoption Get arrived before the Get it follows (the status is shared, so the
    /// order between Standardisation, Sustainability and Stretch is the recorded rows).
    MissingPrevious(&'static str),
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
    /// P2's revert cannot name the procedure versions to restore.
    RevertTarget(String),
    /// The kata is not in a state this act can move (a closed or unknown status).
    NotOpen { status: String },
    /// A candidate needs a pre-registered prediction.
    PredictionMissing,
    /// A `reverted` close must name the version it restored.
    RevertMissing,
    /// The kata's pre-registered instrument is not the benchmark shape this measurement needs.
    BenchmarkMismatch(String),
    /// A named benchmark run cannot measure this kata's suite.
    BenchmarkRunRefused { run_id: Uuid, detail: String },
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
              target_ref, threshold, instrument_version_id, baseline_version_id, created_by,
              project_id, instrument_kind, instrument_id, instrument_version, split,
              split_sha256, prediction_payload, base_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                 $17, $18, $19)
         RETURNING {KATA_COLUMNS}"
    );
    // The unique index is P4.2's guarantee. A failed INSERT aborts the caller's transaction,
    // so the owner lookup that names the existing kata has to run after a rollback to a
    // savepoint — otherwise Postgres refuses every statement until the transaction ends and
    // the refusal degrades to a 500.
    sqlx::query("SAVEPOINT kata_open_insert")
        .execute(&mut *conn)
        .await?;
    let result = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.job_binding_id)
        .bind(&new.release_hash)
        .bind(&new.signature)
        .bind(&new.title)
        .bind(&new.target_surface)
        .bind(&new.target_ref)
        .bind(new.threshold)
        .bind(new.instrument_version_id)
        .bind(new.baseline_version_id)
        .bind(new.created_by)
        .bind(new.project_id)
        .bind(&new.instrument_kind)
        .bind(new.instrument_id)
        .bind(&new.instrument_version)
        .bind(&new.split)
        .bind(&new.split_sha256)
        .bind(&new.prediction_payload)
        .bind(&new.base_ref)
        .fetch_one(&mut *conn)
        .await;
    match result {
        Ok(row) => {
            sqlx::query("RELEASE SAVEPOINT kata_open_insert")
                .execute(&mut *conn)
                .await?;
            Ok(row)
        }
        Err(error) if unique_violation(&error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT kata_open_insert")
                .execute(&mut *conn)
                .await?;
            let existing: Option<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT id FROM kata WHERE tenant_id = $1 AND signature = $2 \
                 AND status IN ({LIVE_STATUSES})"
            )))
            .bind(tenant.as_uuid())
            .bind(&new.signature)
            .fetch_optional(&mut *conn)
            .await?;
            sqlx::query("RELEASE SAVEPOINT kata_open_insert")
                .execute(&mut *conn)
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
    let sql = format!(
        "SELECT {MEASUREMENT_COLUMNS} FROM kata_measurement
          WHERE tenant_id = $1 AND kata_id = $2"
    );
    let row = sqlx::query_as::<_, KataMeasurementRow>(sqlx::AssertSqlSafe(sql))
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
    let current: Option<(String, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, instrument_kind, instrument_id FROM kata
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (current, instrument_kind, instrument_id) = match current {
        Some(row) => row,
        None => return Err(KataStoreError::NotFound(kata_id)),
    };
    let required = get.requires_status().unwrap_or("open");
    if current != required {
        return Err(KataStoreError::GetOutOfOrder {
            current,
            required: required.to_owned(),
        });
    }

    // The three post-adoption Gets share one status, so the stored rows are what order them:
    // the standard is updated before its monitoring is set, and monitoring exists before the
    // change is propagated.
    let previous = match get {
        Get::Sustainability => Some((Get::Standardisation, "standardisation")),
        Get::Stretch => Some((Get::Sustainability, "sustainability")),
        _ => None,
    };
    if let Some((previous, name)) = previous {
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM kata_get
                 WHERE tenant_id = $1 AND kata_id = $2 AND get_key = $3
            )",
        )
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(previous.key())
        .fetch_one(&mut *conn)
        .await?;
        if !recorded {
            return Err(KataStoreError::MissingPrevious(name));
        }
    }

    let (prediction, instrument_version_id) = if get == Get::Hypothesis {
        let Some(prediction) = content.get("prediction").and_then(Value::as_str) else {
            return Err(KataStoreError::HypothesisMalformed("prediction"));
        };
        if prediction.trim().is_empty() {
            return Err(KataStoreError::HypothesisMalformed("prediction"));
        }
        // An eval kata names the version it pre-registers. A benchmark kata pinned its suite at
        // open (`instrument_kind = 'benchmark'`, `instrument_id` set): the hypothesis confirms
        // that instrument, so the content names no eval version.
        match content
            .get("instrument_version_id")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
        {
            Some(instrument) => (Some(prediction.to_owned()), Some(instrument)),
            None if instrument_kind.as_deref() == Some("benchmark") && instrument_id.is_some() => {
                // Main's `prediction` column CHECKs that a prediction names an eval version
                // (0067); a benchmark kata's prediction is its pre-registered payload on the
                // kata row and the Get's content, never that column.
                (None, None)
            }
            None => return Err(KataStoreError::HypothesisMalformed("instrument_version_id")),
        }
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

    let sql = format!(
        "INSERT INTO kata_measurement
             (tenant_id, kata_id, instrument_version_id, metric_key, candidate_run_id,
              baseline_run_id, candidate_cases, baseline_cases, candidate_score,
              baseline_score, delta, result)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING {MEASUREMENT_COLUMNS}"
    );
    let measurement = sqlx::query_as::<_, KataMeasurementRow>(sqlx::AssertSqlSafe(sql))
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

/// One benchmark run's identity and grader counts, as a kata measurement reads them.
pub struct BenchmarkRunScore {
    pub suite_id: Uuid,
    pub status: String,
    pub arm: String,
    pub n_graded: i32,
    pub n_resolved: i32,
}

/// Read one benchmark run's identity and counts in this tenant.
pub async fn benchmark_run_score_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<BenchmarkRunScore>, KataStoreError> {
    let row: Option<(Uuid, String, String, i32, i32)> = sqlx::query_as(
        "SELECT suite_id, status, arm, n_graded, n_resolved FROM benchmark_run
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(suite_id, status, arm, n_graded, n_resolved)| BenchmarkRunScore {
            suite_id,
            status,
            arm,
            n_graded,
            n_resolved,
        },
    ))
}

/// The fields a benchmark measurement writes (the union's second instrument shape).
pub struct NewBenchmarkMeasurement {
    pub kata_id: Uuid,
    /// The candidate's COMPLETE `benchmark_run` on the `conveyor` arm.
    pub candidate_run_id: Uuid,
    /// The baseline's COMPLETE `benchmark_run` on the `baseline` arm.
    pub baseline_run_id: Uuid,
    pub candidate_cases: i32,
    pub baseline_cases: i32,
    pub candidate_score: f64,
    pub baseline_score: f64,
    pub delta: f64,
    pub result: Value,
}

/// Write a benchmark measurement and move the kata to `measured`, atomically.
///
/// The held-out wall here is the suite's locked dataset, not an eval split: the kata pinned
/// `split_sha256 = benchmark_suite.seed_sha256` at open, and both runs must be COMPLETE runs of
/// that same suite — the candidate on the `conveyor` arm, the baseline on the `baseline` arm.
/// A control arm, another suite's run, or an unfinished run is refused (never silently scored).
pub async fn benchmark_measurement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: NewBenchmarkMeasurement,
) -> Result<(KataRow, KataMeasurementRow), KataStoreError> {
    let sql =
        format!("SELECT {KATA_COLUMNS} FROM kata WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let kata: Option<KataRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.kata_id)
        .fetch_optional(&mut *conn)
        .await?;
    let kata = kata.ok_or(KataStoreError::NotFound(new.kata_id))?;
    if kata.status != "proposed" {
        return Err(KataStoreError::GetOutOfOrder {
            current: kata.status,
            required: "proposed".to_owned(),
        });
    }
    if kata.instrument_kind.as_deref() != Some("benchmark") {
        return Err(KataStoreError::BenchmarkMismatch(
            "the kata did not pre-register a benchmark instrument".to_owned(),
        ));
    }
    let (Some(suite_id), Some(pinned_seed)) = (kata.instrument_id, kata.split_sha256.clone())
    else {
        return Err(KataStoreError::BenchmarkMismatch(
            "the kata carries no benchmark suite or locked seed".to_owned(),
        ));
    };
    if kata.split.as_deref() != Some("test") {
        return Err(KataStoreError::BenchmarkMismatch(
            "a benchmark measurement is the suite's held-out number; split must be test".to_owned(),
        ));
    }
    let suite: Option<(String,)> =
        sqlx::query_as("SELECT seed_sha256 FROM benchmark_suite WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(suite_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((seed,)) = suite else {
        return Err(KataStoreError::BenchmarkMismatch(
            "the pinned instrument is no benchmark suite in this tenant".to_owned(),
        ));
    };
    if seed != pinned_seed {
        return Err(KataStoreError::BenchmarkMismatch(format!(
            "the kata pinned seed {pinned_seed}, but the suite's locked seed is {seed}"
        )));
    }
    let mut scores = [0f64; 2];
    let mut cases = [0i32; 2];
    for (index, (run_id, arm)) in [
        (new.candidate_run_id, "conveyor"),
        (new.baseline_run_id, "baseline"),
    ]
    .into_iter()
    .enumerate()
    {
        let run = benchmark_run_score_tx(&mut *conn, tenant, run_id).await?;
        let Some(BenchmarkRunScore {
            suite_id: run_suite,
            status,
            arm: run_arm,
            n_graded,
            n_resolved,
        }) = run
        else {
            return Err(KataStoreError::BenchmarkRunRefused {
                run_id,
                detail: "no such benchmark run in this tenant".to_owned(),
            });
        };
        if run_suite != suite_id {
            return Err(KataStoreError::BenchmarkRunRefused {
                run_id,
                detail: "the run measured another suite".to_owned(),
            });
        }
        if status != "succeeded" {
            return Err(KataStoreError::BenchmarkRunRefused {
                run_id,
                detail: format!("the run is {status}; a measurement reads a complete run"),
            });
        }
        if run_arm != arm {
            return Err(KataStoreError::BenchmarkRunRefused {
                run_id,
                detail: format!(
                    "the run is the {run_arm:?} arm; this side of the measurement is the                      {arm:?} arm"
                ),
            });
        }
        if n_graded <= 0 {
            return Err(KataStoreError::BenchmarkRunRefused {
                run_id,
                detail: "the run graded no case".to_owned(),
            });
        }
        cases[index] = n_graded;
        scores[index] = f64::from(n_resolved) / f64::from(n_graded);
    }
    let sql = format!(
        "INSERT INTO kata_measurement
             (tenant_id, kata_id, candidate_cases, baseline_cases, candidate_score,
              baseline_score, delta, result, instrument_kind, instrument_id, instrument_version,
              split, split_sha256, metric, candidate_benchmark_run_id, baseline_benchmark_run_id,
              metric_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'benchmark', $9, $10, 'test', $11, $12, $13,
                 $14, $15)
         RETURNING {MEASUREMENT_COLUMNS}"
    );
    let measurement = sqlx::query_as::<_, KataMeasurementRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.kata_id)
        .bind(cases[0])
        .bind(cases[1])
        .bind(scores[0])
        .bind(scores[1])
        .bind(new.delta)
        .bind(&new.result)
        .bind(suite_id)
        .bind(kata.instrument_version.clone())
        .bind(&pinned_seed)
        .bind(serde_json::json!({"key": "resolved_rate", "better": "higher"}))
        .bind(new.candidate_run_id)
        .bind(new.baseline_run_id)
        .bind("resolved_rate")
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

/// The kata's append-only revert, when it has one (P2).
pub async fn revert_of_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Option<KataRevertRow>, KataStoreError> {
    let row = sqlx::query_as::<_, KataRevertRow>(
        "SELECT id, kata_id, reason, baseline_version_id, candidate_version_id, restored,
                evidence, reverted_by, created_at
           FROM kata_revert
          WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The kata's adoption decision, when a person has made one (P3).
pub async fn adoption_of_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Option<KataAdoptionRow>, KataStoreError> {
    let row = sqlx::query_as::<_, KataAdoptionRow>(
        "SELECT id, kata_id, note, adopted_by, created_at
           FROM kata_adoption
          WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The candidate reference recorded by the solution Get.
pub async fn candidate_ref_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<Option<String>, KataStoreError> {
    let content: Option<Value> = sqlx::query_scalar(
        "SELECT content FROM kata_get
          WHERE tenant_id = $1 AND kata_id = $2 AND get_key = 'solution'",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(content.and_then(|value| {
        value
            .get("candidate_ref")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }))
}

/// Put a procedure key back at its pre-kata version: retire whatever version is published now
/// (the candidate) and publish the baseline row the kata pinned at open.
///
/// Both rows are locked here, and the two must be versions of the same key with the baseline
/// first. The partial unique index (`one published per key`) is what the caller can rely on
/// after this returns.
pub async fn restore_procedure_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    baseline_id: Uuid,
    candidate_id: Uuid,
) -> Result<(), KataStoreError> {
    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT id, key, version FROM procedure
          WHERE tenant_id = $1 AND id = ANY($2)
          FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(vec![baseline_id, candidate_id])
    .fetch_all(&mut *conn)
    .await?;
    let baseline = rows.iter().find(|row| row.0 == baseline_id);
    let candidate = rows.iter().find(|row| row.0 == candidate_id);
    let (baseline, candidate) = match (baseline, candidate) {
        (Some(baseline), Some(candidate)) => (baseline, candidate),
        _ => {
            return Err(KataStoreError::RevertTarget(format!(
                "procedure versions {baseline_id} and {candidate_id} do not both exist in this \
                 tenant"
            )));
        }
    };
    if baseline.1 != candidate.1 {
        return Err(KataStoreError::RevertTarget(format!(
            "procedure versions {baseline_id} and {candidate_id} belong to different keys \
             ({:?} and {:?})",
            baseline.1, candidate.1
        )));
    }
    if baseline.2 >= candidate.2 {
        return Err(KataStoreError::RevertTarget(format!(
            "the pre-kata version {baseline_id} (v{}) is not older than the candidate {candidate_id} \
             (v{})",
            baseline.2, candidate.2
        )));
    }
    // Retire the candidate (or any other published row) before republishing the baseline: the
    // one-published-per-key index refuses the intermediate state otherwise.
    sqlx::query(
        "UPDATE procedure
            SET status = 'retired', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND status = 'published' AND id <> $3",
    )
    .bind(tenant.as_uuid())
    .bind(&baseline.1)
    .bind(baseline_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE procedure
            SET status = 'published', lock_version = lock_version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status <> 'published'",
    )
    .bind(tenant.as_uuid())
    .bind(baseline_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Write P2's revert and move the kata to `reverted`, atomically.
pub async fn revert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    new: NewRevert,
) -> Result<(KataRow, KataRevertRow), KataStoreError> {
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
    if current != "measured" {
        return Err(KataStoreError::GetOutOfOrder {
            current,
            required: "measured".to_owned(),
        });
    }
    let revert = sqlx::query_as::<_, KataRevertRow>(
        "INSERT INTO kata_revert
             (tenant_id, kata_id, reason, baseline_version_id, candidate_version_id, restored,
              evidence, reverted_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, kata_id, reason, baseline_version_id, candidate_version_id, restored,
                   evidence, reverted_by, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .bind(&new.reason)
    .bind(new.baseline_version_id)
    .bind(new.candidate_version_id)
    .bind(new.restored)
    .bind(&new.evidence)
    .bind(new.reverted_by)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE kata
            SET status = 'reverted', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {KATA_COLUMNS}"
    );
    let kata = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok((kata, revert))
}

/// Record a person's adoption and move the kata to `adopted`, atomically.
pub async fn adopt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    note: &str,
    adopted_by: Uuid,
) -> Result<(KataRow, KataAdoptionRow), KataStoreError> {
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
    if current != "measured" {
        return Err(KataStoreError::GetOutOfOrder {
            current,
            required: "measured".to_owned(),
        });
    }
    let adoption = sqlx::query_as::<_, KataAdoptionRow>(
        "INSERT INTO kata_adoption (tenant_id, kata_id, note, adopted_by)
         VALUES ($1, $2, $3, $4)
         RETURNING id, kata_id, note, adopted_by, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(kata_id)
    .bind(note)
    .bind(adopted_by)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE kata
            SET status = 'adopted', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {KATA_COLUMNS}"
    );
    let kata = sqlx::query_as::<_, KataRow>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok((kata, adoption))
}

// ── The lane's candidate / propagation extension (union, 0070) ──────────────────────────
//
// The union keeps main's loop as the runtime (six Gets, the held-out measurement, the
// adoption/revert evidence) and carries the lane's candidate and propagation records on the
// same kata aggregate. The dependents (coach, ops, architecture) hold `kata_candidate_id`s,
// so the candidate row is the record those columns name.

const CANDIDATE_COLUMNS: &str = "id, kata_id, ordinal, title, rationale, target_ref, prediction, \
    status, version, created_at, updated_at";

const PROPAGATION_COLUMNS: &str =
    "id, kata_id, target_ref, approval_id, status, version, created_at, updated_at";

/// One candidate change under a kata (the lane's record; the union's candidate model).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CandidateRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub ordinal: i32,
    pub title: String,
    pub rationale: String,
    pub target_ref: Value,
    pub prediction: Option<Value>,
    pub status: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One propagation target (the SPEC's propagation record; each target carries its own gate).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PropagationRow {
    pub id: Uuid,
    pub kata_id: Uuid,
    pub target_ref: Value,
    pub approval_id: Option<Uuid>,
    pub status: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The kata's current status, locked for a state move.
async fn lock_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
) -> Result<String, KataStoreError> {
    sqlx::query_scalar("SELECT status FROM kata WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(KataStoreError::NotFound(kata_id))
}

/// True when a prediction is a pre-registered hypothesis: a non-empty JSON object.
fn has_prediction(prediction: Option<&Value>) -> bool {
    prediction
        .and_then(Value::as_object)
        .is_some_and(|object| !object.is_empty())
}

/// Refuse an act on a terminal kata.
fn ensure_movable(status: &str) -> Result<(), KataStoreError> {
    if matches!(status, "closed" | "reverted") {
        return Err(KataStoreError::NotOpen {
            status: status.to_owned(),
        });
    }
    Ok(())
}

/// Insert one candidate (the lane's proposal) under a non-closed kata.
pub async fn insert_candidate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    title: &str,
    rationale: &str,
    target_ref: &Value,
    prediction: &Value,
) -> Result<CandidateRow, KataStoreError> {
    let status = lock_status_tx(&mut *conn, tenant, kata_id).await?;
    ensure_movable(&status)?;
    if !has_prediction(Some(prediction)) {
        return Err(KataStoreError::PredictionMissing);
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
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
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

/// Register one propagation target; it starts `pending` and carries its own later gate.
pub async fn propagate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    target_ref: &Value,
) -> Result<PropagationRow, KataStoreError> {
    let status = lock_status_tx(&mut *conn, tenant, kata_id).await?;
    ensure_movable(&status)?;
    let sql = format!(
        "INSERT INTO kata_propagation (tenant_id, kata_id, target_ref)
         VALUES ($1, $2, $3)
         RETURNING {PROPAGATION_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(target_ref)
        .fetch_one(&mut *conn)
        .await?)
}

/// Close a non-closed kata with its outcome; a `reverted` close must name the restore.
pub async fn close_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kata_id: Uuid,
    outcome: &str,
    reason: &str,
    restored_ref: Option<&Value>,
) -> Result<KataRow, KataStoreError> {
    let status = lock_status_tx(&mut *conn, tenant, kata_id).await?;
    ensure_movable(&status)?;
    let restored = if outcome == "reverted" {
        match restored_ref {
            Some(value) if !value.is_null() => Some(value),
            _ => return Err(KataStoreError::RevertMissing),
        }
    } else {
        None
    };
    let sql = format!(
        "UPDATE kata
            SET status = 'closed', outcome = $3, outcome_reason = $4, restored_ref = $5,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {KATA_COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kata_id)
        .bind(outcome)
        .bind(reason.trim())
        .bind(restored)
        .fetch_one(&mut *conn)
        .await?)
}

/// List the tenant's katas, newest `updated_at` first, optionally filtered by exact status.
pub async fn list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
) -> Result<Vec<KataRow>, KataStoreError> {
    let sql = format!(
        "SELECT {KATA_COLUMNS} FROM kata
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
          ORDER BY updated_at DESC, id DESC"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .fetch_all(&mut *conn)
        .await?)
}
