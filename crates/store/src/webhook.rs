#![forbid(unsafe_code)]
//! Webhook subscriptions and their delivery jobs (S05 §5, data-arch §2.5).
//!
//! A subscription is one row on a board (or, when `board_id` is NULL, on the tenant) that
//! selects events with `events`/`excluded_events`. It is never delivered from a second write
//! path: the emitter that writes the `event` row enqueues one `webhook.deliver` job per
//! matching subscription **in the same transaction** (S5-D7). The worker drains that job,
//! so a mutation never waits for a third party and a rolled-back mutation never announces.
//!
//! The secret is write-only from the service's point of view: [`insert_webhook_tx`] stores it and every read
//! projection omits it. [`secret_tx`] and [`delivery_target`] are the two readers, used by the delivery paths.
//! ⚠️ **Sealed since migration `0014`** under the tenant KEK (`secret::Sealer`). This header used to say the
//! bytes were the bearer token itself because S05 predated the envelope; the column was named
//! `secret_ciphertext` for two slices before it held a ciphertext. A row written before `0014` has a NULL
//! `secret_key_id` and is delivered unchanged until `conveyor admin reseal-webhooks` converts it.
//!
//! A delivery job carries `{subscription_id, body}` only; the URL and the secret are read
//! again at delivery time, so rotating or deleting a subscription takes effect for jobs that
//! are still queued. The dedupe key is `<event id>:<subscription id>`: `job` is unique per
//! `(tenant, kind, dedupe_key)` among live rows, so one event may fan out to many
//! subscriptions without the second insert being treated as a replay of the first.

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::scope::begin_scoped;

/// The job kind the emitter enqueues and the worker drains.
pub const DELIVER_KIND: &str = "webhook.deliver";

/// How many attempts one delivery gets (S05 §5: three).
pub const DELIVERY_MAX_ATTEMPTS: i32 = 3;

/// One subscription as every read returns it — never the secret.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct WebhookRecord {
    /// The subscription id.
    pub id: Uuid,
    /// The board it watches; `None` is tenant-wide.
    pub board_id: Option<Uuid>,
    /// The endpoint.
    pub url: String,
    /// The event types it accepts; empty means every event.
    pub events: Vec<String>,
    /// The event types it refuses even when `events` names them.
    pub excluded_events: Vec<String>,
    /// Whether it is currently delivering.
    pub active: bool,
    /// The row version (the `ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// A subscription to create.
#[derive(Clone)]
pub struct NewWebhook {
    /// The board to watch; `None` is tenant-wide.
    pub board_id: Option<Uuid>,
    /// The endpoint.
    pub url: String,
    /// The bearer token a delivery presents. Stored, never read back by the API.
    pub secret: Vec<u8>,
    /// Accepted event types; empty means every event.
    pub events: Vec<String>,
    /// Refused event types.
    pub excluded_events: Vec<String>,
    /// Whether deliveries start immediately.
    pub active: bool,
}

/// The fields `webhooks.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct WebhookUpdate {
    /// The endpoint.
    pub url: Option<String>,
    /// Accepted event types.
    pub events: Option<Vec<String>>,
    /// Refused event types.
    pub excluded_events: Option<Vec<String>>,
    /// Delivery switch.
    pub active: Option<bool>,
}

/// A delivery's connection details, read at send time.
#[derive(Clone)]
pub struct DeliveryTarget {
    /// The subscription id.
    pub id: Uuid,
    /// The endpoint.
    pub url: String,
    /// The bearer token, when the subscription has one.
    pub secret: Option<Vec<u8>>,
    /// Whether the subscription is still active.
    pub active: bool,
}

// `WebhookRecord` deliberately omits the secret; these two carry it, so a derived `Debug` would put
// the plaintext signing token into any log line that formats them. Redact by hand and keep the rest.
impl std::fmt::Debug for NewWebhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewWebhook")
            .field("board_id", &self.board_id)
            .field("url", &self.url)
            .field("secret", &"[redacted]")
            .field("events", &self.events)
            .field("excluded_events", &self.excluded_events)
            .field("active", &self.active)
            .finish()
    }
}

impl std::fmt::Debug for DeliveryTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeliveryTarget")
            .field("id", &self.id)
            .field("url", &self.url)
            .field("secret", &self.secret.as_ref().map(|_| "[redacted]"))
            .field("active", &self.active)
            .finish()
    }
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum WebhookStoreError {
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The signing secret could not be sealed or opened — a missing KEK is a refusal, never a plaintext write.
    #[error(transparent)]
    Secret(#[from] crate::secret::SecretError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const COLUMNS: &str = "id, board_id, url, events, excluded_events, active, version, \
                       created_at, updated_at";

/// Insert a subscription inside the caller's scoped transaction.
///
/// The signing secret is sealed under the tenant's KEK (S09's envelope). A build with no `CONVEYOR_SECRET_KEK`
/// **refuses** the insert rather than writing the token in the clear — the column is named for what it holds.
pub async fn insert_webhook_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewWebhook,
) -> Result<WebhookRecord, WebhookStoreError> {
    if let Some(board_id) = input.board_id {
        ensure_parent_tx(conn, tenant, Parent::Board, board_id).await?;
    }
    let (ciphertext, key_id) = if input.secret.is_empty() {
        (None, None)
    } else {
        let (sealed, key_id) = crate::secret::sealer()?.seal(tenant, &input.secret)?;
        (Some(sealed), Some(key_id))
    };
    let sql = format!(
        "INSERT INTO webhook_subscription
             (tenant_id, board_id, url, secret_ciphertext, secret_key_id, events, excluded_events, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {COLUMNS}"
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.board_id)
        .bind(&input.url)
        .bind(ciphertext)
        .bind(key_id)
        .bind(&input.events)
        .bind(&input.excluded_events)
        .bind(input.active)
        .fetch_one(&mut *conn)
        .await?)
}

/// A stored secret as plaintext: open a sealed row, pass a pre-`0014` row through unchanged.
///
/// The NULL `secret_key_id` case exists only for rows written before migration `0014`; it is what lets a
/// deployment run the new binary before `conveyor admin reseal-webhooks` has converted every tenant.
fn open_stored_secret(
    tenant: TenantId,
    ciphertext: Vec<u8>,
    key_id: Option<String>,
) -> Result<Vec<u8>, WebhookStoreError> {
    match key_id {
        None => Ok(ciphertext),
        Some(key_id) => Ok(crate::secret::sealer()?.open(tenant, &ciphertext, &key_id)?),
    }
}

/// Read one subscription inside the caller's scoped transaction. Never returns the secret.
pub async fn get_webhook_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<WebhookRecord>, sqlx::Error> {
    let sql =
        format!("SELECT {COLUMNS} FROM webhook_subscription WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// The secret of one subscription, for the delivery path only.
pub async fn secret_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Vec<u8>>, WebhookStoreError> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(
        "SELECT secret_ciphertext, secret_key_id
           FROM webhook_subscription WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((Some(ciphertext), key_id)) = row else {
        return Ok(None);
    };
    Ok(Some(open_stored_secret(tenant, ciphertext, key_id)?))
}

/// Every subscription that fires for a board: its own rows plus the tenant-wide ones.
pub async fn list_webhooks_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Vec<WebhookRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM webhook_subscription
          WHERE tenant_id = $1 AND (board_id = $2 OR board_id IS NULL)
          ORDER BY created_at, id"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(board_id)
        .fetch_all(&mut *conn)
        .await
}

/// Update a subscription, guarded by its version. `None` means absent or stale.
pub async fn update_webhook_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &WebhookUpdate,
    expected_version: Version,
) -> Result<Option<WebhookRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let sql = format!(
        "UPDATE webhook_subscription
            SET url = coalesce($3, url),
                events = coalesce($4, events),
                excluded_events = coalesce($5, excluded_events),
                active = coalesce($6, active),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING {COLUMNS}"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.url.as_deref())
        .bind(patch.events.as_ref())
        .bind(patch.excluded_events.as_ref())
        .bind(patch.active)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await
}

/// Delete a subscription, guarded by its version. `false` means absent or stale.
pub async fn delete_webhook_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<bool, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let rows = sqlx::query(
        "DELETE FROM webhook_subscription WHERE tenant_id = $1 AND id = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// The subscriptions one event must be delivered to, in a stable order.
///
/// `events = '{}'` means "every event"; `excluded_events` always wins. A board subscription
/// matches only its own board; a tenant-wide row (`board_id IS NULL`) matches every board.
pub async fn matching_webhooks_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Option<Uuid>,
    event_type: &str,
) -> Result<Vec<DeliveryTarget>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        url: String,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, url
           FROM webhook_subscription
          WHERE tenant_id = $1
            AND active
            AND (
                  ($2::uuid IS NULL AND board_id IS NULL)
                  OR ($2::uuid IS NOT NULL AND (board_id = $2 OR board_id IS NULL))
                )
            AND (cardinality(events) = 0 OR $3 = ANY(events))
            AND NOT ($3 = ANY(excluded_events))
          ORDER BY id",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(event_type)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| DeliveryTarget {
            id: row.id,
            url: row.url,
            // Deliberately not read here: the emitter only needs the id to enqueue, and the delivery worker
            // reads the secret again at send time through `delivery_target`. Selecting it here would decrypt
            // one secret per subscription per event for a value nothing uses.
            secret: None,
            active: true,
        })
        .collect())
}

/// Enqueue one delivery job on the caller's transaction.
///
/// `(tenant, kind, dedupe_key)` is unique among live jobs, so a re-emitted event does not
/// start a second race; the emitter passes `<event id>:<subscription id>`.
pub async fn enqueue_delivery_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    dedupe_key: &str,
    payload: &Value,
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(DELIVER_KIND)
    .bind(dedupe_key)
    .bind(payload)
    .bind(max_attempts)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Read a delivery's target in its own tenant scope, for the worker.
///
/// `None` means the subscription is gone; the job is then dead-lettered, because retrying
/// cannot resurrect it.
pub async fn delivery_target(
    pool: &PgPool,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DeliveryTarget>, WebhookStoreError> {
    let mut tx = begin_scoped(pool, tenant).await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        url: String,
        secret_ciphertext: Option<Vec<u8>>,
        secret_key_id: Option<String>,
        active: bool,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, url, secret_ciphertext, secret_key_id, active
           FROM webhook_subscription WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let secret = match row.secret_ciphertext {
        None => None,
        Some(ciphertext) => Some(open_stored_secret(tenant, ciphertext, row.secret_key_id)?),
    };
    Ok(Some(DeliveryTarget {
        id: row.id,
        url: row.url,
        secret,
        active: row.active,
    }))
}

/// Convert every pre-`0014` plaintext webhook secret to a sealed one.
///
/// Idempotent: a second run selects nothing. Scoped per tenant because the table is RLS+FORCE — an unscoped
/// statement sees zero rows and would report success while converting nothing (`webhook_seal.rs` runs this as
/// `conveyor_migrate`, the same subject-to-RLS role production uses).
///
/// ⚠️ A NULL `secret_key_id` is read as "plaintext" and re-sealed. If a sealed blob ever lost its key id, this
/// would seal the seal and delivery would then present ciphertext as the bearer token. Nothing can write that
/// state today — `insert_webhook_tx` sets both columns together — so the risk is recorded here rather than
/// guarded by a heuristic that could not actually tell the two apart.
pub async fn reseal_legacy(pool: &PgPool) -> Result<ResealReport, WebhookStoreError> {
    // Fail before writing anything if the key is absent.
    crate::secret::sealer()?;
    let tenants: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenant ORDER BY id")
        .fetch_all(pool)
        .await?;
    let mut report = ResealReport::default();
    for raw in tenants {
        let tenant = TenantId::from_uuid(raw).map_err(|_| {
            WebhookStoreError::Database(sqlx::Error::Protocol("a non-uuid tenant id".to_owned()))
        })?;
        let mut tx = begin_scoped(pool, tenant).await?;
        let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT id, secret_ciphertext FROM webhook_subscription
              WHERE tenant_id = $1 AND secret_ciphertext IS NOT NULL AND secret_key_id IS NULL
              FOR UPDATE",
        )
        .bind(raw)
        .fetch_all(&mut *tx)
        .await?;
        for (id, plaintext) in rows {
            let (sealed, key_id) = crate::secret::sealer()?.seal(tenant, &plaintext)?;
            sqlx::query(
                "UPDATE webhook_subscription
                    SET secret_ciphertext = $2, secret_key_id = $3, updated_at = now()
                  WHERE tenant_id = $1 AND id = $4",
            )
            .bind(raw)
            .bind(sealed)
            .bind(key_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            report.resealed += 1;
        }
        tx.commit().await?;
        report.tenants += 1;
    }
    Ok(report)
}

/// What [`reseal_legacy`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResealReport {
    /// Rows converted from plaintext to sealed.
    pub resealed: u64,
    /// Tenants the scan walked.
    pub tenants: u64,
}

#[cfg(test)]
mod tests {
    use super::{DeliveryTarget, NewWebhook};

    #[test]
    fn debug_never_prints_the_signing_secret() {
        let secret = b"super-secret-token".to_vec();
        let decimal = format!("{secret:?}");
        let new = NewWebhook {
            board_id: None,
            url: "https://example.test/hook".into(),
            secret: secret.clone(),
            events: Vec::new(),
            excluded_events: Vec::new(),
            active: true,
        };
        let delivery = DeliveryTarget {
            id: uuid::Uuid::now_v7(),
            url: "https://example.test/hook".into(),
            secret: Some(secret),
            active: true,
        };
        for rendered in [format!("{new:?}"), format!("{delivery:?}")] {
            assert!(
                rendered.contains("[redacted]"),
                "the redaction marker must be visible: {rendered}"
            );
            assert!(
                !rendered.contains("super-secret-token"),
                "the plaintext secret leaked into Debug: {rendered}"
            );
            assert!(
                !rendered.contains(&decimal),
                "the byte-array form of the secret leaked into Debug: {rendered}"
            );
        }
    }
}
