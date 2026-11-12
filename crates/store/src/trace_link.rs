//! `trace_link` — the single writer of the polymorphic trace edge (S07, data-arch §2.7).
//!
//! Both ends are a `(type, id)` pair with no FK, so the domain function resolves each end
//! inside the caller's tenant in the same transaction (CONTRACT §2). The whitelist of types is
//! this module's, not a client's: [`END_TYPES`] is the only source of an end table name, and
//! no caller-supplied string ever reaches the SQL as an identifier.
//!
//! [`list_trace_links_tx`] takes a [`TraceFilter`] whose predicates may address **either**
//! end, so one function serves both read directions of S07 §5.7: the requirement screen
//! filters the end it anchored the link at, and the card screen filters the other end of the
//! same row. A row is returned by both filters; there is no second read path.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// The end types a trace link may name, and therefore the only tables this module resolves.
///
/// The name **is** the table: `governed_asset` covers requirements, principles and ADRs (the
/// asset's `kind` distinguishes them); `card` is a work item. A new end type is a code change
/// here, never an input.
pub const END_TYPES: [&str; 2] = ["governed_asset", "card"];

/// The five edge kinds `trace_link_kind_check` admits (0002_domain.sql).
pub const LINK_KINDS: [&str; 5] = ["traces", "realises", "derives", "satisfies", "conflicts"];

/// A trace edge to create.
#[derive(Clone, Debug)]
pub struct NewTraceLink {
    /// The end the edge starts at; one of [`END_TYPES`].
    pub from_type: String,
    /// The id in that end's table.
    pub from_id: Uuid,
    /// The end the edge points at; one of [`END_TYPES`].
    pub to_type: String,
    /// The id in that end's table.
    pub to_id: Uuid,
    /// One of [`LINK_KINDS`]; `traces` by default.
    pub kind: String,
}

impl NewTraceLink {
    /// An edge of kind `traces`, the S07 §5.7 demonstration.
    pub fn new(
        from_type: impl Into<String>,
        from_id: Uuid,
        to_type: impl Into<String>,
        to_id: Uuid,
    ) -> Self {
        Self {
            from_type: from_type.into(),
            from_id,
            to_type: to_type.into(),
            to_id,
            kind: "traces".to_owned(),
        }
    }
}

/// One `trace_link` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceLinkRecord {
    /// The row id.
    pub id: Uuid,
    /// The start end's type.
    pub from_type: String,
    /// The start end's id.
    pub from_id: Uuid,
    /// The pointed-at end's type.
    pub to_type: String,
    /// The pointed-at end's id.
    pub to_id: Uuid,
    /// The edge's meaning.
    pub kind: String,
    /// When the edge was written; `trace_link` is an immutable fact, so there is no
    /// `updated_at`.
    pub created_at: DateTime<Utc>,
}

/// A predicate set for [`list_trace_links_tx`].
///
/// Every field is optional and the set is `AND`ed. Give `from_*` to read the edges a node
/// emits, or `to_*` to read the edges that point at it — the same rows answer both. An id
/// without its type is still a valid filter, though supplying the type uses the index.
#[derive(Clone, Debug, Default)]
pub struct TraceFilter {
    /// The start end's type; one of [`END_TYPES`] when present.
    pub from_type: Option<String>,
    /// The start end's id.
    pub from_id: Option<Uuid>,
    /// The pointed-at end's type; one of [`END_TYPES`] when present.
    pub to_type: Option<String>,
    /// The pointed-at end's id.
    pub to_id: Option<Uuid>,
    /// The edge kind; one of [`LINK_KINDS`] when present.
    pub kind: Option<String>,
}

/// Why a trace-link operation failed.
#[derive(Debug, thiserror::Error)]
pub enum TraceLinkStoreError {
    /// An end type outside [`END_TYPES`].
    #[error("unknown trace end type {0:?}: expected one of governed_asset, card")]
    UnknownType(String),
    /// An edge kind outside [`LINK_KINDS`].
    #[error(
        "unknown trace kind {0:?}: expected one of traces, realises, derives, satisfies, \
         conflicts"
    )]
    UnknownKind(String),
    /// An end id that does not resolve **inside the caller's tenant**.
    #[error("the {side} end {id} does not exist in this tenant")]
    EndNotFound {
        /// `"from"` or `"to"`.
        side: &'static str,
        /// The id that did not resolve.
        id: Uuid,
    },
    /// The same `(tenant, from, to, kind)` edge already exists.
    #[error("this trace link already exists")]
    Conflict,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The static table name for an end type — the only source of the identifier.
fn end_table(end_type: &str) -> Option<&'static str> {
    match end_type {
        "governed_asset" => Some("governed_asset"),
        "card" => Some("card"),
        _ => None,
    }
}

/// Resolve one end inside the caller's tenant, in the caller's transaction.
async fn ensure_end_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    side: &'static str,
    end_type: &str,
    id: Uuid,
) -> Result<(), TraceLinkStoreError> {
    let table =
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.to_owned()))?;
    let sql = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE tenant_id = $1 AND id = $2)");
    let present: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    if present {
        Ok(())
    } else {
        Err(TraceLinkStoreError::EndNotFound { side, id })
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

const COLUMNS: &str = "id, from_type, from_id, to_type, to_id, kind, created_at";

/// Insert a trace edge inside the caller's scoped transaction.
///
/// Both ends are resolved with a tenant-scoped `SELECT` **before** the insert: the table has
/// no FK, and even if it had one a single-column FK does not enforce tenancy (CONTRACT §2).
/// A duplicate edge is [`TraceLinkStoreError::Conflict`] (SQLSTATE 23505), not a 500.
pub async fn insert_trace_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTraceLink,
) -> Result<TraceLinkRecord, TraceLinkStoreError> {
    ensure_end_tx(conn, tenant, "from", &input.from_type, input.from_id).await?;
    ensure_end_tx(conn, tenant, "to", &input.to_type, input.to_id).await?;
    if !LINK_KINDS.contains(&input.kind.as_str()) {
        return Err(TraceLinkStoreError::UnknownKind(input.kind.clone()));
    }
    let sql = format!(
        "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id, kind)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    );
    let row: TraceLinkRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.from_type)
        .bind(input.from_id)
        .bind(&input.to_type)
        .bind(input.to_id)
        .bind(&input.kind)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                TraceLinkStoreError::Conflict
            } else {
                TraceLinkStoreError::Database(error)
            }
        })?;
    Ok(row)
}

/// Delete one trace edge. `false` means it does not exist in this tenant.
pub async fn delete_trace_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, TraceLinkStoreError> {
    let rows = sqlx::query("DELETE FROM trace_link WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows == 1)
}

/// One page of trace edges matching [`TraceFilter`], newest first.
///
/// Order is `(created_at DESC, id DESC)`: the id is a uuid v7, so it is both the tiebreaker
/// for rows written in one transaction and the stable key a cursor would carry.
pub async fn list_trace_links_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &TraceFilter,
    limit: i64,
) -> Result<Vec<TraceLinkRecord>, TraceLinkStoreError> {
    if let Some(end_type) = &filter.from_type {
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.clone()))?;
    }
    if let Some(end_type) = &filter.to_type {
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.clone()))?;
    }
    if let Some(kind) = &filter.kind
        && !LINK_KINDS.contains(&kind.as_str())
    {
        return Err(TraceLinkStoreError::UnknownKind(kind.clone()));
    }
    let sql = format!(
        "SELECT {COLUMNS}
           FROM trace_link
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR from_type = $2)
            AND ($3::uuid IS NULL OR from_id = $3)
            AND ($4::text IS NULL OR to_type = $4)
            AND ($5::uuid IS NULL OR to_id = $5)
            AND ($6::text IS NULL OR kind = $6)
          ORDER BY created_at DESC, id DESC
          LIMIT $7"
    );
    let rows: Vec<TraceLinkRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.from_type.as_deref())
        .bind(filter.from_id)
        .bind(filter.to_type.as_deref())
        .bind(filter.to_id)
        .bind(filter.kind.as_deref())
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}
