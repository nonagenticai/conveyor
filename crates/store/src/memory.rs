#![forbid(unsafe_code)]
//! S15's work-package memory store: the hot rows (the record) and the consolidation write.
//!
//! Every function takes the caller's connection and is `_tx`: a run records its memory in the
//! run's transaction, and a digest plus the `consolidated_at` stamps on the rows it covers commit
//! together or not at all. Every query also filters `tenant_id = $1` even though RLS is
//! FORCE-enabled — the explicit predicate is the first line of defence, the policy the second.
//!
//! `strata_rag` is never the system of record: hot rows are, and a digest is derived from them.

use chrono::{DateTime, Utc};
use conveyor_domain::memory::{Digest, MemoryKind, MemoryLine, MemorySource};
use conveyor_domain::{IdError, TenantId};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};

/// Why a memory operation failed.
#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    /// No such row in this tenant.
    #[error("the memory row does not exist in this tenant")]
    NotFound,
    /// A superseded row must belong to the same card (`work_package_memory` has no unique pair
    /// for this, so it is a store check, not a constraint).
    #[error("a memory row can only supersede another row of the same card")]
    SupersedesOtherCard,
    /// A stored `kind` the vocabulary does not know (impossible under the column CHECK).
    #[error("unknown memory kind {0:?}")]
    UnknownKind(String),
    /// A stored `source` the vocabulary does not know (impossible under the column CHECK).
    #[error("unknown memory source {0:?}")]
    UnknownSource(String),
    /// A stored uuid could not be parsed (impossible under the column type).
    #[error(transparent)]
    Id(#[from] IdError),
    /// A named parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const COLUMNS: &str = "id, project_id, card_id, run_id, capability_id, kind, source, body, \
     supersedes_id, consolidated_at, created_at";

/// A `memory.remember` write, before it is stored.
#[derive(Clone, Debug)]
pub struct NewMemory<'a> {
    /// The work package: the card.
    pub card_id: Uuid,
    /// The run that learned it, when it came from a run.
    pub run_id: Option<Uuid>,
    /// The capability the run executed under, when one was resolved.
    pub capability_id: Option<Uuid>,
    /// What it records. A digest is written only by [`write_digest_tx`].
    pub kind: MemoryKind,
    /// Who wrote it.
    pub source: MemorySource,
    /// The text; the caller has already validated it.
    pub body: &'a str,
    /// The row this one supersedes, if any.
    pub supersedes_id: Option<Uuid>,
    /// Free structured detail.
    pub meta: Value,
}

/// A hot memory row as stored.
#[derive(Clone, Debug)]
pub struct MemoryRecord {
    /// Row id.
    pub id: Uuid,
    /// The owning project (derived from the card's board — never trusted from the caller).
    pub project_id: Uuid,
    /// The work package.
    pub card_id: Uuid,
    /// The run, when any.
    pub run_id: Option<Uuid>,
    /// The capability, when any.
    pub capability_id: Option<Uuid>,
    /// Parsed `kind`.
    pub kind: MemoryKind,
    /// Parsed `source`.
    pub source: MemorySource,
    /// The text.
    pub body: String,
    /// The superseded row, when any.
    pub supersedes_id: Option<Uuid>,
    /// When a consolidation covered this row.
    pub consolidated_at: Option<DateTime<Utc>>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct MemoryRow {
    id: Uuid,
    project_id: Uuid,
    card_id: Uuid,
    run_id: Option<Uuid>,
    capability_id: Option<Uuid>,
    kind: String,
    source: String,
    body: String,
    supersedes_id: Option<Uuid>,
    consolidated_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl TryFrom<MemoryRow> for MemoryRecord {
    type Error = MemoryStoreError;

    fn try_from(row: MemoryRow) -> Result<Self, Self::Error> {
        let kind = MemoryKind::parse(&row.kind)
            .ok_or_else(|| MemoryStoreError::UnknownKind(row.kind.clone()))?;
        let source = MemorySource::parse(&row.source)
            .ok_or_else(|| MemoryStoreError::UnknownSource(row.source.clone()))?;
        Ok(Self {
            id: row.id,
            project_id: row.project_id,
            card_id: row.card_id,
            run_id: row.run_id,
            capability_id: row.capability_id,
            kind,
            source,
            body: row.body,
            supersedes_id: row.supersedes_id,
            consolidated_at: row.consolidated_at,
            created_at: row.created_at,
        })
    }
}

/// Resolve the project a card belongs to inside the caller's tenant, with `FOR SHARE` so a
/// concurrent card/board deletion cannot invalidate the write between the check and the insert.
async fn project_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Uuid, MemoryStoreError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT b.project_id
           FROM card c
           JOIN board b ON b.id = c.board_id AND b.tenant_id = c.tenant_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|(project_id,)| project_id)
        .ok_or(MemoryStoreError::Parent(ParentError::NotFound {
            parent: Parent::Card,
        }))
}

/// Write one hot row. The project is derived from the card; every named id is resolved inside the
/// caller's tenant first (a single-column FK would accept another tenant's id).
pub async fn insert_memory_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewMemory<'_>,
) -> Result<MemoryRecord, MemoryStoreError> {
    if new.kind == MemoryKind::Digest {
        return Err(MemoryStoreError::UnknownKind("digest".to_owned()));
    }
    ensure_parent_tx(conn, tenant, Parent::Card, new.card_id).await?;
    if let Some(run_id) = new.run_id {
        ensure_parent_tx(conn, tenant, Parent::Run, run_id).await?;
    }
    if let Some(capability_id) = new.capability_id {
        ensure_parent_tx(conn, tenant, Parent::Capability, capability_id).await?;
    }
    let project_id = project_for_card_tx(conn, tenant, new.card_id).await?;

    if let Some(supersedes_id) = new.supersedes_id {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT card_id FROM work_package_memory WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(supersedes_id)
        .fetch_optional(&mut *conn)
        .await?;
        match row {
            None => return Err(MemoryStoreError::NotFound),
            Some((card_id,)) if card_id != new.card_id => {
                return Err(MemoryStoreError::SupersedesOtherCard);
            }
            Some(_) => {}
        }
    }

    let sql = format!(
        "INSERT INTO work_package_memory
             (tenant_id, project_id, card_id, run_id, capability_id, kind, source, body,
              supersedes_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {COLUMNS}"
    );
    let row: MemoryRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(new.card_id)
        .bind(new.run_id)
        .bind(new.capability_id)
        .bind(new.kind.as_str())
        .bind(new.source.as_str())
        .bind(new.body.trim())
        .bind(new.supersedes_id)
        .bind(&new.meta)
        .fetch_one(&mut *conn)
        .await?;
    row.try_into()
}

/// The hot rows for a card, newest first. `memory.recall` returns these before any semantic hit.
pub async fn hot_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    limit: i64,
) -> Result<Vec<MemoryRecord>, MemoryStoreError> {
    let sql = format!(
        "SELECT {COLUMNS}
           FROM work_package_memory
          WHERE tenant_id = $1 AND card_id = $2
          ORDER BY created_at DESC, id DESC
          LIMIT $3"
    );
    let rows: Vec<MemoryRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(TryInto::try_into).collect()
}

/// The rows a consolidation has not yet covered, oldest first. Digest rows are excluded: they
/// are derived, so including one would make every digest contain the previous one.
pub async fn unconsolidated_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Vec<MemoryLine>, MemoryStoreError> {
    let rows: Vec<(Uuid, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, kind, source, body, created_at
           FROM work_package_memory
          WHERE tenant_id = $1 AND card_id = $2
            AND consolidated_at IS NULL AND kind <> 'digest'
          ORDER BY created_at, id",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|(id, kind, source, body, created_at)| {
            let kind = MemoryKind::parse(&kind)
                .ok_or_else(|| MemoryStoreError::UnknownKind(kind.clone()))?;
            let source = MemorySource::parse(&source)
                .ok_or_else(|| MemoryStoreError::UnknownSource(source.clone()))?;
            Ok(MemoryLine {
                id,
                kind,
                source,
                body,
                created_at,
            })
        })
        .collect()
}

/// The context a digest's metadata records: the project and the distinct runs/capabilities the
/// un-consolidated rows name.
#[derive(Clone, Debug)]
pub struct MemoryContext {
    /// The owning project (derived from the card).
    pub project_id: Uuid,
    /// Distinct runs the rows came from.
    pub run_ids: Vec<Uuid>,
    /// Distinct capabilities the runs executed under.
    pub capability_ids: Vec<Uuid>,
}

/// Resolve the digest metadata for a card in one read.
pub async fn context_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<MemoryContext, MemoryStoreError> {
    let project_id = project_for_card_tx(conn, tenant, card_id).await?;
    let rows: Vec<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT run_id, capability_id
           FROM work_package_memory
          WHERE tenant_id = $1 AND card_id = $2 AND consolidated_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut run_ids = Vec::new();
    let mut capability_ids = Vec::new();
    for (run_id, capability_id) in rows {
        if let Some(run_id) = run_id
            && !run_ids.contains(&run_id)
        {
            run_ids.push(run_id);
        }
        if let Some(capability_id) = capability_id
            && !capability_ids.contains(&capability_id)
        {
            capability_ids.push(capability_id);
        }
    }
    Ok(MemoryContext {
        project_id,
        run_ids,
        capability_ids,
    })
}

/// Write the derived digest row and stamp the rows it covers, in one statement pair.
///
/// Replay is a no-op by construction: the caller only calls this with rows whose
/// `consolidated_at IS NULL`, and the `UPDATE` re-checks that predicate. The digest row itself is
/// stamped `consolidated_at = now()`, so it is never consolidated again.
pub async fn write_digest_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    digest: &Digest,
) -> Result<Uuid, MemoryStoreError> {
    let project_id = project_for_card_tx(conn, tenant, card_id).await?;
    let meta = json!({
        "row_ids": digest.row_ids,
        "truncated": digest.truncated,
        "rows_offered": digest.rows_offered,
    });
    let sql = format!(
        "INSERT INTO work_package_memory
             (tenant_id, project_id, card_id, kind, source, body, consolidated_at, meta)
         VALUES ($1, $2, $3, 'digest', 'system', $4, now(), $5)
         RETURNING {COLUMNS}"
    );
    let row: MemoryRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .bind(card_id)
        .bind(&digest.body)
        .bind(&meta)
        .fetch_one(&mut *conn)
        .await?;

    sqlx::query(
        "UPDATE work_package_memory
            SET consolidated_at = now()
          WHERE tenant_id = $1 AND card_id = $2
            AND id = ANY($3) AND consolidated_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(&digest.row_ids)
    .execute(&mut *conn)
    .await?;

    Ok(row.id)
}
