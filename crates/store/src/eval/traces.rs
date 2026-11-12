#![forbid(unsafe_code)]
//! `eval_trace` — one transcript per `(run, case, rep)`, the evidence behind a score (S19 Part B;
//! CONTRACT §20).
//!
//! A trace is the skill's exact turn list: `system | user | assistant | tool_call | tool_result`.
//! Three rules are enforced here rather than by the writer:
//!
//! - **One trace per slot.** `eval_trace_slot` is `(tenant_id, eval_run_id, case_id, rep)`, so a
//!   re-run of a case is a new `rep` or a new run, never a second transcript for a score that
//!   already has one.
//! - **The hash is computed from the turns**, with the same `content_hash` the instrument digest
//!   uses — not a second SHA-256 that could disagree with it about canonicalisation.
//! - **The size cap is the database's.** `bytes` is computed here and bound; a transcript over
//!   1 MiB is refused by `eval_trace`'s own CHECK, so the guarantee survives any future writer
//!   that forgets to look ([`insert_trace_tx`] does not pre-check and then claim the constraint).
//!
//! ⛔ Case text is untrusted input. Nothing in this module renders it: `turns` is stored as data
//! and read back as data, and no view casts it to HTML.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{EvalStoreError, map_write_error};

/// One transcript to write.
#[derive(Clone, Debug)]
pub struct NewTrace<'a> {
    /// The run this transcript belongs to.
    pub eval_run_id: Uuid,
    /// The case it ran.
    pub case_id: Uuid,
    /// The repetition inside the run, from 0.
    pub rep: i32,
    /// The turn list: `[{role, content, ...}]`, roles from the method's five.
    pub turns: &'a Value,
}

/// A stored transcript.
#[derive(Clone, Debug)]
pub struct Trace {
    /// The row id.
    pub id: Uuid,
    /// The run it belongs to.
    pub eval_run_id: Uuid,
    /// The case it ran.
    pub case_id: Uuid,
    /// The repetition.
    pub rep: i32,
    /// The turn list.
    pub turns: Value,
    /// SHA-256 of the canonical turn list.
    pub sha256: String,
    /// Its canonical size, in bytes (≤ 1 MiB).
    pub bytes: i32,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

const TRACE_COLUMNS: &str = "id, eval_run_id, case_id, rep, turns, sha256, bytes, created_at";

#[derive(Debug, sqlx::FromRow)]
struct TraceRow {
    id: Uuid,
    eval_run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    turns: Value,
    sha256: String,
    bytes: i32,
    created_at: DateTime<Utc>,
}

impl TraceRow {
    fn into_trace(self) -> Trace {
        Trace {
            id: self.id,
            eval_run_id: self.eval_run_id,
            case_id: self.case_id,
            rep: self.rep,
            turns: self.turns,
            sha256: self.sha256,
            bytes: self.bytes,
            created_at: self.created_at,
        }
    }
}

/// Write one transcript, inside the same transaction as the result it explains.
///
/// Returns the stored row. A second write for a slot is [`EvalStoreError::Conflict`] carrying
/// `eval_trace_slot`; a transcript over the cap is [`EvalStoreError::Invalid`] carrying
/// `eval_trace_bytes_check`.
pub async fn insert_trace_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTrace<'_>,
) -> Result<Trace, EvalStoreError> {
    let sha256 = conveyor_domain::compare::content_hash(input.turns);
    // The canonical text is exactly what the hash covered, so `bytes` and `sha256` describe the
    // same bytes; measuring `turns.to_string()` instead would let a pretty-printed copy pass the
    // cap that the hashed form does not.
    let bytes = i32::try_from(conveyor_domain::compare::canonical_json(input.turns).len())
        .unwrap_or(i32::MAX);
    let sql = format!(
        "INSERT INTO eval_trace (tenant_id, eval_run_id, case_id, rep, turns, sha256, bytes)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {TRACE_COLUMNS}"
    );
    let row: TraceRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.eval_run_id)
        .bind(input.case_id)
        .bind(input.rep)
        .bind(input.turns)
        .bind(sha256)
        .bind(bytes)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row.into_trace())
}

/// One transcript by slot, if it exists.
pub async fn get_trace_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<Option<Trace>, EvalStoreError> {
    let sql = format!(
        "SELECT {TRACE_COLUMNS} FROM eval_trace
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = $4"
    );
    let row: Option<TraceRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run_id)
        .bind(case_id)
        .bind(rep)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(TraceRow::into_trace))
}
