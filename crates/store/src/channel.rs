//! Channel bindings and per-user channel preferences (S06 §5.6, data-arch §2.6).
//!
//! A binding is one route of one channel kind in one tenant. `(tenant_id, channel_kind,
//! route_key)` is its natural key, so `channels.bind` upserts on it and `channels.unbind`
//! deletes by it — neither needs a read first. `config` is read whole with the row and holds
//! references only (chat id, topic id, bot ref); no credential is ever written there
//! (data-arch §5).
//!
//! A preference is one row per (user, channel kind). `user_id` is resolved **before** the
//! write: `app_user` is global, so the FK alone would accept an id that never existed
//! (CONTRACT §2). It is a global parent on purpose — a preference names a person, and the
//! tenant scoping is the row's own `tenant_id`.
//!
//! Every function takes the [`TenantId`] immediately after the connection and runs in the
//! caller's already-scoped transaction (`_tx`), so the write shares the caller's outbox
//! event (CONTRACT §7). `channel_kind` is plain text: since S06d the database CHECK owns only
//! the spelling (`^[a-z][a-z0-9_]{1,31}$`), and the connector registry in `conveyor-actions`
//! decides which kinds this build serves — a new connector needs no migration.
//!
//! A provisioned connector has one machine identity per `(tenant, kind)`:
//! `app_user(kind='service', oidc_subject='service:<kind>:<tenant>')` plus a `kata_executor`
//! membership. The subject is deterministic, so it is idempotent and needs no binding column;
//! [`ensure_service_account_tx`] creates it, [`service_account_tx`] resolves it.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::scope::begin_scoped;
use crate::tenants;

/// The kinds this build's core names directly.
///
/// The full set is the connector registry in `conveyor-actions`; this constant exists for
/// callers that need the built-ins without depending on the actions crate. The database CHECK
/// owns only the spelling, so an adapter may declare another kind without a migration.
pub const CHANNEL_KINDS: [&str; 4] = ["web", "telegram", "mcp", "webhook"];

/// A binding to create or replace, keyed by `(channel_kind, route_key)`.
#[derive(Clone, Debug)]
pub struct NewChannelBinding {
    /// `web`, `telegram` or `mcp`.
    pub channel_kind: String,
    /// The route within the channel: a project key or `default`.
    pub route_key: String,
    /// Channel references (chat id, topic id, bot ref); read whole with the row.
    pub config: Value,
    /// Whether the binding is live.
    pub active: bool,
}

impl NewChannelBinding {
    /// A live binding with an empty config.
    pub fn new(channel_kind: &str, route_key: &str) -> Self {
        Self {
            channel_kind: channel_kind.to_owned(),
            route_key: route_key.to_owned(),
            config: Value::Object(serde_json::Map::new()),
            active: true,
        }
    }
}

/// One `channel_binding` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ChannelBindingRecord {
    /// The row id.
    pub id: Uuid,
    /// `web`, `telegram` or `mcp`.
    pub channel_kind: String,
    /// The route within the channel.
    pub route_key: String,
    /// Channel references; never a secret.
    pub config: Value,
    /// Whether the binding is live.
    pub active: bool,
    /// The row version (the `ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One `channel_preference` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ChannelPreferenceRecord {
    /// The row id.
    pub id: Uuid,
    /// The person the preference belongs to.
    pub user_id: Uuid,
    /// The channel the preference applies to.
    pub channel_kind: String,
    /// Whether the channel is enabled for this person.
    pub enabled: bool,
    /// The quiet window, read whole when a send is considered.
    pub quiet_hours: Value,
    /// The row version (the `ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// Why a channel write failed.
#[derive(Debug, thiserror::Error)]
pub enum ChannelStoreError {
    /// A referenced parent (`channel_preference.user_id`) does not exist.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const BINDING_COLUMNS: &str =
    "id, channel_kind, route_key, config, active, version, created_at, updated_at";

const PREFERENCE_COLUMNS: &str = "id, user_id, channel_kind, enabled, quiet_hours, version, \
                                  created_at, updated_at";

/// Read one binding by id inside the caller's scoped transaction.
pub async fn get_binding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ChannelBindingRecord>, sqlx::Error> {
    let sql =
        format!("SELECT {BINDING_COLUMNS} FROM channel_binding WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
}

/// Read one binding by its id, whichever **active** tenant owns it.
///
/// This is the inbound channel callback's resolver: a webhook names a binding, never a
/// tenant, so the tenant has to be derived. `tenant` is the only table without RLS
/// (`data-architecture §2.1`), so each active tenant's scope is entered in turn and the row
/// read inside it — there is no `BYPASSRLS` role to cross tenants with.
///
/// `None` covers both "no such binding" and "more than one": `channel_binding.id` is the
/// primary key, so a second match means the id is not resolvable and the caller must refuse
/// it (`404`), not pick one at random.
pub async fn find_binding_by_id(
    pool: &PgPool,
    binding_id: Uuid,
) -> Result<Option<(TenantId, ChannelBindingRecord)>, sqlx::Error> {
    let mut found: Option<(TenantId, ChannelBindingRecord)> = None;
    for tenant in tenants::active_ids(pool).await? {
        let mut tx = begin_scoped(pool, tenant).await?;
        let row = get_binding_tx(&mut tx, tenant, binding_id).await?;
        tx.commit().await?;
        let Some(row) = row else {
            continue;
        };
        if found.is_some() {
            tracing::error!(
                binding = %binding_id,
                "one channel binding id resolved in more than one tenant"
            );
            return Ok(None);
        }
        found = Some((tenant, row));
    }
    Ok(found)
}

/// The body of the message a delivery hands to a channel, read inside a scoped transaction.
///
/// The drainer holds only the `delivery` row (`message_id`), and the transport speaks what
/// the person wrote — never a rendering of the row. A missing message is `None`, which the
/// caller must treat as a permanent failure rather than sending an empty string.
pub async fn message_body_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    message_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT body FROM message WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(message_id)
        .fetch_optional(&mut *conn)
        .await
}

/// Every binding of the tenant, ordered by `(channel_kind, route_key)`.
///
/// The list is bounded by the tenant's configuration, not by history: a binding is upserted
/// in place and deleted, never appended per event.
pub async fn list_bindings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<ChannelBindingRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {BINDING_COLUMNS} FROM channel_binding
          WHERE tenant_id = $1
          ORDER BY channel_kind, route_key"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await
}

/// Create the binding, or replace its `config`/`active` when the natural key already exists.
///
/// The upsert is the conflict target `(tenant_id, channel_kind, route_key)`, so a retried
/// `channels.bind` converges on one row instead of raising a unique violation.
pub async fn upsert_binding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewChannelBinding,
) -> Result<ChannelBindingRecord, ChannelStoreError> {
    let sql = format!(
        "INSERT INTO channel_binding (tenant_id, channel_kind, route_key, config, active)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT ON CONSTRAINT channel_binding_tenant_channel_route_key DO UPDATE
            SET config = excluded.config,
                active = excluded.active,
                version = channel_binding.version + 1,
                updated_at = now()
         RETURNING {BINDING_COLUMNS}"
    );
    let row: ChannelBindingRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.channel_kind)
        .bind(&input.route_key)
        .bind(&input.config)
        .bind(input.active)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Delete a binding by its natural key. False when there was nothing to delete.
///
/// `channels.unbind` names `(channel_kind, route_key)`, not an id, so the delete uses the
/// same key the bind upserts; a replay is a no-op rather than a 404 path.
pub async fn delete_binding_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
    route_key: &str,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "DELETE FROM channel_binding
          WHERE tenant_id = $1 AND channel_kind = $2 AND route_key = $3",
    )
    .bind(tenant.as_uuid())
    .bind(channel_kind)
    .bind(route_key)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Read one preference by `(user, channel kind)` inside the caller's scoped transaction.
pub async fn get_preference_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    channel_kind: &str,
) -> Result<Option<ChannelPreferenceRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {PREFERENCE_COLUMNS}
           FROM channel_preference
          WHERE tenant_id = $1 AND user_id = $2 AND channel_kind = $3"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(user_id)
        .bind(channel_kind)
        .fetch_optional(&mut *conn)
        .await
}

/// Create the preference, or update the fields that were supplied.
///
/// `enabled`/`quiet_hours` are partial on purpose: `notifications.update` may send one field
/// (`{channel_kind, enabled?}`), and `None` means "leave it as it is". A first insert fills
/// the documented defaults (`enabled = true`, `quiet_hours = '{}'`).
pub async fn upsert_preference_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    user_id: Uuid,
    channel_kind: &str,
    enabled: Option<bool>,
    quiet_hours: Option<&Value>,
) -> Result<ChannelPreferenceRecord, ChannelStoreError> {
    ensure_parent_tx(conn, tenant, Parent::AppUser, user_id).await?;
    let sql = format!(
        "INSERT INTO channel_preference (tenant_id, user_id, channel_kind, enabled, quiet_hours)
         VALUES ($1, $2, $3, coalesce($4::boolean, true), coalesce($5::jsonb, '{{}}'::jsonb))
         ON CONFLICT ON CONSTRAINT channel_preference_tenant_user_channel_key DO UPDATE
            SET enabled = coalesce($4::boolean, channel_preference.enabled),
                quiet_hours = coalesce($5::jsonb, channel_preference.quiet_hours),
                version = channel_preference.version + 1,
                updated_at = now()
         RETURNING {PREFERENCE_COLUMNS}"
    );
    let row: ChannelPreferenceRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(user_id)
        .bind(channel_kind)
        .bind(enabled)
        .bind(quiet_hours.cloned())
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

// ── provisioned connector identities (S06d) ─────────────────────────────────────────────

/// The deterministic subject of a provisioned connector's machine identity.
pub fn service_subject(tenant: TenantId, channel_kind: &str) -> String {
    format!("service:{channel_kind}:{}", tenant.as_uuid())
}

/// Find the tenant's provisioned identity for `channel_kind`, if it exists **and** still has
/// the machine membership. A deleted membership is treated as absent: the identity cannot act.
pub async fn service_account_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let subject = service_subject(tenant, channel_kind);
    sqlx::query_scalar(
        "SELECT u.id
           FROM app_user u
           JOIN membership m ON m.user_id = u.id
          WHERE u.oidc_subject = $1
            AND u.kind = 'service'
            AND u.status = 'active'
            AND m.tenant_id = $2
            AND m.role = 'kata_executor'",
    )
    .bind(subject)
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await
}

/// The same lookup from the pool, in its own tenant-scoped transaction.
pub async fn service_account(
    db: &PgPool,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let mut tx = begin_scoped(db, tenant).await?;
    let found = service_account_tx(&mut tx, tenant, channel_kind).await?;
    tx.commit().await?;
    Ok(found)
}

/// Ensure the tenant's provisioned identity exists, returning its id.
///
/// Idempotent under concurrency: the partial unique index on `oidc_subject` makes the second
/// `app_user` insert a no-op, and the membership's natural key makes the second grant a no-op.
/// `app_user` is global (no RLS) while `membership` is FORCE RLS, so the caller must pass a
/// transaction already scoped to `tenant` (`begin_scoped` / an action's handler tx).
pub async fn ensure_service_account_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    channel_kind: &str,
) -> Result<Uuid, sqlx::Error> {
    let subject = service_subject(tenant, channel_kind);
    sqlx::query(
        "INSERT INTO app_user (id, oidc_subject, display_name, kind, status)
         VALUES (uuidv7(), $1, $2, 'service', 'active')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL DO NOTHING",
    )
    .bind(&subject)
    .bind(format!("{channel_kind} connector"))
    .execute(&mut *conn)
    .await?;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM app_user WHERE oidc_subject = $1")
        .bind(&subject)
        .fetch_one(&mut *conn)
        .await?;
    // The synthetic machine role: the channel's action grants, not this role, are the bound.
    // `members.set_role` refuses it for a human, and this path is a service row by construction.
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role)
         VALUES ($1, $2, 'kata_executor')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    Ok(user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_binding_is_live_and_has_an_empty_config() {
        let binding = NewChannelBinding::new("telegram", "default");
        assert_eq!(binding.channel_kind, "telegram");
        assert_eq!(binding.route_key, "default");
        assert!(binding.active);
        assert_eq!(binding.config, Value::Object(serde_json::Map::new()));
    }

    #[test]
    fn the_advertised_kinds_are_the_built_in_registry() {
        assert_eq!(CHANNEL_KINDS, ["web", "telegram", "mcp", "webhook"]);
    }

    #[test]
    fn a_service_subject_is_deterministic_and_tenant_scoped() {
        let id = Uuid::now_v7();
        let tenant = TenantId::from_uuid(id).expect("non-nil");
        assert_eq!(
            service_subject(tenant, "telegram"),
            format!("service:telegram:{id}")
        );
    }
}
