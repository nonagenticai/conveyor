#![forbid(unsafe_code)]
//! T8 — organisational memory: the lesson lifecycle (SPEC_V2 §7.6, §10.3, §10.4, §12 T8, D36;
//! issue #79).
//!
//! T10's 0056 owns the `lesson` row itself (a retro's proposal). This module owns the lifecycle
//! that turns a proposal into doctrine, and three invariants are implemented here rather than
//! left to the action layer:
//!
//! 1. **Independence is resolved, never claimed.** [`resolve_evidence_tx`] resolves every ref to
//!    a live row of its own kind in the caller's tenant; the independent count is the number of
//!    distinct `kind:id` anchors, so two refs to one card (or one run) are one reference.
//! 2. **A single reference is refused by the database too.** `0069_lesson_lifecycle.sql` carries
//!    `CHECK (independent_count <> 1)`. [`insert_tx`] stores the count the action measured and
//!    never invents one; a direct INSERT with one ref fails at the table.
//! 3. **Promotion creates a draft, never a live pack.** [`promote_tx`] inserts a
//!    `governed_asset(kind = 'lesson_source', status = 'draft')` and records its id on the
//!    lesson. It writes no `capability_knowledge` pin, no `inference_call` and no embedding —
//!    the conveyor hosts neither (§7.6, last rule).
//!
//! A rejection is durable: [`find_by_fingerprint_tx`] gives the action the rejected row, and
//! only a new independent anchor reopens it ([`reopen_rejected_tx`]).
//!
//! Every function is `_tx`: the caller has opened the tenant-scoped transaction (CONTRACT §7).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

/// The reference kinds this build can verify. `incident` arrives with T7's incident table; a
/// reference this build cannot resolve is refused rather than stored unverified.
pub const EVIDENCE_KINDS: [&str; 2] = ["card", "run"];

/// The tenant column every row here carries.
const COLUMNS: &str = "id, tenant_id, programme_id, cadence_id, finding_key, statement, \
                       evidence, independent_count, state, source, meta, proposer_id, \
                       proposer_kind, reviewed_by, reviewed_at, review_note, promoted_asset_id, \
                       promoted_at, fingerprint, version, created_at, updated_at";

/// One evidence reference as the caller named it.
#[derive(Clone, Debug)]
pub struct EvidenceRef {
    /// `card` or `run`.
    pub kind: String,
    /// The referenced row.
    pub id: Uuid,
}

/// A verified evidence reference and the identity the independence count uses.
#[derive(Clone, Debug)]
pub struct ResolvedEvidence {
    /// The resolved kind.
    pub kind: String,
    /// The resolved row id.
    pub id: Uuid,
    /// `kind:id` — the distinct identity, never a payload-supplied claim.
    pub anchor: String,
}

/// One `lesson` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LessonRow {
    /// Row id.
    pub id: Uuid,
    /// The tenant.
    pub tenant_id: Uuid,
    /// The programme the lesson belongs to, when it came from a programme cadence.
    pub programme_id: Option<Uuid>,
    /// The cadence the retro was held under, when any.
    pub cadence_id: Option<Uuid>,
    /// The retro finding key, when any.
    pub finding_key: String,
    /// The lesson's statement.
    pub statement: String,
    /// The evidence refs, as stored.
    pub evidence: Value,
    /// Distinct independent refs. The DB refuses exactly 1.
    pub independent_count: i32,
    /// `proposed | reviewed | promoted | rejected`.
    pub state: String,
    /// `retro | propose | execution`.
    pub source: String,
    /// Free-form metadata.
    pub meta: Value,
    /// Who proposed it, when a principal is known.
    pub proposer_id: Option<Uuid>,
    /// `user | agent | service` of the proposer.
    pub proposer_kind: Option<String>,
    /// The person who reviewed it.
    pub reviewed_by: Option<Uuid>,
    /// When the review decision was recorded.
    pub reviewed_at: Option<DateTime<Utc>>,
    /// The reviewer's note.
    pub review_note: Option<String>,
    /// The governed-asset source draft promotion produced.
    pub promoted_asset_id: Option<Uuid>,
    /// When promotion happened.
    pub promoted_at: Option<DateTime<Utc>>,
    /// The statement hash that identifies a re-proposal.
    pub fingerprint: Option<String>,
    /// The row version.
    pub version: i64,
    /// When it was first recorded.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// The fields a T8 proposal writes.
pub struct NewLesson {
    /// Optional programme context.
    pub programme_id: Option<Uuid>,
    /// Optional cadence context.
    pub cadence_id: Option<Uuid>,
    /// Optional retro finding key.
    pub finding_key: String,
    /// The statement.
    pub statement: String,
    /// The verified refs.
    pub evidence: Vec<ResolvedEvidence>,
    /// The distinct count the action measured.
    pub independent_count: i32,
    /// `propose`.
    pub source: String,
    /// The statement hash.
    pub fingerprint: Option<String>,
    /// The proposer.
    pub proposer_id: Option<Uuid>,
    /// The proposer's principal kind.
    pub proposer_kind: Option<String>,
    /// Free-form metadata.
    pub meta: Value,
}

/// The provenance a failed-run lesson records. The run is the origin, not yet an independent
/// evidence reference — a bad run alone never reaches doctrine.
pub struct NewExecutionLesson {
    /// The failed run.
    pub run_id: Uuid,
    /// Its card, when it has one.
    pub card_id: Option<Uuid>,
    /// Optional programme context.
    pub programme_id: Option<Uuid>,
    /// The lesson's statement.
    pub statement: String,
    /// The principal that recorded it.
    pub recorded_by: Option<Uuid>,
    /// That principal's kind.
    pub proposer_kind: Option<String>,
    /// Free-form metadata.
    pub meta: Value,
}

/// What the lesson store refuses.
#[derive(Debug, thiserror::Error)]
pub enum LessonStoreError {
    /// No such lesson in this tenant.
    #[error("lesson {0} does not exist in this tenant")]
    NotFound(Uuid),
    /// A reference did not resolve in this tenant.
    #[error("the {kind} evidence reference {id} does not exist in this tenant")]
    EvidenceUnknown {
        /// The claimed kind.
        kind: String,
        /// The claimed id.
        id: Uuid,
    },
    /// A reference kind this build cannot verify.
    #[error("evidence kind {0:?} cannot be verified by this build; use card or run")]
    EvidenceKindUnsupported(String),
    /// Fewer than two independent refs.
    #[error("a lesson needs at least two independent evidence references; counted {0}")]
    EvidenceInsufficient(i32),
    /// The same statement is already proposed/reviewed/promoted.
    #[error("lesson {id} already holds this statement in state {state}")]
    FingerprintTaken {
        /// The existing row.
        id: Uuid,
        /// Its state.
        state: String,
    },
    /// The same statement was rejected and carries no new independent ref.
    #[error("lesson {0} was rejected; only a new independent evidence reference reopens it")]
    RejectedWithoutNewEvidence(Uuid),
    /// The lesson is not in a state the operation accepts.
    #[error("lesson {id} is {state}, not {expected}")]
    IllegalState {
        /// The row.
        id: Uuid,
        /// Its current state.
        state: String,
        /// What the operation required.
        expected: String,
    },
    /// The proposal names a programme that is not in this tenant.
    #[error("programme {0} does not exist in this tenant")]
    ProgrammeMissing(Uuid),
    /// The proposal names a cadence that is not in this tenant.
    #[error("cadence {0} does not exist in this tenant")]
    CadenceMissing(Uuid),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The distinct independent anchors in a stored evidence array.
///
/// `{"kind":"card","id":"…"}` (T8's shape), `{"card_id":"…"}` and `{"run_id":"…"}` (T10's retro
/// shape) all resolve; an unrecognised item keeps its own positional identity so a legacy retro
/// row still counts its refs.
pub fn independent_anchors(evidence: &Value) -> Vec<String> {
    let mut anchors: Vec<String> = Vec::new();
    let items = evidence.as_array().map(Vec::as_slice).unwrap_or_default();
    for (index, item) in items.iter().enumerate() {
        let anchor = item
            .as_object()
            .and_then(|object| {
                if let (Some(kind), Some(id)) = (
                    object.get("kind").and_then(Value::as_str),
                    object.get("id").and_then(Value::as_str),
                ) {
                    return Some(format!("{}:{}", kind.trim().to_ascii_lowercase(), id));
                }
                for key in ["card_id", "run_id", "incident_id"] {
                    if let Some(value) = object.get(key) {
                        let rendered = value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string());
                        return Some(format!("{}:{rendered}", key.trim_end_matches("_id")));
                    }
                }
                None
            })
            .unwrap_or_else(|| format!("raw:{index}"));
        if !anchors.contains(&anchor) {
            anchors.push(anchor);
        }
    }
    anchors
}

/// The independent count of a stored evidence array, capped at `i32::MAX`.
pub fn independent_count(evidence: &Value) -> i32 {
    i32::try_from(independent_anchors(evidence).len()).unwrap_or(i32::MAX)
}

/// The evidence array a resolved proposal stores: one object per distinct ref.
pub fn evidence_json(evidence: &[ResolvedEvidence]) -> Value {
    Value::Array(
        evidence
            .iter()
            .map(|reference| json!({"kind": reference.kind, "id": reference.id}))
            .collect(),
    )
}

/// Resolve every claimed ref to a live row of its own kind in this tenant.
///
/// A claimed `kind` is never trusted as the identity: the query is per kind, and a missing row
/// refuses the whole proposal. Duplicate `(kind, id)` pairs collapse to one.
pub async fn resolve_evidence_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    refs: &[EvidenceRef],
) -> Result<Vec<ResolvedEvidence>, LessonStoreError> {
    let mut resolved: Vec<ResolvedEvidence> = Vec::new();
    for reference in refs {
        let kind = reference.kind.trim().to_ascii_lowercase();
        let found: Option<Uuid> = match kind.as_str() {
            "card" => {
                sqlx::query_scalar("SELECT id FROM card WHERE tenant_id = $1 AND id = $2")
                    .bind(tenant.as_uuid())
                    .bind(reference.id)
                    .fetch_optional(&mut *conn)
                    .await?
            }
            "run" => {
                sqlx::query_scalar("SELECT id FROM run WHERE tenant_id = $1 AND id = $2")
                    .bind(tenant.as_uuid())
                    .bind(reference.id)
                    .fetch_optional(&mut *conn)
                    .await?
            }
            other => return Err(LessonStoreError::EvidenceKindUnsupported(other.to_owned())),
        };
        let Some(id) = found else {
            return Err(LessonStoreError::EvidenceUnknown {
                kind,
                id: reference.id,
            });
        };
        let anchor = format!("{kind}:{id}");
        if resolved.iter().any(|existing| existing.anchor == anchor) {
            continue;
        }
        resolved.push(ResolvedEvidence { kind, id, anchor });
    }
    Ok(resolved)
}

/// Whether a programme resolves in this tenant.
pub async fn programme_exists_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, LessonStoreError> {
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM programme WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(found.is_some())
}

/// Whether a cadence resolves in this tenant.
pub async fn cadence_exists_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, LessonStoreError> {
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM programme_cadence WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(found.is_some())
}

/// One lesson in this tenant.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<LessonRow, LessonStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM lesson WHERE tenant_id = $1 AND id = $2");
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(LessonStoreError::NotFound(id))
}

/// The lesson carrying this statement hash, when one exists.
pub async fn find_by_fingerprint_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    fingerprint: &str,
) -> Result<Option<LessonRow>, LessonStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM lesson \
          WHERE tenant_id = $1 AND fingerprint = $2 AND source IN ('propose', 'retro')"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(fingerprint)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Insert a proposal. The table's `independent_count <> 1` CHECK is the backstop: a caller that
/// tries to store a single-ref lesson gets a database error, never a row.
pub async fn insert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: NewLesson,
) -> Result<LessonRow, LessonStoreError> {
    let evidence = evidence_json(&new.evidence);
    let sql = format!(
        "INSERT INTO lesson \
             (tenant_id, programme_id, cadence_id, finding_key, statement, evidence, \
              independent_count, state, source, meta, fingerprint, proposer_id, proposer_kind) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'proposed', $8, $9, $10, $11, $12) \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(new.cadence_id)
        .bind(new.finding_key.trim())
        .bind(new.statement.trim())
        .bind(&evidence)
        .bind(new.independent_count)
        .bind(&new.source)
        .bind(&new.meta)
        .bind(&new.fingerprint)
        .bind(new.proposer_id)
        .bind(&new.proposer_kind)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Reopen a rejected lesson when — and only when — the caller supplies a new independent
/// anchor. The comparison is against the refs the rejected row already held.
pub async fn reopen_rejected_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    evidence: Value,
    independent_count: i32,
    proposer: Option<(Uuid, String)>,
    meta: Value,
) -> Result<LessonRow, LessonStoreError> {
    let current = get_tx(&mut *conn, tenant, id).await?;
    if current.state != "rejected" {
        return Err(LessonStoreError::IllegalState {
            id,
            state: current.state,
            expected: "rejected".to_owned(),
        });
    }
    let old = independent_anchors(&current.evidence);
    let new: Vec<String> = independent_anchors(&evidence)
        .into_iter()
        .filter(|anchor| !old.iter().any(|existing| existing == anchor))
        .collect();
    if new.is_empty() {
        return Err(LessonStoreError::RejectedWithoutNewEvidence(id));
    }
    let sql = format!(
        "UPDATE lesson SET \
             state = 'proposed', evidence = $3, independent_count = $4, proposer_id = $5, \
             proposer_kind = $6, meta = $7, reviewed_by = NULL, reviewed_at = NULL, \
             review_note = NULL, promoted_asset_id = NULL, promoted_at = NULL, \
             version = version + 1, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND state = 'rejected' \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&evidence)
        .bind(independent_count)
        .bind(proposer.as_ref().map(|(id, _)| *id))
        .bind(proposer.as_ref().map(|(_, kind)| kind.as_str()))
        .bind(&meta)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(LessonStoreError::IllegalState {
        id,
        state: "rejected".to_owned(),
        expected: "rejected".to_owned(),
    })
}

/// Record a review decision on a proposed lesson: `approved` → `reviewed`, `rejected` →
/// `rejected`. The decision is durable; only [`reopen_rejected_tx`] can move a rejection.
pub async fn review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    decision: &str,
    reviewer: Uuid,
    note: Option<&str>,
) -> Result<LessonRow, LessonStoreError> {
    let state = match decision {
        "approved" | "approve" => "reviewed",
        "rejected" | "reject" => "rejected",
        other => {
            return Err(LessonStoreError::IllegalState {
                id,
                state: other.to_owned(),
                expected: "approved | rejected".to_owned(),
            });
        }
    };
    let sql = format!(
        "UPDATE lesson SET \
             state = $3, reviewed_by = $4, reviewed_at = now(), review_note = $5, \
             version = version + 1, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND state = 'proposed' \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(state)
        .bind(reviewer)
        .bind(note)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => Ok(row),
        None => {
            let current = get_tx(&mut *conn, tenant, id).await?;
            Err(LessonStoreError::IllegalState {
                id,
                state: current.state,
                expected: "proposed".to_owned(),
            })
        }
    }
}

/// Promote a reviewed lesson to a `governed_asset(kind = 'lesson_source', status = 'draft')`.
///
/// Idempotent on an already-promoted row. No `capability_knowledge` pin is written, nothing is
/// published, and no model is called.
pub async fn promote_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    reviewer: Option<Uuid>,
) -> Result<LessonRow, LessonStoreError> {
    let current = get_tx(&mut *conn, tenant, id).await?;
    if current.state == "promoted" {
        return Ok(current);
    }
    if current.state != "reviewed" {
        return Err(LessonStoreError::IllegalState {
            id,
            state: current.state,
            expected: "reviewed".to_owned(),
        });
    }
    if current.independent_count < 2 {
        return Err(LessonStoreError::EvidenceInsufficient(
            current.independent_count,
        ));
    }
    let key = format!("lesson/{id}");
    let title = asset_title(&current.statement);
    let asset_id: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset \
             (tenant_id, kind, key, title, body, status, owner_user_id, links, meta) \
         VALUES ($1, 'lesson_source', $2, $3, $4, 'draft', $5, $6, $7) \
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(&key)
    .bind(&title)
    .bind(&current.statement)
    .bind(reviewer.or(current.reviewed_by))
    .bind(json!({"lesson_id": id}))
    .bind(json!({
        "lesson_id": id,
        "evidence": current.evidence,
        "independent_count": current.independent_count,
    }))
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "UPDATE lesson SET \
             state = 'promoted', promoted_asset_id = $3, promoted_at = now(), \
             reviewed_by = COALESCE(reviewed_by, $4), \
             reviewed_at = COALESCE(reviewed_at, now()), \
             version = version + 1, updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND state = 'reviewed' \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(asset_id)
        .bind(reviewer)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(LessonStoreError::IllegalState {
        id,
        state: "reviewed".to_owned(),
        expected: "reviewed".to_owned(),
    })
}

/// Record a failed run's lesson as `proposed`.
///
/// The run is provenance, not an independent reference: the row is stored with zero refs, so it
/// can never be reviewed or promoted until a second, genuinely independent ref arrives. No
/// execution path promotes a lesson, and no `capability_knowledge` pin is touched.
pub async fn record_execution_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: NewExecutionLesson,
) -> Result<LessonRow, LessonStoreError> {
    let run: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(new.run_id)
            .fetch_optional(&mut *conn)
            .await?;
    if run.is_none() {
        return Err(LessonStoreError::EvidenceUnknown {
            kind: "run".to_owned(),
            id: new.run_id,
        });
    }
    let mut meta = new.meta;
    if !meta.is_object() {
        meta = json!({});
    }
    if let Some(object) = meta.as_object_mut() {
        object.insert("run_id".to_owned(), json!(new.run_id));
        if let Some(card_id) = new.card_id {
            object.insert("card_id".to_owned(), json!(card_id));
        }
    }
    let sql = format!(
        "INSERT INTO lesson \
             (tenant_id, programme_id, statement, evidence, independent_count, state, source, \
              meta, proposer_id, proposer_kind) \
         VALUES ($1, $2, $3, '[]'::jsonb, 0, 'proposed', 'execution', $4, $5, $6) \
         RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, LessonRow>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.programme_id)
        .bind(new.statement.trim())
        .bind(&meta)
        .bind(new.recorded_by)
        .bind(&new.proposer_kind)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The governed asset's title: the statement, whitespace-collapsed and bounded.
fn asset_title(statement: &str) -> String {
    let one_line = statement.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= 160 {
        return one_line;
    }
    let mut title: String = one_line.chars().take(159).collect();
    title.push('…');
    title
}
