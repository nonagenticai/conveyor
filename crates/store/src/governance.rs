//! Governed assets and the revision row written with every transition.
//!
//! **`revision.seq` equals `governed_asset.version`, dense from 1** (S3-D4): the insert
//! writes revision 1, and every transition writes the revision for the version it just
//! produced, in the same transaction. A transition without its revision would make "revision
//! 7" mean two different things.
//!
//! S07 §5.3 makes every mutation write one immutable revision whose `content` is the
//! **post-mutation snapshot** of the whole asset and whose `content_hash` is the SHA-256 of
//! that document's canonical JSON. [`snapshot`] is the one producer of that document and
//! `write_revision` is the one writer: create, update and transition all route through it,
//! with labels `created`, `updated` and `status:<to>`.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::compare::content_hash;
use conveyor_domain::{
    AssetKind, AssetStatus, GovernanceError, GovernedAsset, RevisionStatus, TenantId, Version,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};
use crate::project::{decode_cursor, encode_cursor};
use crate::version_of;

/// The `revision.entity_type` every governed-asset mutation writes (S07 §5.3).
pub const GOVERNED_ASSET_ENTITY_TYPE: &str = "governed_asset";

/// A governed asset to create.
#[derive(Clone, Debug)]
pub struct NewAsset {
    /// Requirement, principle or ADR.
    pub kind: AssetKind,
    /// The stable key within the kind.
    pub key: String,
    /// The title.
    pub title: String,
    /// The body; empty by default.
    pub body: String,
    /// The owning project.
    pub project_id: Option<Uuid>,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
    /// The delivery phase.
    pub phase_code: Option<String>,
    /// Acceptance criteria; an array by default.
    pub acceptance: Value,
    /// Links to other entities; an object by default.
    pub links: Value,
    /// The review lens; an object by default.
    pub lens: Value,
    /// Kind-specific fields.
    pub meta: Value,
    /// The user whose revision this first version is.
    pub author_id: Option<Uuid>,
}

impl NewAsset {
    /// An asset with empty payloads.
    pub fn new(kind: AssetKind, key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            title: title.into(),
            body: String::new(),
            project_id: None,
            owner_user_id: None,
            phase_code: None,
            acceptance: Value::Array(Vec::new()),
            links: Value::Object(serde_json::Map::new()),
            lens: Value::Object(serde_json::Map::new()),
            meta: Value::Object(serde_json::Map::new()),
            author_id: None,
        }
    }
}

/// Why a governance operation failed.
#[derive(Debug, thiserror::Error)]
pub enum GovernanceStoreError {
    /// No such asset in this tenant.
    #[error("the governed asset does not exist in this tenant")]
    NotFound,
    /// The asset moved; the caller's `If-Match` precondition is stale.
    #[error("the asset is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: Version,
    },
    /// The page cursor is not one this module issued.
    #[error("the page cursor is malformed")]
    BadCursor,
    /// The domain refused the transition.
    #[error(transparent)]
    Illegal(#[from] GovernanceError),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// A referenced parent (`project_id`, `owner_user_id`, `author_id`) does not exist in
    /// this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct AssetRow {
    id: Uuid,
    kind: String,
    key: String,
    title: String,
    status: String,
    supersedes_id: Option<Uuid>,
    superseded_by_id: Option<Uuid>,
    version: i64,
}

impl TryFrom<AssetRow> for GovernedAsset {
    type Error = GovernanceStoreError;
    fn try_from(row: AssetRow) -> Result<Self, Self::Error> {
        let kind =
            AssetKind::from_str(&row.kind).map_err(|_| GovernanceStoreError::Vocabulary {
                what: "governed_asset.kind",
                value: row.kind,
            })?;
        let status =
            AssetStatus::from_str(&row.status).map_err(|_| GovernanceStoreError::Vocabulary {
                what: "governed_asset.status",
                value: row.status,
            })?;
        Ok(Self {
            id: row.id,
            kind,
            key: row.key,
            title: row.title,
            status,
            supersedes_id: row.supersedes_id,
            superseded_by_id: row.superseded_by_id,
            version: version_of(row.version),
        })
    }
}

const COLUMNS: &str = "id, kind, key, title, status, supersedes_id, superseded_by_id, version";

fn revision_status(status: AssetStatus) -> RevisionStatus {
    match status {
        AssetStatus::Draft | AssetStatus::InReview => RevisionStatus::Draft,
        AssetStatus::Approved => RevisionStatus::Published,
        AssetStatus::Superseded | AssetStatus::Archived => RevisionStatus::Superseded,
    }
}

/// Insert a governed asset and its revision 1 inside the caller's scoped transaction.
///
/// The revision is the same full snapshot [`update_asset_tx`] writes (S07 §5.3): entity type
/// `governed_asset`, `seq = version`, `label = "created"`, content = [`snapshot`] and the
/// SHA-256 of that document's canonical JSON.
pub async fn insert_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewAsset,
) -> Result<GovernedAsset, GovernanceStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.owner_user_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.author_id).await?;
    let sql = format!(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, body, project_id,
                                     owner_user_id, phase_code, acceptance, links, lens, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING {RECORD_COLUMNS}"
    );
    let row: AssetRecordRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.kind.as_str())
        .bind(&input.key)
        .bind(&input.title)
        .bind(&input.body)
        .bind(input.project_id)
        .bind(input.owner_user_id)
        .bind(&input.phase_code)
        .bind(&input.acceptance)
        .bind(&input.links)
        .bind(&input.lens)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    let record = GovernedAssetRecord::try_from(row)?;
    write_revision(conn, tenant, &record, "created", input.author_id).await?;
    Ok(record.into())
}

/// Apply a status transition and write its revision, inside the caller's transaction.
///
/// The row is locked before the domain sees it, so the version the domain bumps is the
/// version the `UPDATE` guards on; a concurrent transition waits and then re-reads. The
/// revision is the same full snapshot as the other mutations, labelled `status:<to>`.
pub async fn transition_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    asset_id: Uuid,
    to: AssetStatus,
    author_id: Option<Uuid>,
) -> Result<GovernedAsset, GovernanceStoreError> {
    let (asset, _prior) =
        transition_asset_with_prior_tx(conn, tenant, asset_id, to, author_id).await?;
    Ok(asset)
}

/// [`transition_asset_tx`], also returning the asset **as the lock found it**.
///
/// The emitter needs the prior status for its `from` and the prior document for `prev_data`.
/// Reading either before this call is a race: a concurrent transition can commit in between
/// and the event would then describe a state the row never held at that moment. What this
/// returns is the row the `UPDATE` actually left behind.
pub async fn transition_asset_with_prior_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    asset_id: Uuid,
    to: AssetStatus,
    author_id: Option<Uuid>,
) -> Result<(GovernedAsset, GovernedAssetRecord), GovernanceStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, author_id).await?;
    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM governed_asset WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .fetch_optional(&mut *conn)
        .await?;
    let prior = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;
    let asset: GovernedAsset = prior.clone().into();
    let from = prior.status;
    let next = asset.transition(to)?;

    let sql = format!(
        "UPDATE governed_asset
            SET status = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = $4
         RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .bind(next.status.as_str())
        .bind(from.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    let record = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;
    // The transition changed the status; the re-read row carries the new version, which is
    // the seq the revision must have.
    let label = format!("status:{}", record.status.as_str());
    write_revision(conn, tenant, &record, &label, author_id).await?;
    Ok((record.into(), prior))
}

/// Read one governed asset inside the caller's scoped transaction.
pub async fn get_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<GovernedAsset>, GovernanceStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM governed_asset WHERE tenant_id = $1 AND id = $2");
    let row: Option<AssetRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(GovernedAsset::try_from).transpose()
}

// ── the full record, the snapshot and the edit path (S07 §5.3) ──────────────────────────

/// The full `governed_asset` row, authored jsonb payloads and timestamps included.
///
/// [`GovernedAsset`] is the domain aggregate (identity, kind and the status/version pair a
/// transition needs); this is the representation `requirements.get` returns and the input
/// [`snapshot`] hashes.
#[derive(Clone, Debug, PartialEq)]
pub struct GovernedAssetRecord {
    /// The row id.
    pub id: Uuid,
    /// Requirement, principle or ADR.
    pub kind: AssetKind,
    /// The stable key within the kind; unique per tenant.
    pub key: String,
    /// The title.
    pub title: String,
    /// The body.
    pub body: String,
    /// The asset's status.
    pub status: AssetStatus,
    /// The optimistic-concurrency version; also the latest revision's `seq`.
    pub version: Version,
    /// The asset this one replaces.
    pub supersedes_id: Option<Uuid>,
    /// The asset that replaced this one.
    pub superseded_by_id: Option<Uuid>,
    /// The owning project.
    pub project_id: Option<Uuid>,
    /// The accountable user.
    pub owner_user_id: Option<Uuid>,
    /// The delivery phase code.
    pub phase_code: Option<String>,
    /// Acceptance criteria.
    pub acceptance: Value,
    /// Links to other entities.
    pub links: Value,
    /// The review lens.
    pub lens: Value,
    /// Kind-specific fields.
    pub meta: Value,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

// `Clone` because the supersession locks BOTH rows in one statement and needs each one twice:
// once for the domain pair (which consumes them) and once for the prior representation the
// emitter reports.
#[derive(Clone, Debug, sqlx::FromRow)]
struct AssetRecordRow {
    id: Uuid,
    kind: String,
    key: String,
    title: String,
    body: String,
    status: String,
    version: i64,
    supersedes_id: Option<Uuid>,
    superseded_by_id: Option<Uuid>,
    project_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
    phase_code: Option<String>,
    acceptance: Value,
    links: Value,
    lens: Value,
    meta: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AssetRecordRow> for GovernedAssetRecord {
    type Error = GovernanceStoreError;
    fn try_from(row: AssetRecordRow) -> Result<Self, Self::Error> {
        let kind =
            AssetKind::from_str(&row.kind).map_err(|_| GovernanceStoreError::Vocabulary {
                what: "governed_asset.kind",
                value: row.kind,
            })?;
        let status =
            AssetStatus::from_str(&row.status).map_err(|_| GovernanceStoreError::Vocabulary {
                what: "governed_asset.status",
                value: row.status,
            })?;
        Ok(Self {
            id: row.id,
            kind,
            key: row.key,
            title: row.title,
            body: row.body,
            status,
            version: version_of(row.version),
            supersedes_id: row.supersedes_id,
            superseded_by_id: row.superseded_by_id,
            project_id: row.project_id,
            owner_user_id: row.owner_user_id,
            phase_code: row.phase_code,
            acceptance: row.acceptance,
            links: row.links,
            lens: row.lens,
            meta: row.meta,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl From<GovernedAssetRecord> for GovernedAsset {
    /// The slim aggregate a transition needs, from the full row the store read.
    fn from(record: GovernedAssetRecord) -> Self {
        Self {
            id: record.id,
            kind: record.kind,
            key: record.key,
            title: record.title,
            status: record.status,
            supersedes_id: record.supersedes_id,
            superseded_by_id: record.superseded_by_id,
            version: record.version,
        }
    }
}

const RECORD_COLUMNS: &str = "id, kind, key, title, body, status, version, supersedes_id, \
                              superseded_by_id, project_id, owner_user_id, phase_code, \
                              acceptance, links, lens, meta, created_at, updated_at";

/// The revision `content` document for one asset (S07 §5.3).
///
/// The key list is fixed and deliberately excludes the timestamps (they are not part of the
/// authored representation, and a diff of two revisions must not report `updated_at` as a
/// change) and the two supersession pointers (the chain is a status transition, recorded in
/// its own revision). It is:
///
/// `id, kind, key, title, body, status, version, project_id, owner_user_id, phase_code,
/// acceptance, links, lens, meta`
///
/// Every writer of a governed-asset revision hashes **this** function's output; there is one
/// snapshot definition in the workspace.
pub fn snapshot(record: &GovernedAssetRecord) -> Value {
    json!({
        "id": record.id,
        "kind": record.kind.as_str(),
        "key": record.key.clone(),
        "title": record.title.clone(),
        "body": record.body.clone(),
        "status": record.status.as_str(),
        "version": record.version.get(),
        "project_id": record.project_id,
        "owner_user_id": record.owner_user_id,
        "phase_code": record.phase_code.clone(),
        "acceptance": record.acceptance.clone(),
        "links": record.links.clone(),
        "lens": record.lens.clone(),
        "meta": record.meta.clone(),
    })
}

/// The editable fields of a governed asset. `None` leaves the column unchanged.
///
/// Status and the supersession pointers are deliberately absent: a status change is
/// [`transition_asset_tx`], which the domain state machine guards, and a supersession writes
/// both edges at once. An edit cannot reach either.
#[derive(Clone, Debug, Default)]
pub struct AssetPatch {
    /// The new key within the kind.
    pub key: Option<String>,
    /// The new title.
    pub title: Option<String>,
    /// The new body.
    pub body: Option<String>,
    /// The new owning project.
    pub project_id: Option<Uuid>,
    /// The new accountable user.
    pub owner_user_id: Option<Uuid>,
    /// The new delivery phase code.
    pub phase_code: Option<String>,
    /// The new acceptance criteria (a whole-array replacement).
    pub acceptance: Option<Value>,
    /// The new links object (a whole-object replacement).
    pub links: Option<Value>,
    /// The new review lens (a whole-object replacement).
    pub lens: Option<Value>,
    /// The new kind-specific fields (a whole-object replacement).
    pub meta: Option<Value>,
}

/// A governed-asset page request.
///
/// `limit` is clamped by the caller (the action layer uses
/// [`crate::actions::clamp_limit`]); `cursor` is the opaque value returned by the previous
/// page and is never interpreted by the client.
#[derive(Clone, Debug, Default)]
pub struct PageRequest {
    /// The previous page's cursor, or `None` for the first page.
    pub cursor: Option<String>,
    /// The page size.
    pub limit: i64,
}

impl PageRequest {
    /// A first page of `limit` rows with no cursor.
    pub fn new(limit: i64) -> Self {
        Self {
            cursor: None,
            limit,
        }
    }
}

/// The filters a governed-asset list accepts. `None` does not filter that column.
#[derive(Clone, Debug, Default)]
pub struct AssetFilter {
    /// Only assets of this kind.
    pub kind: Option<AssetKind>,
    /// Only assets in this status.
    pub status: Option<AssetStatus>,
    /// Only assets of this project.
    pub project_id: Option<Uuid>,
}

/// A revision to insert: the full post-mutation snapshot and its hash.
///
/// [`write_snapshot_revision`] is the ONE revision writer, so this is the one input shape
/// every entity family uses — governed assets, models, and any future family.
pub struct SnapshotRevision<'a> {
    /// The entity family (`governed_asset`, `model`, …).
    pub entity_type: &'a str,
    /// The entity's id.
    pub entity_id: Uuid,
    /// The dense sequence within the entity.
    pub seq: i32,
    /// A display label (`created`, `updated`, `published`, …).
    pub label: &'a str,
    /// The complete snapshot document.
    pub content: &'a Value,
    /// The SHA-256 of `content`'s canonical JSON.
    pub content_hash: &'a str,
    /// The author, when one is recorded.
    pub author_id: Option<Uuid>,
    /// The revision's status.
    pub status: RevisionStatus,
}

/// Insert a revision whose content and hash the caller already computed — THE revision
/// writer.
///
/// Every revision in this workspace goes through this one INSERT. `content` is the full
/// post-mutation snapshot and `content_hash` is the SHA-256 of that document's canonical
/// JSON, computed in Rust by [`conveyor_domain::compare::content_hash`] so a verifier never
/// needs a second implementation (S07 §5.3). There is deliberately no md5 variant: one hash
/// convention, so a stored hash and a freshly computed one always compare.
///
/// The caller opens the tenant-scoped transaction; the revision and the mutation it records
/// commit together.
pub async fn write_snapshot_revision(
    conn: &mut PgConnection,
    tenant: TenantId,
    revision: &SnapshotRevision<'_>,
) -> Result<Uuid, sqlx::Error> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO revision (tenant_id, entity_type, entity_id, seq, label, content,
                               content_hash, author_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(revision.entity_type)
    .bind(revision.entity_id)
    .bind(revision.seq)
    .bind(revision.label)
    .bind(revision.content)
    .bind(revision.content_hash)
    .bind(revision.author_id)
    .bind(revision.status.as_str())
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Write the revision for `record`'s current version, in the caller's transaction.
///
/// The one revision writer for every governed-asset mutation (S07 §5.3): `seq` is the version
/// the mutation just produced, `content` is [`snapshot`], `content_hash` is the SHA-256 of
/// that document's canonical JSON, and `label` is `created`, `updated` or `status:<to>`.
async fn write_revision(
    conn: &mut PgConnection,
    tenant: TenantId,
    record: &GovernedAssetRecord,
    label: &str,
    author_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let content = snapshot(record);
    let hash = content_hash(&content);
    write_snapshot_revision(
        conn,
        tenant,
        &SnapshotRevision {
            entity_type: GOVERNED_ASSET_ENTITY_TYPE,
            entity_id: record.id,
            seq: i32::try_from(record.version.get()).unwrap_or(i32::MAX),
            label,
            content: &content,
            content_hash: &hash,
            author_id,
            status: revision_status(record.status),
        },
    )
    .await?;
    Ok(())
}

/// Apply an edit and write its revision, inside the caller's transaction.
///
/// One transaction: lock the row (`FOR UPDATE`), compare `expected_version`, apply only the
/// patch's `Some` fields, bump the version and `updated_at`, then append the revision whose
/// `seq` is the new version, whose `content` is [`snapshot`] and whose `content_hash` is the
/// SHA-256 of that document's canonical JSON. A stale `expected_version` is
/// [`GovernanceStoreError::VersionMismatch`] and writes nothing.
pub async fn update_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &AssetPatch,
    author_id: Option<Uuid>,
) -> Result<GovernedAssetRecord, GovernanceStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::Project, patch.project_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, patch.owner_user_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, author_id).await?;

    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM governed_asset
          WHERE tenant_id = $1 AND id = $2
            FOR UPDATE"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let current = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;
    if current.version != expected_version {
        return Err(GovernanceStoreError::VersionMismatch {
            current: current.version,
        });
    }

    // `COALESCE($n, column)` is exactly "leave the column alone when the patch is `None`";
    // the row is already locked, so the read above and this write cannot interleave.
    let sql = format!(
        "UPDATE governed_asset
            SET key = COALESCE($3, key),
                title = COALESCE($4, title),
                body = COALESCE($5, body),
                project_id = COALESCE($6, project_id),
                owner_user_id = COALESCE($7, owner_user_id),
                phase_code = COALESCE($8, phase_code),
                acceptance = COALESCE($9, acceptance),
                links = COALESCE($10, links),
                lens = COALESCE($11, lens),
                meta = COALESCE($12, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.key.as_deref())
        .bind(patch.title.as_deref())
        .bind(patch.body.as_deref())
        .bind(patch.project_id)
        .bind(patch.owner_user_id)
        .bind(patch.phase_code.as_deref())
        .bind(patch.acceptance.as_ref())
        .bind(patch.links.as_ref())
        .bind(patch.lens.as_ref())
        .bind(patch.meta.as_ref())
        .fetch_optional(&mut *conn)
        .await?;
    let record = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;
    write_revision(conn, tenant, &record, "updated", author_id).await?;
    Ok(record)
}

/// One keyset page of governed assets, newest `updated_at` first.
///
/// The cursor is `(updated_at, id)` DESC, the order of
/// `ix governed_asset (tenant_id, kind, status, updated_at DESC, id)`. The tuple comparison
/// keeps the page stable when two rows share a timestamp. `limit + 1` rows are fetched so a
/// full page can say whether another one exists.
pub async fn list_assets_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &AssetFilter,
    page: &PageRequest,
) -> Result<(Vec<GovernedAssetRecord>, Option<String>), GovernanceStoreError> {
    let limit = page.limit.max(1);
    let cursor = match page.cursor.as_deref() {
        None => None,
        Some(raw) => {
            let (at, id) = decode_cursor(raw).ok_or(GovernanceStoreError::BadCursor)?;
            let at = DateTime::parse_from_rfc3339(&at)
                .map_err(|_| GovernanceStoreError::BadCursor)?
                .with_timezone(&Utc);
            Some((at, id))
        }
    };

    let mut conditions = vec!["tenant_id = $1".to_owned()];
    let mut binds = vec![Bind::Uuid(tenant.as_uuid())];
    if let Some(kind) = filter.kind {
        conditions.push(format!("kind = ${}", binds.len() + 1));
        binds.push(Bind::Text(kind.as_str().to_owned()));
    }
    if let Some(status) = filter.status {
        conditions.push(format!("status = ${}", binds.len() + 1));
        binds.push(Bind::Text(status.as_str().to_owned()));
    }
    if let Some(project_id) = filter.project_id {
        conditions.push(format!("project_id = ${}", binds.len() + 1));
        binds.push(Bind::Uuid(project_id));
    }
    if let Some((at, id)) = cursor {
        let at_index = binds.len() + 1;
        let id_index = at_index + 1;
        conditions.push(format!("(updated_at, id) < (${at_index}, ${id_index})"));
        binds.push(Bind::Timestamp(at));
        binds.push(Bind::Uuid(id));
    }
    let limit_index = binds.len() + 1;
    binds.push(Bind::Int(limit.saturating_add(1)));

    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM governed_asset WHERE {} \
         ORDER BY updated_at DESC, id DESC LIMIT ${limit_index}",
        conditions.join(" AND ")
    );
    let mut query = sqlx::query_as::<_, AssetRecordRow>(sqlx::AssertSqlSafe(sql.as_str()));
    for bind in &binds {
        query = match bind {
            Bind::Text(value) => query.bind(value.clone()),
            Bind::Uuid(value) => query.bind(*value),
            Bind::Timestamp(value) => query.bind(*value),
            Bind::Int(value) => query.bind(*value),
        };
    }
    let mut rows: Vec<AssetRecordRow> = query.fetch_all(&mut *conn).await?;

    let next = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| encode_cursor(&row.updated_at.to_rfc3339(), row.id))
    } else {
        None
    };
    let records = rows
        .into_iter()
        .map(GovernedAssetRecord::try_from)
        .collect::<Result<Vec<_>, GovernanceStoreError>>()?;
    Ok((records, next))
}

/// Replace an approved asset with its successor: both edges, both versions and one revision
/// per row, in the caller's transaction.
///
/// Both rows are locked by ONE `ORDER BY id … FOR UPDATE` statement, so two concurrent
/// supersessions over the same pair cannot take the locks in opposite orders and deadlock.
/// The pair is then handed to [`GovernedAsset::supersede`], which is the only place the rules
/// live: the outgoing asset must be `approved`, it cannot supersede itself, the successor must
/// be live (`draft`/`in_review`/`approved`) and must not already supersede a third asset.
///
/// The two `UPDATE`s and the two revisions are the same transaction as the lock, so the
/// immediate CHECK (`status <> 'superseded' OR superseded_by_id IS NOT NULL`) never sees a
/// half-written pair. `version = version + 1` is read back through `RETURNING`, so each
/// revision's `seq` is the version the database actually wrote. The labels are
/// `status:superseded` for the outgoing row and `updated` for the successor.
pub async fn supersede_asset_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    successor_id: Uuid,
    author_id: Option<Uuid>,
) -> Result<
    (
        GovernedAssetRecord,
        GovernedAssetRecord,
        GovernedAssetRecord,
    ),
    GovernanceStoreError,
> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, author_id).await?;

    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM governed_asset
          WHERE tenant_id = $1 AND id = ANY($2)
          ORDER BY id
            FOR UPDATE"
    );
    let rows: Vec<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&[id, successor_id][..])
        .fetch_all(&mut *conn)
        .await?;
    // When `id == successor_id` the single row fills both slots and the domain refuses the
    // pair with `SelfSupersede`, which is exactly the error the caller should see.
    let prior_outgoing = rows
        .iter()
        .find(|row| row.id == id)
        .cloned()
        .map(GovernedAssetRecord::try_from)
        .transpose()?
        .ok_or(GovernanceStoreError::NotFound)?;
    let prior_successor = rows
        .iter()
        .find(|row| row.id == successor_id)
        .cloned()
        .map(GovernedAssetRecord::try_from)
        .transpose()?
        .ok_or(GovernanceStoreError::NotFound)?;
    let (outgoing, successor) = GovernedAsset::from(prior_outgoing.clone())
        .supersede(GovernedAsset::from(prior_successor))?;

    let sql = format!(
        "UPDATE governed_asset
            SET status = $3, superseded_by_id = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(outgoing.status.as_str())
        .bind(outgoing.superseded_by_id)
        .fetch_optional(&mut *conn)
        .await?;
    let outgoing = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;

    let sql = format!(
        "UPDATE governed_asset
            SET supersedes_id = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {RECORD_COLUMNS}"
    );
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(successor_id)
        .bind(successor.supersedes_id)
        .fetch_optional(&mut *conn)
        .await?;
    let successor = GovernedAssetRecord::try_from(row.ok_or(GovernanceStoreError::NotFound)?)?;

    write_revision(conn, tenant, &outgoing, "status:superseded", author_id).await?;
    write_revision(conn, tenant, &successor, "updated", author_id).await?;
    Ok((outgoing, successor, prior_outgoing))
}

// ── the full-record read and the revision metadata read (S07 §5.3) ───────────────────────

/// Read one governed asset as its FULL representation, inside the caller's transaction.
///
/// [`get_asset_tx`] returns the slim domain aggregate a transition needs; this returns every
/// authored column plus the timestamps, which is what `requirements.get` returns, what a `412`
/// hands back and what `update` reads before it applies a patch. A row in another tenant
/// resolves to `None` (RLS plus the explicit tenant predicate).
pub async fn get_asset_record_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<GovernedAssetRecord>, GovernanceStoreError> {
    let sql =
        format!("SELECT {RECORD_COLUMNS} FROM governed_asset WHERE tenant_id = $1 AND id = $2");
    let row: Option<AssetRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(GovernedAssetRecord::try_from).transpose()
}

/// One revision's display metadata — everything a caller lists a history with, including the
/// `created_at` the domain [`conveyor_domain::Revision`] does not carry.
#[derive(Clone, Debug, PartialEq)]
pub struct RevisionMetaRecord {
    /// The revision sequence; equal to the asset's version at that point.
    pub seq: i32,
    /// `created | updated | status:<to>`.
    pub label: String,
    /// The revision's status.
    pub status: RevisionStatus,
    /// The author of the mutation that wrote it.
    pub author_id: Option<Uuid>,
    /// SHA-256 of the JCS-canonical snapshot.
    pub content_hash: String,
    /// When the revision was written.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct RevisionMetaRow {
    seq: i32,
    label: String,
    status: String,
    author_id: Option<Uuid>,
    content_hash: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<RevisionMetaRow> for RevisionMetaRecord {
    type Error = GovernanceStoreError;
    fn try_from(row: RevisionMetaRow) -> Result<Self, Self::Error> {
        let status = RevisionStatus::from_str(&row.status).map_err(|_| {
            GovernanceStoreError::Vocabulary {
                what: "revision.status",
                value: row.status,
            }
        })?;
        Ok(Self {
            seq: row.seq,
            label: row.label,
            status,
            author_id: row.author_id,
            content_hash: row.content_hash,
            created_at: row.created_at,
        })
    }
}

/// The newest `limit` revisions of one entity, newest first, with their timestamps.
///
/// [`crate::revision::list_revisions_tx`] is the unbounded history (the domain [`Revision`]
/// has no `created_at`); this is the bounded, representable read `get` returns, capped by the
/// caller so a history that grows without bound cannot become a response that does.
pub async fn list_revision_meta_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entity_type: &str,
    entity_id: Uuid,
    limit: i64,
) -> Result<Vec<RevisionMetaRecord>, GovernanceStoreError> {
    let rows: Vec<RevisionMetaRow> = sqlx::query_as(
        "SELECT seq, label, status, author_id, content_hash, created_at
           FROM revision
          WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3
          ORDER BY seq DESC
          LIMIT $4",
    )
    .bind(tenant.as_uuid())
    .bind(entity_type)
    .bind(entity_id)
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter().map(RevisionMetaRecord::try_from).collect()
}

enum Bind {
    Text(String),
    Uuid(Uuid),
    Timestamp(DateTime<Utc>),
    Int(i64),
}
