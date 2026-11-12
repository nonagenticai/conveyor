//! The ONE history table — this module is its read side; the writer is
//! [`crate::governance::write_snapshot_revision`].
//!
//! Append-only: `revision` has `created_at` and no `version` (S03 binding decision 1). For a
//! governed asset the store writes the row with `seq = version`; other entities use their own
//! dense sequence. `content_hash` is always the SHA-256 of the snapshot's canonical JSON
//! (S07 §5.3) — one convention for every entity family. Retention is a job-applied policy
//! ([`RevisionPolicy`](conveyor_domain::RevisionPolicy)); the model arm is enforced by
//! [`crate::prune::model_revision_prune`], never by a trigger.

use std::str::FromStr;

use conveyor_domain::{Revision, RevisionStatus, TenantId};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Why a revision could not be read.
#[derive(Debug, thiserror::Error)]
pub enum RevisionStoreError {
    /// A stored `revision.status` no longer parses.
    #[error("the database holds revision.status = {0:?}, which the domain does not know")]
    UnknownStatus(String),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct RevisionRow {
    id: Uuid,
    entity_type: String,
    entity_id: Uuid,
    seq: i32,
    label: String,
    content: Value,
    content_hash: String,
    author_id: Option<Uuid>,
    status: String,
}

impl TryFrom<RevisionRow> for Revision {
    type Error = RevisionStoreError;
    fn try_from(row: RevisionRow) -> Result<Self, Self::Error> {
        let status = RevisionStatus::from_str(&row.status)
            .map_err(|_| RevisionStoreError::UnknownStatus(row.status))?;
        Ok(Self {
            id: row.id,
            entity_type: row.entity_type,
            entity_id: row.entity_id,
            seq: row.seq,
            label: row.label,
            content: row.content,
            content_hash: row.content_hash,
            author_id: row.author_id,
            status,
        })
    }
}

const COLUMNS: &str = "id, entity_type, entity_id, seq, label, content, content_hash, \
                       author_id, status";

/// Every revision of one entity, newest first.
pub async fn list_revisions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entity_type: &str,
    entity_id: Uuid,
) -> Result<Vec<Revision>, RevisionStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3
          ORDER BY seq DESC"
    );
    let rows: Vec<RevisionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(entity_type)
        .bind(entity_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(Revision::try_from).collect()
}

/// Read one revision by id inside the caller's scoped transaction.
///
/// RLS confines the SELECT to the caller's tenant, so another tenant's revision id resolves
/// to `None` rather than an error.
pub async fn get_revision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Revision>, RevisionStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM revision WHERE tenant_id = $1 AND id = $2");
    let row: Option<RevisionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Revision::try_from).transpose()
}

/// Read one entity's revision at `seq` inside the caller's scoped transaction.
///
/// For a governed asset `seq` is the asset's `version` (S07 §5.3), so this is the read
/// `requirements.compare` uses to load each side of the diff.
pub async fn get_revision_by_seq_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entity_type: &str,
    entity_id: Uuid,
    seq: i32,
) -> Result<Option<Revision>, RevisionStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3 AND seq = $4"
    );
    let row: Option<RevisionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(entity_type)
        .bind(entity_id)
        .bind(seq)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Revision::try_from).transpose()
}
