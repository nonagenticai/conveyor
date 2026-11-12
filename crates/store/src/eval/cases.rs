#![forbid(unsafe_code)]
//! `eval_case` — the instrument's inputs, and the answer key that never leaves this module
//! (S19 Part A step 3; CONTRACT §20 trap 6).
//!
//! ⛔ **`expected` is write-only.** It is part of the instrument digest and it is what the grader
//! scores against; it is on no returned row, no view and no list page. The isolation is
//! structural rather than a filter someone must remember: [`EvalCase`] has no such field, and the
//! only read that returns it is [`get_case_expected_tx`], whose doc comment says out loud that it
//! is grader-only and must never be wired to a response.
//!
//! **Ordinals are allocated under a lock.** [`insert_case_tx`] takes the version row `FOR UPDATE`
//! before reading `COALESCE(MAX(ordinal) + 1, 0)`, so two concurrent appends to one version
//! produce 0 and 1 rather than both producing 0 and one of them failing on
//! `eval_case_ordinal_unique`. The unique constraint still exists — it is the backstop, not the
//! mechanism.
//!
//! `split` is deliberately not patchable: which side of the train/test draw a case sits on is
//! written by the draw's lock ([`super::evals::lock_split_tx`]) and by nothing else, so an edit
//! path cannot quietly move a case after the split hash was fixed.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// The public projection of an `eval_case` row.
///
/// ⛔ There is no `expected` field and there must never be one. A caller holding an [`EvalCase`]
/// has no answer key to leak, which is what makes the isolation a type rather than a rule.
#[derive(Clone, Debug, serde::Serialize, sqlx::FromRow)]
pub struct EvalCase {
    /// The row id.
    pub id: Uuid,
    /// The version this case belongs to.
    pub version_id: Uuid,
    /// The case's stable position inside the version, from 0.
    pub ordinal: i32,
    /// The input a subject is shown (non-empty).
    pub input: String,
    /// The case's tags; the first one is the split's stratum.
    pub tags: Vec<String>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: Value,
    /// `production | ticket | handwritten | synthesized`.
    pub source: String,
    /// How the answer key was produced (`human_written | human_verified | model`).
    pub gold_provenance: String,
    /// The model that produced a `model`-provenance key.
    pub gold_model: Option<String>,
    /// `train | test | none` — written only by the split lock.
    pub split: String,
    /// Free-form case metadata.
    pub meta: Value,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A case to append to a version.
#[derive(Clone, Debug)]
pub struct NewEvalCase<'a> {
    /// The version this case belongs to, resolved in the caller's tenant under a row lock.
    pub version_id: Uuid,
    /// The input a subject is shown (non-empty).
    pub input: &'a str,
    /// The case's tags; the first one is the split's stratum.
    pub tags: &'a [String],
    /// The answer key. Write-only: it never comes back out of the list read.
    pub expected: Option<&'a Value>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: &'a Value,
    /// `production | ticket | handwritten | synthesized`.
    pub source: &'a str,
    /// `human_written | human_verified | model`.
    pub gold_provenance: &'a str,
    /// The model that produced a `model`-provenance key.
    pub gold_model: Option<&'a str>,
    /// Free-form case metadata.
    pub meta: &'a Value,
}

/// The content fields of a case an update may patch.
///
/// `version_id`, `ordinal` and `split` are deliberately absent: a case never moves between
/// versions, never gets to choose its position after the fact, and never changes side except
/// through the split lock.
#[derive(Clone, Debug)]
pub struct UpdateEvalCase<'a> {
    /// The input a subject is shown (non-empty).
    pub input: &'a str,
    /// The case's tags; the first one is the split's stratum.
    pub tags: &'a [String],
    /// The answer key; `None` clears it.
    pub expected: Option<&'a Value>,
    /// Structured attachments (files, context) the case needs.
    pub attachments: &'a Value,
    /// `production | ticket | handwritten | synthesized`.
    pub source: &'a str,
    /// `human_written | human_verified | model`.
    pub gold_provenance: &'a str,
    /// The model that produced a `model`-provenance key.
    pub gold_model: Option<&'a str>,
    /// Free-form case metadata.
    pub meta: &'a Value,
}

const CASE_COLUMNS: &str = "id, version_id, ordinal, input, tags, attachments, source, \
                            gold_provenance, gold_model, split, meta, created_at, updated_at";

/// Append a case to a version, allocating its ordinal under a lock on the version row.
///
/// The version is resolved in the caller's tenant first (`FOR UPDATE`): a version id from another
/// tenant is [`EvalStoreError::NotFound`], never a row accepted by the foreign key (referential
/// integrity bypasses RLS, CONTRACT §2). The lock is also what serialises the ordinal allocation.
pub async fn insert_case_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewEvalCase<'_>,
) -> Result<EvalCase, EvalStoreError> {
    let version: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_version WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(input.version_id)
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;
    if version.is_none() {
        return Err(EvalStoreError::NotFound {
            what: "eval version",
        });
    }

    let ordinal: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(ordinal) + 1, 0) FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(input.version_id)
    .fetch_one(&mut *conn)
    .await?;

    let sql = format!(
        "INSERT INTO eval_case (tenant_id, version_id, ordinal, input, tags, expected,
                                attachments, source, gold_provenance, gold_model, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING {CASE_COLUMNS}"
    );
    let row: EvalCase = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.version_id)
        .bind(ordinal)
        .bind(input.input)
        .bind(input.tags)
        .bind(input.expected)
        .bind(input.attachments)
        .bind(input.source)
        .bind(input.gold_provenance)
        .bind(input.gold_model)
        .bind(input.meta)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}

/// The answer key of one case — **grader-only**.
///
/// ⛔ This is the one read that returns `expected`, and it must never be wired to a view, a list
/// page, an MCP tool or any response body. It exists for the grader (Part B) and for nothing
/// else; everything a caller can see goes through [`list_cases_tx`], whose row type has no such
/// field. `None` means the case carries no key (a subjective case graded against a rubric).
pub async fn get_case_expected_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
) -> Result<Option<Value>, EvalStoreError> {
    let expected: Option<Option<Value>> =
        sqlx::query_scalar("SELECT expected FROM eval_case WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(case_id)
            .fetch_optional(&mut *conn)
            .await?;
    expected.ok_or(EvalStoreError::NotFound { what: "eval case" })
}

/// One case by id, **without** `expected` — what the driver reads before it calls a subject.
///
/// The driver needs the input and the split before the call and the answer key only after it;
/// this is the before-the-call read, and [`get_case_expected_tx`] is the after-the-call one.
/// Keeping them separate is the isolation rule in the shape of the API rather than in a
/// convention: the type this returns *cannot* carry a key.
pub async fn get_case_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
) -> Result<EvalCase, EvalStoreError> {
    let sql = format!("SELECT {CASE_COLUMNS} FROM eval_case WHERE tenant_id = $1 AND id = $2");
    let row: Option<EvalCase> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(case_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(EvalStoreError::NotFound { what: "eval case" })
}

/// One keyset page of a version's cases, in ordinal order — **never** including `expected`.
///
/// `cursor` is the decoded `(ordinal, id)` of the previous page's last row; pages continue
/// strictly above it, so the ordinal order is stable while cases are appended.
pub async fn list_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    version_id: Uuid,
    cursor: Option<(i32, Uuid)>,
    limit: i64,
) -> Result<Vec<EvalCase>, EvalStoreError> {
    let sql = format!(
        "SELECT {CASE_COLUMNS}
           FROM eval_case
          WHERE tenant_id = $1 AND version_id = $2
            AND ($3::int IS NULL OR (ordinal, id) > ($3, $4))
          ORDER BY ordinal, id
          LIMIT $5"
    );
    let rows: Vec<EvalCase> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(version_id)
        .bind(cursor.map(|cursor| cursor.0))
        .bind(cursor.map(|cursor| cursor.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Patch a case's content and stamp `updated_at`.
///
/// `split` and `ordinal` are not patchable ([`UpdateEvalCase`]). Editing a content field changes
/// the instrument digest, which voids both sign-off gates with no status write — that is the
/// point of the digest, not a side effect.
pub async fn update_case_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
    input: &UpdateEvalCase<'_>,
) -> Result<EvalCase, EvalStoreError> {
    let sql = format!(
        "UPDATE eval_case
            SET input = $3, tags = $4, expected = $5, attachments = $6, source = $7,
                gold_provenance = $8, gold_model = $9, meta = $10, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
        RETURNING {CASE_COLUMNS}"
    );
    let row: Option<EvalCase> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(case_id)
        .bind(input.input)
        .bind(input.tags)
        .bind(input.expected)
        .bind(input.attachments)
        .bind(input.source)
        .bind(input.gold_provenance)
        .bind(input.gold_model)
        .bind(input.meta)
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.ok_or(EvalStoreError::NotFound { what: "eval case" })
}

/// Delete a case.
///
/// A deletion changes the instrument digest exactly as an edit does, so the two sign-off gates
/// reopen without a status write. There is no tombstone: a case is authored data, not a fact.
pub async fn delete_case_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
) -> Result<(), EvalStoreError> {
    let deleted = sqlx::query("DELETE FROM eval_case WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(case_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(EvalStoreError::NotFound { what: "eval case" });
    }
    Ok(())
}
