#![forbid(unsafe_code)]
//! `eval`, `eval_version`, the instrument digest, the one-shot split lock and the two sign-off
//! gates (S19 Part A step 3; SPEC D25/D26, CONTRACT §20).
//!
//! **The instrument is versioned and a version is frozen by copy.** `create_version_tx` locks the
//! eval row, mints the next `version` number, copies the previous version's case and metric rows
//! verbatim under the new version id, and recomputes the digest over what it copied — all in the
//! caller's transaction. `eval_version.digest` is therefore the value *as created*; an edit made
//! afterwards does not rewrite it (nothing would be comparing the same thing anymore), which is
//! exactly why readiness calls [`version_digest_tx`] and never trusts the stored column.
//!
//! **A sign-off is a digest match, not a status.** The two gating pauses are ordinary `approval`
//! rows carrying `eval_id`, with the digest in `meta.artifact_sha256`. [`eval_readiness_tx`] takes
//! the **latest** row per gate (a re-request reopens the gate while the older `approved` row still
//! exists) and reports it approved only when its status is `approved` **and** the digest still
//! equals the recomputed one. There is no `void` write: an edit voids a sign-off by making the two
//! hashes disagree.
//!
//! **The split lock is one-shot by construction.** [`lock_split_tx`] is a conditional
//! `UPDATE … WHERE split_sha256 IS NULL`; zero rows means either the version does not exist
//! ([`EvalStoreError::NotFound`]) or it is already locked ([`EvalStoreError::SplitLocked`]), and
//! the follow-up existence probe tells the two apart without a check-then-act race.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::eval::{
    CaseMaterial, CaseSplit, EvalGate, EvalStatus, Instrument, MetricMaterial, ReadinessFacts,
    SubjectKind, instrument_digest, split_sha256,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// The public projection of an `eval` row.
///
/// `status` and `subject_kind` are the domain enums, parsed on the way out: the database's CHECK
/// and [`conveyor_domain::eval`] own the same vocabulary, and a stored value the domain does not
/// know is a [`EvalStoreError::Vocabulary`], never a silently passed-through string.
#[derive(Clone, Debug, PartialEq)]
pub struct Eval {
    /// The row id.
    pub id: Uuid,
    /// The display name (non-empty).
    pub name: String,
    /// What the eval is for.
    pub purpose: String,
    /// What it measures.
    pub subject_kind: SubjectKind,
    /// The registered action name the run enters through.
    pub entry_action: String,
    /// The model the subject runs against, when it names one.
    pub model_ref: Option<String>,
    /// The prompt the subject runs, when it names one.
    pub prompt_ref: Option<String>,
    /// The JSON schema a case input must satisfy.
    pub inputs_schema: Value,
    /// Where traces come from (`synthesized | conversation_turn | run_step`).
    pub trace_source: String,
    /// The lifecycle status.
    pub status: EvalStatus,
    /// The version a run of this eval uses; `None` until the first version exists.
    pub current_version_id: Option<Uuid>,
    /// Who created the eval.
    pub created_by: Option<Uuid>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// An eval to create.
#[derive(Clone, Debug)]
pub struct NewEval<'a> {
    /// The display name (non-empty; the schema refuses `''`).
    pub name: &'a str,
    /// What the eval is for; `""` is legal.
    pub purpose: &'a str,
    /// One of `completions | copilot | process | external | agent_run`.
    pub subject_kind: &'a str,
    /// The registered action name the run enters through.
    pub entry_action: &'a str,
    /// The model reference, when the subject names one.
    pub model_ref: Option<&'a str>,
    /// The prompt reference, when the subject names one.
    pub prompt_ref: Option<&'a str>,
    /// The JSON schema a case input must satisfy; `{}` when there is none.
    pub inputs_schema: &'a Value,
    /// `synthesized | conversation_turn | run_step`.
    pub trace_source: &'a str,
    /// Who created the eval.
    pub created_by: Option<Uuid>,
}

/// The content fields of an eval an update may patch.
///
/// `subject_kind` and `entry_action` are deliberately absent: one eval is one flow, and changing
/// what it measures after a version exists would silently reinterpret every stored version. The
/// actions layer refuses that change explicitly; the store simply has no statement for it.
#[derive(Clone, Debug)]
pub struct UpdateEval<'a> {
    /// The display name (non-empty).
    pub name: &'a str,
    /// What the eval is for.
    pub purpose: &'a str,
    /// The model reference; `None` clears it.
    pub model_ref: Option<&'a str>,
    /// The prompt reference; `None` clears it.
    pub prompt_ref: Option<&'a str>,
    /// The JSON schema a case input must satisfy.
    pub inputs_schema: &'a Value,
    /// `synthesized | conversation_turn | run_step`.
    pub trace_source: &'a str,
}

/// The public projection of an `eval_version` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EvalVersion {
    /// The row id.
    pub id: Uuid,
    /// The eval this version belongs to.
    pub eval_id: Uuid,
    /// The version number inside the eval, from 1.
    pub version: i32,
    /// The instrument digest **as frozen when the version was created**.
    ///
    /// Readiness must not compare against this value: use [`version_digest_tx`], which recomputes
    /// the digest from the current rows.
    pub digest: String,
    /// The hash of the locked split assignment; `None` until a draw is locked.
    pub split_sha256: Option<String>,
    /// When the split was locked; `None` exactly when `split_sha256` is `None`.
    pub split_locked_at: Option<DateTime<Utc>>,
    /// The price table the cost estimate uses.
    pub prices: Value,
    /// The harness files the run needs, in order.
    pub harness_paths: Vec<String>,
    /// When the version was created.
    pub created_at: DateTime<Utc>,
}

/// One eval sign-off row, in the shape the readiness and decide paths need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalApproval {
    /// The `approval.id`.
    pub id: Uuid,
    /// The approval status (`pending`, `approved`, `rejected`, `expired`, `voided`).
    pub status: String,
    /// `approval.meta->>'artifact_sha256'`, the instrument digest the sign-off covers.
    pub artifact_sha256: Option<String>,
    /// The gate (`eval_inputs`, `eval_grading`, `eval_cost`, `eval_harness`).
    pub gate: String,
    /// The eval the gate belongs to; never `None` — non-eval gates are not readable here.
    pub eval_id: Uuid,
}

const EVAL_COLUMNS: &str = "id, name, purpose, subject_kind, entry_action, model_ref, prompt_ref, \
                            inputs_schema, trace_source, status, current_version_id, created_by, \
                            created_at, updated_at";

const VERSION_COLUMNS: &str = "id, eval_id, version, digest, split_sha256, split_locked_at, \
                               prices, harness_paths, created_at";

#[derive(Debug, sqlx::FromRow)]
struct EvalRow {
    id: Uuid,
    name: String,
    purpose: String,
    subject_kind: String,
    entry_action: String,
    model_ref: Option<String>,
    prompt_ref: Option<String>,
    inputs_schema: Value,
    trace_source: String,
    status: String,
    current_version_id: Option<Uuid>,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl EvalRow {
    fn into_eval(self) -> Result<Eval, EvalStoreError> {
        let status =
            EvalStatus::from_str(&self.status).map_err(|_| EvalStoreError::Vocabulary {
                what: "eval.status",
                value: self.status.clone(),
            })?;
        let subject_kind =
            SubjectKind::from_str(&self.subject_kind).map_err(|_| EvalStoreError::Vocabulary {
                what: "eval.subject_kind",
                value: self.subject_kind.clone(),
            })?;
        Ok(Eval {
            id: self.id,
            name: self.name,
            purpose: self.purpose,
            subject_kind,
            entry_action: self.entry_action,
            model_ref: self.model_ref,
            prompt_ref: self.prompt_ref,
            inputs_schema: self.inputs_schema,
            trace_source: self.trace_source,
            status,
            current_version_id: self.current_version_id,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// One case row as the instrument digest reads it — `expected` included.
#[derive(Debug, sqlx::FromRow)]
struct CaseDigestRow {
    ordinal: i32,
    input: String,
    tags: Vec<String>,
    expected: Option<Value>,
    attachments: Value,
    source: String,
    gold_provenance: String,
    gold_model: Option<String>,
    meta: Value,
}

/// One metric row as the instrument digest reads it.
#[derive(Debug, sqlx::FromRow)]
struct MetricDigestRow {
    key: String,
    role: String,
    kind: String,
    better: String,
    label: String,
    unit: Option<String>,
    grader_kind: String,
    grader: Value,
    rubric: Option<String>,
    ordinal: i32,
}

/// One `approval` row as an eval gate.
///
/// `eval_id` is decoded as a plain `Uuid` because every query filters `eval_id IS NOT NULL`;
/// a NULL reaching this type would be a decode error rather than a silently missing gate.
#[derive(Debug, sqlx::FromRow)]
struct EvalApprovalRow {
    id: Uuid,
    status: String,
    artifact_sha256: Option<String>,
    gate: String,
    eval_id: Uuid,
}

impl EvalApprovalRow {
    fn into_approval(self) -> EvalApproval {
        EvalApproval {
            id: self.id,
            status: self.status,
            artifact_sha256: self.artifact_sha256,
            gate: self.gate,
            eval_id: self.eval_id,
        }
    }
}

/// Insert an eval inside the caller's scoped transaction.
pub async fn insert_eval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewEval<'_>,
) -> Result<Eval, EvalStoreError> {
    let sql = format!(
        "INSERT INTO eval (tenant_id, name, purpose, subject_kind, entry_action, model_ref,
                           prompt_ref, inputs_schema, trace_source, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {EVAL_COLUMNS}"
    );
    let row: EvalRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.name)
        .bind(input.purpose)
        .bind(input.subject_kind)
        .bind(input.entry_action)
        .bind(input.model_ref)
        .bind(input.prompt_ref)
        .bind(input.inputs_schema)
        .bind(input.trace_source)
        .bind(input.created_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.into_eval()
}

/// One eval by id in this tenant; [`EvalStoreError::NotFound`] when it does not exist.
pub async fn get_eval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<Eval, EvalStoreError> {
    let sql = format!("SELECT {EVAL_COLUMNS} FROM eval WHERE tenant_id = $1 AND id = $2");
    let row: Option<EvalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound { what: "eval" })?
        .into_eval()
}

/// One keyset page of evals, newest first.
///
/// `cursor` is the decoded `(created_at, id)` of the previous page's last row; the page continues
/// strictly below it, so a row inserted between two calls never shifts the second page.
/// `status` is an optional exact filter on `eval.status`.
pub async fn list_evals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Eval>, EvalStoreError> {
    let sql = format!(
        "SELECT {EVAL_COLUMNS}
           FROM eval
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<EvalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(EvalRow::into_eval).collect()
}

/// Patch the content fields of an eval and stamp `updated_at`.
///
/// `subject_kind` and `entry_action` are not patchable ([`UpdateEval`]); there is no statement
/// here that could change what an eval measures.
pub async fn update_eval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    input: &UpdateEval<'_>,
) -> Result<Eval, EvalStoreError> {
    let sql = format!(
        "UPDATE eval
            SET name = $3, purpose = $4, model_ref = $5, prompt_ref = $6, inputs_schema = $7,
                trace_source = $8, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
        RETURNING {EVAL_COLUMNS}"
    );
    let row: Option<EvalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(input.name)
        .bind(input.purpose)
        .bind(input.model_ref)
        .bind(input.prompt_ref)
        .bind(input.inputs_schema)
        .bind(input.trace_source)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.ok_or(EvalStoreError::NotFound { what: "eval" })?
        .into_eval()
}

/// Move an eval through the domain's transition table.
///
/// The row is locked `FOR UPDATE`, the stored status is parsed into
/// [`conveyor_domain::eval::EvalStatus`] and the domain decides; an illegal edge is
/// [`EvalStoreError::Illegal`] and writes nothing. The `UPDATE` repeats the old status in its
/// `WHERE`, so a status that moved underneath still cannot be overwritten.
pub async fn transition_eval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    to: EvalStatus,
) -> Result<Eval, EvalStoreError> {
    let sql =
        format!("SELECT {EVAL_COLUMNS} FROM eval WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<EvalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .fetch_optional(&mut *conn)
        .await?;
    let current = row
        .ok_or(EvalStoreError::NotFound { what: "eval" })?
        .into_eval()?;
    let next = current.status.transition(to)?;

    let sql = format!(
        "UPDATE eval
            SET status = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = $4
        RETURNING {EVAL_COLUMNS}"
    );
    let row: Option<EvalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .bind(next.as_str())
        .bind(current.status.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound { what: "eval" })?
        .into_eval()
}

/// Mint the next version of an eval by copying the current one, and freeze its digest.
///
/// Everything happens inside the caller's transaction, with the eval row locked `FOR UPDATE`
/// first so two concurrent creates cannot mint the same number:
///
/// 1. lock the eval row and read `current_version_id`;
/// 2. `version = COALESCE(MAX(version), 0) + 1` for this eval;
/// 3. inherit the current version's `prices` and `harness_paths` (v1: `{}` and `[]`);
/// 4. insert the version row and copy its `eval_case` and `eval_metric` rows verbatim under the
///    new id — same ordinals, keys and values;
/// 5. recompute the instrument digest over the copied rows and store it, then point
///    `eval.current_version_id` at the new version.
///
/// The placeholder digest written in step 4 is never observable outside this transaction: the row
/// and its final digest commit together.
///
/// ⚠️ The split is copied verbatim with the cases, but the new version carries **no**
/// `split_sha256`: a fresh version is unsplit until someone draws and locks one. Readiness
/// therefore reports `split_missing` for it even though its cases carry a side.
pub async fn create_version_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    created_by: Option<Uuid>,
) -> Result<EvalVersion, EvalStoreError> {
    let current: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT current_version_id FROM eval WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let current_version_id = current.ok_or(EvalStoreError::NotFound { what: "eval" })?.0;

    let version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM eval_version
          WHERE tenant_id = $1 AND eval_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .fetch_one(&mut *conn)
    .await?;

    let (prices, harness_paths) = match current_version_id {
        Some(parent_id) => {
            sqlx::query_as::<_, (Value, Vec<String>)>(
                "SELECT prices, harness_paths FROM eval_version
                  WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(parent_id)
            .fetch_one(&mut *conn)
            .await?
        }
        None => (Value::Object(serde_json::Map::new()), Vec::new()),
    };

    let version_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO eval_version (id, tenant_id, eval_id, version, digest, prices,
                                   harness_paths, created_by)
         VALUES ($1, $2, $3, $4, '', $5, $6, $7)",
    )
    .bind(version_id)
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(version)
    .bind(&prices)
    .bind(&harness_paths)
    .bind(created_by)
    .execute(&mut *conn)
    .await
    .map_err(map_write_error)?;

    if let Some(parent_id) = current_version_id {
        sqlx::query(
            "INSERT INTO eval_case (tenant_id, version_id, ordinal, input, tags, expected,
                                    attachments, source, gold_provenance, gold_model, split, meta)
             SELECT tenant_id, $3, ordinal, input, tags, expected, attachments, source,
                    gold_provenance, gold_model, split, meta
               FROM eval_case
              WHERE tenant_id = $1 AND version_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(parent_id)
        .bind(version_id)
        .execute(&mut *conn)
        .await
        .map_err(map_write_error)?;

        sqlx::query(
            "INSERT INTO eval_metric (tenant_id, version_id, key, role, kind, better, label,
                                      unit, grader_kind, grader, rubric, ordinal)
             SELECT tenant_id, $3, key, role, kind, better, label, unit, grader_kind, grader,
                    rubric, ordinal
               FROM eval_metric
              WHERE tenant_id = $1 AND version_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(parent_id)
        .bind(version_id)
        .execute(&mut *conn)
        .await
        .map_err(map_write_error)?;
    }

    let digest = version_digest_tx(&mut *conn, tenant, version_id).await?;

    let sql = format!(
        "UPDATE eval_version SET digest = $3
          WHERE tenant_id = $1 AND id = $2
        RETURNING {VERSION_COLUMNS}"
    );
    let version_row: EvalVersion = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(version_id)
        .bind(&digest)
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query(
        "UPDATE eval SET current_version_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(version_id)
    .execute(&mut *conn)
    .await?;

    Ok(version_row)
}

/// One eval version by id in this tenant.
pub async fn get_version_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<EvalVersion, EvalStoreError> {
    let sql =
        format!("SELECT {VERSION_COLUMNS} FROM eval_version WHERE tenant_id = $1 AND id = $2");
    let row: Option<EvalVersion> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(version_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound {
        what: "eval version",
    })
}

/// Every version of an eval, newest number first.
pub async fn list_versions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
) -> Result<Vec<EvalVersion>, EvalStoreError> {
    let sql = format!(
        "SELECT {VERSION_COLUMNS} FROM eval_version
          WHERE tenant_id = $1 AND eval_id = $2
          ORDER BY version DESC"
    );
    let rows: Vec<EvalVersion> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(eval_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// The instrument digest of a version **as its rows stand right now**.
///
/// This is the value readiness compares a sign-off against, and deliberately not
/// [`EvalVersion::digest`]: the stored column is frozen at creation, so a case edited afterwards
/// would leave a stale digest that still matched an old approval. The digest covers the case
/// rows (ordered by ordinal), the metric rows (ordered by ordinal), `prices` and
/// `harness_paths` — see [`conveyor_domain::eval::instrument_digest`].
pub async fn version_digest_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<String, EvalStoreError> {
    let version: Option<(Value, Vec<String>)> = sqlx::query_as(
        "SELECT prices, harness_paths FROM eval_version WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (prices, harness_paths) = version.ok_or(EvalStoreError::NotFound {
        what: "eval version",
    })?;

    let cases: Vec<CaseDigestRow> = sqlx::query_as(
        "SELECT ordinal, input, tags, expected, attachments, source, gold_provenance,
                gold_model, meta
           FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2
          ORDER BY ordinal, id",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;

    let metrics: Vec<MetricDigestRow> = sqlx::query_as(
        "SELECT key, role, kind, better, label, unit, grader_kind, grader, rubric, ordinal
           FROM eval_metric
          WHERE tenant_id = $1 AND version_id = $2
          ORDER BY ordinal, id",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;

    let case_materials: Vec<CaseMaterial<'_>> = cases
        .iter()
        .map(|row| CaseMaterial {
            ordinal: row.ordinal,
            input: &row.input,
            tags: &row.tags,
            expected: row.expected.as_ref(),
            attachments: &row.attachments,
            source: &row.source,
            gold_provenance: &row.gold_provenance,
            gold_model: row.gold_model.as_deref(),
            meta: &row.meta,
        })
        .collect();
    let metric_materials: Vec<MetricMaterial<'_>> = metrics
        .iter()
        .map(|row| MetricMaterial {
            key: &row.key,
            role: &row.role,
            kind: &row.kind,
            better: &row.better,
            label: &row.label,
            unit: row.unit.as_deref(),
            grader_kind: &row.grader_kind,
            grader: &row.grader,
            rubric: row.rubric.as_deref(),
            ordinal: row.ordinal,
        })
        .collect();

    Ok(instrument_digest(&Instrument {
        cases: &case_materials,
        metrics: &metric_materials,
        prices: &prices,
        harness_paths: &harness_paths,
    }))
}

/// Lock a drawn split on a version and write every case's side, in the caller's transaction.
///
/// The lock is the `UPDATE … WHERE split_sha256 IS NULL` — not a read followed by a write — so two
/// concurrent draws cannot both win: the loser affects zero rows and is told
/// [`EvalStoreError::SplitLocked`] (or [`EvalStoreError::NotFound`] when the version is not in
/// this tenant). `assignments` is applied only after the lock is held.
///
/// `split_sha256` is the hash the domain's draw returned; [`eval_readiness_tx`] recomputes it from
/// the stored `(case_id, split)` rows, so a write that skipped this call would be detected as a
/// stale split rather than trusted.
pub async fn lock_split_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    split_sha256: &str,
    assignments: &[(Uuid, CaseSplit)],
) -> Result<EvalVersion, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_version
            SET split_sha256 = $3, split_locked_at = now()
          WHERE id = $1 AND tenant_id = $2 AND split_sha256 IS NULL
        RETURNING {VERSION_COLUMNS}"
    );
    let row: Option<EvalVersion> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(version_id)
        .bind(tenant.as_uuid())
        .bind(split_sha256)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(version) = row else {
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM eval_version WHERE id = $1 AND tenant_id = $2")
                .bind(version_id)
                .bind(tenant.as_uuid())
                .fetch_optional(&mut *conn)
                .await?;
        return Err(match exists {
            Some(_) => EvalStoreError::SplitLocked,
            None => EvalStoreError::NotFound {
                what: "eval version",
            },
        });
    };

    for (case_id, split) in assignments {
        sqlx::query(
            "UPDATE eval_case SET split = $3, updated_at = now()
              WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(case_id)
        .bind(split.as_str())
        .execute(&mut *conn)
        .await?;
    }

    Ok(version)
}

/// The `(case id, tags)` of every case in a version, ordered by ordinal — the split's whole input.
///
/// No score, no input, no answer key: the draw cannot see anything a baseline measured (CONTRACT
/// §20), and this is the read that makes that true.
pub async fn cases_for_split_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<Vec<(Uuid, Vec<String>)>, EvalStoreError> {
    let rows: Vec<(Uuid, Vec<String>)> = sqlx::query_as(
        "SELECT id, tags FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2
          ORDER BY ordinal, id",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// One eval gate by approval id.
///
/// A row that is absent — or present but not an eval gate (`eval_id IS NULL`, i.e. every
/// run-scoped and card-scoped approval) — is [`EvalStoreError::NotFound`] with
/// `what = "eval approval"`, so `eval_signoffs.decide` cannot be pointed at an unrelated gate.
pub async fn get_eval_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<EvalApproval, EvalStoreError> {
    let row: Option<EvalApprovalRow> = sqlx::query_as(
        "SELECT id, status, meta->>'artifact_sha256' AS artifact_sha256, gate, eval_id
           FROM approval
          WHERE tenant_id = $1 AND id = $2 AND eval_id IS NOT NULL",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.ok_or(EvalStoreError::NotFound {
        what: "eval approval",
    })
    .map(EvalApprovalRow::into_approval)
}

/// The latest approval row for one `(eval, gate)`, or `None` when none was ever requested.
///
/// Readiness takes the latest row, not "any approved row": a re-request reopens the gate while
/// the older `approved` row still exists, and returning the newer pending row is what makes that
/// reopen visible without a status write on the old row.
pub async fn latest_eval_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    gate: &str,
) -> Result<Option<EvalApproval>, EvalStoreError> {
    let row: Option<EvalApprovalRow> = sqlx::query_as(
        "SELECT id, status, meta->>'artifact_sha256' AS artifact_sha256, gate, eval_id
           FROM approval
          WHERE tenant_id = $1 AND eval_id = $2 AND gate = $3
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(eval_id)
    .bind(gate)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(EvalApprovalRow::into_approval))
}

/// The facts [`conveyor_domain::eval::Readiness::evaluate`] turns into the reasons a run is
/// refused (CONTRACT §20; the driver in Part B calls this before `eval_runs.start`).
///
/// - the digest is **recomputed** from the version's rows ([`version_digest_tx`]), never read from
///   `eval_version.digest`, so an edit since the sign-off voids it;
/// - `inputs_approved` / `grading_approved` take the latest approval per gate and require both
///   `status = 'approved'` and `meta->>'artifact_sha256' = <recomputed digest>`;
/// - `metric_count` and `unsplit_cases` are counted, not remembered (the schema keeps no
///   `case_count` on purpose);
/// - `split_matches` recomputes `split_sha256` over the stored `(case_id, split)` rows (ignoring
///   `none`) and compares it with the locked value, so the domain's own hash is the only
///   implementation of "the split is still the split".
///
/// The version must belong to `eval_id`; a version of another eval is
/// [`EvalStoreError::NotFound`], not a fact set assembled from two instruments.
pub async fn eval_readiness_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    version_id: Uuid,
) -> Result<ReadinessFacts, EvalStoreError> {
    let version: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT split_sha256 FROM eval_version
          WHERE tenant_id = $1 AND id = $2 AND eval_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .bind(eval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((stored_split,)) = version else {
        return Err(EvalStoreError::NotFound {
            what: "eval version",
        });
    };

    let digest = version_digest_tx(&mut *conn, tenant, version_id).await?;

    let inputs =
        latest_eval_approval_tx(&mut *conn, tenant, eval_id, EvalGate::Inputs.as_str()).await?;
    let grading =
        latest_eval_approval_tx(&mut *conn, tenant, eval_id, EvalGate::Grading.as_str()).await?;

    let metric_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_metric WHERE tenant_id = $1 AND version_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_one(&mut *conn)
    .await?;

    let unsplit_cases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2 AND split = 'none'",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_one(&mut *conn)
    .await?;

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, split FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2 AND split <> 'none'
          ORDER BY ordinal, id",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut assignments: Vec<(Uuid, CaseSplit)> = Vec::with_capacity(rows.len());
    for (case_id, stored) in rows {
        let split = CaseSplit::from_str(&stored).map_err(|_| EvalStoreError::Vocabulary {
            what: "eval_case.split",
            value: stored,
        })?;
        assignments.push((case_id, split));
    }
    let recomputed_split = split_sha256(&assignments);

    let approved = |row: Option<EvalApproval>| {
        row.is_some_and(|approval| {
            approval.status == "approved"
                && approval.artifact_sha256.as_deref() == Some(digest.as_str())
        })
    };

    Ok(ReadinessFacts {
        inputs_approved: approved(inputs),
        grading_approved: approved(grading),
        metric_count,
        split_locked: stored_split.is_some(),
        unsplit_cases,
        split_matches: stored_split.as_deref() == Some(recomputed_split.as_str()),
    })
}

/// Every eval id in this tenant — the set a process's `eval` inspection must pin one of.
///
/// Publish refuses a pin that names a missing eval (S13 §5.5). The check is a set membership, so
/// it reads ids only: the eval's metrics, versions and gates belong to `eval_runs.start`, not to
/// publishing the process that names it.
pub async fn all_ids_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<Uuid>, EvalStoreError> {
    let rows: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM eval WHERE tenant_id = $1 ORDER BY id")
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}
