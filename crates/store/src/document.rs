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
use conveyor_domain::{RevisionStatus, TenantId, Version};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::HashMap;
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
    /// The owning card, when the output belongs to one.
    pub card_id: Option<Uuid>,
    /// The output this row is an attachment of, when `kind='attachment'`.
    pub parent_document_id: Option<Uuid>,
    /// `document | report | export | attachment`.
    pub kind: String,
    /// The display title.
    pub title: String,
    /// `active | rejected | failed | deleted`.
    pub status: String,
    /// The object key, when the document has bytes.
    pub object_key: Option<String>,
    /// The sha256 of the bytes or the body, lowercase hex.
    pub content_hash: Option<String>,
    /// The Markdown body, when this is a text output. Never set beside `object_key`.
    pub body: Option<String>,
    /// The external ingestion reference (strata's job id).
    pub external_ref: Option<String>,
    /// Free metadata (`content_type`, `size_bytes`, report parameters).
    pub meta: Value,
    /// Whether the public snapshot may carry this row.
    pub publishable: bool,
    /// The output-standard slot (`project-brief`, `evidence`, …), when one is recorded.
    pub category: Option<String>,
    /// The programme unit (a project key or an issue number), when one is recorded.
    pub unit: Option<String>,
    /// Who created the row; NULL for the rows that predate provenance.
    pub created_by: Option<Uuid>,
    /// Who last changed the row.
    pub updated_by: Option<Uuid>,
    /// When the row last became public; NULL on a legacy published row.
    pub published_at: Option<DateTime<Utc>>,
    /// Who last published it.
    pub published_by: Option<Uuid>,
    /// When an author last asked an admin to publish it.
    pub publish_requested_at: Option<DateTime<Utc>>,
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
    /// The owning card, resolved in the caller's tenant.
    pub card_id: Option<Uuid>,
    /// The output an attachment belongs to.
    pub parent_document_id: Option<Uuid>,
    /// `document | report | export | attachment`.
    pub kind: String,
    /// The display title.
    pub title: String,
    /// `active` unless the create is a report or a failed ingest.
    pub status: String,
    /// The object key, when bytes were stored.
    pub object_key: Option<String>,
    /// The sha256 of the bytes or the body.
    pub content_hash: Option<String>,
    /// The Markdown body, when this is a text output.
    pub body: Option<String>,
    /// The external reference.
    pub external_ref: Option<String>,
    /// Free metadata.
    pub meta: Value,
    /// Whether the public snapshot may carry this row.
    pub publishable: bool,
    /// The output-standard slot, when the caller records one.
    pub category: Option<String>,
    /// The programme unit, when the caller records one.
    pub unit: Option<String>,
    /// The creating principal.
    pub created_by: Option<Uuid>,
    /// The principal whose write produced this row.
    pub updated_by: Option<Uuid>,
}

/// What an update may change. Bytes and kind are immutable: a new payload is a new document.
#[derive(Clone, Debug, Default)]
pub struct DocumentPatch {
    /// The new title.
    pub title: Option<String>,
    /// The new metadata (replaced whole, like every `meta` in this workspace).
    pub meta: Option<Value>,
    /// The new Markdown body, when the caller changes the content.
    pub body: Option<String>,
    /// The sha256 of the new body; required beside `body`.
    pub content_hash: Option<String>,
    /// The owning card, when the caller links the output.
    pub card_id: Option<Uuid>,
    /// Whether the public snapshot may carry this row.
    pub publishable: Option<bool>,
    /// The output-standard slot; `Some("")` clears it, `None` leaves it unchanged.
    pub category: Option<String>,
    /// The programme unit; `Some("")` clears it, `None` leaves it unchanged.
    pub unit: Option<String>,
    /// The principal whose write produced this version.
    pub updated_by: Option<Uuid>,
}

/// A list filter: both fields are optional and combine with AND.
#[derive(Clone, Debug, Default)]
pub struct DocumentFilter {
    /// Only this project's documents.
    pub project_id: Option<Uuid>,
    /// Only this card's documents.
    pub card_id: Option<Uuid>,
    /// Only this output's attachments.
    pub parent_document_id: Option<Uuid>,
    /// Only this kind.
    pub kind: Option<String>,
    /// Only publishable rows, or only unpublishable ones.
    pub publishable: Option<bool>,
    /// Only this output-standard slot.
    pub category: Option<String>,
    /// Only this programme unit.
    pub unit: Option<String>,
    /// Only rows with (or without) a pending publish request.
    pub publish_requested: Option<bool>,
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

const DOCUMENT_COLUMNS: &str = "id, project_id, card_id, parent_document_id, kind, title, status, \
                                object_key, content_hash, body, external_ref, meta, publishable, \
                                category, unit, created_by, updated_by, published_at, published_by, \
                                publish_requested_at, version, created_at, updated_at";

/// The list projection omits `body`: a page of outputs must not carry every body. The placeholder
/// keeps the projection one shape, and `DocumentOut` reports `body: None` for a list row.
const DOCUMENT_LIST_COLUMNS: &str = "id, project_id, card_id, parent_document_id, kind, title, \
     status, object_key, content_hash, NULL::text AS body, external_ref, meta, publishable, \
     category, unit, created_by, updated_by, published_at, published_by, publish_requested_at, \
     version, created_at, updated_at";

/// One `document` row. By name, not by position: the provenance columns took the row past the
/// arity sqlx implements for tuples, and a name is clearer than a column order anyway.
#[derive(sqlx::FromRow)]
struct DocumentRow {
    id: Uuid,
    project_id: Option<Uuid>,
    card_id: Option<Uuid>,
    parent_document_id: Option<Uuid>,
    kind: String,
    title: String,
    status: String,
    object_key: Option<String>,
    content_hash: Option<String>,
    body: Option<String>,
    external_ref: Option<String>,
    meta: Value,
    publishable: bool,
    category: Option<String>,
    unit: Option<String>,
    created_by: Option<Uuid>,
    updated_by: Option<Uuid>,
    published_at: Option<DateTime<Utc>>,
    published_by: Option<Uuid>,
    publish_requested_at: Option<DateTime<Utc>>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<DocumentRow> for DocumentRecord {
    fn from(row: DocumentRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            card_id: row.card_id,
            parent_document_id: row.parent_document_id,
            kind: row.kind,
            title: row.title,
            status: row.status,
            object_key: row.object_key,
            content_hash: row.content_hash,
            body: row.body,
            external_ref: row.external_ref,
            meta: row.meta,
            publishable: row.publishable,
            category: row.category,
            unit: row.unit,
            created_by: row.created_by,
            updated_by: row.updated_by,
            published_at: row.published_at,
            published_by: row.published_by,
            publish_requested_at: row.publish_requested_at,
            version: crate::version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
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
    crate::parent::ensure_optional_parent_tx(
        &mut *conn,
        tenant,
        crate::parent::Parent::Card,
        input.card_id,
    )
    .await?;
    let sql = format!(
        "INSERT INTO document (id, tenant_id, project_id, card_id, parent_document_id, kind, title, \
                               status, object_key, content_hash, body, external_ref, meta, \
                               publishable, category, unit, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
         RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(input.id)
        .bind(tenant.as_uuid())
        .bind(input.project_id)
        .bind(input.card_id)
        .bind(input.parent_document_id)
        .bind(&input.kind)
        .bind(&input.title)
        .bind(&input.status)
        .bind(&input.object_key)
        .bind(&input.content_hash)
        .bind(&input.body)
        .bind(&input.external_ref)
        .bind(&input.meta)
        .bind(input.publishable)
        .bind(&input.category)
        .bind(&input.unit)
        .bind(input.created_by)
        .bind(input.updated_by)
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
        "SELECT {DOCUMENT_LIST_COLUMNS} FROM document
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR project_id = $2)
            AND ($3::uuid IS NULL OR card_id = $3)
            AND ($4::uuid IS NULL OR parent_document_id = $4)
            AND ($5::text IS NULL OR kind = $5)
            AND ($6::boolean IS NULL OR publishable = $6)
            AND ($7::text IS NULL OR category = $7)
            AND ($8::text IS NULL OR unit = $8)
            AND ($9::boolean IS NULL OR (publish_requested_at IS NOT NULL) = $9)
            AND ($10::timestamptz IS NULL OR (updated_at, id) < ($10::timestamptz, $11::uuid))
          ORDER BY updated_at DESC, id DESC
          LIMIT $12"
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
        .bind(filter.card_id)
        .bind(filter.parent_document_id)
        .bind(filter.kind.as_deref())
        .bind(filter.publishable)
        .bind(filter.category.as_deref())
        .bind(filter.unit.as_deref())
        .bind(filter.publish_requested)
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

/// Every published, active text output, oldest first.
///
/// This is the publish path's read: the filter is the publication decision (`publishable AND
/// status='active'`), and a body is required because an output page has something to render.
/// There is deliberately no page: a tenant's published output set is small, and a publish must
/// see all of it to build one complete index.
pub async fn list_published_outputs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<DocumentRecord>, DocumentStoreError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS} FROM document
          WHERE tenant_id = $1 AND publishable AND status = 'active' AND body IS NOT NULL
          ORDER BY created_at, id"
    );
    let rows: Vec<DocumentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
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
    apply_patch_tx(&mut *conn, tenant, &current, patch).await
}

/// Apply an edit **and** append its history, in one transaction.
///
/// A row created before 0042 has content but no revision. The first mutation writes the row as it
/// was (`pre-image`) before it writes the result (`updated`/`restored`), so history begins at the
/// last state the legacy row actually held instead of at the first edit.
pub async fn update_document_revisioned_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &DocumentPatch,
    revision_label: &str,
    revision_author: Option<Uuid>,
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    if !revision_exists_tx(&mut *conn, tenant, id, revision_seq(&current)).await? {
        write_document_revision_tx(&mut *conn, tenant, &current, "pre-image", None).await?;
    }
    let updated = apply_patch_tx(&mut *conn, tenant, &current, patch).await?;
    write_document_revision_tx(
        &mut *conn,
        tenant,
        &updated,
        revision_label,
        revision_author,
    )
    .await?;
    Ok(updated)
}

/// The update itself, against a row the caller already locked and version-checked.
async fn apply_patch_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    current: &DocumentRecord,
    patch: &DocumentPatch,
) -> Result<DocumentRecord, DocumentStoreError> {
    let title = patch.title.clone().unwrap_or(current.title.clone());
    let meta = patch.meta.clone().unwrap_or(current.meta.clone());
    let body = patch.body.clone().or(current.body.clone());
    // A hash is only meaningful beside the body it describes. A caller that changes the body
    // supplies the new hash; a caller that does not keeps the stored one.
    let content_hash = if patch.body.is_some() {
        patch.content_hash.clone()
    } else {
        current.content_hash.clone()
    };
    let card_id = patch.card_id.or(current.card_id);
    let publishable = patch.publishable.unwrap_or(current.publishable);
    let category = normalize_slot(patch.category.clone(), current.category.clone());
    let unit = normalize_slot(patch.unit.clone(), current.unit.clone());
    let updated_by = patch.updated_by.or(current.updated_by);
    let sql = format!(
        "UPDATE document
            SET title = $3, meta = $4, body = $5, content_hash = $6, card_id = $7,
                publishable = $8, category = $9, unit = $10, updated_by = $11,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(current.id)
        .bind(title)
        .bind(meta)
        .bind(body)
        .bind(content_hash)
        .bind(card_id)
        .bind(publishable)
        .bind(category)
        .bind(unit)
        .bind(updated_by)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// A promoted slot (`category`, `unit`): absent leaves the current value, an empty or blank string
/// clears it, anything else is trimmed and replaces it.
pub fn normalize_slot(patch: Option<String>, current: Option<String>) -> Option<String> {
    match patch {
        None => current,
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        }
    }
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

/// Clear the content of a tombstoned document — the `document.purge` job's write.
///
/// Clears the object pointer **and** the text body: a delete is a tombstone for either content
/// kind, and a purged row must not keep a body the bytes path has already lost.
///
/// Refuses a row that is not `deleted`: purging an active document would orphan live bytes.
pub async fn clear_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, DocumentStoreError> {
    let affected = sqlx::query(
        "UPDATE document
            SET object_key = NULL, body = NULL, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'deleted'
            AND (object_key IS NOT NULL OR body IS NOT NULL)",
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

/// `true` when `card_id` is this tenant's, and — when `project_id` is supplied — the card's board
/// belongs to that project.
///
/// A single-column FK enforces neither: `document.card_id` names a card row, and the card's board
/// is the only link to a project. The action layer calls this before it binds an output to a card.
pub async fn card_belongs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    project_id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
               FROM card c
               JOIN board b ON b.id = c.board_id AND b.tenant_id = c.tenant_id
              WHERE c.tenant_id = $1 AND c.id = $2
                AND ($3::uuid IS NULL OR b.project_id = $3)
         )",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(project_id)
    .fetch_one(&mut *conn)
    .await
}

/// The project a card belongs to, or `None` when the card is absent from this tenant.
///
/// `documents.compose` uses this to infer the owning project when the caller supplies a card but
/// no project: an output attached to a card belongs to that card's project.
pub async fn card_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT b.project_id
           FROM card c
           JOIN board b ON b.id = c.board_id AND b.tenant_id = c.tenant_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *conn)
    .await
}

// ── the publish workflow ─────────────────────────────────────────────────────────────────

/// Publish or unpublish one document, at the caller's version.
///
/// The history rows (`revision`) record content, not the public bit; the state change is an
/// outbox event plus these columns. Published rows therefore keep their content version.
pub async fn set_publishable_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    publish: bool,
    actor: Uuid,
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE document
            SET publishable = $3,
                published_at = CASE WHEN $3 THEN now() ELSE NULL END,
                published_by = CASE WHEN $3 THEN $4 ELSE NULL END,
                publish_requested_at = NULL,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(publish)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// An admin's rework order on a requested draft: clear the request and record the note and the
/// sections the standard found missing, in one UPDATE. The content version does not move — the
/// row is unchanged; the order is metadata about it.
pub async fn return_publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    actor: Uuid,
    note: &str,
    missing: &[String],
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE document
            SET publish_requested_at = NULL,
                meta = jsonb_set(
                    coalesce(meta, '{{}}'::jsonb),
                    '{{rework}}',
                    jsonb_build_object(
                        'note', $3::text,
                        'missing', $4::jsonb,
                        'by', $5::uuid,
                        'at', now(),
                        'of_version', $6::bigint
                    ),
                    true
                ),
                updated_by = $5,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(note)
        .bind(serde_json::to_value(missing).unwrap_or_else(|_| serde_json::json!([])))
        .bind(actor)
        .bind(i64::try_from(current.version.get()).unwrap_or(i64::MAX))
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Record an author's "ready for an admin" signal. No content version bump: the request is a flag,
/// not an edit, and a second request is harmless.
pub async fn request_publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    actor: Uuid,
) -> Result<DocumentRecord, DocumentStoreError> {
    let current = lock_document_tx(&mut *conn, tenant, id).await?;
    if current.version != expected_version {
        return Err(DocumentStoreError::VersionMismatch {
            current: current.version,
        });
    }
    let sql = format!(
        "UPDATE document
            SET publish_requested_at = now(), updated_by = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {DOCUMENT_COLUMNS}"
    );
    let row: DocumentRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

// ── attachments ──────────────────────────────────────────────────────────────────────────

/// Every active attachment of one output, oldest first.
pub async fn list_attachments_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent_document_id: Uuid,
) -> Result<Vec<DocumentRecord>, DocumentStoreError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS} FROM document
          WHERE tenant_id = $1 AND parent_document_id = $2 AND status = 'active'
          ORDER BY created_at, id"
    );
    let rows: Vec<DocumentRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(parent_document_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Every attachment whose parent is currently public — the pack builds one index from this.
pub async fn list_published_attachments_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<DocumentRecord>, DocumentStoreError> {
    let rows: Vec<DocumentRow> = sqlx::query_as(
        "SELECT a.*
           FROM document a
           JOIN document p
             ON p.id = a.parent_document_id AND p.tenant_id = a.tenant_id
          WHERE a.tenant_id = $1
            AND a.kind = 'attachment' AND a.status = 'active'
            AND p.status = 'active' AND p.publishable
          ORDER BY a.parent_document_id, a.created_at, a.id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One attachment that the public may serve: active, and its parent active and publishable.
pub async fn get_published_attachment_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DocumentRecord>, DocumentStoreError> {
    let row: Option<DocumentRow> = sqlx::query_as(
        "SELECT a.*
           FROM document a
           JOIN document p
             ON p.id = a.parent_document_id AND p.tenant_id = a.tenant_id
          WHERE a.tenant_id = $1 AND a.id = $2
            AND a.kind = 'attachment' AND a.status = 'active'
            AND p.status = 'active' AND p.publishable",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}

/// The display names for a set of `app_user` ids, for the author columns on an output.
pub async fn author_names_tx(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, String>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, display_name FROM app_user WHERE id = ANY($1)")
            .bind(ids.to_vec())
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.into_iter().collect())
}

// ── revisions ────────────────────────────────────────────────────────────────────────────

/// The `revision.entity_type` a document's history rows carry.
pub const DOCUMENT_ENTITY_TYPE: &str = "document";

/// One entry in a document's append-only history.
#[derive(Clone, Debug)]
pub struct DocumentRevisionRecord {
    /// The revision row id.
    pub id: Uuid,
    /// The document version this snapshot is the post-image of.
    pub seq: i32,
    /// `created`, `updated`, `pre-image`, `restored`, `deleted`.
    pub label: String,
    /// The complete post-image.
    pub content: Value,
    /// The SHA-256 of the canonical JSON.
    pub content_hash: String,
    /// Who produced the version, when recorded.
    pub author_id: Option<Uuid>,
    /// `draft | published | superseded`.
    pub status: String,
    /// When the version was written.
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RevisionRow {
    id: Uuid,
    seq: i32,
    label: String,
    content: Value,
    content_hash: String,
    author_id: Option<Uuid>,
    status: String,
    created_at: DateTime<Utc>,
}

impl From<RevisionRow> for DocumentRevisionRecord {
    fn from(row: RevisionRow) -> Self {
        Self {
            id: row.id,
            seq: row.seq,
            label: row.label,
            content: row.content,
            content_hash: row.content_hash,
            author_id: row.author_id,
            status: row.status,
            created_at: row.created_at,
        }
    }
}

/// The full post-image a document revision stores. Content only: `created_at`/`updated_at` move on
/// every write and would make an identical edit look different.
fn document_snapshot(record: &DocumentRecord) -> Value {
    json!({
        "id": record.id,
        "project_id": record.project_id,
        "card_id": record.card_id,
        "parent_document_id": record.parent_document_id,
        "kind": record.kind,
        "title": record.title,
        "status": record.status,
        "object_key": record.object_key,
        "content_hash": record.content_hash,
        "body": record.body,
        "meta": record.meta,
        "publishable": record.publishable,
        "category": record.category,
        "unit": record.unit,
        "version": record.version.get(),
    })
}

fn revision_seq(record: &DocumentRecord) -> i32 {
    i32::try_from(record.version.get()).unwrap_or(i32::MAX)
}

/// Append `record`'s current version to its history, through the workspace's one revision writer.
pub async fn write_document_revision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    record: &DocumentRecord,
    label: &str,
    author_id: Option<Uuid>,
) -> Result<Uuid, DocumentStoreError> {
    let content = document_snapshot(record);
    let content_hash = conveyor_domain::compare::content_hash(&content);
    let status = if record.publishable {
        RevisionStatus::Published
    } else {
        RevisionStatus::Draft
    };
    crate::governance::write_snapshot_revision(
        conn,
        tenant,
        &crate::governance::SnapshotRevision {
            entity_type: DOCUMENT_ENTITY_TYPE,
            entity_id: record.id,
            seq: revision_seq(record),
            label,
            content: &content,
            content_hash: &content_hash,
            author_id,
            status,
        },
    )
    .await
    .map_err(DocumentStoreError::Database)
}

/// Whether a version already has its history row — the legacy pre-image check.
pub async fn revision_exists_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    document_id: Uuid,
    seq: i32,
) -> Result<bool, DocumentStoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM revision
              WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3 AND seq = $4
         )",
    )
    .bind(tenant.as_uuid())
    .bind(DOCUMENT_ENTITY_TYPE)
    .bind(document_id)
    .bind(seq)
    .fetch_one(&mut *conn)
    .await
    .map_err(DocumentStoreError::Database)
}

/// Remove a purged document's history.
///
/// The tombstone keeps the row until the purge; from then on the content is gone, and the
/// revisions hold the same content. Leaving them behind would make delete a request that never
/// removes what the author wrote.
pub async fn delete_revisions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    document_id: Uuid,
) -> Result<u64, DocumentStoreError> {
    let affected = sqlx::query(
        "DELETE FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(DOCUMENT_ENTITY_TYPE)
    .bind(document_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(affected)
}

/// One document's history, newest version first.
pub async fn list_revisions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    document_id: Uuid,
    limit: i64,
) -> Result<Vec<DocumentRevisionRecord>, DocumentStoreError> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT id, seq, label, content, content_hash, author_id, status, created_at
           FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3
          ORDER BY seq DESC
          LIMIT $4",
    )
    .bind(tenant.as_uuid())
    .bind(DOCUMENT_ENTITY_TYPE)
    .bind(document_id)
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One version of one document's history.
pub async fn get_revision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    document_id: Uuid,
    seq: i32,
) -> Result<Option<DocumentRevisionRecord>, DocumentStoreError> {
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT id, seq, label, content, content_hash, author_id, status, created_at
           FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3 AND seq = $4",
    )
    .bind(tenant.as_uuid())
    .bind(DOCUMENT_ENTITY_TYPE)
    .bind(document_id)
    .bind(seq)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Into::into))
}
