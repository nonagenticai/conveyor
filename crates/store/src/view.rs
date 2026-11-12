//! Model views: `model_view`, `view_object` and `view_connection` (S08 Part B, data-arch §2.8).
//!
//! A model is one aggregate and a view is its drawing surface: a `model_view` belongs to one
//! model, a `view_object` places an element (or another object) on that drawing, and a
//! `view_connection` joins two objects, optionally naming the `model_relationship` it draws.
//! `bounds`, `style` and `bendpoints` are jsonb and are read whole with the view (§5): nothing
//! here filters on a jsonb path; hit-testing and culling parse the rect in Rust (trap 3), and a
//! measured hot predicate is what would turn `bounds` into four generated numeric columns.
//!
//! Every query is a `_tx` function: the caller has already opened and scoped a transaction
//! with [`crate::begin_scoped`], so the write and its outbox row commit together (CONTRACT §7).
//! This module writes **no** event — the action layer emits (S08 Part B decision 1: one
//! pattern, the store never writes an event).
//!
//! **Every parent id is resolved in the caller's tenant before a write** (CONTRACT §2). A
//! single-column FK does not enforce tenancy — referential-integrity checks bypass RLS — so a
//! `tenant_id = A` row could name tenant B's view and the FK would accept it. [`crate::parent`]
//! is the one resolver; a miss is [`ModelStoreError::Parent`].
//!
//! `view_connection.source_object_id` / `target_object_id` deliberately carry **no** foreign
//! key (0009): a dangling connector is exactly what `models.validate` reports, and an FK would
//! make that finding unreachable. The store therefore resolves both ends itself — and requires
//! both to be drawn on the connection's own `view_id`, a rule no constraint can express.
//!
//! **A view's `kind` is a closed vocabulary** — `diagram`, `archimate_view`, `uml_diagram`
//! (0009's CHECK) — and it is refused in Rust before the statement runs, so caller input never
//! arrives as a scrubbed constraint violation. `ModelStoreError` has no dedicated "value
//! refused" variant and is matched exhaustively by `conveyor_actions::model::model_error`,
//! which this slice does not own, so the kind check borrows
//! [`ModelStoreError::KindRefused`], which that layer maps to a `422 invalid_value` naming the
//! `kind` field.
//! The views action slice should give it its own variant and a `kind` field in the problem
//! document.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::actions::{decode_time_cursor, encode_time_cursor};
use crate::model::{ModelPage, ModelStoreError};
use crate::parent::{Parent, ensure_optional_parent_tx, ensure_parent_tx};
use crate::version_of;

/// The three legal `model_view.kind` values (0009's CHECK).
pub const VIEW_KINDS: [&str; 3] = ["diagram", "archimate_view", "uml_diagram"];

/// True when `kind` is a column value 0009 accepts.
pub fn is_view_kind(kind: &str) -> bool {
    VIEW_KINDS.contains(&kind)
}

/// Refuse a `kind` outside the three before the statement runs.
///
/// The CHECK would refuse it too — as a `23514` the action layer scrubs to a `500`. The kind
/// is caller input, so it must be refused as input, not as a database failure.
fn ensure_view_kind(kind: &str) -> Result<(), ModelStoreError> {
    if is_view_kind(kind) {
        Ok(())
    } else {
        Err(ModelStoreError::KindRefused {
            field: "kind",
            value: kind.to_owned(),
        })
    }
}

// ── pagination ──────────────────────────────────────────────────────────────────────────

/// Decode the page's `(created_at, id)` cursor.
///
/// The encoder is [`crate::actions`]'s, so the cursor is the same opaque value every list in
/// this crate issues; a malformed one is a refusal, never a silent first page. This pair is
/// kept local for the same reason [`crate::portfolio`] keeps its own: `model`'s copy is
/// private to that module.
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

// ── model_view ──────────────────────────────────────────────────────────────────────────

/// One `model_view` row.
#[derive(Clone, Debug)]
pub struct ViewRecord {
    /// The view id.
    pub id: Uuid,
    /// The model the view belongs to.
    pub model_id: Uuid,
    /// `diagram`, `archimate_view` or `uml_diagram`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// Free-form metadata, read whole.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

impl ViewRecord {
    /// The `If-Match` value for this row: its version as a decimal string.
    pub fn etag(&self) -> String {
        self.version.get().to_string()
    }
}

/// A view to insert.
#[derive(Clone, Debug)]
pub struct NewView {
    /// The model the view belongs to, resolved in the caller's tenant.
    pub model_id: Uuid,
    /// `diagram`, `archimate_view` or `uml_diagram`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// Free-form metadata.
    pub meta: Value,
}

/// The editable fields of a view. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ViewPatch {
    /// The new kind — re-checked against the three-value vocabulary here.
    pub kind: Option<String>,
    /// The new name.
    pub name: Option<String>,
    /// Replace the metadata document.
    pub meta: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct ViewRow {
    id: Uuid,
    model_id: Uuid,
    kind: String,
    name: String,
    meta: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ViewRow> for ViewRecord {
    fn from(row: ViewRow) -> Self {
        Self {
            id: row.id,
            model_id: row.model_id,
            kind: row.kind,
            name: row.name,
            meta: row.meta,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const VIEW_COLUMNS: &str = "id, model_id, kind, name, meta, version, created_at, updated_at";

/// Insert a view inside the caller's transaction.
///
/// The model is resolved in the caller's tenant ([`Parent::Model`]) before the insert, and the
/// kind is checked before either — a bad value costs no query.
pub async fn create_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewView,
) -> Result<ViewRecord, ModelStoreError> {
    ensure_view_kind(&input.kind)?;
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, input.model_id).await?;
    let sql = format!(
        "INSERT INTO model_view (tenant_id, model_id, kind, name, meta)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {VIEW_COLUMNS}"
    );
    let row: ViewRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.model_id)
        .bind(&input.kind)
        .bind(&input.name)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Read one view inside the caller's tenant; another tenant's id is `None`.
pub async fn get_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ViewRecord>, ModelStoreError> {
    let sql = format!("SELECT {VIEW_COLUMNS} FROM model_view WHERE tenant_id = $1 AND id = $2");
    let row: Option<ViewRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Lock and read one view inside the caller's tenant.
async fn lock_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ViewRow, ModelStoreError> {
    let sql = format!(
        "SELECT {VIEW_COLUMNS} FROM model_view WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ViewRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(ModelStoreError::NotFound)
}

/// Apply an edit guarded by `expected_version`, or write nothing.
///
/// The row is locked before its version is compared, so two concurrent edits of the same
/// version cannot both pass; the loser gets [`ModelStoreError::VersionMismatch`] and has
/// written nothing.
pub async fn update_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ViewPatch,
) -> Result<ViewRecord, ModelStoreError> {
    if let Some(kind) = &patch.kind {
        ensure_view_kind(kind)?;
    }
    let row = lock_view_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let sql = format!(
        "UPDATE model_view
            SET kind = coalesce($3, kind),
                name = coalesce($4, name),
                meta = coalesce($5, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {VIEW_COLUMNS}"
    );
    let row: ViewRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.kind)
        .bind(&patch.name)
        .bind(&patch.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete a view and everything drawn on it, inside the caller's transaction.
///
/// One view is one drawing: its connectors and objects mean nothing without the header, so
/// they go with it — connectors first, then objects, then the view (0009's foreign-key order;
/// an object's `parent_object_id` points inside the same set, which one statement deletes
/// together).
pub async fn delete_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ViewRecord, ModelStoreError> {
    let row = lock_view_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    for statement in [
        "DELETE FROM view_connection WHERE tenant_id = $1 AND view_id = $2",
        "DELETE FROM view_object WHERE tenant_id = $1 AND view_id = $2",
        "DELETE FROM model_view WHERE tenant_id = $1 AND id = $2",
    ] {
        sqlx::query(statement)
            .bind(tenant.as_uuid())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(row.into())
}

/// List a model's views, newest first.
///
/// The page is `(created_at, id)` DESC: a view's creation order is stable, while `updated_at`
/// moves under every edit and would reshuffle a reader's pages mid-walk.
pub async fn list_views_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
    page: &ModelPage,
) -> Result<(Vec<ViewRecord>, Option<String>), ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::Model, model_id).await?;
    let limit = page.limit.max(1);
    let (stamp, cursor_id) = page_bindings(page)?;
    let sql = format!(
        "SELECT {VIEW_COLUMNS} FROM model_view
          WHERE tenant_id = $1
            AND model_id = $2
            AND ($3::timestamptz IS NULL OR (created_at, id) < ($3::timestamptz, $4::uuid))
          ORDER BY created_at DESC, id DESC
          LIMIT $5"
    );
    let rows: Vec<ViewRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(model_id)
        .bind(stamp)
        .bind(cursor_id)
        .bind(limit.saturating_add(1))
        .fetch_all(&mut *conn)
        .await?;
    let (rows, next) = finish_page(rows, limit, |row| {
        encode_time_cursor(row.created_at, row.id)
    });
    Ok((rows.into_iter().map(Into::into).collect(), next))
}

// ── view_object ─────────────────────────────────────────────────────────────────────────

/// One `view_object` row.
#[derive(Clone, Debug)]
pub struct ObjectRecord {
    /// The object id.
    pub id: Uuid,
    /// The view it is drawn on.
    pub view_id: Uuid,
    /// The element it draws, when it draws one.
    pub element_id: Option<Uuid>,
    /// The object it is nested inside, when it is nested.
    pub parent_object_id: Option<Uuid>,
    /// The object kind — `element` for a drawn element, free text otherwise.
    pub kind: String,
    /// The rect, read whole (never filtered on).
    pub bounds: Value,
    /// The presentation, read whole.
    pub style: Value,
    /// The caption; `None` when the object carries none.
    pub label: Option<String>,
    /// The drawing order. `numeric` in the column, carried as `f64` like the snapshot does:
    /// it is a stacking index, not an ordering key a midpoint edit has to keep exact.
    pub position: f64,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A view object to insert.
#[derive(Clone, Debug)]
pub struct NewObject {
    /// The view it is drawn on, resolved in the caller's tenant.
    pub view_id: Uuid,
    /// The element it draws, resolved in the caller's tenant when set.
    pub element_id: Option<Uuid>,
    /// The object it is nested inside, resolved in the caller's tenant when set.
    pub parent_object_id: Option<Uuid>,
    /// The object kind.
    pub kind: String,
    /// The rect, read whole.
    pub bounds: Value,
    /// The presentation, read whole.
    pub style: Value,
    /// The caption.
    pub label: Option<String>,
    /// The drawing order.
    pub position: f64,
}

/// The editable fields of a view object. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ObjectPatch {
    /// Replace the rect.
    pub bounds: Option<Value>,
    /// Replace the presentation.
    pub style: Option<Value>,
    /// Replace the caption. `None` means "leave it", so a caller that wants to clear the
    /// label writes the empty string (the column is nullable, but an absent patch field is
    /// "unchanged" on every patch in this crate).
    pub label: Option<String>,
    /// The new drawing order.
    pub position: Option<f64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ObjectRow {
    id: Uuid,
    view_id: Uuid,
    element_id: Option<Uuid>,
    parent_object_id: Option<Uuid>,
    kind: String,
    bounds: Value,
    style: Value,
    label: Option<String>,
    position: f64,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ObjectRow> for ObjectRecord {
    fn from(row: ObjectRow) -> Self {
        Self {
            id: row.id,
            view_id: row.view_id,
            element_id: row.element_id,
            parent_object_id: row.parent_object_id,
            kind: row.kind,
            bounds: row.bounds,
            style: row.style,
            label: row.label,
            position: row.position,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// `position` is `numeric` in 0009 and sqlx has no `f64` decoder for `NUMERIC`, so the read
/// states the cast; `RETURNING` accepts the same expression.
const OBJECT_COLUMNS: &str = "id, view_id, element_id, parent_object_id, kind, bounds, style, \
                              label, position::float8 AS position, version, created_at, \
                              updated_at";

/// Insert a view object inside the caller's transaction.
///
/// `element_id` is optional — a view may hold a frame or a note that no model element backs —
/// and `parent_object_id` nests the object inside another object. Both are resolved in the
/// caller's tenant first; the FK would otherwise accept another tenant's id (CONTRACT §2).
pub async fn create_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewObject,
) -> Result<ObjectRecord, ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::ModelView, input.view_id).await?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::ModelElement, input.element_id).await?;
    ensure_optional_parent_tx(
        &mut *conn,
        tenant,
        Parent::ViewObject,
        input.parent_object_id,
    )
    .await?;
    let sql = format!(
        "INSERT INTO view_object (tenant_id, view_id, element_id, parent_object_id, kind,
                                  bounds, style, label, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::float8::numeric)
         RETURNING {OBJECT_COLUMNS}"
    );
    let row: ObjectRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.view_id)
        .bind(input.element_id)
        .bind(input.parent_object_id)
        .bind(&input.kind)
        .bind(&input.bounds)
        .bind(&input.style)
        .bind(&input.label)
        .bind(input.position)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Lock and read one view object inside the caller's tenant.
async fn lock_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ObjectRow, ModelStoreError> {
    let sql = format!(
        "SELECT {OBJECT_COLUMNS} FROM view_object WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ObjectRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(ModelStoreError::NotFound)
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ObjectPatch,
) -> Result<ObjectRecord, ModelStoreError> {
    let row = lock_object_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let sql = format!(
        "UPDATE view_object
            SET bounds = coalesce($3, bounds),
                style = coalesce($4, style),
                label = coalesce($5, label),
                position = coalesce($6::float8::numeric, position),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {OBJECT_COLUMNS}"
    );
    let row: ObjectRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.bounds)
        .bind(&patch.style)
        .bind(&patch.label)
        .bind(patch.position)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete a view object and the connectors that name it, refusing while it still contains
/// objects.
///
/// Nested objects are the drawing's own tree: orphaning or promoting them is a modelling
/// decision, so a delete that would break the tree is refused with
/// [`ModelStoreError::HasChildren`] rather than surfacing the FK violation. Connectors that
/// name the object at either end go with it — a line to a deleted node draws to nowhere, and
/// 0009 puts no FK on either end to clean them up.
pub async fn delete_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ObjectRecord, ModelStoreError> {
    let row = lock_object_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let children: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM view_object WHERE tenant_id = $1 AND parent_object_id = $2",
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
           WHERE tenant_id = $1 AND (source_object_id = $2 OR target_object_id = $2)",
        "DELETE FROM view_object WHERE tenant_id = $1 AND id = $2",
    ] {
        sqlx::query(statement)
            .bind(tenant.as_uuid())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(row.into())
}

// ── view_connection ─────────────────────────────────────────────────────────────────────

/// One `view_connection` row.
#[derive(Clone, Debug)]
pub struct ConnectionRecord {
    /// The connection id.
    pub id: Uuid,
    /// The view it is drawn on.
    pub view_id: Uuid,
    /// The relationship it draws, when it draws one.
    pub relationship_id: Option<Uuid>,
    /// The object the connector starts at.
    pub source_object_id: Uuid,
    /// The object the connector ends at.
    pub target_object_id: Uuid,
    /// The route, read whole.
    pub bendpoints: Value,
    /// The presentation, read whole.
    pub style: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A connector to insert.
#[derive(Clone, Debug)]
pub struct NewConnection {
    /// The view it is drawn on, resolved in the caller's tenant.
    pub view_id: Uuid,
    /// The relationship it draws, resolved in the caller's tenant when set.
    pub relationship_id: Option<Uuid>,
    /// The source object, resolved in the caller's tenant and required to be on `view_id`.
    pub source_object_id: Uuid,
    /// The target object, resolved in the caller's tenant and required to be on `view_id`.
    pub target_object_id: Uuid,
    /// The route, read whole.
    pub bendpoints: Value,
    /// The presentation, read whole.
    pub style: Value,
}

/// The editable fields of a connector. `None` leaves the column unchanged.
#[derive(Clone, Debug, Default)]
pub struct ConnectionPatch {
    /// Replace the route.
    pub bendpoints: Option<Value>,
    /// Replace the presentation.
    pub style: Option<Value>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConnectionRow {
    id: Uuid,
    view_id: Uuid,
    relationship_id: Option<Uuid>,
    source_object_id: Uuid,
    target_object_id: Uuid,
    bendpoints: Value,
    style: Value,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ConnectionRow> for ConnectionRecord {
    fn from(row: ConnectionRow) -> Self {
        Self {
            id: row.id,
            view_id: row.view_id,
            relationship_id: row.relationship_id,
            source_object_id: row.source_object_id,
            target_object_id: row.target_object_id,
            bendpoints: row.bendpoints,
            style: row.style,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const CONNECTION_COLUMNS: &str = "id, view_id, relationship_id, source_object_id, \
                                  target_object_id, bendpoints, style, version, created_at, \
                                  updated_at";

/// Resolve an object **inside one view**, for a connector's ends.
///
/// [`ensure_parent_tx`] proves the object exists in this tenant; this proves it is drawn on
/// the connection's own view. A connector between an object on this view and one on another
/// would render as a line to nowhere, and no constraint can express the rule because the
/// endpoints carry no foreign key at all.
async fn ensure_object_in_view_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    view_id: Uuid,
    object_id: Uuid,
) -> Result<(), ModelStoreError> {
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM view_object
                         WHERE tenant_id = $1 AND view_id = $2 AND id = $3)",
    )
    .bind(tenant.as_uuid())
    .bind(view_id)
    .bind(object_id)
    .fetch_one(&mut *conn)
    .await?;
    if present {
        Ok(())
    } else {
        Err(ModelStoreError::NotFound)
    }
}

/// Insert a connector inside the caller's transaction.
///
/// Both ends are resolved in the caller's tenant **and required to be on `view_id`**. The
/// relationship is optional: a view may draw a connector before the model's relationship
/// exists (0009), so a miss there is a real refusal, not a dangling edge to repair later.
pub async fn create_connection_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewConnection,
) -> Result<ConnectionRecord, ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::ModelView, input.view_id).await?;
    ensure_optional_parent_tx(
        &mut *conn,
        tenant,
        Parent::ModelRelationship,
        input.relationship_id,
    )
    .await?;
    ensure_parent_tx(
        &mut *conn,
        tenant,
        Parent::ViewObject,
        input.source_object_id,
    )
    .await?;
    ensure_parent_tx(
        &mut *conn,
        tenant,
        Parent::ViewObject,
        input.target_object_id,
    )
    .await?;
    ensure_object_in_view_tx(&mut *conn, tenant, input.view_id, input.source_object_id).await?;
    ensure_object_in_view_tx(&mut *conn, tenant, input.view_id, input.target_object_id).await?;
    let sql = format!(
        "INSERT INTO view_connection (tenant_id, view_id, relationship_id, source_object_id,
                                      target_object_id, bendpoints, style)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {CONNECTION_COLUMNS}"
    );
    let row: ConnectionRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.view_id)
        .bind(input.relationship_id)
        .bind(input.source_object_id)
        .bind(input.target_object_id)
        .bind(&input.bendpoints)
        .bind(&input.style)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Lock and read one connector inside the caller's tenant.
async fn lock_connection_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<ConnectionRow, ModelStoreError> {
    let sql = format!(
        "SELECT {CONNECTION_COLUMNS} FROM view_connection
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let row: Option<ConnectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(ModelStoreError::NotFound)
}

/// Apply an edit guarded by `expected_version`, or write nothing.
pub async fn update_connection_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
    patch: &ConnectionPatch,
) -> Result<ConnectionRecord, ModelStoreError> {
    let row = lock_connection_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    let sql = format!(
        "UPDATE view_connection
            SET bendpoints = coalesce($3, bendpoints),
                style = coalesce($4, style),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
         RETURNING {CONNECTION_COLUMNS}"
    );
    let row: ConnectionRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&patch.bendpoints)
        .bind(&patch.style)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// Delete one connector inside the caller's transaction.
pub async fn delete_connection_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<ConnectionRecord, ModelStoreError> {
    let row = lock_connection_tx(&mut *conn, tenant, id).await?;
    if version_of(row.version) != expected_version {
        return Err(ModelStoreError::VersionMismatch {
            current: version_of(row.version),
        });
    }
    sqlx::query("DELETE FROM view_connection WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(row.into())
}

// ── generate ────────────────────────────────────────────────────────────────────────────

/// Replace a view's whole drawing: drop its connectors and objects, then place `objects`.
///
/// This is the store half of `model_views.generate`. The view is rewritten as one unit inside
/// the caller's transaction, so a failed insert leaves the previous drawing intact — the
/// caller's rollback takes the deletes with it.
///
/// Each tuple is `(element_id, bounds, position)`: the generator computes the rect and the
/// stacking order, and `kind` takes 0009's default (`element`) with an empty style. Duplicates
/// are legal — one element may be drawn twice — so the return value is the number of rows
/// inserted, not the number of distinct elements. The view header is untouched: its `version`
/// is the ETag of its own metadata, not of the drawing.
pub async fn replace_objects_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    view_id: Uuid,
    objects: &[(Uuid, Value, f64)],
) -> Result<usize, ModelStoreError> {
    ensure_parent_tx(&mut *conn, tenant, Parent::ModelView, view_id).await?;
    for statement in [
        "DELETE FROM view_connection WHERE tenant_id = $1 AND view_id = $2",
        "DELETE FROM view_object WHERE tenant_id = $1 AND view_id = $2",
    ] {
        sqlx::query(statement)
            .bind(tenant.as_uuid())
            .bind(view_id)
            .execute(&mut *conn)
            .await?;
    }
    let mut inserted = 0usize;
    for (element_id, bounds, position) in objects {
        ensure_parent_tx(&mut *conn, tenant, Parent::ModelElement, *element_id).await?;
        inserted += sqlx::query(
            "INSERT INTO view_object (tenant_id, view_id, element_id, bounds, position)
             VALUES ($1, $2, $3, $4, $5::float8::numeric)",
        )
        .bind(tenant.as_uuid())
        .bind(view_id)
        .bind(*element_id)
        .bind(bounds)
        .bind(*position)
        .execute(&mut *conn)
        .await?
        .rows_affected() as usize;
    }
    Ok(inserted)
}

/// Read one view object inside the caller's tenant.
pub async fn get_object_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ObjectRecord>, ModelStoreError> {
    let sql = format!("SELECT {OBJECT_COLUMNS} FROM view_object WHERE tenant_id = $1 AND id = $2");
    let row: Option<ObjectRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Read one view connection inside the caller's tenant.
pub async fn get_connection_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ConnectionRecord>, ModelStoreError> {
    let sql = format!(
        "SELECT {CONNECTION_COLUMNS} FROM view_connection WHERE tenant_id = $1 AND id = $2"
    );
    let row: Option<ConnectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}
