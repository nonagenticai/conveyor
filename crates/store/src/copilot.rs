//! Conversations, turns and copilot proposals (S10).
//!
//! Two rules shape this module:
//!
//! 1. **`seq` is allocated under the conversation row's lock.** `append_turn_tx` takes
//!    `SELECT … FOR UPDATE` on `conversation` before reading `MAX(seq)`, so two concurrent appends
//!    serialize on one row instead of racing the `uq (conversation_id, seq)` index. The index is the
//!    backstop; the lock is why the happy path never hits it.
//! 2. **A terminal turn is immutable.** `finish_turn_tx` updates only `meta->>'status' = 'streaming'`
//!    and returns `None` otherwise — a late writer is a no-op, not a second outcome (S10 trap 4).
//!
//! Proposals are `approval` rows (`kind = 'copilot.action'`), not a table of their own (S10-D2). The
//! claim is a conditional `UPDATE … RETURNING` whose lease makes a killed worker's claim reclaimable;
//! the idempotency key `proposal:<approval_id>` makes the re-Invocation safe.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};

/// The columns every `Turn` row carries.
pub const TURN_COLUMNS: &str = "id, tenant_id, conversation_id, seq, role, body, meta, trace_id, \
                                created_at, updated_at";

/// The columns every `Conversation` row carries.
pub const CONVERSATION_COLUMNS: &str =
    "id, tenant_id, user_id, project_id, title, status, created_at, updated_at";

/// One `conversation` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Conversation {
    /// Primary key.
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// The person the conversation belongs to.
    pub user_id: Option<Uuid>,
    /// The project it discusses, when there is one.
    pub project_id: Option<Uuid>,
    /// Display title; `''` until something sets it.
    pub title: String,
    /// `active` or `archived`.
    pub status: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last activity.
    pub updated_at: DateTime<Utc>,
}

/// One `conversation_turn` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Turn {
    /// Primary key.
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// The conversation it belongs to.
    pub conversation_id: Uuid,
    /// Position inside the conversation; unique per conversation.
    pub seq: i32,
    /// `user`, `assistant` or `system`.
    pub role: String,
    /// The message text.
    pub body: String,
    /// The turn record (S10 §5.4).
    pub meta: Value,
    /// Trace id inherited from the request.
    pub trace_id: Option<String>,
    /// Insertion time.
    pub created_at: DateTime<Utc>,
    /// Last update (the terminal update, or the reaper's).
    pub updated_at: DateTime<Utc>,
}

/// A conversation to create.
#[derive(Clone, Debug)]
pub struct NewConversation {
    /// The person it belongs to.
    pub user_id: Option<Uuid>,
    /// The project it discusses.
    pub project_id: Option<Uuid>,
    /// Display title.
    pub title: String,
}

impl NewConversation {
    /// A conversation with no project and no title.
    pub fn new(user_id: Option<Uuid>) -> Self {
        Self {
            user_id,
            project_id: None,
            title: String::new(),
        }
    }
}

/// A turn to append.
#[derive(Clone, Debug)]
pub struct NewTurn {
    /// The conversation it belongs to.
    pub conversation_id: Uuid,
    /// `user`, `assistant` or `system`.
    pub role: String,
    /// The message text.
    pub body: String,
    /// The turn record; `{"status":"streaming"}` for an assistant turn inserted at start.
    pub meta: Value,
    /// Trace id inherited from the request.
    pub trace_id: Option<String>,
}

/// Why a conversation/turn operation failed.
#[derive(Debug, thiserror::Error)]
pub enum CopilotStoreError {
    /// No such conversation in this tenant.
    #[error("the conversation does not exist in this tenant")]
    ConversationNotFound,
    /// No such turn in this tenant.
    #[error("the turn does not exist in this tenant")]
    TurnNotFound,
    /// A turn may not be appended to an archived conversation.
    #[error("conversation {0} is archived; no turn may be appended")]
    Archived(Uuid),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// A referenced parent (`user_id`, `project_id`) does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Insert a conversation inside the caller's scoped transaction.
///
/// Both optional parents are resolved in the caller's tenant first (CONTRACT §2): a cross-tenant
/// `project_id` is a store error, not an FK acceptance.
pub async fn insert_conversation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewConversation,
) -> Result<Conversation, CopilotStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.user_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    let sql = format!(
        "INSERT INTO conversation (tenant_id, user_id, project_id, title)
         VALUES ($1, $2, $3, $4)
         RETURNING {CONVERSATION_COLUMNS}"
    );
    let row: Conversation = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.user_id)
        .bind(input.project_id)
        .bind(&input.title)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one conversation in this tenant.
pub async fn conversation(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Conversation>, CopilotStoreError> {
    let sql =
        format!("SELECT {CONVERSATION_COLUMNS} FROM conversation WHERE tenant_id = $1 AND id = $2");
    let row: Option<Conversation> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Append one turn, allocating `seq` under the conversation row's lock.
///
/// The lock is the ordering guarantee; `uq (conversation_id, seq)` is the backstop. An archived
/// conversation refuses the append — archiving is a display choice, but a conversation that keeps
/// growing is not archived.
pub async fn append_turn_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTurn,
) -> Result<Turn, CopilotStoreError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM conversation WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(input.conversation_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(status) = status else {
        return Err(CopilotStoreError::ConversationNotFound);
    };
    if status == "archived" {
        return Err(CopilotStoreError::Archived(input.conversation_id));
    }
    let seq: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM conversation_turn
          WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(input.conversation_id)
    .fetch_one(&mut *conn)
    .await?;
    let sql = format!(
        "INSERT INTO conversation_turn (tenant_id, conversation_id, seq, role, body, meta, trace_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {TURN_COLUMNS}"
    );
    let row: Turn = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.conversation_id)
        .bind(seq)
        .bind(&input.role)
        .bind(&input.body)
        .bind(&input.meta)
        .bind(&input.trace_id)
        .fetch_one(&mut *conn)
        .await?;
    // Activity: the conversation's own clock is what the user's list sorts on.
    sqlx::query("UPDATE conversation SET updated_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(input.conversation_id)
        .execute(&mut *conn)
        .await?;
    Ok(row)
}

/// Finish a streaming turn: one terminal `meta`, never a second one.
///
/// `None` means the turn was already terminal (or does not exist in this tenant) — the caller must
/// not write an event in that case, because nothing changed.
pub async fn finish_turn_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    turn_id: Uuid,
    body: &str,
    meta: &Value,
) -> Result<Option<Turn>, CopilotStoreError> {
    let sql = format!(
        "UPDATE conversation_turn
            SET body = $4, meta = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND meta->>'status' = 'streaming'
          RETURNING {TURN_COLUMNS}"
    );
    let row: Option<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(turn_id)
        .bind(meta)
        .bind(body)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Complete a `proposed` turn once its proposal has executed.
///
/// S10 §7 (b) requires the turn to end `completed` with the executed tool call recorded, so this is
/// the one permitted write to a turn that is already terminal: it is guarded to `status='proposed'`
/// and both writes — the status and the tool call — are one statement. `None` means the turn was not
/// in `proposed` (already completed, or reaped), and the caller must not write a second event.
pub async fn complete_proposed_turn_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    turn_id: Uuid,
    action: &str,
    invocation_id: Uuid,
) -> Result<Option<Turn>, CopilotStoreError> {
    let sql = format!(
        "UPDATE conversation_turn
            SET meta = jsonb_set(
                    jsonb_set(meta, '{{status}}', '\"completed\"'::jsonb),
                    '{{tool_calls}}',
                    coalesce(meta->'tool_calls', '[]'::jsonb)
                      || jsonb_build_array(jsonb_build_object(
                             'action', $3::text,
                             'invocation_id', $4::text,
                             'outcome', 'ok'::text))
                ),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND meta->>'status' = 'proposed'
          RETURNING {TURN_COLUMNS}"
    );
    let row: Option<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(turn_id)
        .bind(action)
        .bind(invocation_id.to_string())
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one turn in this tenant.
pub async fn turn(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Turn>, CopilotStoreError> {
    let sql =
        format!("SELECT {TURN_COLUMNS} FROM conversation_turn WHERE tenant_id = $1 AND id = $2");
    let row: Option<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// A page of turns, ascending by `seq`, optionally after a cursor.
pub async fn turns_page(
    conn: &mut PgConnection,
    tenant: TenantId,
    conversation_id: Uuid,
    after_seq: Option<i32>,
    limit: i64,
) -> Result<Vec<Turn>, CopilotStoreError> {
    let sql = format!(
        "SELECT {TURN_COLUMNS} FROM conversation_turn
          WHERE tenant_id = $1 AND conversation_id = $2
            AND ($3::int IS NULL OR seq > $3)
          ORDER BY seq
          LIMIT $4"
    );
    let rows: Vec<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(conversation_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// The most recent turns, returned oldest-first — the model's history window (S10 open question 3:
/// the last 20 turns).
pub async fn recent_turns(
    conn: &mut PgConnection,
    tenant: TenantId,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<Turn>, CopilotStoreError> {
    let sql = format!(
        "SELECT {TURN_COLUMNS} FROM (
             SELECT {TURN_COLUMNS} FROM conversation_turn
              WHERE tenant_id = $1 AND conversation_id = $2
              ORDER BY seq DESC
              LIMIT $3
         ) recent
         ORDER BY seq"
    );
    let rows: Vec<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// One copilot proposal as the executor reads it: the status that gates execution and the meta
/// that carries the action, its input and its proposer.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProposalRow {
    /// The approval id (the proposal id).
    pub id: Uuid,
    /// `pending | approved | rejected | expired | voided`.
    pub status: String,
    /// The proposal document written at creation.
    pub meta: Value,
}

/// Read one copilot proposal in this tenant.
pub async fn proposal(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProposalRow>, CopilotStoreError> {
    let row: Option<ProposalRow> = sqlx::query_as(
        "SELECT id, status, meta FROM approval
          WHERE tenant_id = $1 AND id = $2 AND kind = 'copilot.action'",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Claim an approved, unexecuted copilot proposal for execution.
///
/// The lease makes a killed worker's claim reclaimable: a claim older than `lease_secs` may be
/// taken again, and the idempotency key `proposal:<approval_id>` makes the second Invocation a
/// replay instead of a second effect. `None` means somebody else holds a live claim, or the
/// proposal is not approved/unexecuted.
pub async fn claim_proposal_execution_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    lease_secs: i64,
) -> Result<Option<Uuid>, CopilotStoreError> {
    let claimed: Option<Uuid> = sqlx::query_scalar(
        "UPDATE approval
            SET meta = jsonb_set(meta, '{execute_claim_at}', to_jsonb(now())),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2
            AND kind = 'copilot.action' AND status = 'approved'
            AND meta->>'executed_invocation_id' IS NULL
            AND (meta->>'execute_claim_at' IS NULL
                 OR (meta->>'execute_claim_at')::timestamptz
                    < now() - ($3::bigint * interval '1 second'))
          RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(lease_secs)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(claimed)
}

/// Record the invocation that executed a proposal — the step that makes it terminal.
pub async fn record_proposal_execution_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    invocation_id: Uuid,
) -> Result<bool, CopilotStoreError> {
    let done = sqlx::query(
        "UPDATE approval
            SET meta = (meta - 'execute_claim_at')
                       || jsonb_build_object('executed_invocation_id', $3::text,
                                             'executed_at', now()),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND kind = 'copilot.action'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(invocation_id.to_string())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The approved proposals the sweep may execute, oldest first.
pub async fn proposals_ready(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<Uuid>, CopilotStoreError> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND kind = 'copilot.action' AND status = 'approved'
            AND meta->>'executed_invocation_id' IS NULL
          ORDER BY updated_at
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Mark `streaming` turns older than `older_than_secs` as `interrupted`, returning them so the
/// caller can emit each turn's `conversation_turn.created` event in the same transaction.
pub async fn reap_streaming_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    older_than_secs: i64,
) -> Result<Vec<Turn>, CopilotStoreError> {
    let sql = format!(
        "UPDATE conversation_turn
            SET meta = jsonb_set(meta, '{{status}}', '\"interrupted\"'::jsonb),
                updated_at = now()
          WHERE tenant_id = $1
            AND meta->>'status' = 'streaming'
            AND created_at < now() - ($2::bigint * interval '1 second')
          RETURNING {TURN_COLUMNS}"
    );
    let rows: Vec<Turn> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(older_than_secs)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}
