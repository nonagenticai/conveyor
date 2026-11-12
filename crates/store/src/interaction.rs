//! The multicast dispatch ledger (S06d).
//!
//! An **interaction is an approval** (a question is an approval, CONTRACT §8). When it is
//! pending, one dispatcher tick offers it to every eligible recipient — machine connector
//! bindings and human web inboxes — and writes one `interaction_dispatch` row per offer.
//! That row's primary key is the fan-out idempotency: a restarted or duplicated tick inserts
//! nothing new, and the row is the witness the close sweep uses to expire the losing channels.
//!
//! The tables are tenant data (RLS); every function takes the caller's already-scoped
//! transaction, like the rest of the store.

use conveyor_domain::TenantId;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// One pending interaction the dispatcher may fan out.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PendingApproval {
    /// The approval id (the interaction).
    pub id: Uuid,
    /// The gate name.
    pub gate: String,
    /// The gate's kind (`gate`, `eval`, `procedure`, `workflow`, …).
    pub kind: String,
    /// The eval id, when this is a human-only sign-off gate.
    pub eval_id: Option<Uuid>,
    /// The question text, from `meta.description` or `meta.ask`.
    pub description: Option<String>,
    /// What the requester proposed, when recorded.
    pub proposed_answer: Option<String>,
}

/// One dispatch row to close.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OpenDispatch {
    pub approval_id: Uuid,
    pub channel_kind: String,
    pub recipient_ref: String,
    pub binding_id: Option<Uuid>,
    pub message_id: Uuid,
}

/// A human member eligible for a web inbox delivery.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct WebRecipient {
    pub user_id: Uuid,
    pub role: String,
}

/// Pending approvals, oldest first; the dispatcher bounds one tick.
pub async fn pending_approvals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<PendingApproval>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, gate, kind, eval_id,
                coalesce(meta->>'description', meta->>'ask') AS description,
                meta->>'proposed_answer' AS proposed_answer
           FROM approval
          WHERE tenant_id = $1 AND status = 'pending'
          ORDER BY created_at, id
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit.clamp(1, 500))
    .fetch_all(&mut *conn)
    .await
}

/// The recipients already offered this interaction.
pub async fn dispatched_recipients_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT channel_kind, recipient_ref FROM interaction_dispatch
          WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_all(&mut *conn)
    .await
}

/// Offer one interaction to one recipient. `false` means the row already existed.
#[allow(clippy::too_many_arguments)]
pub async fn insert_dispatch_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    channel_kind: &str,
    recipient_ref: &str,
    binding_id: Option<Uuid>,
    message_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO interaction_dispatch
             (tenant_id, approval_id, channel_kind, recipient_ref, binding_id, message_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, approval_id, channel_kind, recipient_ref) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(channel_kind)
    .bind(recipient_ref)
    .bind(binding_id)
    .bind(message_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Dispatches whose interaction is terminal and whose close notice has not been handled.
pub async fn open_dispatches_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<OpenDispatch>, sqlx::Error> {
    sqlx::query_as(
        "SELECT d.approval_id, d.channel_kind, d.recipient_ref, d.binding_id, d.message_id
           FROM interaction_dispatch d
           JOIN approval a ON a.tenant_id = d.tenant_id AND a.id = d.approval_id
          WHERE d.tenant_id = $1
            AND d.closed_notified_at IS NULL
            AND a.status IN ('approved', 'rejected', 'expired', 'voided')
          ORDER BY d.approval_id, d.channel_kind, d.recipient_ref
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit.clamp(1, 500))
    .fetch_all(&mut *conn)
    .await
}

/// Stamp one dispatch's close notice; a second tick finds nothing to do.
pub async fn mark_dispatch_notified_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    channel_kind: &str,
    recipient_ref: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE interaction_dispatch
            SET closed_notified_at = now()
          WHERE tenant_id = $1 AND approval_id = $2
            AND channel_kind = $3 AND recipient_ref = $4
            AND closed_notified_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(channel_kind)
    .bind(recipient_ref)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// The channel named by the most recent decision on an interaction, when one recorded it.
pub async fn resolved_via_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT via->>'channel'
           FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await
    .map(|found: Option<Option<String>>| found.flatten())
}

/// Human members with the web channel not disabled (absence of a preference = enabled).
pub async fn web_recipients_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<WebRecipient>, sqlx::Error> {
    sqlx::query_as(
        "SELECT m.user_id, m.role
           FROM membership m
           JOIN app_user u ON u.id = m.user_id
          WHERE m.tenant_id = $1
            AND u.kind = 'human'
            AND u.status = 'active'
            AND NOT EXISTS (
                SELECT 1 FROM channel_preference p
                 WHERE p.tenant_id = m.tenant_id AND p.user_id = m.user_id
                   AND p.channel_kind = 'web' AND p.enabled = false
            )
          ORDER BY m.user_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await
}

/// The tenant's role→scope overrides, for evaluating a member's response scope.
pub async fn role_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<(String, String, bool)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT role, scope, allowed FROM tenant_role_scope WHERE tenant_id = $1 ORDER BY role, scope",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await
}

/// The body one recipient sees when an interaction is offered.
pub fn interaction_text(approval: &PendingApproval) -> String {
    let mut text = format!(
        "approval {} ({}/{})",
        approval.id, approval.kind, approval.gate
    );
    if let Some(description) = approval
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
    {
        text.push('\n');
        text.push_str(description.trim());
    }
    if let Some(proposed) = approval
        .proposed_answer
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        text.push_str("\nproposed: ");
        text.push_str(proposed.trim());
    }
    text.push_str(&format!(
        "\nreply: approve {} [note] | reject {} [note]",
        approval.id, approval.id
    ));
    text
}

/// A message's body for a dispatch, used by the close sweep's notice.
pub fn close_text(approval_id: Uuid, status: &str, resolver: Option<&str>, notice: bool) -> String {
    if notice {
        format!(
            "approval {approval_id} was {status} via {}; no answer is needed here.",
            resolver.unwrap_or("another channel")
        )
    } else {
        format!("approval {approval_id} is {status}")
    }
}

/// Read one message's metadata (the close message needs only the id/body).
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

/// A small helper for callers that need the tenant-scoped transaction around one read.
pub async fn begin(
    db: &PgPool,
    tenant: TenantId,
) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
    begin_scoped(db, tenant).await
}
