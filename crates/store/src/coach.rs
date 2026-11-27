#![forbid(unsafe_code)]
//! The kanban coach's records (SPEC_V2 §6.2; T4, issue #75).
//!
//! One [`CoachInterruption`] per admitted interruption, and at most one [`CoachRemedy`] per
//! interruption. The coach CARD is an ordinary `card` row of the catalogue type `coach`; this
//! module never creates it (the action layer does, through `cards.create`, so the board's own
//! rules apply) — it records what the interruption is and what came of it.
//!
//! # The one-open-per-signature rule
//!
//! [`uq_coach_interruption_open`] is a partial unique index on `(tenant, card, signature)`
//! over rows with `closed_at IS NULL`. [`close_expired_tx`] is what makes a post-window repeat
//! legitimate: the action closes windows that have passed **before** it looks for an open row,
//! so the index's predicate stays immutable (no `now()` in an index expression).
//!
//! # Append-only
//!
//! `coach_remedy` is written once and never edited; the runtime role holds no UPDATE/DELETE on
//! it (`scripts/grants-runtime.sql`). `coach_interruption` is mutable by design: the triage, the
//! escalation and the kata candidate arrive after the row exists.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// The catalogue card type a coach card carries (decision Q1, issue #75).
pub const COACH_CARD_TYPE: &str = "coach";

/// The closed triage set (§6.2). The migration's CHECK is the backstop; this is the vocabulary
/// the actions validate against so a caller gets a `422`, not a database error.
pub const TRIAGE_CLASSES: [&str; 5] = [
    "transient",
    "conveyor_defect",
    "job_defect",
    "project_decision",
    "external",
];

/// The closed remedy set (§6.2). `retry` is the SPEC_V2 spelling of the SOP's "rerun".
pub const REMEDY_CLASSES: [&str; 6] = ["retry", "move", "repull", "block", "ask", "note"];

/// One interruption, as stored.
#[derive(Clone, Debug, Serialize)]
pub struct CoachInterruption {
    /// The row id.
    pub id: Uuid,
    /// The coach card carrying this interruption.
    pub coach_card_id: Uuid,
    /// The card whose run failed.
    pub interrupted_card_id: Uuid,
    /// The failed run.
    pub failed_run_id: Uuid,
    /// The repeat key: `card|error class|process|step`.
    pub signature: String,
    /// The rendered parts of the signature.
    pub signature_parts: serde_json::Value,
    /// The closed-set triage, once the coach has triaged.
    pub triage_class: Option<String>,
    /// The triage's confidence, 0..=1.
    pub confidence: Option<Decimal>,
    /// The model's diagnosis narrative (never the remedy).
    pub narrative: Option<String>,
    /// `open`, `remedied`, `escalated` or `closed`.
    pub state: String,
    /// When this interruption stops deduping a repeat.
    pub window_ends_at: DateTime<Utc>,
    /// When a repeat signature escalated it.
    pub escalated_at: Option<DateTime<Utc>>,
    /// The escalation or `ask` approval opened for it.
    pub approval_id: Option<Uuid>,
    /// The kata run a systemic class opened.
    pub kata_candidate_id: Option<Uuid>,
    /// When it left the open index.
    pub closed_at: Option<DateTime<Utc>>,
    /// Insert time.
    pub created_at: DateTime<Utc>,
}

/// One remedy, as stored.
#[derive(Clone, Debug, Serialize)]
pub struct CoachRemedy {
    /// The row id.
    pub id: Uuid,
    /// The interruption it belongs to.
    pub interruption_id: Uuid,
    /// The closed-set remedy.
    pub remedy_class: String,
    /// The remedy's own detail document.
    pub detail: serde_json::Value,
    /// Insert time.
    pub created_at: DateTime<Utc>,
}

/// What a new interruption needs.
#[derive(Clone, Debug)]
pub struct NewInterruption {
    /// The coach card carrying it.
    pub coach_card_id: Uuid,
    /// The card whose run failed.
    pub interrupted_card_id: Uuid,
    /// The failed run.
    pub failed_run_id: Uuid,
    /// The rendered signature.
    pub signature: String,
    /// The signature's parts.
    pub signature_parts: serde_json::Value,
    /// When a repeat stops deduping.
    pub window_ends_at: DateTime<Utc>,
}

/// Why a coach read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum CoachStoreError {
    /// A stored value is outside the vocabulary the CHECK constraints allow.
    #[error("{what} is {value:?}, which is not a {what} this build knows")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// The row the caller named does not exist in this tenant.
    #[error("no coach_interruption {0} in this tenant")]
    NotFound(Uuid),
    /// The interruption already carries the field the caller is setting.
    #[error("the interruption already carries a {what}")]
    AlreadySet {
        /// Which field.
        what: &'static str,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// A remedy the consumer cannot apply (a `move` with no lane on the card's board).
    #[error("the remedy cannot be applied: {0}")]
    RemedyInvalid(String),
}

const COLUMNS: &str = "id, coach_card_id, interrupted_card_id, failed_run_id, signature, \
                       signature_parts, triage_class, confidence, narrative, state, \
                       window_ends_at, escalated_at, approval_id, kata_candidate_id, closed_at, \
                       created_at";

#[derive(Debug, sqlx::FromRow)]
struct InterruptionRow {
    id: Uuid,
    coach_card_id: Uuid,
    interrupted_card_id: Uuid,
    failed_run_id: Uuid,
    signature: String,
    signature_parts: serde_json::Value,
    triage_class: Option<String>,
    confidence: Option<Decimal>,
    narrative: Option<String>,
    state: String,
    window_ends_at: DateTime<Utc>,
    escalated_at: Option<DateTime<Utc>>,
    approval_id: Option<Uuid>,
    kata_candidate_id: Option<Uuid>,
    closed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl TryFrom<InterruptionRow> for CoachInterruption {
    type Error = CoachStoreError;
    fn try_from(row: InterruptionRow) -> Result<Self, Self::Error> {
        // The CHECK constraints make an unknown state unreachable; the conversion keeps a
        // corrupted row a typed error rather than a silently accepted one.
        if !matches!(
            row.state.as_str(),
            "open" | "remedied" | "escalated" | "closed"
        ) {
            return Err(CoachStoreError::Vocabulary {
                what: "coach_interruption.state",
                value: row.state,
            });
        }
        Ok(Self {
            id: row.id,
            coach_card_id: row.coach_card_id,
            interrupted_card_id: row.interrupted_card_id,
            failed_run_id: row.failed_run_id,
            signature: row.signature,
            signature_parts: row.signature_parts,
            triage_class: row.triage_class,
            confidence: row.confidence,
            narrative: row.narrative,
            state: row.state,
            window_ends_at: row.window_ends_at,
            escalated_at: row.escalated_at,
            approval_id: row.approval_id,
            kata_candidate_id: row.kata_candidate_id,
            closed_at: row.closed_at,
            created_at: row.created_at,
        })
    }
}

/// Insert one interruption.
pub async fn insert_interruption_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewInterruption,
) -> Result<CoachInterruption, CoachStoreError> {
    let sql = format!(
        "INSERT INTO coach_interruption
             (tenant_id, coach_card_id, interrupted_card_id, failed_run_id, signature,
              signature_parts, window_ends_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {COLUMNS}"
    );
    let row: InterruptionRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.coach_card_id)
        .bind(new.interrupted_card_id)
        .bind(new.failed_run_id)
        .bind(&new.signature)
        .bind(&new.signature_parts)
        .bind(new.window_ends_at)
        .fetch_one(&mut *conn)
        .await?;
    CoachInterruption::try_from(row)
}

/// The one OPEN interruption for a card and signature, if any.
pub async fn open_interruption_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    signature: &str,
) -> Result<Option<CoachInterruption>, CoachStoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2 AND signature = $3
            AND closed_at IS NULL"
    );
    let row: Option<InterruptionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(card_id)
        .bind(signature)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(CoachInterruption::try_from).transpose()
}

/// Close every open interruption for a card and signature whose window has passed.
///
/// Returns how many rows closed. The action calls this before it looks for an open row, so a
/// repeat after the window is a new interruption instead of a second card.
pub async fn close_expired_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    signature: &str,
    now: DateTime<Utc>,
) -> Result<u64, CoachStoreError> {
    let closed = sqlx::query(
        "UPDATE coach_interruption
            SET state = 'closed', closed_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND interrupted_card_id = $2 AND signature = $3
            AND closed_at IS NULL AND window_ends_at <= $4",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .bind(signature)
    .bind(now)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(closed)
}

/// Read one interruption.
pub async fn get_interruption_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CoachInterruption>, CoachStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM coach_interruption WHERE tenant_id = $1 AND id = $2");
    let row: Option<InterruptionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(CoachInterruption::try_from).transpose()
}

/// Record the triage. Refuses a second triage on the same interruption.
pub async fn record_triage_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    triage_class: &str,
    confidence: Decimal,
    narrative: &str,
) -> Result<CoachInterruption, CoachStoreError> {
    let sql = format!(
        "UPDATE coach_interruption
            SET triage_class = $3, confidence = $4, narrative = $5, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND triage_class IS NULL
          RETURNING {COLUMNS}"
    );
    let row: Option<InterruptionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(triage_class)
        .bind(confidence)
        .bind(narrative)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => CoachInterruption::try_from(row),
        None => {
            // Either the row is absent or it is already triaged; the caller's `get` tells
            // them which, and the message must not guess.
            if get_interruption_tx(&mut *conn, tenant, id).await?.is_some() {
                Err(CoachStoreError::AlreadySet { what: "triage" })
            } else {
                Err(CoachStoreError::NotFound(id))
            }
        }
    }
}

/// The interruption's one remedy, if it has one.
pub async fn get_remedy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    interruption_id: Uuid,
) -> Result<Option<CoachRemedy>, CoachStoreError> {
    let row: Option<(Uuid, Uuid, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, interruption_id, remedy_class, detail, created_at
           FROM coach_remedy WHERE tenant_id = $1 AND interruption_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(interruption_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(id, interruption_id, remedy_class, detail, created_at)| CoachRemedy {
            id,
            interruption_id,
            remedy_class,
            detail,
            created_at,
        },
    ))
}

/// Insert the one remedy. `None` means one already exists (`uq_coach_remedy_one_per`).
pub async fn insert_remedy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    interruption_id: Uuid,
    remedy_class: &str,
    detail: &serde_json::Value,
) -> Result<Option<CoachRemedy>, CoachStoreError> {
    let row: Option<(Uuid, Uuid, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "INSERT INTO coach_remedy (tenant_id, interruption_id, remedy_class, detail)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, interruption_id) DO NOTHING
         RETURNING id, interruption_id, remedy_class, detail, created_at",
    )
    .bind(tenant.as_uuid())
    .bind(interruption_id)
    .bind(remedy_class)
    .bind(detail)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(id, interruption_id, remedy_class, detail, created_at)| CoachRemedy {
            id,
            interruption_id,
            remedy_class,
            detail,
            created_at,
        },
    ))
}

/// Mark the interruption remedied (the remedy row already exists).
pub async fn mark_remedied_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<(), CoachStoreError> {
    sqlx::query(
        "UPDATE coach_interruption
            SET state = 'remedied', updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND state = 'open'",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// What applying a recorded remedy did to the interrupted card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemedyEffect {
    /// The card is blocked; the next dispatch must refuse it.
    Blocked,
    /// The card was moved to the named lane.
    Moved {
        /// The lane the remedy named.
        list_id: Uuid,
    },
    /// The card may run again.
    Retry,
    /// The card is re-pullable; its inputs resolve fresh on the next dispatch.
    Repulled,
    /// The remedy is `ask` and its human gate is still pending.
    AskPending,
    /// The remedy is `note`; it carries information, not an action.
    Noted,
}

impl RemedyEffect {
    /// The stored/wire spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Moved { .. } => "moved",
            Self::Retry => "retry",
            Self::Repulled => "repulled",
            Self::AskPending => "ask_pending",
            Self::Noted => "noted",
        }
    }
}

/// A remedy the consumer applied, with the effect it had.
#[derive(Clone, Debug)]
pub struct AppliedRemedy {
    /// The interruption the remedy belongs to.
    pub interruption_id: Uuid,
    /// The remedy row.
    pub remedy_id: Uuid,
    /// The closed-set class.
    pub remedy_class: String,
    /// What it did.
    pub effect: RemedyEffect,
}

/// Apply one recorded remedy's bounded effect to the interrupted card (v1-p6-1).
///
/// `coach.remedy` calls this in the same transaction that records the remedy, so the effect
/// commits with the remedy and a later dispatch refusal cannot roll it back. Exactly one
/// remedy per interruption is written, and the card records the applied remedy id, so a replay
/// applies nothing twice. The bounded effects are the SPEC's: `block` sets
/// `meta.coach_blocked`, `move` places the card in the named lane on its own board,
/// `retry`/`repull` clear a block, and `note` records itself without changing the card. `ask`
/// is the human gate; the dispatch enforces it (`dispatch_refusal_tx`).
pub async fn apply_remedy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    remedy_id: Uuid,
) -> Result<Option<AppliedRemedy>, CoachStoreError> {
    let row: Option<(Uuid, Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT i.id, i.interrupted_card_id, r.id, r.remedy_class, r.detail
           FROM coach_remedy r
           JOIN coach_interruption i
             ON i.tenant_id = r.tenant_id AND i.id = r.interruption_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(remedy_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((interruption_id, card_id, remedy_id, remedy_class, detail)) = row else {
        return Ok(None);
    };
    // `ask` is a human gate, not a card effect; the dispatch enforces the pending gate.
    if remedy_class == "ask" {
        return Ok(None);
    }
    let applied: Option<String> = sqlx::query_scalar(
        "SELECT meta->>'coach_remedy_applied_id' FROM card
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    if applied.as_deref() == Some(remedy_id.to_string().as_str()) {
        return Ok(None);
    }

    let record = |effect: &str| {
        serde_json::json!({
            "coach_remedy_applied_id": remedy_id.to_string(),
            "coach_remedy_effect": effect,
        })
    };
    let effect = match remedy_class.as_str() {
        "block" => {
            sqlx::query(
                "UPDATE card
                    SET meta = meta || $3::jsonb || jsonb_build_object('coach_blocked', true),
                        version = version + 1, updated_at = now()
                  WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(card_id)
            .bind(record("blocked"))
            .execute(&mut *conn)
            .await?;
            RemedyEffect::Blocked
        }
        "move" => {
            let list_id = detail
                .get("list_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or_else(|| {
                    CoachStoreError::RemedyInvalid(
                        "a move remedy must name the destination lane in detail.list_id".to_owned(),
                    )
                })?;
            let on_board: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM list l
                      JOIN card c ON c.tenant_id = l.tenant_id AND c.board_id = l.board_id
                     WHERE l.tenant_id = $1 AND l.id = $2 AND c.id = $3
                )",
            )
            .bind(tenant.as_uuid())
            .bind(list_id)
            .bind(card_id)
            .fetch_one(&mut *conn)
            .await?;
            if !on_board {
                return Err(CoachStoreError::RemedyInvalid(
                    "a move remedy must name a lane on the card's own board".to_owned(),
                ));
            }
            sqlx::query(
                "UPDATE card
                    SET list_id = $3, meta = meta || $4::jsonb,
                        version = version + 1, updated_at = now()
                  WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(card_id)
            .bind(list_id)
            .bind(record("moved"))
            .execute(&mut *conn)
            .await?;
            RemedyEffect::Moved { list_id }
        }
        "retry" | "repull" => {
            sqlx::query(
                "UPDATE card
                    SET meta = (meta - 'coach_blocked') || $3::jsonb,
                        version = version + 1, updated_at = now()
                  WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(card_id)
            .bind(record(if remedy_class == "retry" {
                "retry"
            } else {
                "repulled"
            }))
            .execute(&mut *conn)
            .await?;
            if remedy_class == "retry" {
                RemedyEffect::Retry
            } else {
                RemedyEffect::Repulled
            }
        }
        _ => {
            sqlx::query(
                "UPDATE card
                    SET meta = meta || $3::jsonb, version = version + 1, updated_at = now()
                  WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(card_id)
            .bind(record("noted"))
            .execute(&mut *conn)
            .await?;
            RemedyEffect::Noted
        }
    };
    Ok(Some(AppliedRemedy {
        interruption_id,
        remedy_id,
        remedy_class,
        effect,
    }))
}

/// What the next dispatch must refuse for this card (v1-p6-1).
///
/// The effect itself was applied when the remedy was recorded (`apply_remedy_tx`); this read
/// enforces it. A blocked card refuses; an `ask` whose human gate is still pending refuses;
/// everything else dispatches. No write here, so a refusal never rolls a remedy back.
pub async fn dispatch_refusal_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<RemedyEffect>, CoachStoreError> {
    let blocked: Option<bool> = sqlx::query_scalar(
        "SELECT coalesce(meta->>'coach_blocked' = 'true', false) FROM card
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *conn)
    .await?;
    if blocked.unwrap_or(false) {
        return Ok(Some(RemedyEffect::Blocked));
    }
    let pending_ask: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM coach_interruption i
              JOIN coach_remedy r
                ON r.tenant_id = i.tenant_id AND r.interruption_id = i.id
              JOIN approval a
                ON a.tenant_id = i.tenant_id AND a.id = i.approval_id
             WHERE i.tenant_id = $1 AND i.interrupted_card_id = $2
               AND r.remedy_class = 'ask' AND a.status = 'pending'
        )",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await?;
    if pending_ask {
        return Ok(Some(RemedyEffect::AskPending));
    }
    Ok(None)
}

/// Mark the interruption escalated, naming the approval that carries the decision.
pub async fn mark_escalated_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    approval_id: Option<Uuid>,
) -> Result<CoachInterruption, CoachStoreError> {
    let sql = format!(
        "UPDATE coach_interruption
            SET state = 'escalated', escalated_at = now(), approval_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {COLUMNS}"
    );
    let row: Option<InterruptionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(approval_id)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        Some(row) => CoachInterruption::try_from(row),
        None => Err(CoachStoreError::NotFound(id)),
    }
}

/// Record the kata run a systemic class opened.
pub async fn set_kata_candidate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    kata_run_id: Uuid,
) -> Result<(), CoachStoreError> {
    sqlx::query(
        "UPDATE coach_interruption
            SET kata_candidate_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND kata_candidate_id IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(kata_run_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Record the approval that a low-confidence `ask` opened.
pub async fn set_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    approval_id: Uuid,
) -> Result<(), CoachStoreError> {
    sqlx::query(
        "UPDATE coach_interruption
            SET approval_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(approval_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The position of the failed step of a process run, when one exists.
///
/// The signature's third part (decision Q2): two failures of the same card, class and process
/// but at different steps are different faults and must not dedupe into each other.
pub async fn failing_step_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Option<i32>, CoachStoreError> {
    let position: Option<i32> = sqlx::query_scalar(
        "SELECT position FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'failed'
          ORDER BY position DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(position)
}

/// How many interruptions a card has ever had, of every signature.
///
/// The per-card depth cap (SPEC_V2 §1.4 · §12 T4): three interruptions are enough to show the
/// coach is not converging, and a fourth would only repeat the first three.
pub async fn count_interruptions_for_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<i64, CoachStoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(count)
}

/// Is this card a coach card?
///
/// The enqueue-time recursion guard (`store::run::enqueue_run_failed_trigger_tx`) and `err.card`
/// both ask this: a failure raised on a coach card is terminal for the interruption path.
pub async fn is_coach_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<bool, CoachStoreError> {
    let code: Option<String> = sqlx::query_scalar(
        "SELECT ct.code
           FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(code.as_deref() == Some(COACH_CARD_TYPE))
}
