#![forbid(unsafe_code)]
//! The per-card append-only ledger: claims and facts, written once and read oldest first.
//!
//! ADR 0008: the home is `card_ledger`, keyed by the conveyor `card.id`. There is deliberately
//! no update and no delete function here — [`append_ledger_tx`] is the only write — so
//! append-only is a property of this module's surface, not a trigger or a grant.
//!
//! Both functions take the caller's scoped `_tx` connection plus the tenant and filter
//! `tenant_id = $1` explicitly, like every other store module (RLS is the second line). The
//! append resolves the card in the caller's tenant first: a cross-tenant id is a store error,
//! never an FK acceptance (CONTRACT §2).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};

/// The two kinds a ledger entry may have (the column's CHECK is the same vocabulary).
pub const KINDS: [&str; 2] = ["claim", "fact"];

/// Why a ledger operation failed.
#[derive(Debug, thiserror::Error)]
pub enum LedgerStoreError {
    /// The card does not exist in this tenant (or is not visible to it).
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A ledger row to append, before it is stored.
#[derive(Clone, Debug)]
pub struct NewLedgerEntry<'a> {
    /// The card the entry belongs to.
    pub card_id: Uuid,
    /// `claim | fact`; the caller has already validated it.
    pub kind: &'a str,
    /// The claim or fact itself.
    pub body: &'a str,
    /// Where it came from (`list`, `node`, `execution_id`, …); an object, `{}` when unknown.
    pub source: &'a Value,
    /// The laya verdict, for a claim.
    pub verdict: Option<&'a Value>,
    /// The writing user; `None` for a service principal.
    pub created_by: Option<Uuid>,
}

/// One stored ledger row.
#[derive(Clone, Debug)]
pub struct LedgerEntry {
    /// Row id.
    pub id: Uuid,
    /// The card the entry belongs to.
    pub card_id: Uuid,
    /// `claim | fact`.
    pub kind: String,
    /// The claim or fact itself.
    pub body: String,
    /// Where it came from.
    pub source: Value,
    /// The laya verdict, when one was recorded.
    pub verdict: Option<Value>,
    /// The writing user, when one was recorded.
    pub created_by: Option<Uuid>,
    /// When the entry was written.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct LedgerRow {
    id: Uuid,
    card_id: Uuid,
    kind: String,
    body: String,
    source: Value,
    verdict: Option<Value>,
    created_by: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<LedgerRow> for LedgerEntry {
    fn from(row: LedgerRow) -> Self {
        Self {
            id: row.id,
            card_id: row.card_id,
            kind: row.kind,
            body: row.body,
            source: row.source,
            verdict: row.verdict,
            created_by: row.created_by,
            created_at: row.created_at,
        }
    }
}

const COLUMNS: &str = "id, card_id, kind, body, source, verdict, created_by, created_at";

/// Append one claim or fact inside the caller's scoped transaction.
///
/// The card is resolved in this tenant first; a foreign or missing card is a
/// [`ParentError::NotFound`], which the action layer maps to `404` before any row is written.
pub async fn append_ledger_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewLedgerEntry<'_>,
) -> Result<LedgerEntry, LedgerStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, new.card_id).await?;
    let sql = format!(
        "INSERT INTO card_ledger
             (tenant_id, card_id, kind, body, source, verdict, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {COLUMNS}"
    );
    let row: LedgerRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.card_id)
        .bind(new.kind)
        .bind(new.body.trim())
        .bind(new.source)
        .bind(new.verdict)
        .bind(new.created_by)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// One card's ledger, oldest first, optionally restricted to one kind.
///
/// No cursor: a card's ledger is small (one entry per claim/fact), and the caller bounds it with
/// `limit`. Ordering is `(created_at, id)` — the same tuple as the index — so two entries written
/// in one transaction still read back in a stable order.
pub async fn list_ledger_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    kind: Option<&str>,
    limit: i64,
) -> Result<Vec<LedgerEntry>, LedgerStoreError> {
    let limit = limit.max(1);
    let sql = format!(
        "SELECT {COLUMNS}
           FROM card_ledger
          WHERE tenant_id = $1 AND card_id = $2
            AND ($3::text IS NULL OR kind = $3)
          ORDER BY created_at, id
          LIMIT $4"
    );
    let rows: Vec<LedgerRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(kind)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}
