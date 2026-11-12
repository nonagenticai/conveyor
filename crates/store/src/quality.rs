//! `quality_evaluation` — the append-only score history (S07, data-arch §2.7) — and the
//! `principle_conflict` verdicts that share its rubric read.
//!
//! Scoring is advisory: a row here never changes the scored asset, its status or its revision.
//! The table is polymorphic (`entity_type`/`entity_id`) because S08 scores models with the same
//! rubric; S07 only writes `entity_type = 'revision'`. There is no FK on the pair — the value
//! is a fact about an immutable revision and is never edited (the runtime role has no
//! UPDATE/DELETE on this table; see `scripts/grants-runtime.sql`).
//!
//! `principle_conflict` is the one sibling table this module owns. Its unique key
//! `(tenant_id, principle_a_id, principle_b_id)` is direction-sensitive in the schema, so
//! every write goes through [`canonical_pair`] (lower uuid first) and the pair is unique in
//! both directions. A verdict is not edited, it is dismissed once.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
/// The `numeric` score type, re-exported because it is part of this module's public API
/// ([`NewQualityEvaluation::score`]): a caller outside this crate must be able to name the
/// type without declaring a `rust_decimal` dependency of its own.
pub use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};

/// A score evaluation to append.
#[derive(Clone, Debug)]
pub struct NewQualityEvaluation {
    /// What the score is about; `revision` in S07.
    pub entity_type: String,
    /// The scored row's id.
    pub entity_id: Uuid,
    /// The computed score; `None` when the rubric ran degraded and only `criteria` is known.
    ///
    /// The column is `numeric`, so this is a [`Decimal`]: `domain::quality::rubric` returns
    /// an `f64`, and the caller converts it (`Decimal::try_from(score)`) rather than binding
    /// a float to a `numeric` column.
    pub score: Option<Decimal>,
    /// The rubric version plus each raw and weighted value, which makes the score auditable.
    pub criteria: Value,
    /// How the score was produced (`heuristic` by default).
    pub evaluator_kind: String,
}

impl NewQualityEvaluation {
    /// An evaluation with an empty criteria object and the default evaluator.
    pub fn new(entity_type: impl Into<String>, entity_id: Uuid) -> Self {
        Self {
            entity_type: entity_type.into(),
            entity_id,
            score: None,
            criteria: Value::Object(serde_json::Map::new()),
            evaluator_kind: "heuristic".to_owned(),
        }
    }
}

/// One `quality_evaluation` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct QualityEvaluationRecord {
    /// The row id.
    pub id: Uuid,
    /// What the score is about.
    pub entity_type: String,
    /// The scored row's id.
    pub entity_id: Uuid,
    /// The score, when the rubric produced one.
    pub score: Option<Decimal>,
    /// The rubric version and its raw/weighted values.
    pub criteria: Value,
    /// How the score was produced.
    pub evaluator_kind: String,
    /// When the score was recorded; the table is append-only, so there is no `updated_at`.
    pub created_at: DateTime<Utc>,
}

/// One `principle_conflict` verdict, with its pair already canonicalised.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PrincipleConflictRecord {
    /// The row id.
    pub id: Uuid,
    /// The lower uuid of the pair.
    pub principle_a_id: Uuid,
    /// The higher uuid of the pair.
    pub principle_b_id: Uuid,
    /// The verdict text.
    pub verdict: String,
    /// Who dismissed the verdict.
    pub dismissed_by: Option<Uuid>,
    /// When it was dismissed; `None` for a live verdict.
    pub dismissed_at: Option<DateTime<Utc>>,
    /// When the verdict was recorded.
    pub created_at: DateTime<Utc>,
    /// When it was last changed (the dismissal).
    pub updated_at: DateTime<Utc>,
}

/// Why a quality read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum QualityStoreError {
    /// A principle id that does not resolve inside the caller's tenant.
    #[error("principle {id} does not exist in this tenant")]
    PrincipleNotFound {
        /// The id that did not resolve.
        id: Uuid,
    },
    /// The same canonical pair already has a verdict.
    #[error("this principle pair already has a conflict verdict")]
    Conflict,
    /// A referenced `app_user` (`dismissed_by`) does not exist.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

/// Append one evaluation inside the caller's scoped transaction.
pub async fn insert_evaluation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewQualityEvaluation,
) -> Result<QualityEvaluationRecord, QualityStoreError> {
    let row: QualityEvaluationRecord = sqlx::query_as(
        "INSERT INTO quality_evaluation (tenant_id, entity_type, entity_id, score, criteria,
                                         evaluator_kind)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, entity_type, entity_id, score, criteria, evaluator_kind, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(&input.entity_type)
    .bind(input.entity_id)
    .bind(input.score)
    .bind(&input.criteria)
    .bind(&input.evaluator_kind)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// The evaluations of one entity, newest first. The table is append-only, so a re-score
/// appends a row and the whole history stays readable.
pub async fn list_evaluations_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entity_type: &str,
    entity_id: Uuid,
    limit: i64,
) -> Result<Vec<QualityEvaluationRecord>, QualityStoreError> {
    let rows: Vec<QualityEvaluationRecord> = sqlx::query_as(
        "SELECT id, entity_type, entity_id, score, criteria, evaluator_kind, created_at
           FROM quality_evaluation
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3
          ORDER BY created_at DESC, id DESC
          LIMIT $4",
    )
    .bind(tenant.as_uuid())
    .bind(entity_type)
    .bind(entity_id)
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The rubric's `uniqueness` raw value: the highest trigram similarity between `title` and
/// the other titles of the same kind in this tenant, excluding the asset being scored.
///
/// **It does not use `governed_asset_title_trgm_idx`.** A trigram index serves `%`, `ILIKE`
/// and `similarity()` only when the query carries one of those predicates per row; a bare
/// `max(similarity(...))` is a scan. That is accepted here because the rubric runs **once per
/// revision**, not once per request, and the scan is confined to one tenant's one kind. If
/// this ever becomes a hot path, add a `title % $3` predicate (which would then use the
/// index) rather than a new index.
pub async fn max_similarity_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    title: &str,
    exclude_id: Uuid,
) -> Result<f64, QualityStoreError> {
    let max: f64 = sqlx::query_scalar(
        "SELECT coalesce(max(similarity(title, $3)), 0)::float8
           FROM governed_asset
          WHERE tenant_id = $1 AND kind = $2 AND id <> $4",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(title)
    .bind(exclude_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(max)
}

// ── the scoring job (S07 §5.5) ──────────────────────────────────────────────────────────

/// The job kind that runs the rubric over one revision.
pub const QUALITY_SCORE_KIND: &str = "quality.score";

/// How many attempts one `quality.score` job gets before it is dead-lettered.
///
/// The rubric is pure and the row is immutable, so a deterministic failure is a bug, not a
/// transient condition; three attempts still ride out a database blip.
pub const QUALITY_SCORE_MAX_ATTEMPTS: i32 = 3;

/// The dedupe key that ties a revision to its scoring job.
///
/// One spelling, never a literal at a call site: the action enqueues on it and the worker's
/// `dedupe_key` read matches the same string.
pub fn quality_score_dedupe_key(revision_id: Uuid) -> String {
    format!("revision:{revision_id}")
}

/// Enqueue the `quality.score` job for one revision on the caller's transaction.
///
/// `quality.score` must commit the job with whatever transaction it was called in
/// (CONTRACT §7): a crash between an asset change and this insert would leave a revision
/// nobody scores, and a rolled-back action must leave no job behind. The statement is
/// `crate::jobs::enqueue`'s, on the caller's connection; `None` means a live job with the
/// same key already covers the revision, so the caller reports "queued" without a new row.
pub async fn enqueue_quality_score_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    revision_id: Uuid,
) -> Result<Option<Uuid>, QualityStoreError> {
    let payload = serde_json::json!({ "revision_id": revision_id });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(QUALITY_SCORE_KIND)
    .bind(quality_score_dedupe_key(revision_id))
    .bind(payload)
    .bind(QUALITY_SCORE_MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// The trace-link counts the rubric's traceability criterion reads, for one asset.
///
/// Returns `(total, of which reach a card)`: the edges the asset **emits**
/// (`from_type = 'governed_asset'`, `from_id = asset_id`), which is the direction S07 §5.7
/// demonstrates (requirement → card). The second number is what distinguishes "≥2 including
/// a card" from "≥2 alone", so both are counted in one statement — a second round trip could
/// see a different row set.
pub async fn count_for_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    asset_id: Uuid,
) -> Result<(i64, i64), QualityStoreError> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE to_type = 'card')::bigint
           FROM trace_link
          WHERE tenant_id = $1 AND from_type = 'governed_asset' AND from_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(asset_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

/// Order a pair so that the unique key is direction-insensitive: lower uuid first.
///
/// `(a, b)` and `(b, a)` are the same conflict, and the schema's
/// `uq (tenant_id, principle_a_id, principle_b_id)` alone would accept both. Every write in
/// this module canonicalises through this function; a caller may also use it to look a pair
/// up. The comparison is `Uuid`'s byte order, which is stable across processes.
pub fn canonical_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Resolve one principle inside the caller's tenant, in the caller's transaction.
async fn ensure_principle_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<(), QualityStoreError> {
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM governed_asset
                         WHERE tenant_id = $1 AND id = $2 AND kind = 'principle')",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if present {
        Ok(())
    } else {
        Err(QualityStoreError::PrincipleNotFound { id })
    }
}

const CONFLICT_COLUMNS: &str = "id, principle_a_id, principle_b_id, verdict, dismissed_by, \
                                dismissed_at, created_at, updated_at";

/// Record a verdict between two principles, in the caller's scoped transaction.
///
/// Both ids are resolved as `governed_asset` rows of `kind='principle'` **inside the tenant**
/// before the insert (the FKs alone would accept another tenant's id — CONTRACT §2), the pair
/// is canonicalised, and a repeated pair in either direction is
/// [`QualityStoreError::Conflict`].
pub async fn insert_principle_conflict_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    principle_a: Uuid,
    principle_b: Uuid,
    verdict: &str,
) -> Result<PrincipleConflictRecord, QualityStoreError> {
    ensure_principle_tx(conn, tenant, principle_a).await?;
    ensure_principle_tx(conn, tenant, principle_b).await?;
    let (a, b) = canonical_pair(principle_a, principle_b);
    let sql = format!(
        "INSERT INTO principle_conflict (tenant_id, principle_a_id, principle_b_id, verdict)
         VALUES ($1, $2, $3, $4)
         RETURNING {CONFLICT_COLUMNS}"
    );
    let row: PrincipleConflictRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(a)
        .bind(b)
        .bind(verdict)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                QualityStoreError::Conflict
            } else {
                QualityStoreError::Database(error)
            }
        })?;
    Ok(row)
}

/// Dismiss a live verdict. `None` means it is absent or already dismissed.
///
/// The verdict text is never edited; a dismissal stamps `dismissed_by`/`dismissed_at` once.
pub async fn dismiss_conflict_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    dismissed_by: Option<Uuid>,
) -> Result<Option<PrincipleConflictRecord>, QualityStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, dismissed_by).await?;
    let sql = format!(
        "UPDATE principle_conflict
            SET dismissed_by = $3, dismissed_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND dismissed_at IS NULL
          RETURNING {CONFLICT_COLUMNS}"
    );
    let row: Option<PrincipleConflictRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(dismissed_by)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// One page of verdicts, newest first, keyset on `(created_at, id)`.
///
/// `verdict` is free text (the schema fixes no vocabulary), so it is a plain equality filter.
/// Encode/decode `cursor` with [`crate::actions::encode_time_cursor`] /
/// [`crate::actions::decode_time_cursor`].
pub async fn list_conflicts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    verdict: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<PrincipleConflictRecord>, QualityStoreError> {
    let sql = format!(
        "SELECT {CONFLICT_COLUMNS}
           FROM principle_conflict
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR verdict = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<PrincipleConflictRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(verdict)
        .bind(cursor.map(|c| c.0))
        .bind(cursor.map(|c| c.1))
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_pair_is_direction_insensitive_and_lower_first() {
        let a = Uuid::parse_str("00000000-0000-7000-8000-000000000001").unwrap();
        let b = Uuid::parse_str("ffffffff-ffff-7fff-bfff-ffffffffffff").unwrap();
        assert_eq!(canonical_pair(a, b), (a, b));
        assert_eq!(canonical_pair(b, a), (a, b));
        assert_eq!(canonical_pair(a, a), (a, a));
    }

    #[test]
    fn the_scoring_dedupe_key_names_the_revision() {
        let id = Uuid::parse_str("00000000-0000-7000-8000-000000000001").unwrap();
        assert_eq!(
            quality_score_dedupe_key(id),
            "revision:00000000-0000-7000-8000-000000000001"
        );
        assert_eq!(QUALITY_SCORE_KIND, "quality.score");
    }
}
