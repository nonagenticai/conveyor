//! The governance model: `model`, `model_element`, `model_relationship`, and the one
//! snapshot/publish path (S08 Part B, data-arch §2.8).
//!
//! Every query is a `_tx` function: the caller has already opened and scoped a transaction
//! with [`crate::begin_scoped`], so the write and its outbox row commit together
//! (CONTRACT §7). This module writes **no** event — the action layer emits (S08 Part B
//! decision 1: one pattern, the store never writes an event).
//!
//! **Revisions are written at publish or import, not per mutation** (decision 3).
//! `revision.content` is capped at 256 KiB by the data architecture, and a full-model
//! snapshot per `view_objects.create` would be O(N²) and would serialise every writer on the
//! model. So every ordinary edit bumps `version` and nothing else; [`publish_model_tx`]
//! freezes the whole graph into a published revision, [`write_head_revision_tx`] freezes a
//! draft (the import path, which advances `head_revision` without publishing), and both
//! refuse — rather than truncate — a graph over the cap.
//!
//! **Every parent id is resolved in the caller's tenant before a write.** A single-column FK
//! does not enforce tenancy — referential-integrity checks bypass RLS — so a `tenant_id=A` row
//! could name tenant B's model and the FK would accept it (CONTRACT §2). [`crate::parent`] is
//! the one resolver; a miss is [`ModelStoreError::Parent`].
//!
//! `model_relationship.source_id` / `target_id` deliberately carry **no** foreign key: a
//! dangling end is exactly what `models.validate` reports, and an FK would make that check
//! unreachable. The store still resolves both ends in the caller's tenant, so a relationship
//! can never be written against another tenant's element.

use chrono::{DateTime, Utc};
use conveyor_domain::model::{
    ModelSnapshot, REVISION_CONTENT_MAX_BYTES, SnapshotConnection, SnapshotElement, SnapshotModel,
    SnapshotObject, SnapshotRelationship, SnapshotView,
};
use conveyor_domain::{RevisionStatus, TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::actions::{decode_time_cursor, encode_time_cursor};
use crate::governance::{SnapshotRevision, write_snapshot_revision};
use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};
use crate::version_of;

/// The `revision.entity_type` every model revision carries.
pub const MODEL_ENTITY_TYPE: &str = "model";

/// The two legal `model.language` values (0009's CHECK).
pub const LANGUAGES: [&str; 2] = ["archimate", "uml"];

/// True when `language` is a column value 0009 accepts.
pub fn is_language(language: &str) -> bool {
    LANGUAGES.contains(&language)
}

/// Why a model read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum ModelStoreError {
    /// No such row in this tenant.
    #[error("the model row does not exist in this tenant")]
    NotFound,
    /// The row moved; the caller's `If-Match` precondition is stale.
    #[error("the row is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: Version,
    },
    /// The page cursor is not one this module issued.
    #[error("the page cursor is malformed")]
    BadCursor,
    /// A `kind` outside a column's CHECKed vocabulary, refused as *input* before the
    /// statement runs — a constraint violation would reach the caller as a `500`.
    #[error("{field} {value:?} is not a value this column accepts")]
    KindRefused {
        /// The input field the caller must fix.
        field: &'static str,
        /// The value that was refused.
        value: String,
    },
    /// An element still has children; deleting it would orphan them.
    #[error("the element still has {count} child element(s)")]
    HasChildren {
        /// How many children the element still has.
        count: i64,
    },
    /// The frozen graph would exceed `revision.content`'s 256 KiB cap.
    #[error(
        "the model serialises to {bytes} bytes; revision.content is capped at \
         {REVISION_CONTENT_MAX_BYTES} and is never truncated"
    )]
    RevisionTooLarge {
        /// The canonical size of the snapshot that was refused.
        bytes: usize,
    },
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

// ── pagination ──────────────────────────────────────────────────────────────────────────

/// A page request: an opaque `(updated_at, id)` cursor and a limit.
#[derive(Clone, Debug, Default)]
pub struct ModelPage {
    /// The previous page's cursor, or `None` for the first page.
    pub cursor: Option<String>,
    /// The page size, already clamped by the caller.
    pub limit: i64,
}

impl ModelPage {
    /// A first page of `limit` rows with no cursor.
    pub fn first(limit: i64) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }
}

fn page_bindings(
    page: &ModelPage,
) -> Result<(Option<DateTime<Utc>>, Option<Uuid>), ModelStoreError> {
    match &page.cursor {
        None => Ok((None, None)),
        Some(raw) => {
            let (stamp, id) = decode_time_cursor(raw).ok_or(ModelStoreError::BadCursor)?;
            Ok((Some(stamp), Some(id)))
        }
    }
}

/// Take the page out of an over-fetched row list and build the next cursor.
///
/// The query fetches `limit + 1` rows: a full page plus one proves another page exists, so a
/// cursor is never issued for a page that turns out empty.
fn finish_page<T>(
    mut rows: Vec<T>,
    limit: i64,
    cursor_of: impl Fn(&T) -> String,
) -> (Vec<T>, Option<String>) {
    if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        let next = rows.last().map(&cursor_of);
        (rows, next)
    } else {
        (rows, None)
    }
}

// ── model ───────────────────────────────────────────────────────────────────────────────

/// One `model` row.
#[derive(Clone, Debug)]
pub struct ModelRecord {
    /// The model id.
    pub id: Uuid,
    /// The owning project, when it has one.
    pub project_id: Option<Uuid>,
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The display title.
    pub title: String,
    /// The description; `''` when unset.
    pub description: String,
    /// The last published revision's `seq`; `0` before the first publish.
    pub head_revision: i32,
    /// The revision `models.publish` froze last.
    pub published_revision_id: Option<Uuid>,
    /// Free-form metadata, read whole.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

impl ModelRecord {
    /// The `If-Match` value for this row: its version as a decimal string.
    pub fn etag(&self) -> String {
        self.version.get().to_string()
    }
}

/// A model to insert.
#[derive(Clone, Debug)]
pub struct NewModel {
    /// The owning project, resolved in the caller's tenant when set.
    pub project_id: Option<Uuid>,
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The display title.
    pub title: String,
    /// The description.
    pub description: String,
    /// Free-form metadata.
    pub meta: Value,
}

/// The editable fields of a model. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ModelPatch {
    /// The new title.
    pub title: Option<String>,
    /// The new description.
    pub description: Option<String>,
    /// Attach the model to a project. `None` leaves the column unchanged; there is no
    /// "detach" in v1 — a model that was attached stays attached.
    pub project_id: Option<Uuid>,
    /// Replace the metadata document.
    pub meta: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct ModelRow {
    id: Uuid,
    project_id: Option<Uuid>,
    language: String,
    language_version: String,
    title: String,
    description: String,
    head_revision: i32,
    published_revision_id: Option<Uuid>,
    meta: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ModelRow> for ModelRecord {
    fn from(row: ModelRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            language: row.language,
            language_version: row.language_version,
            title: row.title,
            description: row.description,
            head_revision: row.head_revision,
            published_revision_id: row.published_revision_id,
            meta: row.meta,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const MODEL_COLUMNS: &str = "id, project_id, language, language_version, title, description, \
                             head_revision, published_revision_id, meta, version, created_at, \
                             updated_at";

/// Insert a model inside the caller's transaction.
pub async fn create_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewModel,
) -> Result<ModelRecord, ModelStoreError> {
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, input.project_id).await?;
    let sql = format!(
        "INSERT INTO model (tenant_id, project_id, language, language_version, title, description,
                            meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {MODEL_COLUMNS}"
    );
    let row: ModelRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.project_id)
        .bind(&input.language)
        .bind(&input.language_version)
        .bind(&input.title)
        .bind(&input.description)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Read one model inside the caller's tenant; another tenant's id is `None`.
pub async fn get_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ModelRecord>, ModelStoreError> {
    let sql = format!("SELECT {MODEL_COLUMNS} FROM model WHERE tenant_id = $1 AND id = $2");
    let row: Option<ModelRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Lock and read one model inside the caller's tenant.
async fn lock_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ModelRow, ModelStoreError> {
    let sql =
        format!("SELECT {MODEL_COLUMNS} FROM model WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<ModelRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(ModelStoreError::NotFound)
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ModelPatch,
) -> Result<ModelRecord, ModelStoreError> {
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, patch.project_id).await?;
    let row = lock_model_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let sql = format!(
        "UPDATE model
            SET title = coalesce($3, title),
                description = coalesce($4, description),
                project_id = coalesce($5, project_id),
                meta = coalesce($6, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {MODEL_COLUMNS}"
    );
    let row: ModelRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.title)
        .bind(&patch.description)
        .bind(patch.project_id)
        .bind(&patch.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete a model and everything it owns, inside the caller's transaction.
///
/// One model is one aggregate: its views, objects, connections, relationships, elements,
/// validation runs and revisions all go with it. The order is the foreign-key order — children
/// before parents, then the header, then the revisions (which no row references once the
/// header is gone).
pub async fn delete_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ModelRecord, ModelStoreError> {
    let row = lock_model_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }

    for statement in [
        "DELETE FROM model_validation_run WHERE tenant_id = $1 AND model_id = $2",
        "DELETE FROM view_connection
           WHERE tenant_id = $1
             AND view_id IN (SELECT id FROM model_view WHERE tenant_id = $1 AND model_id = $2)",
        "DELETE FROM view_object
           WHERE tenant_id = $1
             AND view_id IN (SELECT id FROM model_view WHERE tenant_id = $1 AND model_id = $2)",
        "DELETE FROM model_view WHERE tenant_id = $1 AND model_id = $2",
        "DELETE FROM model_relationship WHERE tenant_id = $1 AND model_id = $2",
        "DELETE FROM model_element WHERE tenant_id = $1 AND model_id = $2",
        "DELETE FROM model WHERE tenant_id = $1 AND id = $2",
        "DELETE FROM revision WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2",
    ] {
        sqlx::query(statement)
            .bind(tenant.as_uuid())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(row.into())
}

/// List a tenant's models, newest first, optionally narrowed to one project.
pub async fn list_models_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    page: &ModelPage,
    project_id: Option<Uuid>,
) -> Result<(Vec<ModelRecord>, Option<String>), ModelStoreError> {
    let limit = page.limit.max(1);
    let (stamp, cursor_id) = page_bindings(page)?;
    let sql = format!(
        "SELECT {MODEL_COLUMNS} FROM model
          WHERE tenant_id = $1
            AND ($2::timestamptz IS NULL OR (updated_at, id) < ($2::timestamptz, $3::uuid))
            AND ($4::uuid IS NULL OR project_id = $4)
          ORDER BY updated_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<ModelRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(stamp)
        .bind(cursor_id)
        .bind(project_id)
        .bind(limit.saturating_add(1))
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = finish_page(rows, limit, |row| {
        encode_time_cursor(row.updated_at, row.id)
    });
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

// ── model_element ───────────────────────────────────────────────────────────────────────

/// One `model_element` row.
#[derive(Clone, Debug)]
pub struct ElementRecord {
    /// The element id.
    pub id: Uuid,
    /// The model it belongs to.
    pub model_id: Uuid,
    /// The profile kind.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The long description.
    pub documentation: String,
    /// The containment parent.
    pub parent_element_id: Option<Uuid>,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// An element to insert.
#[derive(Clone, Debug)]
pub struct NewElement {
    /// The model it belongs to, resolved in the caller's tenant.
    pub model_id: Uuid,
    /// The profile kind.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The long description.
    pub documentation: String,
    /// The containment parent, resolved in the caller's tenant.
    pub parent_element_id: Option<Uuid>,
    /// Free-form metadata.
    pub meta: Value,
}

/// The editable fields of an element. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ElementPatch {
    /// The new kind — re-validated against the profile by the action.
    pub kind: Option<String>,
    /// The new name.
    pub name: Option<String>,
    /// The new documentation.
    pub documentation: Option<String>,
    /// Re-parent the element.
    pub parent_element_id: Option<Uuid>,
    /// Replace the metadata document.
    pub meta: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct ElementRow {
    id: Uuid,
    model_id: Uuid,
    kind: String,
    name: String,
    documentation: String,
    parent_element_id: Option<Uuid>,
    meta: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ElementRow> for ElementRecord {
    fn from(row: ElementRow) -> Self {
        Self {
            id: row.id,
            model_id: row.model_id,
            kind: row.kind,
            name: row.name,
            documentation: row.documentation,
            parent_element_id: row.parent_element_id,
            meta: row.meta,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const ELEMENT_COLUMNS: &str = "id, model_id, kind, name, documentation, parent_element_id, meta, \
                               version, created_at, updated_at";

/// Insert an element inside the caller's transaction.
pub async fn create_element_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewElement,
) -> Result<ElementRecord, ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, input.model_id).await?;
    ensure_optional_parent_tx(
        &mut *conn,
        tenant,
        Parent::ModelElement,
        input.parent_element_id,
    )
    .await?;
    let sql = format!(
        "INSERT INTO model_element (tenant_id, model_id, kind, name, documentation,
                                    parent_element_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {ELEMENT_COLUMNS}"
    );
    let row: ElementRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.model_id)
        .bind(&input.kind)
        .bind(&input.name)
        .bind(&input.documentation)
        .bind(input.parent_element_id)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Read one element inside the caller's tenant.
pub async fn get_element_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ElementRecord>, ModelStoreError> {
    let sql =
        format!("SELECT {ELEMENT_COLUMNS} FROM model_element WHERE tenant_id = $1 AND id = $2");
    let row: Option<ElementRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_element_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ElementPatch,
) -> Result<ElementRecord, ModelStoreError> {
    ensure_optional_parent_tx(
        &mut *conn,
        tenant,
        Parent::ModelElement,
        patch.parent_element_id,
    )
    .await?;
    let sql = format!(
        "SELECT {ELEMENT_COLUMNS} FROM model_element WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ElementRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ModelStoreError::NotFound)?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    if let Some(parent_id) = patch.parent_element_id
        && parent_id == id
    {
        return Err(ModelStoreError::HasChildren { count: 0 });
    }
    let sql = format!(
        "UPDATE model_element
            SET kind = coalesce($3, kind),
                name = coalesce($4, name),
                documentation = coalesce($5, documentation),
                parent_element_id = coalesce($6, parent_element_id),
                meta = coalesce($7, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {ELEMENT_COLUMNS}"
    );
    let row: ElementRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.kind)
        .bind(&patch.name)
        .bind(&patch.documentation)
        .bind(patch.parent_element_id)
        .bind(&patch.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete an element, its drawing, and every relationship that names it.
///
/// Refused while the element still has children: promoting or orphaning them is a modelling
/// decision the caller has to make, not something a delete may do silently. Everything else
/// the element owns goes with it — the view objects that draw it (and their connectors) and
/// the relationships that name it at either end, none of which mean anything alone.
pub async fn delete_element_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ElementRecord, ModelStoreError> {
    let sql = format!(
        "SELECT {ELEMENT_COLUMNS} FROM model_element WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ElementRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ModelStoreError::NotFound)?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let children: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM model_element WHERE tenant_id = $1 AND parent_element_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if children > 0 {
        return Err(ModelStoreError::HasChildren { count: children });
    }
    for statement in [
        "DELETE FROM view_connection
           WHERE tenant_id = $1
             AND (source_object_id IN (SELECT id FROM view_object
                                        WHERE tenant_id = $1 AND element_id = $2)
               OR target_object_id IN (SELECT id FROM view_object
                                        WHERE tenant_id = $1 AND element_id = $2))",
        "DELETE FROM view_object WHERE tenant_id = $1 AND element_id = $2",
        "DELETE FROM model_relationship
           WHERE tenant_id = $1 AND (source_id = $2 OR target_id = $2)",
        "DELETE FROM model_element WHERE tenant_id = $1 AND id = $2",
    ] {
        sqlx::query(statement)
            .bind(tenant.as_uuid())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(row.into())
}

/// List a model's elements, ordered by id.
pub async fn list_elements_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
    kind: Option<&str>,
    page: &ModelPage,
) -> Result<(Vec<ElementRecord>, Option<String>), ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, model_id).await?;
    let limit = page.limit.max(1);
    let (stamp, cursor_id) = page_bindings(page)?;
    let sql = format!(
        "SELECT {ELEMENT_COLUMNS} FROM model_element
          WHERE tenant_id = $1
            AND model_id = $4
            AND ($2::timestamptz IS NULL OR (created_at, id) < ($2::timestamptz, $3::uuid))
            AND ($5::text IS NULL OR kind = $5)
          ORDER BY created_at DESC, id DESC
          LIMIT $6"
    );
    let rows: Vec<ElementRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(stamp)
        .bind(cursor_id)
        .bind(model_id)
        .bind(kind)
        .bind(limit.saturating_add(1))
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = finish_page(rows, limit, |row| {
        encode_time_cursor(row.created_at, row.id)
    });
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

// ── model_relationship ──────────────────────────────────────────────────────────────────

/// One `model_relationship` row.
#[derive(Clone, Debug)]
pub struct RelationshipRecord {
    /// The relationship id.
    pub id: Uuid,
    /// The model it belongs to.
    pub model_id: Uuid,
    /// The profile relationship kind.
    pub kind: String,
    /// The source element.
    pub source_id: Uuid,
    /// The target element.
    pub target_id: Uuid,
    /// Free-form metadata.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A relationship to insert.
#[derive(Clone, Debug)]
pub struct NewRelationship {
    /// The model it belongs to, resolved in the caller's tenant.
    pub model_id: Uuid,
    /// The profile relationship kind.
    pub kind: String,
    /// The source element, resolved in the caller's tenant.
    pub source_id: Uuid,
    /// The target element, resolved in the caller's tenant.
    pub target_id: Uuid,
    /// Free-form metadata.
    pub meta: Value,
}

/// The editable fields of a relationship. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct RelationshipPatch {
    /// The new kind — re-validated against the profile by the action.
    pub kind: Option<String>,
    /// Replace the metadata document.
    pub meta: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct RelationshipRow {
    id: Uuid,
    model_id: Uuid,
    kind: String,
    source_id: Uuid,
    target_id: Uuid,
    meta: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<RelationshipRow> for RelationshipRecord {
    fn from(row: RelationshipRow) -> Self {
        Self {
            id: row.id,
            model_id: row.model_id,
            kind: row.kind,
            source_id: row.source_id,
            target_id: row.target_id,
            meta: row.meta,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const RELATIONSHIP_COLUMNS: &str =
    "id, model_id, kind, source_id, target_id, meta, version, created_at, updated_at";

/// Insert a relationship inside the caller's transaction.
pub async fn create_relationship_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewRelationship,
) -> Result<RelationshipRecord, ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, input.model_id).await?;
    ensure_parent_tx(&mut *conn, tenant, Parent::ModelElement, input.source_id).await?;
    ensure_parent_tx(&mut *conn, tenant, Parent::ModelElement, input.target_id).await?;
    let sql = format!(
        "INSERT INTO model_relationship (tenant_id, model_id, kind, source_id, target_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {RELATIONSHIP_COLUMNS}"
    );
    let row: RelationshipRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.model_id)
        .bind(&input.kind)
        .bind(input.source_id)
        .bind(input.target_id)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Read one relationship inside the caller's tenant.
pub async fn get_relationship_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<RelationshipRecord>, ModelStoreError> {
    let sql = format!(
        "SELECT {RELATIONSHIP_COLUMNS} FROM model_relationship WHERE tenant_id = $1 AND id = $2"
    );
    let row: Option<RelationshipRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_relationship_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &RelationshipPatch,
) -> Result<RelationshipRecord, ModelStoreError> {
    let sql = format!(
        "SELECT {RELATIONSHIP_COLUMNS} FROM model_relationship
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<RelationshipRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ModelStoreError::NotFound)?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let sql = format!(
        "UPDATE model_relationship
            SET kind = coalesce($3, kind),
                meta = coalesce($4, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {RELATIONSHIP_COLUMNS}"
    );
    let row: RelationshipRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.kind)
        .bind(&patch.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete a relationship and every view connector that drew it.
pub async fn delete_relationship_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<RelationshipRecord, ModelStoreError> {
    let sql = format!(
        "SELECT {RELATIONSHIP_COLUMNS} FROM model_relationship
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<RelationshipRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ModelStoreError::NotFound)?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    sqlx::query("DELETE FROM view_connection WHERE tenant_id = $1 AND relationship_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM model_relationship WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(row.into())
}

/// List a model's relationships, ordered by id.
pub async fn list_relationships_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
    page: &ModelPage,
) -> Result<(Vec<RelationshipRecord>, Option<String>), ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, model_id).await?;
    let limit = page.limit.max(1);
    let (stamp, cursor_id) = page_bindings(page)?;
    let sql = format!(
        "SELECT {RELATIONSHIP_COLUMNS} FROM model_relationship
          WHERE tenant_id = $1
            AND model_id = $4
            AND ($2::timestamptz IS NULL OR (created_at, id) < ($2::timestamptz, $3::uuid))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<RelationshipRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(stamp)
        .bind(cursor_id)
        .bind(model_id)
        .bind(limit.saturating_add(1))
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = finish_page(rows, limit, |row| {
        encode_time_cursor(row.created_at, row.id)
    });
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

// ── the snapshot and publish ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct SnapshotElementRow {
    id: Uuid,
    kind: String,
    name: String,
    documentation: String,
    parent_element_id: Option<Uuid>,
    meta: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotRelationshipRow {
    id: Uuid,
    kind: String,
    source_id: Uuid,
    target_id: Uuid,
    meta: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotViewRow {
    id: Uuid,
    kind: String,
    name: String,
    meta: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotObjectRow {
    id: Uuid,
    view_id: Uuid,
    element_id: Option<Uuid>,
    parent_object_id: Option<Uuid>,
    kind: String,
    bounds: Value,
    style: Value,
    label: Option<String>,
    position: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct SnapshotConnectionRow {
    id: Uuid,
    view_id: Uuid,
    relationship_id: Option<Uuid>,
    source_object_id: Uuid,
    target_object_id: Uuid,
    bendpoints: Value,
    style: Value,
}

/// Read a model's whole graph, normalised and ready to hash or validate.
///
/// The caller must already be scoped to `tenant`; every query binds it as well.
pub async fn load_snapshot_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
) -> Result<ModelSnapshot, ModelStoreError> {
    let model = get_model_tx(&mut *conn, tenant, model_id)
        .await?
        .ok_or(ModelStoreError::NotFound)?;

    let elements: Vec<SnapshotElementRow> = sqlx::query_as(
        "SELECT id, kind, name, documentation, parent_element_id, meta
           FROM model_element WHERE tenant_id = $1 AND model_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .fetch_all(&mut *conn)
    .await?;

    let relationships: Vec<SnapshotRelationshipRow> = sqlx::query_as(
        "SELECT id, kind, source_id, target_id, meta
           FROM model_relationship WHERE tenant_id = $1 AND model_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .fetch_all(&mut *conn)
    .await?;

    let views: Vec<SnapshotViewRow> = sqlx::query_as(
        "SELECT id, kind, name, meta FROM model_view WHERE tenant_id = $1 AND model_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .fetch_all(&mut *conn)
    .await?;

    let objects: Vec<SnapshotObjectRow> = sqlx::query_as(
        "SELECT o.id, o.view_id, o.element_id, o.parent_object_id, o.kind, o.bounds, o.style,
                o.label, o.position::float8 AS position
           FROM view_object o
           JOIN model_view v ON v.id = o.view_id AND v.tenant_id = o.tenant_id
          WHERE o.tenant_id = $1 AND v.model_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .fetch_all(&mut *conn)
    .await?;

    let connections: Vec<SnapshotConnectionRow> = sqlx::query_as(
        "SELECT c.id, c.view_id, c.relationship_id, c.source_object_id, c.target_object_id,
                c.bendpoints, c.style
           FROM view_connection c
           JOIN model_view v ON v.id = c.view_id AND v.tenant_id = c.tenant_id
          WHERE c.tenant_id = $1 AND v.model_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut snapshot = ModelSnapshot {
        model: SnapshotModel {
            id: model.id,
            language: model.language,
            language_version: model.language_version,
            title: model.title,
            description: model.description,
            meta: model.meta,
        },
        elements: elements
            .into_iter()
            .map(|row| SnapshotElement {
                id: row.id,
                kind: row.kind,
                name: row.name,
                documentation: row.documentation,
                parent_element_id: row.parent_element_id,
                meta: row.meta,
            })
            .collect(),
        relationships: relationships
            .into_iter()
            .map(|row| SnapshotRelationship {
                id: row.id,
                kind: row.kind,
                source_id: row.source_id,
                target_id: row.target_id,
                meta: row.meta,
            })
            .collect(),
        views: views
            .into_iter()
            .map(|row| SnapshotView {
                id: row.id,
                kind: row.kind,
                name: row.name,
                meta: row.meta,
                objects: objects
                    .iter()
                    .filter(|object| object.view_id == row.id)
                    .map(|object| SnapshotObject {
                        id: object.id,
                        element_id: object.element_id,
                        parent_object_id: object.parent_object_id,
                        kind: object.kind.clone(),
                        bounds: object.bounds.clone(),
                        style: object.style.clone(),
                        label: object.label.clone(),
                        position: object.position,
                    })
                    .collect(),
                connections: connections
                    .iter()
                    .filter(|connection| connection.view_id == row.id)
                    .map(|connection| SnapshotConnection {
                        id: connection.id,
                        relationship_id: connection.relationship_id,
                        source_object_id: connection.source_object_id,
                        target_object_id: connection.target_object_id,
                        bendpoints: connection.bendpoints.clone(),
                        style: connection.style.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };
    snapshot.normalize();
    Ok(snapshot)
}

/// What a publish produced.
#[derive(Clone, Debug)]
pub struct PublishOutcome {
    /// The updated model header, at its new version.
    pub model: ModelRecord,
    /// The revision that was frozen.
    pub revision_id: Uuid,
    /// The revision's `seq` (= `head_revision`).
    pub seq: i32,
    /// `revision.content_hash` — the same value `models.validate` dedupes on.
    pub content_hash: String,
}

/// Freeze the model's graph into a new revision and make it the published one.
///
/// One transaction: lock the header, snapshot the graph, refuse an over-cap document, write
/// the revision, supersede the previous published revision, and point the header at the new
/// one. A concurrent publish of the same version is a [`ModelStoreError::VersionMismatch`]
/// and writes nothing.
pub async fn publish_model_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    author_id: Option<Uuid>,
) -> Result<PublishOutcome, ModelStoreError> {
    freeze_tx(
        conn,
        tenant,
        id,
        expected_version,
        author_id,
        RevisionStatus::Published,
    )
    .await
}

/// Freeze the model's graph into a new DRAFT revision and advance `head_revision`.
///
/// This is the import path's writer: an XMI import produces a new version of the graph that
/// has never been published, so it must not supersede the published revision or move
/// `published_revision_id` — publishing here would make `models.compare` claim a review that
/// never happened. `seq` is still `head_revision + 1`, so the next publish cannot collide.
/// A stale `expected_version` is a [`ModelStoreError::VersionMismatch`] and writes nothing.
pub async fn write_head_revision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    author_id: Option<Uuid>,
) -> Result<PublishOutcome, ModelStoreError> {
    freeze_tx(
        conn,
        tenant,
        id,
        expected_version,
        author_id,
        RevisionStatus::Draft,
    )
    .await
}

/// The one freeze path: snapshot the graph, hash it, write the revision at
/// `head_revision + 1`, then advance the header.
///
/// A `published` freeze additionally supersedes the previous published revision and points
/// `published_revision_id` at the new one; a `draft` freeze leaves the previous published
/// revision, its status and the header's pointer untouched.
async fn freeze_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    author_id: Option<Uuid>,
    status: RevisionStatus,
) -> Result<PublishOutcome, ModelStoreError> {
    let row = lock_model_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let snapshot = load_snapshot_tx(&mut *conn, tenant, id).await?;
    let bytes = snapshot.content_len();
    if bytes > REVISION_CONTENT_MAX_BYTES {
        return Err(ModelStoreError::RevisionTooLarge { bytes });
    }
    let content = snapshot.content_value();
    let content_hash = snapshot.content_hash();
    let seq = row.head_revision.saturating_add(1);

    let revision_id = write_snapshot_revision(
        &mut *conn,
        tenant,
        &SnapshotRevision {
            entity_type: MODEL_ENTITY_TYPE,
            entity_id: id,
            seq,
            label: status.as_str(),
            content: &content,
            content_hash: &content_hash,
            author_id,
            status,
        },
    )
    .await?;

    let published_revision_id = if status == RevisionStatus::Published {
        if let Some(previous) = row.published_revision_id {
            sqlx::query(
                "UPDATE revision SET status = 'superseded'
                  WHERE tenant_id = $1 AND id = $2 AND status = 'published'",
            )
            .bind(tenant.as_uuid())
            .bind(previous)
            .execute(&mut *conn)
            .await?;
        }
        Some(revision_id)
    } else {
        row.published_revision_id
    };

    let sql = format!(
        "UPDATE model
            SET head_revision = $3,
                published_revision_id = $4,
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {MODEL_COLUMNS}"
    );
    let updated: ModelRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(seq)
        .bind(published_revision_id)
        .fetch_one(&mut *conn)
        .await?;

    Ok(PublishOutcome {
        model: updated.into(),
        revision_id,
        seq,
        content_hash,
    })
}

/// Read one revision's stored content, inside the caller's tenant.
pub async fn revision_content_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
    revision_id: Uuid,
) -> Result<Option<Value>, ModelStoreError> {
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT content FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2 AND id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(model_id)
    .bind(revision_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(content,)| content))
}

/// Enqueue a `model.validate` job on the caller's transaction.
///
/// The statement is `store::quality::enqueue_quality_score_tx`'s, on the caller's connection:
/// the job and the graph hash it was computed from commit together, and a rolled-back action
/// leaves no job behind (CONTRACT §7). `None` means a live job with the same key already
/// covers this exact graph, so the caller reports "queued" without a new row.
pub async fn enqueue_validate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
    content_hash: &str,
) -> Result<Option<Uuid>, ModelStoreError> {
    let payload = serde_json::json!({
        "model_id": model_id,
        "content_hash": content_hash,
    });
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, 'model.validate', $2, $3, 3)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("model:{model_id}:{content_hash}"))
    .bind(payload)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Outgoing and incoming `trace_link` counts for a set of model elements.
///
/// One query over `unnest`, so a 500-element model costs one round trip rather than 500. The
/// counts feed `models.traceability`; nothing here writes.
pub async fn trace_counts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    element_ids: &[Uuid],
) -> Result<Vec<(Uuid, i64, i64)>, ModelStoreError> {
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "SELECT u.id,
                (SELECT count(*) FROM trace_link t
                  WHERE t.tenant_id = $1 AND t.from_type = 'model_element' AND t.from_id = u.id),
                (SELECT count(*) FROM trace_link t
                  WHERE t.tenant_id = $1 AND t.to_type = 'model_element' AND t.to_id = u.id)
           FROM unnest($2::uuid[]) AS u(id)",
    )
    .bind(tenant.as_uuid())
    .bind(element_ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}
