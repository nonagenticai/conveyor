//! Documents: the metadata row, never the bytes (S09 §5.2).
//!
//! `document.object_key` is a pointer into the object store and `content_hash` is the sha256 the bytes must
//! reproduce on download. Nothing in this module moves a byte: the action layer streams to and from the
//! store, and this module only keeps the row honest about which key and which hash a document has.
//!
//! Two states matter and are the whole lifecycle:
//!
//! - **active** — the row and its object exist (an upload), or the row is a *report* whose numbers are live
//!   queries and whose `object_key` is null (S09 §5.5).
//! - **deleted** — a tombstone. `delete` sets it and enqueues `document.purge`; the purge removes the object
//!   and clears `object_key`, so the row keeps the id, the title and the audit trail after the bytes are gone.
//!
//! Every write is `If-Match`-guarded on `version` except the purge, which is a job and therefore not a caller
//! action: it clears the key only for a row already in `deleted`.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::ParentError;
use crate::project::{decode_cursor, encode_cursor};

/// A document row as the API returns it.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRecord {
    /// The row id.
    pub id: Uuid,
    /// The owning project, when the document belongs to one.
    pub project_id: Option<Uuid>,
    /// `document | report | export`.
    pub kind: String,
    /// The display title.
    pub title: String,
    /// `active | rejected | failed | deleted`.
    pub status: String,
    /// The object key, when the document has bytes.
    pub object_key: Option<String>,
    /// The sha256 of the bytes, lowercase hex.
    pub content_hash: Option<String>,
    /// The external ingestion reference (strata's job id).
    pub external_ref: Option<String>,
    /// Free metadata (`content_type`, `size_bytes`, report parameters).
    pub meta: Value,
    /// The optimistic-concurrency version (`ETag`).
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// The fields a create supplies.
#[derive(Clone, Debug)]
pub struct NewDocument {
    /// The row id, minted by the caller: the object key embeds it, so it must exist before the bytes move.
    pub id: Uuid,
    /// The owning project, resolved in the caller's tenant.
    pub project_id: Option<Uuid>,
    /// `document | report | export`.
    pub kind: String,
    /// The display title.
    pub title: String,
    /// `active` unless the create is a report or a failed ingest.
    pub status: String,
    /// The object key, when bytes were stored.
    pub object_key: Option<String>,
    /// The sha256 of the bytes.
    pub content_hash: Option<String>,
    /// The external reference.
    pub external_ref: Option<String>,
    /// Free metadata.
    pub meta: Value,
}

/// What an update may change. Bytes and kind are immutable: a new payload is a new document.
#[derive(Clone, Debug, Default)]
pub struct DocumentPatch {
    /// The new title.
    pub title: Option<String>,
    /// The new metadata (replaced whole, like every `meta` in this workspace).
    pub meta: Option<Value>,
}

/// A list filter: both fields are optional and combine with AND.
#[derive(Clone, Debug, Default)]
pub struct DocumentFilter {
    /// Only this project's documents.
    pub project_id: Option<Uuid>,
    /// Only this kind.
    pub kind: Option<String>,
}

/// One page request: an opaque `(updated_at, id)` cursor and a limit.
#[derive(Clone, Debug, Default)]
pub struct DocumentPage {
    /// The previous page's cursor, or `None` for the first page.
    pub cursor: Option<String>,
    /// The page size; the action layer clamps it.
    pub limit: i64,
}

impl DocumentPage {
    /// A first page of `limit` rows.
    pub fn new(limit: i64) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }
}

/// Why a document read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum DocumentStoreError {
    /// No such row in this tenant.
    #[error("the document does not exist in this tenant")]
    NotFound,
    /// The row moved; the caller's `If-Match` is stale.
    #[error("the document is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: Version,
    },
    /// The cursor is not one this module issued.
    #[error("the page cursor is malformed")]
    BadCursor,
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const DOCUMENT_COLUMNS: &str = "id, project_id, kind, title, status, object_key, content_hash, \
                                external_ref, meta, version, created_at, updated_at";

type DocumentRow = (
    Uuid,
    Option<Uuid>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Value,
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
);

impl From<DocumentRow> for DocumentRecord {
    fn from(row: DocumentRow) -> Self {
        Self {
            id: row.0,
            project_id: row.1,
            kind: row.2,
            title: row.3,
            status: row.4,
            object_key: row.5,
            content_hash: row.6,
            external_ref: row.7,
            meta: row.8,
            version: crate::version_of(row.9),
            created_at: row.10,
            updated_at: row.11,
        }
    }
}

/// Insert one document row.
pub async fn create_document_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewDocument,
) -> Result<DocumentRecord, DocumentStoreError> {
    if let Some(project) = input.project_id {
        crate::parent::ensure_parent_tx(
            &mut *conn,
            tenant,
            crate::parent::Parent::Project,
            project,
        )
        .await?;
    }
    let sql = format!(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, object_key, \
                               content_hash, external_ref, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(input.id)
        .bind(tenant.as_uuid())
        .bind(input.project_id)
        .bind(&input.kind)
        .bind(&input.title)
        .bind(&input.status)
        .bind(&input.object_key)
        .bind(&input.content_hash)
        .bind(&input.external_ref)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// One document, or `None` when it is not this tenant's.
pub async fn get_document_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DocumentRecord>, DocumentStoreError> {
    let sql = format!("SELECT {DOCUMENT_COLUMNS} FROM document WHERE tenant_id = $1 AND id = $2");
    let row: Option<DocumentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One page of this tenant's documents, newest first.
pub async fn list_documents_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &DocumentFilter,
    page: &DocumentPage,
) -> Result<(Vec<DocumentRecord>, Option<String>), DocumentStoreError> {
    let limit = page.limit.max(1);
    let cursor = match page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(decode_cursor(raw).ok_or(DocumentStoreError::BadCursor)?),
    };
    // One statement, one fetch of limit+1: the extra row answers "is there a next page" without a count.
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS} FROM document
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR project_id = $2)
            AND ($3::text IS NULL OR kind = $3)
            AND ($4::timestamptz IS NULL OR (updated_at, id) < ($4::timestamptz, $5::uuid))
          ORDER BY updated_at DESC, id DESC
          LIMIT $6"
    );
    let (cursor_at, cursor_id) = match &cursor {
        Some((key, id)) => {
            let at = key
                .parse::<DateTime<Utc>>()
                .map_err(|_| DocumentStoreError::BadCursor)?;
            (Some(at), Some(*id))
        }
        None => (None, None),
    };
    let rows: Vec<DocumentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.project_id)
        .bind(filter.kind.as_deref())
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&mut *conn)
        .await?;
    let mut records: Vec<DocumentRecord> = rows.into_iter().map(Into::into).collect();
    let next = if records.len() as i64 > limit {
        records.truncate(limit as usize);
        records
            .last()
            .map(|last| encode_cursor(&last.updated_at.to_rfc3339(), last.id))
    } else {
        None
    };
    Ok((records, next))
}

/// Update the title and/or metadata of one document, at the caller's version.
pub async fn update_document_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &DocumentPatch,
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let title = patch.title.clone().unwrap_or(current.title.clone());
    let meta = patch.meta.clone().unwrap_or(current.meta.clone());
    let sql = format!(
        "UPDATE document
            SET title = $3, meta = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(title)
        .bind(meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Set `status`, at the caller's version — the tombstone write and the ingest failure path.
pub async fn set_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Option<Version>,
    status: &str,
    external_ref: Option<&str>,
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if let Some(expected) = expected_version
        && current.version != expected
    {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE document
            SET status = $3,
                external_ref = COALESCE($4, external_ref),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(status)
        .bind(external_ref)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Read one document **and hold its row lock until the transaction ends** (`FOR UPDATE`).
///
/// `documents.ingest` is the only caller. It reads `external_ref`, performs an upstream call that takes
/// seconds, and only then writes `external_ref` — so between the check and the write there is a window in
/// which a second caller would pass the same conflict check and push the document twice. The lock closes it:
/// the second caller blocks, and after the first commits its `SELECT` re-reads the row and sees the reference.
pub async fn get_document_locked_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DocumentRecord>, DocumentStoreError> {
    match lock_document_tx(conn, tenant, id).await {
        Ok(record) => Ok(Some(record)),
        Err(DocumentStoreError::NotFound) => Ok(None),
        Err(other) => Err(other),
    }
}

/// Record the job id `documents.ingest` got back from `strata_ingest`.
///
/// ⚠️ **Deliberately does not touch `status`.** The slice plan says a failed ingest sets `status='failed'`,
/// but `fetch_bytes` (the one path `documents.download` and the byte route share) requires `status='active'`
/// — so a transient `strata_ingest` outage would make the document unreadable, a worse outcome than an
/// un-ingested document. A failed push writes nothing here at all: the engine's `record_failure` already
/// records it on the audit trail, and this transaction is rolled back when the handler returns an error.
pub async fn set_external_ref_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    external_ref: &str,
) -> Result<DocumentRecord, DocumentStoreError> {
    let sql = format!(
        "UPDATE document
            SET external_ref = $3,
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(external_ref)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(DocumentStoreError::NotFound)?;
    Ok(row.into())
}

/// Clear the object pointer of a tombstoned document — the `document.purge` job's write.
///
/// Refuses a row that is not `deleted`: purging an active document would orphan live bytes.
pub async fn clear_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, DocumentStoreError> {
    let affected = sqlx::query(
        "UPDATE document
            SET object_key = NULL, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'deleted' AND object_key IS NOT NULL",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// Read the row `FOR UPDATE` and fail with `NotFound` when it is absent.
async fn lock_document_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<DocumentRecord, DocumentStoreError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS} FROM document WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<DocumentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(DocumentStoreError::NotFound)
}
