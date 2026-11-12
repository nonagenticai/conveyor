//! The ONE vocabulary table: the eleven namespaces and the project-type matrix.
//!
//! `kind` has no CHECK in the migration on purpose (S03 §5): `CatalogueKind` is the
//! vocabulary, and the matrix a project type carries is read here, not modelled as 280 rows.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::{CatalogueEntry, CatalogueKind, Code, CodeError, TenantId, Version};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::version_of;

/// A catalogue entry to create.
#[derive(Clone, Debug)]
pub struct NewCatalogueEntry {
    /// Which vocabulary this row belongs to.
    pub kind: CatalogueKind,
    /// The stable code within the kind.
    pub code: Code,
    /// The display label.
    pub label: String,
    /// The kind-specific payload; `meta.matrix` for a project type.
    pub meta: Value,
    /// Display order within the kind.
    pub sort: i32,
    /// Whether the row is offered.
    pub is_active: bool,
}

impl NewCatalogueEntry {
    /// An active entry with empty meta and sort `0`.
    pub fn new(kind: CatalogueKind, code: Code, label: impl Into<String>) -> Self {
        Self {
            kind,
            code,
            label: label.into(),
            meta: Value::Object(serde_json::Map::new()),
            sort: 0,
            is_active: true,
        }
    }
}

/// Why a catalogue read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum CatalogueStoreError {
    /// A catalogue kind that is not one of the eleven kinds — an input string at import
    /// time, or a stored `catalogue_entry.kind` the domain does not know.
    #[error("unknown catalogue kind {0:?}: S03 fixes eleven kinds")]
    UnknownKind(String),
    /// The capability vocabulary moved to S15's `capability` aggregate. The kind still parses so a
    /// legacy export can be READ, but importing it would re-create the rows the migration removed.
    #[error("catalogue kind \"capability\" is S15's capability aggregate, not a catalogue entry")]
    CapabilityMoved,
    /// A stored code no longer parses.
    #[error("a catalogue code in the database is invalid: {0}")]
    InvalidCode(#[from] CodeError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct CatalogueRow {
    id: Uuid,
    kind: String,
    code: String,
    label: String,
    meta: Value,
    sort: i32,
    is_active: bool,
    version: i64,
}

impl TryFrom<CatalogueRow> for CatalogueEntry {
    type Error = CatalogueStoreError;
    fn try_from(row: CatalogueRow) -> Result<Self, Self::Error> {
        let kind = CatalogueKind::from_str(&row.kind)
            .map_err(|_| CatalogueStoreError::UnknownKind(row.kind))?;
        Ok(Self {
            id: row.id,
            kind,
            code: Code::parse(&row.code)?,
            label: row.label,
            meta: row.meta,
            sort: row.sort,
            is_active: row.is_active,
            version: version_of(row.version),
        })
    }
}

const COLUMNS: &str = "id, kind, code, label, meta, sort, is_active, version";

/// Insert a catalogue entry inside the caller's scoped transaction.
pub async fn insert_catalogue_entry_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewCatalogueEntry,
) -> Result<CatalogueEntry, CatalogueStoreError> {
    let sql = format!(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort, is_active)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {COLUMNS}"
    );
    let row: CatalogueRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.kind.as_str())
        .bind(input.code.as_str())
        .bind(&input.label)
        .bind(&input.meta)
        .bind(input.sort)
        .bind(input.is_active)
        .fetch_one(&mut *conn)
        .await?;
    row.try_into()
}

/// Read one catalogue entry by `(kind, code)` inside the caller's scoped transaction.
pub async fn get_catalogue_entry_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: CatalogueKind,
    code: &Code,
) -> Result<Option<CatalogueEntry>, CatalogueStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = $2 AND code = $3"
    );
    let row: Option<CatalogueRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind.as_str())
        .bind(code.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    row.map(CatalogueEntry::try_from).transpose()
}

// ── S07: the catalogue read, the importer and the card_type table ─────────────────────────

/// One `catalogue_entry` row **including its timestamps**, for the `catalogues.list` read.
///
/// The domain's [`CatalogueEntry`] is the vocabulary projection and carries no timestamps;
/// the catalogue screen shows when a row was seeded and last changed, so the store record is
/// the full row.
#[derive(Clone, Debug)]
pub struct CatalogueRecord {
    /// The row id.
    pub id: Uuid,
    /// Which vocabulary this row belongs to.
    pub kind: CatalogueKind,
    /// The stable code within the kind.
    pub code: Code,
    /// The display label.
    pub label: String,
    /// The kind-specific payload; `meta.matrix` for a project type.
    pub meta: Value,
    /// Display order within the kind.
    pub sort: i32,
    /// Whether the row is offered.
    pub is_active: bool,
    /// The optimistic-concurrency version.
    pub version: Version,
    /// When the row was first seeded.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct CatalogueRecordRow {
    id: Uuid,
    kind: String,
    code: String,
    label: String,
    meta: Value,
    sort: i32,
    is_active: bool,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<CatalogueRecordRow> for CatalogueRecord {
    type Error = CatalogueStoreError;
    fn try_from(row: CatalogueRecordRow) -> Result<Self, Self::Error> {
        let kind = CatalogueKind::from_str(&row.kind)
            .map_err(|_| CatalogueStoreError::UnknownKind(row.kind))?;
        Ok(Self {
            id: row.id,
            kind,
            code: Code::parse(&row.code)?,
            label: row.label,
            meta: row.meta,
            sort: row.sort,
            is_active: row.is_active,
            version: version_of(row.version),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

const RECORD_COLUMNS: &str = "id, kind, code, label, meta, sort, is_active, version, \
                              created_at, updated_at";

/// One page of catalogue entries, ordered by `(kind, sort, code)`.
///
/// `kind` is validated through [`CatalogueKind`] when supplied; `include_inactive = false`
/// hides retired rows. The kind/sort order matches `catalogue_entry_kind_sort_idx`.
pub async fn list_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: Option<&str>,
    include_inactive: bool,
    limit: i64,
) -> Result<Vec<CatalogueRecord>, CatalogueStoreError> {
    let kind = match kind {
        Some(raw) => Some(
            CatalogueKind::from_str(raw)
                .map_err(|_| CatalogueStoreError::UnknownKind(raw.to_owned()))?,
        ),
        None => None,
    };
    let sql = format!(
        "SELECT {RECORD_COLUMNS}
           FROM catalogue_entry
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR kind = $2)
            AND ($3::boolean OR is_active)
          ORDER BY kind, sort, code
          LIMIT $4"
    );
    let rows: Vec<CatalogueRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind.map(CatalogueKind::as_str))
        .bind(include_inactive)
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter().map(CatalogueRecord::try_from).collect()
}

/// What an import did, per row.
///
/// `skipped` is a row whose stored `(label, meta, sort, is_active)` was already identical, so
/// the `ON CONFLICT … DO UPDATE … WHERE … IS DISTINCT FROM` wrote nothing and bumped no
/// version. `created + updated + skipped` equals the number of entries handed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    /// Rows that did not exist.
    pub created: u64,
    /// Rows that existed with at least one changed field.
    pub updated: u64,
    /// Rows that existed and were already identical.
    pub skipped: u64,
}

/// One catalogue entry to import from `seed/catalogue.json` or the per-tenant seed.
///
/// Raw strings, not domain types: the importer validates `kind` through [`CatalogueKind`] and
/// `code` through [`Code::parse`] before any statement runs, so a bad line in the export is a
/// field error naming the value, never a 500 from the unique key or a CHECK.
#[derive(Clone, Debug)]
pub struct ImportEntry {
    /// The catalogue kind (`project_type`, `phase`, …).
    pub kind: String,
    /// The stable code within the kind.
    pub code: String,
    /// The display label.
    pub label: String,
    /// The kind-specific payload.
    pub meta: Value,
    /// Display order within the kind.
    pub sort: i32,
    /// Whether the row is offered.
    pub is_active: bool,
}

impl ImportEntry {
    /// An active entry with empty meta and sort `0`.
    pub fn new(kind: impl Into<String>, code: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            code: code.into(),
            label: label.into(),
            meta: Value::Object(serde_json::Map::new()),
            sort: 0,
            is_active: true,
        }
    }
}

/// One `card_type` row to import. `card_type` is its own table — it is **not** one of the
/// eleven catalogue kinds (S07 §5.4) — but the same importer feeds both.
#[derive(Clone, Debug)]
pub struct CardTypeImport {
    /// The stable code the project-type matrix is keyed by.
    pub code: String,
    /// The display label.
    pub label: String,
    /// Type-specific fields (`inputs`, `gates`, …).
    pub definition: Value,
    /// Whether the type may be chosen.
    pub is_active: bool,
}

impl CardTypeImport {
    /// An active card type with an empty definition.
    pub fn new(code: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            label: label.into(),
            definition: Value::Object(serde_json::Map::new()),
            is_active: true,
        }
    }
}

/// Import catalogue entries into the caller's tenant, one statement per entry.
///
/// Every `kind` and `code` is validated **before** the first statement, so a bad export line
/// cannot leave a half-imported tenant in the caller's transaction. Each row is
/// `INSERT … ON CONFLICT (tenant_id, kind, code) DO UPDATE … WHERE (label, meta, sort,
/// is_active) IS DISTINCT FROM (EXCLUDED.…)`, and `RETURNING (xmax = 0)` distinguishes a
/// fresh insert (`true`, counting `created`) from an update (`false`, counting `updated`);
/// the `WHERE` returning no row at all is a no-op and counts `skipped`. The version bumps
/// only when the update actually writes.
pub async fn import_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entries: &[ImportEntry],
) -> Result<ImportCounts, CatalogueStoreError> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let kind = CatalogueKind::from_str(&entry.kind)
            .map_err(|_| CatalogueStoreError::UnknownKind(entry.kind.clone()))?;
        if kind == CatalogueKind::Capability {
            return Err(CatalogueStoreError::CapabilityMoved);
        }
        let code = Code::parse(&entry.code)?;
        parsed.push((kind, code, entry));
    }

    let mut counts = ImportCounts::default();
    for (kind, code, entry) in parsed {
        let inserted: Option<bool> = sqlx::query_scalar(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort, is_active)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (tenant_id, kind, code) DO UPDATE
                SET label = EXCLUDED.label,
                    meta = EXCLUDED.meta,
                    sort = EXCLUDED.sort,
                    is_active = EXCLUDED.is_active,
                    version = catalogue_entry.version + 1,
                    updated_at = now()
              WHERE (catalogue_entry.label, catalogue_entry.meta, catalogue_entry.sort,
                     catalogue_entry.is_active)
                    IS DISTINCT FROM
                    (EXCLUDED.label, EXCLUDED.meta, EXCLUDED.sort, EXCLUDED.is_active)
             RETURNING (xmax = 0)",
        )
        .bind(tenant.as_uuid())
        .bind(kind.as_str())
        .bind(code.as_str())
        .bind(&entry.label)
        .bind(&entry.meta)
        .bind(entry.sort)
        .bind(entry.is_active)
        .fetch_optional(&mut *conn)
        .await?;
        match inserted {
            Some(true) => counts.created += 1,
            Some(false) => counts.updated += 1,
            None => counts.skipped += 1,
        }
    }
    Ok(counts)
}

/// Import `card_type` rows into the caller's tenant, one statement per entry.
///
/// Same contract as [`import_entries_tx`] against `card_type`'s own unique key
/// `(tenant_id, code)`; the `(xmax = 0)` returning trick is how a caller can report
/// `created` and `updated` honestly instead of counting the batch twice.
pub async fn import_card_types_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entries: &[CardTypeImport],
) -> Result<ImportCounts, CatalogueStoreError> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let code = Code::parse(&entry.code)?;
        parsed.push((code, entry));
    }

    let mut counts = ImportCounts::default();
    for (code, entry) in parsed {
        let inserted: Option<bool> = sqlx::query_scalar(
            "INSERT INTO card_type (tenant_id, code, label, definition, is_active)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id, code) DO UPDATE
                SET label = EXCLUDED.label,
                    definition = EXCLUDED.definition,
                    is_active = EXCLUDED.is_active,
                    version = card_type.version + 1,
                    updated_at = now()
              WHERE (card_type.label, card_type.definition, card_type.is_active)
                    IS DISTINCT FROM
                    (EXCLUDED.label, EXCLUDED.definition, EXCLUDED.is_active)
             RETURNING (xmax = 0)",
        )
        .bind(tenant.as_uuid())
        .bind(code.as_str())
        .bind(&entry.label)
        .bind(&entry.definition)
        .bind(entry.is_active)
        .fetch_optional(&mut *conn)
        .await?;
        match inserted {
            Some(true) => counts.created += 1,
            Some(false) => counts.updated += 1,
            None => counts.skipped += 1,
        }
    }
    Ok(counts)
}
