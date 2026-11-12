//! `model_validation_run` — one row per `model.validate` job (S08 Part B, data-arch §2.8).
//!
//! A run is an append-only fact: it has `created_at` and no `version`, and nothing in this
//! slice updates or deletes one. Its `findings` document is read whole with the run; a
//! findings table would be a second shape for a report that is never queried by field.
//!
//! The writer is the **worker**, not an action: `models.validate` enqueues a job in the
//! caller's transaction ([`crate::model::enqueue_validate_tx`]) and the check runs after
//! commit, so a full graph walk never holds the model's row lock (slice trap 6).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::model::ModelStoreError;

/// The three legal `model_validation_run.status` values (0009's CHECK).
pub const RUN_STATUSES: [&str; 3] = ["ok", "findings", "error"];

/// One `model_validation_run` row.
#[derive(Clone, Debug)]
pub struct ValidationRunRecord {
    /// The run id.
    pub id: Uuid,
    /// The model that was validated.
    pub model_id: Uuid,
    /// The revision the run was started from, when the caller named one.
    pub revision_id: Option<Uuid>,
    /// The profile's ruleset id, e.g. `conveyor-archimate-3.2-v1`.
    pub ruleset: String,
    /// The ruleset's own version.
    pub ruleset_version: String,
    /// `ok`, `findings` or `error`.
    pub status: String,
    /// The whole findings document.
    pub findings: Value,
    /// The hash of the graph the run actually saw.
    pub content_hash: String,
    /// When the run finished.
    pub created_at: DateTime<Utc>,
}

/// A validation run to insert.
#[derive(Clone, Debug)]
pub struct NewValidationRun {
    /// The model that was validated.
    pub model_id: Uuid,
    /// The revision the run was started from, when one was named.
    pub revision_id: Option<Uuid>,
    /// The profile's ruleset id.
    pub ruleset: String,
    /// The ruleset's own version.
    pub ruleset_version: String,
    /// `ok`, `findings` or `error` — the worker decides from the finding severities.
    pub status: String,
    /// The whole findings document.
    pub findings: Value,
    /// The hash of the graph the run saw.
    pub content_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ValidationRunRow {
    id: Uuid,
    model_id: Uuid,
    revision_id: Option<Uuid>,
    ruleset: String,
    ruleset_version: String,
    status: String,
    findings: Value,
    content_hash: String,
    created_at: DateTime<Utc>,
}

impl From<ValidationRunRow> for ValidationRunRecord {
    fn from(row: ValidationRunRow) -> Self {
        Self {
            id: row.id,
            model_id: row.model_id,
            revision_id: row.revision_id,
            ruleset: row.ruleset,
            ruleset_version: row.ruleset_version,
            status: row.status,
            findings: row.findings,
            content_hash: row.content_hash,
            created_at: row.created_at,
        }
    }
}

const RUN_COLUMNS: &str = "id, model_id, revision_id, ruleset, ruleset_version, status, \
                           findings, content_hash, created_at";

/// Append one run inside the caller's transaction.
pub async fn insert_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewValidationRun,
) -> Result<ValidationRunRecord, ModelStoreError> {
    crate::parent::ensure_parent_tx(
        &mut *conn,
        tenant,
        crate::parent::Parent::Model,
        input.model_id,
    )
    .await?;
    let sql = format!(
        "INSERT INTO model_validation_run (tenant_id, model_id, revision_id, ruleset,
                                           ruleset_version, status, findings, content_hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {RUN_COLUMNS}"
    );
    let row: ValidationRunRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.model_id)
        .bind(input.revision_id)
        .bind(&input.ruleset)
        .bind(&input.ruleset_version)
        .bind(&input.status)
        .bind(&input.findings)
        .bind(&input.content_hash)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.into())
}

/// The newest run for a model, if any — what `models.get` returns as `latest_validation_run`.
pub async fn latest_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    model_id: Uuid,
) -> Result<Option<ValidationRunRecord>, ModelStoreError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM model_validation_run
          WHERE tenant_id = $1 AND model_id = $2
          ORDER BY created_at DESC, id DESC
          LIMIT 1"
    );
    let row: Option<ValidationRunRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(model_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}
