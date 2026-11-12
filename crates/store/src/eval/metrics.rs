#![forbid(unsafe_code)]
//! `eval_metric` — the fields a run is graded and measured on (S19 Part A step 3; CONTRACT §20).
//!
//! A metric is either **quality** (graded by a program, a rubric, a judge or a human) or **perf**
//! (measured, never graded); the schema's `eval_metric_perf_ungraded` CHECK is the guarantee, and
//! `eval_metric_rubric_required` is what stops a rubric grader that has no rubric. This module
//! does not re-implement either: it surfaces both as [`EvalStoreError::Invalid`] carrying the
//! constraint name, so a caller can tell a refused label from a refused rubric.
//!
//! The metric rows are part of the instrument digest (`version_digest_tx`), which is why every
//! write here reopens a sign-off without touching one. The unique `(tenant, version, key)` index
//! is reported as [`EvalStoreError::Conflict`] — the `key` is the report's column identity, so two
//! metrics with one key is a writer bug, not something to merge.
//!
//! `version_id` is not patchable: a metric never moves between versions (that is what minting a
//! new version is for).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// The public projection of an `eval_metric` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EvalMetric {
    /// The row id.
    pub id: Uuid,
    /// The version this metric belongs to.
    pub version_id: Uuid,
    /// The report's column key (`^[a-z][a-z0-9_]*$`).
    pub key: String,
    /// `quality | perf`.
    pub role: String,
    /// `binary | judge | float`.
    pub kind: String,
    /// `higher | lower`.
    pub better: String,
    /// The legend text, at most 14 characters.
    pub label: String,
    /// The unit shown next to the value, when it has one.
    pub unit: Option<String>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: String,
    /// The grader's configuration object.
    pub grader: Value,
    /// The rubric text, required by `rubric` and `pairwise` graders.
    pub rubric: Option<String>,
    /// The metric's stable position in the version.
    pub ordinal: i32,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

/// A metric to add to a version.
#[derive(Clone, Debug)]
pub struct NewEvalMetric<'a> {
    /// The version this metric belongs to, resolved in the caller's tenant.
    pub version_id: Uuid,
    /// The report's column key (`^[a-z][a-z0-9_]*$`).
    pub key: &'a str,
    /// `quality | perf`.
    pub role: &'a str,
    /// `binary | judge | float`.
    pub kind: &'a str,
    /// `higher | lower`.
    pub better: &'a str,
    /// The legend text, at most 14 characters.
    pub label: &'a str,
    /// The unit shown next to the value, when it has one.
    pub unit: Option<&'a str>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: &'a str,
    /// The grader's configuration object.
    pub grader: &'a Value,
    /// The rubric text; required by `rubric` and `pairwise` graders.
    pub rubric: Option<&'a str>,
    /// The metric's stable position in the version.
    pub ordinal: i32,
}

/// The content fields of a metric an update may patch.
///
/// `version_id` is absent: a metric belongs to the version that defined it, and a new version is
/// minted by copying the old one, never by moving rows between them.
#[derive(Clone, Debug)]
pub struct UpdateEvalMetric<'a> {
    /// The report's column key (`^[a-z][a-z0-9_]*$`).
    pub key: &'a str,
    /// `quality | perf`.
    pub role: &'a str,
    /// `binary | judge | float`.
    pub kind: &'a str,
    /// `higher | lower`.
    pub better: &'a str,
    /// The legend text, at most 14 characters.
    pub label: &'a str,
    /// The unit shown next to the value, when it has one.
    pub unit: Option<&'a str>,
    /// `programmatic | pairwise | rubric | human | none`.
    pub grader_kind: &'a str,
    /// The grader's configuration object.
    pub grader: &'a Value,
    /// The rubric text; required by `rubric` and `pairwise` graders.
    pub rubric: Option<&'a str>,
    /// The metric's stable position in the version.
    pub ordinal: i32,
}

const METRIC_COLUMNS: &str = "id, version_id, key, role, kind, better, label, unit, grader_kind, \
                              grader, rubric, ordinal, created_at";

/// Add a metric to a version.
///
/// The version is resolved in the caller's tenant first (referential integrity bypasses RLS, so a
/// foreign `version_id` would otherwise be accepted — CONTRACT §2). A duplicate `key` is
/// [`EvalStoreError::Conflict`] naming `eval_metric_key_unique`; a violated CHECK is
/// [`EvalStoreError::Invalid`] naming the constraint.
pub async fn insert_metric_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewEvalMetric<'_>,
) -> Result<EvalMetric, EvalStoreError> {
    let version: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM eval_version WHERE id = $1 AND tenant_id = $2")
            .bind(input.version_id)
            .bind(tenant.as_uuid())
            .fetch_optional(&mut *conn)
            .await?;
    if version.is_none() {
        return Err(EvalStoreError::NotFound {
            what: "eval version",
        });
    }

    let sql = format!(
        "INSERT INTO eval_metric (tenant_id, version_id, key, role, kind, better, label, unit,
                                  grader_kind, grader, rubric, ordinal)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING {METRIC_COLUMNS}"
    );
    let row: EvalMetric = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.version_id)
        .bind(input.key)
        .bind(input.role)
        .bind(input.kind)
        .bind(input.better)
        .bind(input.label)
        .bind(input.unit)
        .bind(input.grader_kind)
        .bind(input.grader)
        .bind(input.rubric)
        .bind(input.ordinal)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}

/// Patch a metric's content.
///
/// Editing any field changes the instrument digest and therefore voids the two sign-off gates;
/// that is the digest's job, not a status write here.
pub async fn update_metric_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    metric_id: Uuid,
    input: &UpdateEvalMetric<'_>,
) -> Result<EvalMetric, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_metric
            SET key = $3, role = $4, kind = $5, better = $6, label = $7, unit = $8,
                grader_kind = $9, grader = $10, rubric = $11, ordinal = $12
          WHERE tenant_id = $1 AND id = $2
        RETURNING {METRIC_COLUMNS}"
    );
    let row: Option<EvalMetric> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(metric_id)
        .bind(input.key)
        .bind(input.role)
        .bind(input.kind)
        .bind(input.better)
        .bind(input.label)
        .bind(input.unit)
        .bind(input.grader_kind)
        .bind(input.grader)
        .bind(input.rubric)
        .bind(input.ordinal)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.ok_or(EvalStoreError::NotFound {
        what: "eval metric",
    })
}

/// Delete a metric.
pub async fn delete_metric_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    metric_id: Uuid,
) -> Result<(), EvalStoreError> {
    let deleted = sqlx::query("DELETE FROM eval_metric WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(metric_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(EvalStoreError::NotFound {
            what: "eval metric",
        });
    }
    Ok(())
}

/// Every metric of a version, in ordinal order.
pub async fn list_metrics_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<Vec<EvalMetric>, EvalStoreError> {
    let sql = format!(
        "SELECT {METRIC_COLUMNS} FROM eval_metric
          WHERE tenant_id = $1 AND version_id = $2
          ORDER BY ordinal, id"
    );
    let rows: Vec<EvalMetric> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(version_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}
