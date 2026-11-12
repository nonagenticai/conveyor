//! Cards: create, read, move, claim and release.
//!
//! `board_id` is denormalised. It is never taken from the caller: on insert it is copied from
//! the list's row, and on move the `UPDATE … FROM list` re-copies it under the same predicate
//! that refuses a cross-board move (S03 §8 trap 8). A card therefore cannot disagree with its
//! lane about which board it is on.
//!
//! Every parent id (`list_id`, `card_type_id`, `parent_card_id`) is resolved in the caller's
//! tenant before the insert, and the target lane of a move is resolved before the update
//! (CONTRACT §2). A cross-tenant parent is a store error, never a silent FK acceptance.
//!
//! The claim is one conditional `UPDATE` on the card row — no lease table (data-arch §4).
//! Zero rows means "not granted"; there is no fallback and no grant on a database error: a
//! lease that grants itself when the arbiter is down is not a lease.

use conveyor_domain::{Card, TenantId, Version};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgConnection;
use sqlx::postgres::types::PgInterval;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx, ensure_parent_tx};
use crate::version_of;

/// A card to create. `board_id` is derived from `list_id`, never supplied.
#[derive(Clone, Debug)]
pub struct NewCard {
    /// The lane the card starts in.
    pub list_id: Uuid,
    /// The card's type.
    pub card_type_id: Uuid,
    /// The name; indexed for full-text search.
    pub name: String,
    /// Free text, empty by default; indexed for full-text search.
    pub description: String,
    /// Position within the list.
    pub position: Decimal,
    /// A parent card, for subtasks.
    pub parent_card_id: Option<Uuid>,
    /// When the card is due.
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The initial assignee, when the card is created already claimed.
    pub assignee_id: Option<Uuid>,
    /// The creating user.
    pub created_by: Option<Uuid>,
    /// Kind-specific fields.
    pub meta: Value,
}

impl NewCard {
    /// A card at the end of a list (position `0`), with empty payloads.
    pub fn new(
        list_id: Uuid,
        card_type_id: Uuid,
        name: impl Into<String>,
        position: Decimal,
    ) -> Self {
        Self {
            list_id,
            card_type_id,
            name: name.into(),
            description: String::new(),
            position,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            meta: Value::Object(serde_json::Map::new()),
        }
    }
}

/// What a successful claim grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimGranted {
    /// The card's new (monotonic) fence. A completion must present it.
    pub claim_fence: i64,
    /// The card's new version, for the caller's `ETag`.
    pub version: Version,
}

/// Failures that are more than a database error.
#[derive(Debug, thiserror::Error)]
pub enum CardStoreError {
    /// The target list does not exist in this tenant (or is not visible to it).
    #[error("the list does not exist in this tenant")]
    ListNotFound,
    /// A `parent_card_id` names a card on another board.
    #[error("the parent card is on another board")]
    ParentCardOnAnotherBoard,
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct CardRow {
    id: Uuid,
    list_id: Uuid,
    card_type_id: Uuid,
    board_id: Uuid,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    assignee_id: Option<Uuid>,
    lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    lease_token: Option<Uuid>,
    claim_fence: i64,
    prev_list_id: Option<Uuid>,
    list_changed_at: chrono::DateTime<chrono::Utc>,
    version: i64,
}

impl From<CardRow> for Card {
    fn from(row: CardRow) -> Self {
        Self {
            id: row.id,
            list_id: row.list_id,
            card_type_id: row.card_type_id,
            board_id: row.board_id,
            archived_at: row.archived_at,
            assignee_id: row.assignee_id,
            lease_expires_at: row.lease_expires_at,
            lease_token: row.lease_token,
            claim_fence: row.claim_fence,
            prev_list_id: row.prev_list_id,
            list_changed_at: row.list_changed_at,
            version: version_of(row.version),
        }
    }
}

const COLUMNS: &str = "id, list_id, card_type_id, board_id, archived_at, assignee_id, \
                       lease_expires_at, lease_token, claim_fence, prev_list_id, \
                       list_changed_at, version";

/// Insert a card inside the caller's scoped transaction.
///
/// Every parent is resolved in this tenant first. The board id still comes from the list in
/// the same statement: a card created against a list that vanished between the resolution and
/// the insert reports [`CardStoreError::ListNotFound`].
///
/// A parent card, when supplied, must be on the same board as the target lane — a subtask
/// that hung off a card on another board would make the two boards disagree about which
/// board the card belongs to.
pub async fn insert_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewCard,
) -> Result<Card, CardStoreError> {
    ensure_parent_tx(conn, tenant, Parent::List, input.list_id).await?;
    ensure_parent_tx(conn, tenant, Parent::CardType, input.card_type_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.assignee_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.created_by).await?;
    if let Some(parent_card_id) = input.parent_card_id {
        ensure_parent_tx(conn, tenant, Parent::Card, parent_card_id).await?;
        let parent_board: Option<Uuid> =
            sqlx::query_scalar("SELECT board_id FROM card WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(parent_card_id)
                .fetch_optional(&mut *conn)
                .await?;
        let list_board: Option<Uuid> =
            sqlx::query_scalar("SELECT board_id FROM list WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(input.list_id)
                .fetch_optional(&mut *conn)
                .await?;
        match (parent_board, list_board) {
            (Some(parent_board), Some(list_board)) if parent_board != list_board => {
                return Err(CardStoreError::ParentCardOnAnotherBoard);
            }
            (Some(_), Some(_)) => {}
            // Both ids resolved above; only a concurrent delete can land here, and the
            // missing side names its own parent.
            (None, _) => {
                return Err(CardStoreError::Parent(ParentError::NotFound {
                    parent: Parent::Card,
                }));
            }
            (_, None) => return Err(CardStoreError::ListNotFound),
        }
    }
    let sql = format!(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, parent_card_id,
                           name, description, position, due_at, assignee_id, created_by, meta)
         SELECT $1, l.board_id, l.id, $3, $4, $5, $6, $7, $8, $9, $10, $11
           FROM list l
          WHERE l.tenant_id = $1 AND l.id = $2
         RETURNING {COLUMNS}"
    );
    let row: Option<CardRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.list_id)
        .bind(input.card_type_id)
        .bind(input.parent_card_id)
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.position)
        .bind(input.due_at)
        .bind(input.assignee_id)
        .bind(input.created_by)
        .bind(&input.meta)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Card::from).ok_or(CardStoreError::ListNotFound)
}

/// Read one card inside the caller's scoped transaction.
pub async fn get_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<Card>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM card WHERE tenant_id = $1 AND id = $2");
    let row: Option<CardRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Claim (or renew) a card. `None` means the claim was not granted.
///
/// The statement is data-architecture §4 verbatim: the version precondition makes a claim
/// from stale state lose, the lease arm makes a live lease held by someone else lose, and the
/// third arm allows the current assignee to renew. `claim_fence` is bumped on every grant.
///
/// `assignee` is resolved against `app_user` first: a bad id is a store error (404), not an
/// FK violation surfacing as a 500.
pub async fn claim_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    assignee: Uuid,
    lease_secs: i64,
    expected_version: Version,
) -> Result<Option<ClaimGranted>, CardStoreError> {
    ensure_parent_tx(conn, tenant, Parent::AppUser, assignee).await?;
    let lease = PgInterval {
        months: 0,
        days: 0,
        microseconds: lease_secs.max(1).saturating_mul(1_000_000),
    };
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let row: Option<(i64, i64)> = sqlx::query_as(
        "UPDATE card
            SET assignee_id = $3, lease_expires_at = now() + $4::interval,
                lease_token = uuidv7(), claim_fence = claim_fence + 1,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $5
            AND (lease_expires_at IS NULL OR lease_expires_at < now() OR assignee_id = $3)
        RETURNING claim_fence, version",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(assignee)
    .bind(lease)
    .bind(expected)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(claim_fence, version)| ClaimGranted {
        claim_fence,
        version: version_of(version),
    }))
}

/// Release a claim, fenced by the fence the holder was granted.
///
/// A stale fence writes nothing (`false`): the card moved beneath the sleeper, which is the
/// ABA window the fence exists to close. A released card leaves `claim_fence` untouched so
/// the next claim keeps increasing it (it is never reset).
pub async fn release_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    claim_fence: i64,
) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE card
            SET assignee_id = NULL, lease_expires_at = NULL, lease_token = NULL,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND claim_fence = $3 AND lease_token IS NOT NULL",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(claim_fence)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Extend a live claim, fenced by the `claim_fence` the holder was granted.
///
/// This is the renew path, so it deliberately does **not** take a version: an unrelated
/// `cards.update` bumps `version`, and a holder that slept must not lose its lease because
/// someone edited the card's title (S06 §8 trap 5). What it does re-check is the fence, which
/// only a new claim advances, and `lease_token IS NOT NULL`, which a release or a lease sweep
/// clears. Without that second arm a stale renew after a release would resurrect a claim the
/// holder no longer owns — the fence is intentionally never reset on release.
///
/// The fence does not change here: a renew is the same claim held longer. `version` does, so
/// every reader holding the old representation re-reads (CONTRACT §3).
pub async fn renew_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    claim_fence: i64,
    lease_secs: i64,
) -> Result<Option<ClaimGranted>, CardStoreError> {
    let lease = PgInterval {
        months: 0,
        days: 0,
        microseconds: lease_secs.max(1).saturating_mul(1_000_000),
    };
    let row: Option<(i64, i64)> = sqlx::query_as(
        "UPDATE card
            SET lease_expires_at = now() + $4::interval,
                version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND claim_fence = $3
            AND lease_token IS NOT NULL
        RETURNING claim_fence, version",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(claim_fence)
    .bind(lease)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(claim_fence, version)| ClaimGranted {
        claim_fence,
        version: version_of(version),
    }))
}

/// Clear every claim whose lease has expired, at most `limit` per call.
///
/// Returns `(card_id, claim_fence)` for each cleared row so the **caller** emits
/// `card.lease_expired` in the same transaction: this module writes state, the action/job
/// layer owns the outbox (CONTRACT §7). The fence is returned unchanged — it is the witness
/// the next claim will advance past.
///
/// `FOR UPDATE SKIP LOCKED` keeps two sweepers from clearing the same row; `limit` bounds one
/// sweep so a backlog cannot hold one long transaction. The deadline is the database's
/// `now()`, never a Rust clock (data-arch §8 trap 6).
pub async fn sweep_expired_leases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let limit = limit.max(1);
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "WITH expired AS (
             SELECT id FROM card
              WHERE tenant_id = $1 AND lease_expires_at IS NOT NULL
                AND lease_expires_at < now()
              ORDER BY lease_expires_at, id
              FOR UPDATE SKIP LOCKED
              LIMIT $2
         )
         UPDATE card c
            SET assignee_id = NULL, lease_expires_at = NULL, lease_token = NULL,
                version = c.version + 1, updated_at = now()
           FROM expired e
          WHERE c.id = e.id
        RETURNING c.id, c.claim_fence",
    )
    .bind(tenant.as_uuid())
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Move a card to another lane on the same board, guarded by its version.
///
/// The card and the target lane are both resolved in the caller's tenant first, so a
/// cross-tenant id is this error rather than a zero-row update that looks like a stale
/// version. Legality (`may_move`) is the domain's question; this statement enforces the
/// structural invariants the database owns: the target list must be on the card's board, an
/// archived card cannot move, and a stale version cannot win. `None` therefore means one of
/// those held — the caller re-reads once to distinguish 404 from 412 (data-arch §4).
pub async fn move_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    to_list_id: Uuid,
    position: Decimal,
    expected_version: Version,
) -> Result<Option<Card>, CardStoreError> {
    ensure_parent_tx(conn, tenant, Parent::Card, card_id).await?;
    ensure_parent_tx(conn, tenant, Parent::List, to_list_id).await?;
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    // The RETURNING list is qualified: `card` and `list` are both in scope in this statement
    // and an unqualified `id` is ambiguous (the same trap `jobs::claim` documents).
    let sql = "UPDATE card
            SET list_id = l.id, board_id = l.board_id, prev_list_id = card.list_id,
                list_changed_at = now(), position = $4, version = card.version + 1,
                updated_at = now()
           FROM list l
          WHERE card.tenant_id = $1 AND card.id = $2 AND card.version = $5
            AND card.archived_at IS NULL
            AND l.tenant_id = $1 AND l.id = $3 AND l.board_id = card.board_id
         RETURNING card.id, card.list_id, card.card_type_id, card.board_id, card.archived_at,
                   card.assignee_id, card.lease_expires_at, card.lease_token,
                   card.claim_fence, card.prev_list_id, card.list_changed_at, card.version";
    let row: Option<CardRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(to_list_id)
        .bind(position)
        .bind(expected)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

// ── the read record and the S05 transitions ─────────────────────────────────────────────

/// One card row with everything a read projection returns.
///
/// The domain [`Card`] carries the lifecycle (lane, stamp, lease); this record adds the
/// content columns and the denormalised card-type code, so `CardOut` needs no second query.
#[derive(Clone, Debug)]
pub struct CardRecord {
    /// The domain value.
    pub card: Card,
    /// The card's name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Order within the lane; exact decimal.
    pub position: Decimal,
    /// A parent card, for subtasks.
    pub parent_card_id: Option<Uuid>,
    /// When the card is due.
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
    /// When the card's due date was marked done.
    pub due_done_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The read-only stopwatch's start, when running.
    pub stopwatch_started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Accumulated stopwatch seconds.
    pub stopwatch_seconds: i32,
    /// The creating user.
    pub created_by: Option<Uuid>,
    /// Kind-specific fields.
    pub meta: Value,
    /// The card type's stable code (`card_type.code`).
    pub card_type_code: String,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct CardRecordRow {
    id: Uuid,
    list_id: Uuid,
    card_type_id: Uuid,
    board_id: Uuid,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
    assignee_id: Option<Uuid>,
    lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    lease_token: Option<Uuid>,
    claim_fence: i64,
    prev_list_id: Option<Uuid>,
    list_changed_at: chrono::DateTime<chrono::Utc>,
    version: i64,
    name: String,
    description: String,
    position: Decimal,
    parent_card_id: Option<Uuid>,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
    due_done_at: Option<chrono::DateTime<chrono::Utc>>,
    stopwatch_started_at: Option<chrono::DateTime<chrono::Utc>>,
    stopwatch_seconds: i32,
    created_by: Option<Uuid>,
    meta: Value,
    card_type_code: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<CardRecordRow> for CardRecord {
    fn from(row: CardRecordRow) -> Self {
        Self {
            card: Card {
                id: row.id,
                list_id: row.list_id,
                card_type_id: row.card_type_id,
                board_id: row.board_id,
                archived_at: row.archived_at,
                assignee_id: row.assignee_id,
                lease_expires_at: row.lease_expires_at,
                lease_token: row.lease_token,
                claim_fence: row.claim_fence,
                prev_list_id: row.prev_list_id,
                list_changed_at: row.list_changed_at,
                version: version_of(row.version),
            },
            name: row.name,
            description: row.description,
            position: row.position,
            parent_card_id: row.parent_card_id,
            due_at: row.due_at,
            due_done_at: row.due_done_at,
            stopwatch_started_at: row.stopwatch_started_at,
            stopwatch_seconds: row.stopwatch_seconds,
            created_by: row.created_by,
            meta: row.meta,
            card_type_code: row.card_type_code,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const RECORD_COLUMNS: &str = "c.id, c.list_id, c.card_type_id, c.board_id, c.archived_at, \
     c.assignee_id, c.lease_expires_at, c.lease_token, c.claim_fence, c.prev_list_id, \
     c.list_changed_at, c.version, c.name, c.description, c.position, c.parent_card_id, \
     c.due_at, c.due_done_at, c.stopwatch_started_at, c.stopwatch_seconds, c.created_by, \
     c.meta, c.created_at, c.updated_at, t.code AS card_type_code";

/// Read one card as a record inside the caller's scoped transaction.
pub async fn get_card_record_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CardRecord>, sqlx::Error> {
    let sql = format!(
        "SELECT {RECORD_COLUMNS}
           FROM card c
           JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.id = $2"
    );
    let row: Option<CardRecordRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// One keyset page of a board's live cards, ordered by `(position, id)`.
///
/// Never `OFFSET`: a concurrent drag inserts above the cursor and every later page would
/// skip a row (trap 6). `cursor` is the last row of the previous page.
pub async fn card_page_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    cursor: Option<(Decimal, Uuid)>,
    limit: i64,
) -> Result<Vec<CardRecord>, sqlx::Error> {
    let limit = limit.max(1);
    let rows: Vec<CardRecordRow> = match cursor {
        None => {
            let sql = format!(
                "SELECT {RECORD_COLUMNS}
                   FROM card c
                   JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
                  WHERE c.tenant_id = $1 AND c.board_id = $2 AND c.archived_at IS NULL
                  ORDER BY c.position, c.id
                  LIMIT $3"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(board_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
        }
        Some((position, id)) => {
            let sql = format!(
                "SELECT {RECORD_COLUMNS}
                   FROM card c
                   JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
                  WHERE c.tenant_id = $1 AND c.board_id = $2 AND c.archived_at IS NULL
                    AND (c.position, c.id) > ($3, $4)
                  ORDER BY c.position, c.id
                  LIMIT $5"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(board_id)
                .bind(position)
                .bind(id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
        }
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One page of a board's active cards, optionally restricted to one lane.
///
/// The board route's [`card_page_tx`] is board-wide; a workflow that pulls a lane needs the
/// same cursor contract with a lane filter, so this is the filtered sibling rather than a
/// second page implementation. `None` means every lane.
pub async fn card_page_in_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    list_id: Option<Uuid>,
    cursor: Option<(Decimal, Uuid)>,
    limit: i64,
) -> Result<Vec<CardRecord>, sqlx::Error> {
    let limit = limit.max(1);
    let rows: Vec<CardRecordRow> = match cursor {
        None => {
            let sql = format!(
                "SELECT {RECORD_COLUMNS}
                   FROM card c
                   JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
                  WHERE c.tenant_id = $1 AND c.board_id = $2 AND c.archived_at IS NULL
                    AND ($3::uuid IS NULL OR c.list_id = $3)
                  ORDER BY c.position, c.id
                  LIMIT $4"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(board_id)
                .bind(list_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
        }
        Some((position, id)) => {
            let sql = format!(
                "SELECT {RECORD_COLUMNS}
                   FROM card c
                   JOIN card_type t ON t.tenant_id = c.tenant_id AND t.id = c.card_type_id
                  WHERE c.tenant_id = $1 AND c.board_id = $2 AND c.archived_at IS NULL
                    AND ($3::uuid IS NULL OR c.list_id = $3)
                    AND (c.position, c.id) > ($4, $5)
                  ORDER BY c.position, c.id
                  LIMIT $6"
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(board_id)
                .bind(list_id)
                .bind(position)
                .bind(id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
        }
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

/// How many live (unstamped) cards sit in one lane.
///
/// The WIP pull counts the active lane before it moves anything; a page would cap the count
/// at the page size and silently over-admit.
pub async fn count_live_cards_in_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    list_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM card
          WHERE tenant_id = $1 AND list_id = $2 AND archived_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(list_id)
    .fetch_one(&mut *conn)
    .await
}

/// The fields a `cards.update` may change. `None` means unchanged.
#[derive(Clone, Debug, Default)]
pub struct CardUpdate {
    /// The name.
    pub name: Option<String>,
    /// Free text.
    pub description: Option<String>,
    /// The due instant.
    pub due_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `Some(true)` stamps `due_done_at = now()`; `Some(false)` clears it.
    pub due_done: Option<bool>,
}

/// Update a card's content, guarded by its version. `None` means absent or stale.
pub async fn update_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &CardUpdate,
    expected_version: Version,
) -> Result<Option<CardRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE card
            SET name = coalesce($3, name),
                description = coalesce($4, description),
                due_at = coalesce($5::timestamptz, due_at),
                due_done_at = CASE
                    WHEN $6::boolean IS NULL THEN due_done_at
                    WHEN $6 THEN now()
                    ELSE NULL
                END,
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $7
          RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(patch.name.as_deref())
    .bind(patch.description.as_deref())
    .bind(patch.due_at)
    .bind(patch.due_done)
    .bind(expected)
    .fetch_optional(&mut *conn)
    .await?;
    match updated {
        None => Ok(None),
        Some(_) => get_card_record_tx(conn, tenant, id).await,
    }
}

/// Bump a card's `version` without touching any other column.
///
/// A card's representation includes its labels (`CardView`), so labelling or unlabelling a
/// card changes what an `ETag` addresses even though no `card` column carries the label.
/// The bump keeps `If-Match` honest: a client holding the old version must re-read before it
/// writes. `None` means the card is absent or the version is stale.
pub async fn bump_card_version_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<Option<CardRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE card
            SET version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $3
          RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(expected)
    .fetch_optional(&mut *conn)
    .await?;
    match updated {
        None => Ok(None),
        Some(_) => get_card_record_tx(conn, tenant, id).await,
    }
}

/// Write a lane transition the domain already approved.
///
/// `card` is the POST-transition value from [`conveyor_domain::Card::archive`] or
/// [`conveyor_domain::Card::restore`]; `expected_version` is the version it was read at. The
/// statement refuses a stale version and a stamped card, so a concurrent move cannot be
/// overwritten by a transition that read the row before it.
pub async fn apply_card_transition_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card: &Card,
    expected_version: Version,
) -> Result<Option<CardRecord>, sqlx::Error> {
    let expected = i64::try_from(expected_version.get()).unwrap_or(i64::MAX);
    let updated: Option<Uuid> = sqlx::query_scalar(
        "UPDATE card
            SET list_id = $3, board_id = $4, prev_list_id = $5, archived_at = $6,
                list_changed_at = $7, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND version = $8
            AND (($6::timestamptz IS NULL) OR archived_at IS NULL)
          RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(card.id)
    .bind(card.list_id)
    .bind(card.board_id)
    .bind(card.prev_list_id)
    .bind(card.archived_at)
    .bind(card.list_changed_at)
    .bind(expected)
    .fetch_optional(&mut *conn)
    .await?;
    match updated {
        None => Ok(None),
        Some(_) => get_card_record_tx(conn, tenant, card.id).await,
    }
}
