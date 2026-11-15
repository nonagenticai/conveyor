//! DB reads/writes for the four catalogue OFFERING operations.
//!
//! Ported from the pinned reference (`4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`create_offering`] | `models.py:6350` `DatabaseManager.create_offering`, `main.py:8092` |
//! | [`list_offerings`] | `models.py:6422` `DatabaseManager.list_offerings`, `main.py:8178` |
//! | [`get_offering`] | `models.py:6405` `DatabaseManager.get_offering`, `main.py:8193` |
//! | [`delete_offering`] | `models.py:6437` `DatabaseManager.delete_offering`, `main.py:8212` |
//!
//! The predicates, ordering and side effects are the reference's, not improvements on them:
//!
//! - `create_offering` validates `byo_mode` / `access_cadence` BEFORE it resolves the app, so an
//!   invalid `byo_mode` on an UNKNOWN app is the route's 422, not its 404 (the DAL's own order);
//! - the upsert is per `(app_id, code)`: the first write INSERTs, and every later one UPDATEs
//!   `name`, `byo_mode`, `access_price_usd`, `access_cadence`, `included_allowance_usd` and `notes`
//!   unconditionally while writing a usage field ONLY when the caller sent one. Two consequences
//!   are the reference's, not accidents: an omitted `access_price_usd` CLEARS a previous value,
//!   and an omitted usage field KEEPS it (`COALESCE` is exactly the ORM's `if field in usage`
//!   guard, because the handler never puts an explicit null in the map);
//! - the reference's read-then-insert has no lock, so two concurrent creates can collide on
//!   `ux_offering_app_code`. It retries ONCE as an update and answers 409 on a second
//!   `IntegrityError`; [`create_offering`] reproduces both arms;
//! - `list_offerings` filters to one app key, answers `[]` for an unknown key, and orders
//!   `created_at` ASCENDING (the reference's `order_by(Offering.created_at)`);
//! - `delete_offering` is a plain DELETE. Nothing references `offering`: the only column that
//!   names an offering is `tenant_app_byo_mode.offering_code`, a bare `varchar(64)` with no FK, so
//!   deleting an offering neither refuses nor clears a tenant's posture row. `deleted=false` when
//!   the app key is unknown or the pair is absent — the route is idempotent and never 404s.
//!
//! `offering` is a GLOBAL catalogue row (no `tenant_id`); `app` is its product.
//!
//! The numeric columns are selected as `float8` because Python's `Offering.to_dict` renders every
//! `Numeric` through `float(v)`; the same cast is load-bearing on the write side, where a Rust
//! `f64` is a `float8` parameter and the column is `numeric`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// `BYO_MODES` (models.py): the only values `ck_offering_byo_mode` accepts, checked by the DAL.
pub const BYO_MODES: [&str; 3] = ["local_allowed", "byo_required", "platform_only"];

/// `ck_offering_access_cadence`, as `create_offering` spells it.
pub const ACCESS_CADENCES: [&str; 2] = ["monthly", "one_time"];

/// One `offering` row joined to its app, the columns `OfferingResponse` publishes.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfferingRow {
    pub id: Uuid,
    pub app_id: Uuid,
    pub app_key: String,
    pub code: String,
    pub name: String,
    pub byo_mode: String,
    pub markup_pct: Option<f64>,
    pub surcharge_input_per_1m_usd: Option<f64>,
    pub surcharge_output_per_1m_usd: Option<f64>,
    pub surcharge_cache_creation_per_1m_usd: Option<f64>,
    pub surcharge_cache_read_per_1m_usd: Option<f64>,
    pub surcharge_per_request_usd: Option<f64>,
    pub access_price_usd: Option<f64>,
    pub access_cadence: String,
    pub included_allowance_usd: Option<f64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT_OFFERING: &str = "SELECT o.id, o.app_id, a.key AS app_key, o.code, o.name, \
     o.byo_mode, o.markup_pct::float8 AS markup_pct, \
     o.surcharge_input_per_1m_usd::float8 AS surcharge_input_per_1m_usd, \
     o.surcharge_output_per_1m_usd::float8 AS surcharge_output_per_1m_usd, \
     o.surcharge_cache_creation_per_1m_usd::float8 AS surcharge_cache_creation_per_1m_usd, \
     o.surcharge_cache_read_per_1m_usd::float8 AS surcharge_cache_read_per_1m_usd, \
     o.surcharge_per_request_usd::float8 AS surcharge_per_request_usd, \
     o.access_price_usd::float8 AS access_price_usd, o.access_cadence, \
     o.included_allowance_usd::float8 AS included_allowance_usd, o.notes, \
     o.created_at, o.updated_at \
     FROM offering o JOIN app a ON a.id = o.app_id";

/// The six usage-policy fields. `None` = the caller did not send it: keep (update) / NULL
/// (insert). An explicit JSON null is the same as absent — Pydantic hands both through as None.
#[derive(Debug, Clone)]
pub struct NewOffering {
    pub app_key: String,
    pub code: String,
    pub name: String,
    pub byo_mode: String,
    pub markup_pct: Option<f64>,
    pub surcharge_input_per_1m_usd: Option<f64>,
    pub surcharge_output_per_1m_usd: Option<f64>,
    pub surcharge_cache_creation_per_1m_usd: Option<f64>,
    pub surcharge_cache_read_per_1m_usd: Option<f64>,
    pub surcharge_per_request_usd: Option<f64>,
    pub access_price_usd: Option<f64>,
    pub access_cadence: String,
    pub included_allowance_usd: Option<f64>,
    pub notes: Option<String>,
}

/// What one upsert produced, including the two refusals the DAL itself authors.
#[derive(Debug)]
pub enum CreateOffering {
    /// Boxed: the row is ~330 bytes and the other arms are a string or nothing.
    Upserted(Box<OfferingRow>),
    /// The app key does not exist (the route's 404, after the owner check it could not reach).
    AppNotFound,
    /// `invalid byo_mode: '…'` — the route's 422 with a STRING detail.
    InvalidByoMode(String),
    /// `invalid access_cadence: '…'` — the route's 422 with a STRING detail.
    InvalidAccessCadence(String),
    /// Two consecutive `IntegrityError`s on the upsert — the reference's 409.
    Conflict,
}

/// A SQLSTATE class 23 (`IntegrityError`) — the reference's `except IntegrityError`.
fn is_integrity(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database)
        if database.code().is_some_and(|code| code.starts_with("23")))
}

/// `DatabaseManager.create_offering` — the upsert, with the reference's retry-once-then-409.
pub async fn create_offering(
    pool: &PgPool,
    offering: &NewOffering,
) -> Result<CreateOffering, sqlx::Error> {
    // BEFORE the app lookup: the ORM validates the two closed sets at the top of the DAL, so an
    // invalid byo_mode on an unknown app is a 422, never the 404.
    if !BYO_MODES.contains(&offering.byo_mode.as_str()) {
        return Ok(CreateOffering::InvalidByoMode(offering.byo_mode.clone()));
    }
    if !ACCESS_CADENCES.contains(&offering.access_cadence.as_str()) {
        return Ok(CreateOffering::InvalidAccessCadence(
            offering.access_cadence.clone(),
        ));
    }

    match upsert_once(pool, offering).await {
        Ok(Some(row)) => Ok(CreateOffering::Upserted(Box::new(row))),
        Ok(None) => Ok(CreateOffering::AppNotFound),
        Err(error) if is_integrity(&error) => {
            tracing::info!(
                app = %offering.app_key,
                code = %offering.code,
                "offering upsert lost an insert race; retrying as an update"
            );
            match upsert_once(pool, offering).await {
                Ok(Some(row)) => Ok(CreateOffering::Upserted(Box::new(row))),
                Ok(None) => Ok(CreateOffering::AppNotFound),
                // Two consecutive violations are not the benign create/create race.
                Err(error) if is_integrity(&error) => Ok(CreateOffering::Conflict),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// `get_app_by_key` for the routes' owner check: the app id, or `None`.
pub async fn app_by_key(pool: &PgPool, app_key: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app WHERE key = $1 LIMIT 1")
        .bind(app_key)
        .fetch_optional(pool)
        .await
}

/// One read-then-insert/update, exactly as the ORM does it (no lock between the two).
async fn upsert_once(
    pool: &PgPool,
    offering: &NewOffering,
) -> Result<Option<OfferingRow>, sqlx::Error> {
    let Some(app_id) = app_by_key(pool, &offering.app_key).await? else {
        return Ok(None);
    };

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM offering WHERE app_id = $1 AND code = $2")
            .bind(app_id)
            .bind(&offering.code)
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        // The reference's update branch: name/byo_mode/updated_at, the usage fields it was told
        // about, and the four access/notes fields unconditionally.
        sqlx::query(
            "UPDATE offering
                SET name = $3,
                    byo_mode = $4,
                    markup_pct = COALESCE($5::float8, markup_pct::float8),
                    surcharge_input_per_1m_usd = COALESCE($6::float8, surcharge_input_per_1m_usd::float8),
                    surcharge_output_per_1m_usd = COALESCE($7::float8, surcharge_output_per_1m_usd::float8),
                    surcharge_cache_creation_per_1m_usd = COALESCE($8::float8, surcharge_cache_creation_per_1m_usd::float8),
                    surcharge_cache_read_per_1m_usd = COALESCE($9::float8, surcharge_cache_read_per_1m_usd::float8),
                    surcharge_per_request_usd = COALESCE($10::float8, surcharge_per_request_usd::float8),
                    access_price_usd = $11::float8,
                    access_cadence = $12,
                    included_allowance_usd = $13::float8,
                    notes = $14,
                    updated_at = now()
              WHERE app_id = $1 AND code = $2",
        )
        .bind(app_id)
        .bind(&offering.code)
        .bind(&offering.name)
        .bind(&offering.byo_mode)
        .bind(offering.markup_pct)
        .bind(offering.surcharge_input_per_1m_usd)
        .bind(offering.surcharge_output_per_1m_usd)
        .bind(offering.surcharge_cache_creation_per_1m_usd)
        .bind(offering.surcharge_cache_read_per_1m_usd)
        .bind(offering.surcharge_per_request_usd)
        .bind(offering.access_price_usd)
        .bind(&offering.access_cadence)
        .bind(offering.included_allowance_usd)
        .bind(offering.notes.as_deref())
        .execute(pool)
        .await?;
    } else {
        // `offering.id` has NO server default (the ORM's `uuid7` supplies it) and neither
        // `created_at` / `updated_at`: all three are written here, exactly as the ORM does.
        sqlx::query(
            "INSERT INTO offering
                 (id, app_id, code, name, byo_mode, markup_pct, surcharge_input_per_1m_usd,
                  surcharge_output_per_1m_usd, surcharge_cache_creation_per_1m_usd,
                  surcharge_cache_read_per_1m_usd, surcharge_per_request_usd,
                  access_price_usd, access_cadence, included_allowance_usd, notes,
                  created_at, updated_at)
             VALUES (uuidv7(), $1, $2, $3, $4, $5::float8, $6::float8, $7::float8, $8::float8,
                     $9::float8, $10::float8, $11::float8, $12, $13::float8, $14, now(), now())",
        )
        .bind(app_id)
        .bind(&offering.code)
        .bind(&offering.name)
        .bind(&offering.byo_mode)
        .bind(offering.markup_pct)
        .bind(offering.surcharge_input_per_1m_usd)
        .bind(offering.surcharge_output_per_1m_usd)
        .bind(offering.surcharge_cache_creation_per_1m_usd)
        .bind(offering.surcharge_cache_read_per_1m_usd)
        .bind(offering.surcharge_per_request_usd)
        .bind(offering.access_price_usd)
        .bind(&offering.access_cadence)
        .bind(offering.included_allowance_usd)
        .bind(offering.notes.as_deref())
        .execute(pool)
        .await?;
    }

    // `session.refresh(row)`: read back the committed row with its app key.
    let row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_OFFERING} WHERE o.app_id = $1 AND o.code = $2"
    )))
    .bind(app_id)
    .bind(&offering.code)
    .fetch_optional(pool)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(row))
}

/// `DatabaseManager.get_offering` — one offering by `(app_key, code)`.
pub async fn get_offering(
    pool: &PgPool,
    app_key: &str,
    code: &str,
) -> Result<Option<OfferingRow>, sqlx::Error> {
    let sql = format!("{SELECT_OFFERING} WHERE a.key = $1 AND o.code = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(app_key)
        .bind(code)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.list_offerings(app_key=…)` — an unknown key is an empty list, and the order is
/// `created_at` ascending.
pub async fn list_offerings(
    pool: &PgPool,
    app_key: Option<&str>,
) -> Result<Vec<OfferingRow>, sqlx::Error> {
    let sql = match app_key {
        Some(_) => format!("{SELECT_OFFERING} WHERE a.key = $1 ORDER BY o.created_at"),
        None => format!("{SELECT_OFFERING} ORDER BY o.created_at"),
    };
    let mut query = sqlx::query_as(sqlx::AssertSqlSafe(sql));
    if let Some(app_key) = app_key {
        query = query.bind(app_key);
    }
    query.fetch_all(pool).await
}

/// `DatabaseManager.delete_offering` — true iff a row was removed, false for an unknown app or a
/// missing pair. No row references `offering`, so nothing is constrained or cascaded.
pub async fn delete_offering(
    pool: &PgPool,
    app_key: &str,
    code: &str,
) -> Result<bool, sqlx::Error> {
    let app_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM app WHERE key = $1")
        .bind(app_key)
        .fetch_optional(pool)
        .await?;
    let Some(app_id) = app_id else {
        return Ok(false);
    };
    let deleted = sqlx::query("DELETE FROM offering WHERE app_id = $1 AND code = $2")
        .bind(app_id)
        .bind(code)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() > 0)
}
