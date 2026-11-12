//! `workflow`'s rows: create/fork, read, list, edit, publish and retire.

use chrono::{DateTime, Utc};
use conveyor_domain::compare::content_hash as canonical_hash;
use conveyor_domain::{RevisionStatus, TenantId, Version};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::governance::{SnapshotRevision, write_snapshot_revision};
use crate::version_of;

use super::{WORKFLOW_ENTITY_TYPE, WorkflowStoreError, ensure_definition_fits, map_write_error};

/// The columns every read returns, in the order [`WorkflowRecord`] names them.
const WORKFLOW_COLUMNS: &str = "id, key, title, status, version, definition, published_revision_id, \
                                created_at, updated_at";

/// A workflow row as stored. `version` is the row's own counter: a fork copies the key at
/// `max(version) + 1`, an edit increments it, and publish freezes a revision at that number.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct WorkflowRecord {
    /// The row id.
    pub id: Uuid,
    /// The stable name (`cap.x`); not unique on its own — `(tenant, key, version)` is.
    pub key: String,
    /// Display title.
    pub title: String,
    /// `draft` | `published` | `retired`.
    pub status: String,
    /// The dense version counter.
    pub version: i64,
    /// The whole graph document.
    pub definition: Value,
    /// The frozen revision of the last publish, when there was one.
    pub published_revision_id: Option<Uuid>,
    /// Row creation time — the list's sort key.
    pub created_at: DateTime<Utc>,
    /// Last mutation.
    pub updated_at: DateTime<Utc>,
}

/// A create request. `definition` defaults to the empty graph the table declares.
#[derive(Clone, Debug)]
pub struct NewWorkflow {
    /// The stable name.
    pub key: String,
    /// Display title.
    pub title: String,
    /// The graph document; `None` uses `{"schema":1,"nodes":[],"edges":[]}`.
    pub definition: Option<Value>,
}

/// An edit request. Every field is optional; at least one must be present (the action checks).
#[derive(Clone, Debug)]
pub struct WorkflowPatch<'a> {
    /// `If-Match` — the version the caller read.
    pub expected: Option<Version>,
    /// A new title.
    pub title: Option<&'a str>,
    /// A replacement document (never merged — the document is the unit).
    pub definition: Option<&'a Value>,
}

/// What a publish returned: the row, the frozen revision and the hash a verifier recomputes.
#[derive(Clone, Debug)]
pub struct PublishOutcome {
    /// The now-published row.
    pub workflow: WorkflowRecord,
    /// The `revision` row this publish froze.
    pub revision_id: Uuid,
    /// The revision's `seq` — the workflow's version at publish time.
    pub seq: i32,
    /// The SHA-256 of the document's canonical JSON.
    pub content_hash: String,
}

/// Insert the next version of `key` as a draft: v1 for a new key, `max(version) + 1` for a fork.
///
/// The subquery is not racy in the way it looks: two concurrent inserts for the same key compute
/// the same version, and `workflow_key_version_key` refuses one of them as
/// [`WorkflowStoreError::Conflict`] — the caller retries and gets the next number.
pub async fn create_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: NewWorkflow,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let definition = input
        .definition
        .unwrap_or_else(|| json!({"schema": 1, "nodes": [], "edges": []}));
    ensure_definition_fits(&definition)?;
    let sql = format!(
        "INSERT INTO workflow (tenant_id, key, title, version, definition)
         VALUES ($1, $2, $3,
                 (SELECT coalesce(max(version), 0) + 1
                    FROM workflow WHERE tenant_id = $1 AND key = $2),
                 $4)
         RETURNING {WORKFLOW_COLUMNS}"
    );
    let row: WorkflowRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.key.trim())
        .bind(input.title.trim())
        .bind(&definition)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}

/// Read one row by id, inside the tenant.
pub async fn get_by_id_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let sql = format!("SELECT {WORKFLOW_COLUMNS} FROM workflow WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WorkflowStoreError::NotFound)
}

/// Read one row by key, at `version` or — when it is `None` — at the highest version.
pub async fn get_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: Option<i64>,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let sql = match version {
        Some(_) => format!(
            "SELECT {WORKFLOW_COLUMNS} FROM workflow
              WHERE tenant_id = $1 AND key = $2 AND version = $3"
        ),
        None => format!(
            "SELECT {WORKFLOW_COLUMNS} FROM workflow
              WHERE tenant_id = $1 AND key = $2
              ORDER BY version DESC LIMIT 1"
        ),
    };
    let mut query = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key);
    if let Some(version) = version {
        query = query.bind(version);
    }
    query
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WorkflowStoreError::NotFound)
}

/// The one **published** row for a key.
///
/// `get_by_key_tx` answers "highest version", which after an edit is a draft. A trigger must
/// start what the product ships, so it resolves through this instead; the partial unique index
/// `(tenant_id, key) WHERE status = 'published'` makes the answer single.
pub async fn get_published_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let sql = format!(
        "SELECT {WORKFLOW_COLUMNS} FROM workflow
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WorkflowStoreError::NotFound)
}

/// Every published workflow key in the tenant — a process `call` step may target one (the
/// workflow engine is retained as the tool-call-sequence executor, owner 2026-11-09).
pub async fn published_keys_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT key FROM workflow WHERE tenant_id = $1 AND status = 'published'")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.into_iter().map(|(key,)| key).collect())
}

/// The page a list call returns: newest first, keyset-paged by `(created_at, id)`.
///
/// The clamp allows one row above the page ceiling on purpose: a caller asks for `limit + 1`
/// rows to learn whether another page exists, and the extra row is truncated before it is
/// returned. A clamp of exactly the page ceiling would hide the last page boundary.
pub async fn list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    before: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<WorkflowRecord>, WorkflowStoreError> {
    let (before_at, before_id) = match before {
        Some((at, id)) => (Some(at), Some(id)),
        None => (None, None),
    };
    let sql = format!(
        "SELECT {WORKFLOW_COLUMNS} FROM workflow
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR status = $2)
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3::timestamptz, $4::uuid))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<WorkflowRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(before_at)
        .bind(before_id)
        .bind(limit.clamp(1, 201))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// Edit a draft. An edit increments `version`, so `If-Match` has a counter that moves on write.
pub async fn update_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: WorkflowPatch<'_>,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let current = lock_tx(&mut *conn, tenant, id).await?;
    if let Some(expected) = patch.expected
        && version_of(current.version) != expected
    {
        return Err(WorkflowStoreError::VersionMismatch {
            current: version_of(current.version),
        });
    }
    if current.status != "draft" {
        return Err(WorkflowStoreError::NotDraft(current.status));
    }
    if let Some(definition) = patch.definition {
        ensure_definition_fits(definition)?;
    }
    let sql = format!(
        "UPDATE workflow
            SET title      = coalesce($3, title),
                definition = coalesce($4, definition),
                version    = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {WORKFLOW_COLUMNS}"
    );
    let row: WorkflowRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.title.map(str::to_owned))
        .bind(patch.definition.cloned())
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}

/// Publish a draft: retire the key's current published row, freeze this document as a revision,
/// then flip this row to `published`.
///
/// The order is load-bearing: `uq_workflow_published` is an immediate unique index, so the previous
/// row must leave `published` before this one enters it. The previous row's revision is marked
/// `superseded` — the same vocabulary [`crate::model`] uses — because a verifier reading two
/// `published` revisions for one entity would otherwise not know which one won.
pub async fn publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: Option<Version>,
    author_id: Option<Uuid>,
) -> Result<PublishOutcome, WorkflowStoreError> {
    let current = lock_tx(&mut *conn, tenant, id).await?;
    if let Some(expected) = expected
        && version_of(current.version) != expected
    {
        return Err(WorkflowStoreError::VersionMismatch {
            current: version_of(current.version),
        });
    }
    if current.status != "draft" {
        return Err(WorkflowStoreError::NotDraft(current.status));
    }
    ensure_definition_fits(&current.definition)?;

    let previous_revision: Option<Option<Uuid>> = sqlx::query_scalar(
        "UPDATE workflow
            SET status = 'retired', updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'
          RETURNING published_revision_id",
    )
    .bind(tenant.as_uuid())
    .bind(&current.key)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(Some(revision_id)) = previous_revision {
        sqlx::query(
            "UPDATE revision SET status = 'superseded'
              WHERE tenant_id = $1 AND id = $2 AND status = 'published'",
        )
        .bind(tenant.as_uuid())
        .bind(revision_id)
        .execute(&mut *conn)
        .await?;
    }

    let content_hash = canonical_hash(&current.definition);
    let seq = current.version.clamp(1, i64::from(i32::MAX)) as i32;
    let revision_id = write_snapshot_revision(
        &mut *conn,
        tenant,
        &SnapshotRevision {
            entity_type: WORKFLOW_ENTITY_TYPE,
            entity_id: id,
            seq,
            label: "published",
            content: &current.definition,
            content_hash: &content_hash,
            author_id,
            status: RevisionStatus::Published,
        },
    )
    .await?;

    let sql = format!(
        "UPDATE workflow
            SET status = 'published',
                published_revision_id = $3,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {WORKFLOW_COLUMNS}"
    );
    let row: WorkflowRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(revision_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;

    Ok(PublishOutcome {
        workflow: row,
        revision_id,
        seq,
        content_hash,
    })
}

/// Retire a published row. A draft is not retired — it is edited or replaced.
pub async fn retire_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: Option<Version>,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let current = lock_tx(&mut *conn, tenant, id).await?;
    if let Some(expected) = expected
        && version_of(current.version) != expected
    {
        return Err(WorkflowStoreError::VersionMismatch {
            current: version_of(current.version),
        });
    }
    if current.status != "published" {
        return Err(WorkflowStoreError::NotPublished);
    }
    let sql = format!(
        "UPDATE workflow
            SET status = 'retired', updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {WORKFLOW_COLUMNS}"
    );
    let row: WorkflowRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_write_error)?;
    Ok(row)
}

/// Read a row under `FOR UPDATE`; every mutating path starts here.
async fn lock_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<WorkflowRecord, WorkflowStoreError> {
    let sql = format!(
        "SELECT {WORKFLOW_COLUMNS} FROM workflow
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(WorkflowStoreError::NotFound)
}
