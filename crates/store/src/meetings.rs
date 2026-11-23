//! T19 decision forums — the meeting aggregate (SPEC_V2 §6.8, D66).
//!
//! A meeting is a card plus a typed record: agenda items resolve existing objects, positions are
//! the deliberation, and `close_item` derives the outcome by arithmetic over those positions and
//! the linked approval's stricter policy. This module owns the rows and their invariants; the
//! action family in `conveyor-actions` validates input and maps refusals.
//!
//! Store conventions are the same as every other module: every function takes the caller's
//! scoped transaction, every query carries `tenant_id`, and a policy/row change is a version
//! bump. The meeting's own decision rows are written through
//! [`crate::approval::record_meeting_decisions_tx`], never as a second decision store.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::card::NewCard;
use crate::outbox::{self, NewEvent};

#[derive(Debug, thiserror::Error)]
pub enum MeetingStoreError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    #[error("the {0} moved; the caller's version is stale")]
    Stale(&'static str),
    #[error("the {what}: {why}")]
    Conflict { what: &'static str, why: String },
    #[error("the {what} is not permitted: {why}")]
    Illegal { what: &'static str, why: String },
    /// The linked approval's policy is stricter than the meeting's rule (dual control, required
    /// decisions, evidence, P4). No decision rows are written.
    #[error("{detail}")]
    Stricter { detail: String },
    /// A named state refusal with its own wire code (minutes gating, open items, a closed forum).
    #[error("{detail}")]
    Refused { code: &'static str, detail: String },
    #[error("{detail}")]
    Unprocessable { code: &'static str, detail: String },
    #[error(transparent)]
    Card(#[from] crate::card::CardStoreError),
    #[error(transparent)]
    Outbox(#[from] crate::outbox::OutboxError),
}

type Result<T> = std::result::Result<T, MeetingStoreError>;

const MEETING_TYPE_COLUMNS: &str = "id, key, name, purpose, agenda_sources,
    required_participation, quorum, decision_rule, evidence_minimum,
    EXTRACT(EPOCH FROM decision_window)::bigint AS decision_window_seconds,
    EXTRACT(EPOCH FROM timebox)::bigint AS timebox_seconds,
    allowed_outcomes, convener_authority, status, version";

const MEETING_COLUMNS: &str = "m.id, m.card_id, m.convener_kind, m.convener_id,
    m.convener_governance_body_id, m.meeting_type_id, m.meeting_type_version,
    t.key AS meeting_type_key, t.name AS meeting_type_name, t.decision_rule,
    EXTRACT(EPOCH FROM t.decision_window)::bigint AS decision_window_seconds,
    m.cadence_slot, m.opens_at, m.decision_window_end, m.closed_at, m.status, m.version,
    c.name AS card_name,
    (m.status = 'closed' AND NOT EXISTS (
        SELECT 1 FROM meeting_agenda_item i
         WHERE i.tenant_id = m.tenant_id AND i.meeting_id = m.id
           AND i.outcome = 'decision')) AS retirement_candidate";

const AGENDA_COLUMNS: &str = "id, meeting_id, sequence, resolved_kind, resolved_id,
    participating_departments, evidence_refs, outcome, outcome_ref, defer_reason, closed_at,
    version";

const ATTENDANCE_COLUMNS: &str = "id, meeting_id, participant_kind, participant_id, seat,
    department_id, invitation_state, response_state, delegated_from, version";

const POSITION_COLUMNS: &str = "id, meeting_id, agenda_item_id, participant_kind, participant_id,
    stance, rationale, conditions, commitment, surface, superseded_at, recorded_at, version";

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MeetingTypeRecord {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub purpose: String,
    pub agenda_sources: Value,
    pub required_participation: Value,
    pub quorum: i32,
    pub decision_rule: String,
    pub evidence_minimum: i32,
    pub decision_window_seconds: i64,
    pub timebox_seconds: i64,
    pub allowed_outcomes: Vec<String>,
    pub convener_authority: Value,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct MeetingRecord {
    pub id: Uuid,
    pub card_id: Uuid,
    pub convener_kind: String,
    pub convener_id: Uuid,
    pub convener_governance_body_id: Option<Uuid>,
    pub meeting_type_id: Uuid,
    pub meeting_type_version: i64,
    pub meeting_type_key: String,
    pub meeting_type_name: String,
    pub decision_rule: String,
    pub decision_window_seconds: i64,
    pub cadence_slot: Option<Uuid>,
    pub opens_at: Option<DateTime<Utc>>,
    pub decision_window_end: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub status: String,
    pub version: i64,
    pub card_name: String,
    /// A closed forum whose instances produced no decision: eligible for retirement (P10).
    pub retirement_candidate: bool,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AgendaItemRecord {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub sequence: i32,
    pub resolved_kind: String,
    pub resolved_id: Uuid,
    pub participating_departments: Vec<Uuid>,
    pub evidence_refs: Value,
    pub outcome: Option<String>,
    pub outcome_ref: Option<Uuid>,
    pub defer_reason: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub version: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AttendanceRecord {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub participant_kind: String,
    pub participant_id: Uuid,
    pub seat: String,
    pub department_id: Option<Uuid>,
    pub invitation_state: String,
    pub response_state: String,
    pub delegated_from: Option<Uuid>,
    pub version: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PositionRecord {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub agenda_item_id: Uuid,
    pub participant_kind: String,
    pub participant_id: Uuid,
    pub stance: String,
    pub rationale: String,
    pub conditions: Value,
    pub commitment: Option<Value>,
    pub surface: String,
    pub superseded_at: Option<DateTime<Utc>>,
    pub recorded_at: DateTime<Utc>,
    pub version: i64,
}

#[derive(Clone, Debug, Default)]
pub struct MeetingInstruments {
    pub decisions: i64,
    pub items: i64,
    pub open_items: i64,
    pub decision_latency_seconds: Option<i64>,
    pub agenda_conversion: f64,
    pub quorum_miss_rate: f64,
    pub commitment_slippage: i64,
    pub action_closure: f64,
    pub participant_hours: f64,
    pub person_hours_per_decision: f64,
    pub decisions_per_meeting: f64,
}

#[derive(Clone, Debug)]
pub struct MeetingDetail {
    pub meeting: MeetingRecord,
    pub meeting_type: MeetingTypeRecord,
    pub agenda: Vec<AgendaItemRecord>,
    pub attendance: Vec<AttendanceRecord>,
    pub positions: Vec<PositionRecord>,
    pub instruments: MeetingInstruments,
}

/// The acting principal, as the action layer names it. Authorization is the action scope's;
/// this is the per-forum authority row's input, never a second gate.
#[derive(Clone, Copy, Debug)]
pub struct MeetingActor<'a> {
    pub id: Uuid,
    pub role: &'a str,
}

pub struct NewMeetingType<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub purpose: &'a str,
    pub agenda_sources: &'a Value,
    pub required_participation: &'a Value,
    pub quorum: i32,
    pub decision_rule: &'a str,
    pub evidence_minimum: i32,
    pub decision_window_seconds: i64,
    pub timebox_seconds: i64,
    pub allowed_outcomes: &'a [String],
    pub convener_authority: &'a Value,
}

pub struct NewMeeting<'a> {
    pub meeting_type_key: &'a str,
    /// Present only when the key does not exist yet. A change to an existing policy is the
    /// versioned governance route, not a schedule.
    pub policy: Option<&'a NewMeetingType<'a>>,
    pub title: &'a str,
    pub description: &'a str,
    pub list_id: Uuid,
    pub board_id: Option<Uuid>,
    pub convener_kind: &'a str,
    pub convener_id: Uuid,
    pub cadence_slot: Option<Uuid>,
}

pub struct NewAgendaItem<'a> {
    pub resolved_kind: &'a str,
    pub resolved_id: Uuid,
    pub participating_departments: &'a [Uuid],
    pub evidence_refs: &'a Value,
}

pub struct NewPosition<'a> {
    pub agenda_item_id: Uuid,
    pub participant_kind: &'a str,
    pub participant_id: Uuid,
    pub stance: &'a str,
    pub rationale: &'a str,
    pub conditions: &'a Value,
    pub commitment: Option<&'a Value>,
    pub surface: &'a str,
}

/// The outcome a non-decision item closes with. A commitment and an action both become a card;
/// information is typed and counted only. `deferred` has its own action.
pub struct NonDecision<'a> {
    pub outcome: &'a str,
    pub commitment: Option<&'a Value>,
    pub action: Option<&'a Value>,
}

/// Vocabulary shared with the database CHECK constraints; the action layer produces the same
/// values, so a typo is a 422 at the door rather than a 500 from Postgres.
pub const CONVENER_KINDS: [&str; 3] = ["governance_body", "job_binding", "app_user"];
pub const PARTICIPANT_KINDS: [&str; 2] = ["job_binding", "app_user"];
pub const SEATS: [&str; 5] = ["chair", "member", "presenter", "observer", "delegate"];
pub const STANCES: [&str; 5] = [
    "approve",
    "approve_with_conditions",
    "reject",
    "abstain",
    "request_evidence",
];
pub const SURFACES: [&str; 3] = ["web", "telegram", "mcp"];
pub const OUTCOMES: [&str; 5] = [
    "decision",
    "commitment",
    "action",
    "information",
    "deferred",
];
pub const RESOLVED_KINDS: [&str; 4] = ["approval", "register_item", "asset", "cadence"];

fn is_unique_on(error: &sqlx::Error, constraint: &str) -> bool {
    matches!(
        error.as_database_error().and_then(|e| e.constraint()),
        Some(name) if name == constraint
    )
}

fn evidence_count(meta: &Value) -> i64 {
    match meta.get("evidence") {
        Some(Value::Array(items)) => items.len() as i64,
        Some(Value::Null) | None => 0,
        Some(_) => 1,
    }
}

fn evidence_value(meta: &Value) -> Value {
    match meta.get("evidence") {
        Some(Value::Array(items)) => Value::Array(items.clone()),
        Some(Value::Null) | None => json!([]),
        Some(other) => json!([other]),
    }
}

// ── meeting_type ────────────────────────────────────────────────────────────────────────

pub async fn create_meeting_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewMeetingType<'_>,
) -> Result<MeetingTypeRecord> {
    if new.allowed_outcomes.is_empty() {
        return Err(MeetingStoreError::Illegal {
            what: "meeting type",
            why: "allowed_outcomes must name at least one outcome".into(),
        });
    }
    let sql = format!(
        "INSERT INTO meeting_type (tenant_id, key, name, purpose, agenda_sources,
             required_participation, quorum, decision_rule, evidence_minimum, decision_window,
             timebox, allowed_outcomes, convener_authority, status, version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                 make_interval(secs => $10::double precision),
                 make_interval(secs => $11::double precision), $12, $13, 'active',
                 COALESCE((SELECT max(version) FROM meeting_type
                            WHERE tenant_id = $1 AND key = $2), 0) + 1)
         RETURNING {MEETING_TYPE_COLUMNS}"
    );
    sqlx::query_as::<_, MeetingTypeRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.key)
        .bind(new.name)
        .bind(new.purpose)
        .bind(new.agenda_sources)
        .bind(new.required_participation)
        .bind(new.quorum)
        .bind(new.decision_rule)
        .bind(new.evidence_minimum)
        .bind(new.decision_window_seconds)
        .bind(new.timebox_seconds)
        .bind(new.allowed_outcomes)
        .bind(new.convener_authority)
        .fetch_one(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn latest_meeting_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<MeetingTypeRecord>> {
    let sql = format!(
        "SELECT {MEETING_TYPE_COLUMNS} FROM meeting_type
          WHERE tenant_id = $1 AND key = $2 AND status <> 'retired'
          ORDER BY version DESC LIMIT 1"
    );
    sqlx::query_as::<_, MeetingTypeRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn get_meeting_type_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<MeetingTypeRecord>> {
    let sql =
        format!("SELECT {MEETING_TYPE_COLUMNS} FROM meeting_type WHERE tenant_id = $1 AND id = $2");
    sqlx::query_as::<_, MeetingTypeRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

// ── schedule / open ─────────────────────────────────────────────────────────────────────

pub async fn schedule_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: MeetingActor<'_>,
    new: &NewMeeting<'_>,
) -> Result<MeetingRecord> {
    if !CONVENER_KINDS.contains(&new.convener_kind) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_convener_kind",
            detail: format!("convener_kind must be one of {CONVENER_KINDS:?}"),
        });
    }
    // A governance body's standing meeting names no type: the body's key is the standing type
    // (the SPEC's "the body carries the standing meeting_type", resolved by key, never code).
    let meeting_type_key = if new.meeting_type_key.trim().is_empty() {
        if new.convener_kind != "governance_body" {
            return Err(MeetingStoreError::Unprocessable {
                code: "meeting_type_required",
                detail: "only a governance body has a standing meeting type".into(),
            });
        }
        let key: Option<String> =
            sqlx::query_scalar("SELECT key FROM governance_body WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(new.convener_id)
                .fetch_optional(&mut *conn)
                .await?;
        key.ok_or(MeetingStoreError::NotFound("governance body"))?
    } else {
        new.meeting_type_key.to_owned()
    };
    let policy = new.policy.map(|policy| NewMeetingType {
        key: &meeting_type_key,
        name: policy.name,
        purpose: policy.purpose,
        agenda_sources: policy.agenda_sources,
        required_participation: policy.required_participation,
        quorum: policy.quorum,
        decision_rule: policy.decision_rule,
        evidence_minimum: policy.evidence_minimum,
        decision_window_seconds: policy.decision_window_seconds,
        timebox_seconds: policy.timebox_seconds,
        allowed_outcomes: policy.allowed_outcomes,
        convener_authority: policy.convener_authority,
    });
    let meeting_type = match latest_meeting_type_tx(&mut *conn, tenant, &meeting_type_key).await? {
        Some(existing) => {
            if policy.is_some() {
                return Err(MeetingStoreError::Conflict {
                    what: "meeting type",
                    why: "this key already exists; a policy change is a new version through the governance route, not a schedule".into(),
                });
            }
            existing
        }
        None => {
            let policy = policy
                .as_ref()
                .ok_or_else(|| MeetingStoreError::Unprocessable {
                    code: "meeting_type_policy_required",
                    detail:
                        "the meeting type does not exist; supply its policy to create version 1"
                            .into(),
                })?;
            create_meeting_type_tx(&mut *conn, tenant, policy).await?
        }
    };
    ensure_convener_tx(
        &mut *conn,
        tenant,
        &meeting_type,
        new.convener_kind,
        new.convener_id,
        actor,
    )
    .await?;

    let board_id: Option<Uuid> =
        sqlx::query_scalar("SELECT board_id FROM list WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(new.list_id)
            .fetch_optional(&mut *conn)
            .await?;
    let board_id = board_id.ok_or(MeetingStoreError::NotFound("list"))?;
    if let Some(given) = new.board_id {
        if given != board_id {
            return Err(MeetingStoreError::Illegal {
                what: "meeting",
                why: "the list is not on the given board".into(),
            });
        }
    }

    let card_type_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM card_type WHERE tenant_id = $1 AND code = 'meeting' AND is_active",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;
    let card_type_id = card_type_id.ok_or_else(|| MeetingStoreError::Conflict {
        what: "meeting card type",
        why: "no `meeting` card type is installed in this tenant; seed the catalogue first".into(),
    })?;

    crate::position::lock_board_tx(&mut *conn, tenant, board_id).await?;
    let position =
        crate::position::next_card_position_tx(&mut *conn, tenant, board_id, new.list_id).await?;
    let card = crate::card::insert_card_tx(
        &mut *conn,
        tenant,
        &NewCard {
            list_id: new.list_id,
            card_type_id,
            name: new.title.to_owned(),
            description: new.description.to_owned(),
            position,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: Some(actor.id),
            job_binding_id: None,
            meta: json!({
                "meeting_type": meeting_type.key,
                "meeting_type_version": meeting_type.version,
            }),
        },
    )
    .await?;

    let convener_body = if new.convener_kind == "governance_body" {
        Some(new.convener_id)
    } else {
        None
    };
    let meeting_id: Uuid = sqlx::query_scalar(
        "INSERT INTO meeting (tenant_id, card_id, convener_kind, convener_id,
             convener_governance_body_id, meeting_type_id, meeting_type_version, cadence_slot)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(card.id)
    .bind(new.convener_kind)
    .bind(new.convener_id)
    .bind(convener_body)
    .bind(meeting_type.id)
    .bind(meeting_type.version)
    .bind(new.cadence_slot)
    .fetch_one(&mut *conn)
    .await?;

    get_meeting_tx(&mut *conn, tenant, meeting_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting"))
}

pub async fn open_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Option<i64>,
) -> Result<MeetingRecord> {
    let row: Option<(Uuid, String, i64, Uuid)> = sqlx::query_as(
        "SELECT id, status, version, meeting_type_id FROM meeting
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let (_, status, version, meeting_type_id) =
        row.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if let Some(expected) = expected_version {
        if expected != version {
            return Err(MeetingStoreError::Stale("meeting"));
        }
    }
    match status.as_str() {
        "scheduled" => {}
        "live" => {
            return Err(MeetingStoreError::Conflict {
                what: "meeting",
                why: "this meeting is already live".into(),
            });
        }
        _ => {
            return Err(MeetingStoreError::Illegal {
                what: "meeting",
                why: format!("a {status} meeting cannot be opened"),
            });
        }
    }
    let meeting_type = get_meeting_type_tx(&mut *conn, tenant, meeting_type_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting type"))?;
    let updated = sqlx::query(
        "UPDATE meeting
            SET status = 'live', opens_at = now(),
                decision_window_end = now() + make_interval(secs => $3::double precision),
                updated_at = now(), version = version + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .bind(meeting_type.decision_window_seconds)
    .execute(&mut *conn)
    .await;
    if let Err(error) = updated {
        if is_unique_on(&error, "uq_meeting_live") {
            return Err(MeetingStoreError::Conflict {
                what: "cadence slot",
                why: "another live meeting already occupies this slot".into(),
            });
        }
        return Err(error.into());
    }
    materialise_agenda_tx(&mut *conn, tenant, id).await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{id}"),
            event_type: "meeting.opened".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: id,
            payload: json!({
                "meeting_id": id,
                "meeting_type": meeting_type.key,
                "meeting_type_version": meeting_type.version,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    get_meeting_tx(&mut *conn, tenant, id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting"))
}

// ── attendance ──────────────────────────────────────────────────────────────────────────

pub async fn invite_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    participant_kind: &str,
    participant_id: Uuid,
    seat: &str,
    department_id: Option<Uuid>,
    delegated_from: Option<Uuid>,
) -> Result<AttendanceRecord> {
    if !PARTICIPANT_KINDS.contains(&participant_kind) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_participant_kind",
            detail: format!("participant_kind must be one of {PARTICIPANT_KINDS:?}"),
        });
    }
    if !SEATS.contains(&seat) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_seat",
            detail: format!("seat must be one of {SEATS:?}"),
        });
    }
    let meeting: Option<(String,)> =
        sqlx::query_as("SELECT status FROM meeting WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(meeting_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (status,) = meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if !matches!(status.as_str(), "scheduled" | "live") {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting takes no invitations"),
        });
    }
    ensure_participant_tx(&mut *conn, tenant, participant_kind, participant_id).await?;
    if let Some(from) = delegated_from {
        let same: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM meeting_attendance WHERE tenant_id = $1 AND id = $2 AND meeting_id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(from)
        .bind(meeting_id)
        .fetch_optional(&mut *conn)
        .await?;
        if same.is_none() {
            return Err(MeetingStoreError::NotFound("delegated seat"));
        }
    }
    let sql = format!(
        "INSERT INTO meeting_attendance (tenant_id, meeting_id, participant_kind, participant_id,
             seat, department_id, delegated_from)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (meeting_id, participant_kind, participant_id) DO UPDATE
            SET seat = EXCLUDED.seat, department_id = EXCLUDED.department_id,
                delegated_from = EXCLUDED.delegated_from, invitation_state = 'invited',
                updated_at = now(), version = meeting_attendance.version + 1
         RETURNING {ATTENDANCE_COLUMNS}"
    );
    sqlx::query_as::<_, AttendanceRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .bind(participant_kind)
        .bind(participant_id)
        .bind(seat)
        .bind(department_id)
        .bind(delegated_from)
        .fetch_one(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn respond_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    participant_kind: &str,
    participant_id: Uuid,
    response: &str,
) -> Result<AttendanceRecord> {
    if !matches!(response, "accepted" | "declined" | "tentative") {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_response",
            detail: "response must be accepted, declined or tentative".into(),
        });
    }
    let sql = format!(
        "UPDATE meeting_attendance
            SET response_state = $4, invitation_state = $4, updated_at = now(),
                version = version + 1
          WHERE tenant_id = $1 AND meeting_id = $2 AND participant_kind = $3
            AND participant_id = $5
          RETURNING {ATTENDANCE_COLUMNS}"
    );
    sqlx::query_as::<_, AttendanceRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .bind(participant_kind)
        .bind(response)
        .bind(participant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(MeetingStoreError::NotFound("attendance"))
}

// ── agenda ──────────────────────────────────────────────────────────────────────────────

pub async fn materialise_agenda_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<Vec<AgendaItemRecord>> {
    let row: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
        "SELECT id, meeting_type_id, meeting_type_version FROM meeting
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (_, meeting_type_id, _) = row.ok_or(MeetingStoreError::NotFound("meeting"))?;
    let ty = get_meeting_type_tx(&mut *conn, tenant, meeting_type_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting type"))?;

    let mut next_sequence: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(sequence), 0) + 1 FROM meeting_agenda_item
          WHERE tenant_id = $1 AND meeting_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_one(&mut *conn)
    .await?;

    let sources = ty.agenda_sources.as_array().cloned().unwrap_or_default();
    for source in &sources {
        let kind = source.get("kind").and_then(Value::as_str).unwrap_or("");
        let limit = source
            .get("limit")
            .and_then(Value::as_i64)
            .unwrap_or(20)
            .clamp(1, 100);
        let kinds: Vec<String> = source
            .get("kinds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        match kind {
            "approval" => {
                let rows: Vec<(Uuid, Value)> = sqlx::query_as(
                    "SELECT id, meta FROM approval
                      WHERE tenant_id = $1 AND status = 'pending'
                      ORDER BY created_at, id LIMIT $2",
                )
                .bind(tenant.as_uuid())
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?;
                for (approval_id, meta) in rows {
                    let evidence = evidence_value(&meta);
                    if evidence_count(&meta) < i64::from(ty.evidence_minimum) {
                        continue;
                    }
                    insert_agenda_item(
                        &mut *conn,
                        tenant,
                        meeting_id,
                        &mut next_sequence,
                        "approval",
                        approval_id,
                        &[],
                        &evidence,
                    )
                    .await?;
                }
            }
            "register_item" => {
                let wanted = if kinds.is_empty() {
                    vec![
                        "risk".to_owned(),
                        "issue".to_owned(),
                        "assumption".to_owned(),
                        "dependency".to_owned(),
                    ]
                } else {
                    kinds
                };
                let rows: Vec<(Uuid,)> = sqlx::query_as(
                    "SELECT id FROM project_register_item
                      WHERE tenant_id = $1 AND kind = ANY($2::text[]) AND status <> 'closed'
                      ORDER BY created_at, id LIMIT $3",
                )
                .bind(tenant.as_uuid())
                .bind(&wanted)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?;
                for (id,) in rows {
                    insert_agenda_item(
                        &mut *conn,
                        tenant,
                        meeting_id,
                        &mut next_sequence,
                        "register_item",
                        id,
                        &[],
                        &json!([]),
                    )
                    .await?;
                }
            }
            "asset" => {
                let wanted = if kinds.is_empty() {
                    vec!["waiver".to_owned(), "compliance_review".to_owned()]
                } else {
                    kinds
                };
                let rows: Vec<(Uuid,)> = sqlx::query_as(
                    "SELECT id FROM governed_asset
                      WHERE tenant_id = $1 AND kind = ANY($2::text[]) AND status IN ('draft', 'in_review')
                      ORDER BY updated_at DESC, id LIMIT $3",
                )
                .bind(tenant.as_uuid())
                .bind(&wanted)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?;
                for (id,) in rows {
                    insert_agenda_item(
                        &mut *conn,
                        tenant,
                        meeting_id,
                        &mut next_sequence,
                        "asset",
                        id,
                        &[],
                        &json!([]),
                    )
                    .await?;
                }
            }
            "cadence" => {
                let rows: Vec<(Uuid,)> = if kinds.is_empty() {
                    sqlx::query_as(
                        "SELECT id FROM programme_cadence
                          WHERE tenant_id = $1 AND state = 'planned'
                          ORDER BY created_at, id LIMIT $2",
                    )
                    .bind(tenant.as_uuid())
                    .bind(limit)
                    .fetch_all(&mut *conn)
                    .await?
                } else {
                    sqlx::query_as(
                        "SELECT id FROM programme_cadence
                          WHERE tenant_id = $1 AND state = 'planned' AND kind = ANY($2::text[])
                          ORDER BY created_at, id LIMIT $3",
                    )
                    .bind(tenant.as_uuid())
                    .bind(&kinds)
                    .bind(limit)
                    .fetch_all(&mut *conn)
                    .await?
                };
                for (id,) in rows {
                    insert_agenda_item(
                        &mut *conn,
                        tenant,
                        meeting_id,
                        &mut next_sequence,
                        "cadence",
                        id,
                        &[],
                        &json!([]),
                    )
                    .await?;
                }
            }
            "" => {}
            other => {
                return Err(MeetingStoreError::Illegal {
                    what: "agenda source",
                    why: format!("unknown source kind {other:?}"),
                });
            }
        }
    }

    list_agenda_tx(&mut *conn, tenant, meeting_id).await
}

pub async fn agenda_add_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    new: &NewAgendaItem<'_>,
) -> Result<AgendaItemRecord> {
    if !RESOLVED_KINDS.contains(&new.resolved_kind) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_resolved_kind",
            detail: format!("resolved_kind must be one of {RESOLVED_KINDS:?}"),
        });
    }
    let row: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT status, meeting_type_id FROM meeting WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (status, meeting_type_id) = row.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if !matches!(status.as_str(), "scheduled" | "live") {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting takes no agenda items"),
        });
    }
    let ty = get_meeting_type_tx(&mut *conn, tenant, meeting_type_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting type"))?;

    if new.resolved_kind == "approval" {
        let approval: Option<(String, Value)> =
            sqlx::query_as("SELECT status, meta FROM approval WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(new.resolved_id)
                .fetch_optional(&mut *conn)
                .await?;
        let (approval_status, meta) = approval.ok_or(MeetingStoreError::NotFound("approval"))?;
        if approval_status != "pending" {
            return Err(MeetingStoreError::Conflict {
                what: "approval",
                why: format!(
                    "the approval is {approval_status}; a deciding item names a pending approval"
                ),
            });
        }
        let fresh = evidence_count(new.evidence_refs);
        let existing = evidence_count(&meta);
        if fresh.max(existing) < i64::from(ty.evidence_minimum) {
            return Err(MeetingStoreError::Unprocessable {
                code: "item_evidence_missing",
                detail: format!(
                    "the meeting type requires at least {} evidence refs",
                    ty.evidence_minimum
                ),
            });
        }
    }

    let mut sequence: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(sequence), 0) + 1 FROM meeting_agenda_item
          WHERE tenant_id = $1 AND meeting_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_one(&mut *conn)
    .await?;
    let inserted = insert_agenda_item(
        &mut *conn,
        tenant,
        meeting_id,
        &mut sequence,
        new.resolved_kind,
        new.resolved_id,
        new.participating_departments,
        new.evidence_refs,
    )
    .await?;
    match inserted {
        Some(item) => Ok(item),
        None => Err(MeetingStoreError::Conflict {
            what: "agenda item",
            why: "this object is already on the agenda".into(),
        }),
    }
}

async fn insert_agenda_item(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    next_sequence: &mut i32,
    resolved_kind: &str,
    resolved_id: Uuid,
    participating_departments: &[Uuid],
    evidence_refs: &Value,
) -> Result<Option<AgendaItemRecord>> {
    let sql = format!(
        "INSERT INTO meeting_agenda_item (tenant_id, meeting_id, sequence, resolved_kind,
             resolved_id, participating_departments, evidence_refs)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (meeting_id, resolved_kind, resolved_id) DO NOTHING
         RETURNING {AGENDA_COLUMNS}"
    );
    let row = sqlx::query_as::<_, AgendaItemRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .bind(*next_sequence)
        .bind(resolved_kind)
        .bind(resolved_id)
        .bind(participating_departments)
        .bind(evidence_refs)
        .fetch_optional(&mut *conn)
        .await?;
    if row.is_some() {
        *next_sequence += 1;
    }
    Ok(row)
}

// ── positions ───────────────────────────────────────────────────────────────────────────

pub async fn position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    new: &NewPosition<'_>,
) -> Result<PositionRecord> {
    if !PARTICIPANT_KINDS.contains(&new.participant_kind) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_participant_kind",
            detail: format!("participant_kind must be one of {PARTICIPANT_KINDS:?}"),
        });
    }
    if !STANCES.contains(&new.stance) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_stance",
            detail: format!("stance must be one of {STANCES:?}"),
        });
    }
    if !SURFACES.contains(&new.surface) {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_surface",
            detail: format!("surface must be one of {SURFACES:?}"),
        });
    }
    if let Some(commitment) = new.commitment {
        validate_commitment(commitment)?;
    }
    let meeting: Option<(String,)> =
        sqlx::query_as("SELECT status FROM meeting WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(meeting_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (status,) = meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if status != "live" {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting takes no positions"),
        });
    }
    let item: Option<(Option<String>, String, Uuid)> = sqlx::query_as(
        "SELECT outcome, resolved_kind, resolved_id FROM meeting_agenda_item
          WHERE tenant_id = $1 AND id = $2 AND meeting_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(new.agenda_item_id)
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (outcome, resolved_kind, resolved_id) =
        item.ok_or(MeetingStoreError::NotFound("agenda item"))?;
    if outcome.is_some() {
        return Err(MeetingStoreError::Illegal {
            what: "agenda item",
            why: "the item is closed; its positions are immutable".into(),
        });
    }
    if resolved_kind == "approval" {
        // First decision wins: once the linked approval holds this seat's decision, no
        // position may replace it, whichever surface arrives second.
        let gate: Option<(String,)> =
            sqlx::query_as("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
                .bind(tenant.as_uuid())
                .bind(resolved_id)
                .fetch_optional(&mut *conn)
                .await?;
        let (status,) = gate.ok_or(MeetingStoreError::NotFound("approval"))?;
        if status != "pending" {
            return Err(MeetingStoreError::Refused {
                code: "decision_already_recorded",
                detail: format!("the linked approval is {status}; the first decision wins"),
            });
        }
        let (decision_kind, column) = if new.participant_kind == "job_binding" {
            ("job", "job_binding_id")
        } else {
            ("human", "approver_id")
        };
        let sql = format!(
            "SELECT count(*) FROM approval_decision
              WHERE tenant_id = $1 AND approval_id = $2 AND participant_kind = $3
                AND {column} = $4"
        );
        let decided: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(resolved_id)
            .bind(decision_kind)
            .bind(new.participant_id)
            .fetch_one(&mut *conn)
            .await?;
        if decided > 0 {
            return Err(MeetingStoreError::Refused {
                code: "decision_already_recorded",
                detail: "this seat already decided; the first decision wins".into(),
            });
        }
    }
    let attending: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM meeting_attendance
          WHERE tenant_id = $1 AND meeting_id = $2 AND participant_kind = $3
            AND participant_id = $4",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .bind(new.participant_kind)
    .bind(new.participant_id)
    .fetch_optional(&mut *conn)
    .await?;
    if attending.is_none() {
        return Err(MeetingStoreError::Illegal {
            what: "position",
            why: "the participant is not attending this meeting".into(),
        });
    }

    sqlx::query(
        "UPDATE meeting_position SET superseded_at = now(), updated_at = now(),
                version = version + 1
          WHERE tenant_id = $1 AND agenda_item_id = $2 AND participant_kind = $3
            AND participant_id = $4 AND superseded_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(new.agenda_item_id)
    .bind(new.participant_kind)
    .bind(new.participant_id)
    .execute(&mut *conn)
    .await?;

    let sql = format!(
        "INSERT INTO meeting_position (tenant_id, meeting_id, agenda_item_id, participant_kind,
             participant_id, stance, rationale, conditions, commitment, surface)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {POSITION_COLUMNS}"
    );
    sqlx::query_as::<_, PositionRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .bind(new.agenda_item_id)
        .bind(new.participant_kind)
        .bind(new.participant_id)
        .bind(new.stance)
        .bind(new.rationale)
        .bind(new.conditions)
        .bind(new.commitment)
        .bind(new.surface)
        .fetch_one(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

// ── reads ──────────────────────────────────────────────────────────────────────────────

pub async fn get_meeting_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<MeetingRecord>> {
    let sql = format!(
        "SELECT {MEETING_COLUMNS}
           FROM meeting m
           JOIN meeting_type t ON t.id = m.meeting_type_id AND t.tenant_id = m.tenant_id
           JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
          WHERE m.tenant_id = $1 AND m.id = $2"
    );
    sqlx::query_as::<_, MeetingRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn list_meetings_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<MeetingRecord>> {
    let sql = format!(
        "SELECT {MEETING_COLUMNS}
           FROM meeting m
           JOIN meeting_type t ON t.id = m.meeting_type_id AND t.tenant_id = m.tenant_id
           JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
          WHERE m.tenant_id = $1 AND ($2::text IS NULL OR m.status = $2)
          ORDER BY COALESCE(m.opens_at, m.created_at) DESC, m.id DESC
          LIMIT $3"
    );
    sqlx::query_as::<_, MeetingRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(status)
        .bind(limit.clamp(1, 100))
        .fetch_all(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn list_agenda_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<Vec<AgendaItemRecord>> {
    let sql = format!(
        "SELECT {AGENDA_COLUMNS} FROM meeting_agenda_item
          WHERE tenant_id = $1 AND meeting_id = $2 ORDER BY sequence"
    );
    sqlx::query_as::<_, AgendaItemRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn list_attendance_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<Vec<AttendanceRecord>> {
    let sql = format!(
        "SELECT {ATTENDANCE_COLUMNS} FROM meeting_attendance
          WHERE tenant_id = $1 AND meeting_id = $2 ORDER BY seat, participant_kind, participant_id"
    );
    sqlx::query_as::<_, AttendanceRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn list_positions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<Vec<PositionRecord>> {
    let sql = format!(
        "SELECT {POSITION_COLUMNS} FROM meeting_position
          WHERE tenant_id = $1 AND meeting_id = $2 AND superseded_at IS NULL
          ORDER BY agenda_item_id, recorded_at, id"
    );
    sqlx::query_as::<_, PositionRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

pub async fn detail_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<Option<MeetingDetail>> {
    let Some(meeting) = get_meeting_tx(&mut *conn, tenant, meeting_id).await? else {
        return Ok(None);
    };
    let meeting_type = get_meeting_type_tx(&mut *conn, tenant, meeting.meeting_type_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting type"))?;
    let agenda = list_agenda_tx(&mut *conn, tenant, meeting_id).await?;
    let attendance = list_attendance_tx(&mut *conn, tenant, meeting_id).await?;
    let positions = list_positions_tx(&mut *conn, tenant, meeting_id).await?;
    let instruments =
        instruments_tx(&mut *conn, tenant, &meeting, &meeting_type, &attendance).await?;
    Ok(Some(MeetingDetail {
        meeting,
        meeting_type,
        agenda,
        attendance,
        positions,
        instruments,
    }))
}

// ── outcomes, minutes and the forum's close ────────────────────────────────────────────

/// A commitment names its owner and its due date; without both it is not recordable.
fn validate_commitment(commitment: &Value) -> Result<()> {
    let owner = commitment
        .get("owner")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let due = commitment
        .get("by")
        .or_else(|| commitment.get("due"))
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if !owner || !due {
        return Err(MeetingStoreError::Unprocessable {
            code: "commitment_needs_owner_and_date",
            detail: "a commitment names an owner and a due date".into(),
        });
    }
    Ok(())
}

fn parse_due(value: Option<&Value>) -> Result<Option<DateTime<Utc>>> {
    let Some(raw) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Ok(Some(parsed.with_timezone(&Utc)));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(Some(date.and_hms_opt(0, 0, 0).expect("midnight").and_utc()));
    }
    Err(MeetingStoreError::Unprocessable {
        code: "invalid_due_date",
        detail: format!("{raw:?} is not an RFC 3339 instant or a YYYY-MM-DD date"),
    })
}

async fn outcome_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
    title: &str,
    due_at: Option<DateTime<Utc>>,
    meta: Value,
) -> Result<Uuid> {
    let card: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT c.list_id, c.board_id
           FROM meeting m
           JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
          WHERE m.tenant_id = $1 AND m.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (list_id, board_id) = card.ok_or(MeetingStoreError::NotFound("meeting card"))?;
    let card_type_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM card_type
          WHERE tenant_id = $1 AND code = 'meeting_action' AND is_active",
    )
    .bind(tenant.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;
    let card_type_id = card_type_id.ok_or_else(|| MeetingStoreError::Conflict {
        what: "meeting action card type",
        why: "no `meeting_action` card type is installed in this tenant; seed the catalogue first"
            .into(),
    })?;
    crate::position::lock_board_tx(&mut *conn, tenant, board_id).await?;
    let position =
        crate::position::next_card_position_tx(&mut *conn, tenant, board_id, list_id).await?;
    let created = crate::card::insert_card_tx(
        &mut *conn,
        tenant,
        &NewCard {
            list_id,
            card_type_id,
            name: title.to_owned(),
            description: String::new(),
            position,
            parent_card_id: None,
            due_at,
            assignee_id: None,
            created_by: None,
            job_binding_id: None,
            meta: json!({
                "meeting_id": meeting_id,
                "agenda_item_id": agenda_item_id,
                "source": meta,
            }),
        },
    )
    .await?;
    Ok(created.id)
}

async fn close_non_decision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
    ty: &MeetingTypeRecord,
    non_decision: &NonDecision<'_>,
) -> Result<CloseItemResult> {
    let outcome = non_decision.outcome;
    if !matches!(outcome, "commitment" | "action" | "information") {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_outcome",
            detail: "a non-decision item closes as commitment, action or information".into(),
        });
    }
    if !ty.allowed_outcomes.iter().any(|allowed| allowed == outcome) {
        return Err(MeetingStoreError::Unprocessable {
            code: "outcome_not_allowed",
            detail: format!("this meeting type does not allow the {outcome} outcome"),
        });
    }
    let outcome_ref = match outcome {
        "commitment" => {
            let commitment = non_decision
                .commitment
                .ok_or(MeetingStoreError::Unprocessable {
                    code: "commitment_required",
                    detail: "a commitment outcome names its commitment".into(),
                })?;
            validate_commitment(commitment)?;
            let title = commitment
                .get("what")
                .or_else(|| commitment.get("title"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(MeetingStoreError::Unprocessable {
                    code: "commitment_required",
                    detail: "a commitment names what is committed".into(),
                })?;
            let due = parse_due(commitment.get("by").or_else(|| commitment.get("due")))?;
            Some(
                outcome_card_tx(
                    &mut *conn,
                    tenant,
                    meeting_id,
                    agenda_item_id,
                    title,
                    due,
                    json!({"kind": "commitment", "commitment": commitment}),
                )
                .await?,
            )
        }
        "action" => {
            let action = non_decision
                .action
                .ok_or(MeetingStoreError::Unprocessable {
                    code: "action_required",
                    detail: "an action outcome names its action".into(),
                })?;
            let title = action
                .get("title")
                .or_else(|| action.get("what"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(MeetingStoreError::Unprocessable {
                    code: "action_required",
                    detail: "an action names what must be done".into(),
                })?;
            let due = parse_due(action.get("by").or_else(|| action.get("due")))?;
            Some(
                outcome_card_tx(
                    &mut *conn,
                    tenant,
                    meeting_id,
                    agenda_item_id,
                    title,
                    due,
                    json!({"kind": "action", "action": action}),
                )
                .await?,
            )
        }
        _ => None,
    };
    let item = close_item_row_tx(
        &mut *conn,
        tenant,
        agenda_item_id,
        outcome,
        outcome_ref,
        None,
    )
    .await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting_item.closed".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({
                "meeting_id": meeting_id,
                "agenda_item_id": agenda_item_id,
                "outcome": outcome,
                "outcome_ref": outcome_ref,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    Ok(CloseItemResult {
        item,
        approval_status: None,
        decisions_written: 0,
        counted_positions: 0,
        quorum: 0,
    })
}

pub async fn defer_agenda_item_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
    reason: &str,
) -> Result<AgendaItemRecord> {
    if reason.trim().is_empty() {
        return Err(MeetingStoreError::Unprocessable {
            code: "defer_reason_required",
            detail: "a deferral names its reason".into(),
        });
    }
    let meeting: Option<(String,)> =
        sqlx::query_as("SELECT status FROM meeting WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
            .bind(tenant.as_uuid())
            .bind(meeting_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (status,) = meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if status != "live" {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting cannot defer an item"),
        });
    }
    let item: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT outcome FROM meeting_agenda_item
          WHERE tenant_id = $1 AND id = $2 AND meeting_id = $3 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(agenda_item_id)
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (outcome,) = item.ok_or(MeetingStoreError::NotFound("agenda item"))?;
    if outcome.is_some() {
        return Err(MeetingStoreError::Illegal {
            what: "agenda item",
            why: "the item is already closed".into(),
        });
    }
    let item = close_item_row_tx(
        &mut *conn,
        tenant,
        agenda_item_id,
        "deferred",
        None,
        Some(reason),
    )
    .await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting_item.closed".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({
                "meeting_id": meeting_id,
                "agenda_item_id": agenda_item_id,
                "outcome": "deferred",
                "defer_reason": reason,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    Ok(item)
}

pub async fn close_meeting_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
) -> Result<MeetingRecord> {
    let meeting: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT status, card_id FROM meeting
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (status, card_id) = meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if status != "live" {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting cannot be closed"),
        });
    }
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_agenda_item
          WHERE tenant_id = $1 AND meeting_id = $2 AND outcome IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_one(&mut *conn)
    .await?;
    if open > 0 {
        return Err(MeetingStoreError::Refused {
            code: "meeting_items_open",
            detail: format!("{open} agenda item(s) have no outcome"),
        });
    }
    let minutes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM governed_asset
          WHERE tenant_id = $1 AND kind = 'minutes' AND meta->>'meeting_id' = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id.to_string())
    .fetch_one(&mut *conn)
    .await?;
    if minutes == 0 {
        return Err(MeetingStoreError::Refused {
            code: "minutes_not_published",
            detail: "the meeting DoD requires published minutes".into(),
        });
    }
    sqlx::query(
        "UPDATE meeting
            SET status = 'closed', closed_at = now(), updated_at = now(), version = version + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .execute(&mut *conn)
    .await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting.closed".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({"meeting_id": meeting_id, "card_id": card_id}),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    get_meeting_tx(&mut *conn, tenant, meeting_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting"))
}

pub struct NewMinutes<'a> {
    pub approval_id: Uuid,
    pub title: &'a str,
    pub lines: &'a [(Uuid, String)],
}

pub async fn minutes_publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    new: &NewMinutes<'_>,
) -> Result<Uuid> {
    let meeting: Option<(String, Uuid)> =
        sqlx::query_as("SELECT status, card_id FROM meeting WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(meeting_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (status, meeting_card) = meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if !matches!(status.as_str(), "live" | "closed") {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {status} meeting has no minutes"),
        });
    }
    let chair: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT participant_kind, participant_id FROM meeting_attendance
          WHERE tenant_id = $1 AND meeting_id = $2 AND seat = 'chair'
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (chair_kind, chair_id) = chair.ok_or(MeetingStoreError::Refused {
        code: "minutes_need_chair_approval",
        detail: "the meeting has no chair seat to approve the minutes".into(),
    })?;
    let approval: Option<(String, Option<Uuid>, Value)> = sqlx::query_as(
        "SELECT status, card_id, meta FROM approval WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(new.approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (approval_status, approval_card, approval_meta) =
        approval.ok_or(MeetingStoreError::NotFound("minutes approval"))?;
    let meeting_text = meeting_id.to_string();
    let references_meeting = approval_card == Some(meeting_card)
        || approval_meta.get("meeting_id").and_then(Value::as_str) == Some(meeting_text.as_str());
    if approval_status != "approved" || !references_meeting {
        return Err(MeetingStoreError::Refused {
            code: "minutes_need_chair_approval",
            detail: "publication needs an approved approval that names this meeting".into(),
        });
    }
    let (decision_kind, approver_column) = if chair_kind == "job_binding" {
        ("job", "job_binding_id")
    } else {
        ("human", "approver_id")
    };
    let sql = format!(
        "SELECT count(*) FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2 AND participant_kind = $3
            AND {approver_column} = $4"
    );
    let chair_decided: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(new.approval_id)
        .bind(decision_kind)
        .bind(chair_id)
        .fetch_one(&mut *conn)
        .await?;
    if chair_decided == 0 {
        return Err(MeetingStoreError::Refused {
            code: "minutes_need_chair_approval",
            detail: "the chair has not decided the publication approval".into(),
        });
    }
    let mut sequence = 0_i64;
    let mut body = format!("# {}\n\n", new.title);
    for (agenda_item_id, text) in new.lines {
        if text.trim().is_empty() {
            return Err(MeetingStoreError::Unprocessable {
                code: "minute_line_empty",
                detail: "a minute line needs text".into(),
            });
        }
        let belongs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM meeting_agenda_item
              WHERE tenant_id = $1 AND meeting_id = $2 AND id = $3",
        )
        .bind(tenant.as_uuid())
        .bind(meeting_id)
        .bind(agenda_item_id)
        .fetch_one(&mut *conn)
        .await?;
        if belongs == 0 {
            return Err(MeetingStoreError::Unprocessable {
                code: "minute_line_unmatched",
                detail: format!("{agenda_item_id} is not an item of this meeting"),
            });
        }
        sequence += 1;
        body.push_str(&format!("- ({sequence}) {text}\n"));
    }
    let asset: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, body, status, project_id,
             meta)
         SELECT $1, 'minutes', $2, $3, $4, 'approved', b.project_id, $5
           FROM meeting m
           JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
           JOIN board b ON b.id = c.board_id
          WHERE m.tenant_id = $1 AND m.id = $6
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("minutes-{meeting_id}"))
    .bind(new.title)
    .bind(&body)
    .bind(json!({"meeting_id": meeting_id, "approval_id": new.approval_id}))
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| {
        if is_unique_on(&error, "governed_asset_tenant_kind_key_key") {
            MeetingStoreError::Refused {
                code: "minutes_already_published",
                detail: "this meeting's minutes are already published".into(),
            }
        } else {
            error.into()
        }
    })?
    .ok_or(MeetingStoreError::NotFound("meeting card"))?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting_minutes.published".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({
                "meeting_id": meeting_id,
                "asset_id": asset,
                "approval_id": new.approval_id,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    Ok(asset)
}

/// The seven inspect-and-adapt instruments, recomputed from rows (never stored).
async fn instruments_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting: &MeetingRecord,
    meeting_type: &MeetingTypeRecord,
    attendance: &[AttendanceRecord],
) -> Result<MeetingInstruments> {
    let row: (i64, i64, i64, i64, i64, i64, i64, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE i.outcome = 'decision')::bigint,
                count(*)::bigint,
                count(*) FILTER (WHERE i.outcome IS NULL)::bigint,
                count(*) FILTER (WHERE i.outcome = 'deferred'
                                  AND i.defer_reason = 'inquorate')::bigint,
                count(*) FILTER (WHERE i.outcome = 'commitment')::bigint,
                count(*) FILTER (WHERE i.outcome = 'action')::bigint,
                count(*) FILTER (WHERE i.outcome IN ('commitment', 'action')
                                  AND ol.kind = 'done')::bigint,
                min(i.closed_at) FILTER (WHERE i.outcome = 'decision')
           FROM meeting_agenda_item i
           LEFT JOIN card oi ON oi.id = i.outcome_ref AND oi.tenant_id = i.tenant_id
           LEFT JOIN list ol ON ol.id = oi.list_id AND ol.tenant_id = oi.tenant_id
          WHERE i.tenant_id = $1 AND i.meeting_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(meeting.id)
    .fetch_one(&mut *conn)
    .await?;
    let (
        decisions,
        items,
        open_items,
        inquorate,
        commitments,
        actions,
        closed_outcomes,
        first_decision,
    ) = row;
    let slipped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_agenda_item i
           JOIN card oi ON oi.id = i.outcome_ref AND oi.tenant_id = i.tenant_id
           LEFT JOIN list ol ON ol.id = oi.list_id AND ol.tenant_id = oi.tenant_id
          WHERE i.tenant_id = $1 AND i.meeting_id = $2
            AND i.outcome IN ('commitment', 'action')
            AND oi.due_at IS NOT NULL AND oi.due_at < now()
            AND (ol.kind IS NULL OR ol.kind <> 'done')",
    )
    .bind(tenant.as_uuid())
    .bind(meeting.id)
    .fetch_one(&mut *conn)
    .await?;
    let accepted = attendance
        .iter()
        .filter(|row| row.response_state == "accepted" && row.seat != "observer")
        .count() as f64;
    let hours = match (meeting.opens_at, meeting.closed_at) {
        (Some(opens), Some(closed)) => {
            let seconds = (closed - opens).num_seconds().max(0) as f64;
            let capped = seconds.min(meeting_type.timebox_seconds.max(0) as f64);
            accepted * capped / 3600.0
        }
        _ => 0.0,
    };
    let latency = meeting
        .opens_at
        .and_then(|opens| first_decision.map(|first| (first - opens).num_seconds().max(0)));
    let conversion = if items > 0 {
        decisions as f64 / items as f64
    } else {
        0.0
    };
    let quorum_miss = if items > 0 {
        inquorate as f64 / items as f64
    } else {
        0.0
    };
    let action_closure = if commitments + actions > 0 {
        closed_outcomes as f64 / (commitments + actions) as f64
    } else {
        0.0
    };
    Ok(MeetingInstruments {
        decisions,
        items,
        open_items,
        decision_latency_seconds: latency,
        agenda_conversion: conversion,
        quorum_miss_rate: quorum_miss,
        commitment_slippage: slipped,
        action_closure,
        participant_hours: hours,
        person_hours_per_decision: if decisions > 0 {
            hours / decisions as f64
        } else {
            0.0
        },
        decisions_per_meeting: decisions as f64,
    })
}

// ── authority ──────────────────────────────────────────────────────────────────────────

async fn ensure_convener_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    ty: &MeetingTypeRecord,
    convener_kind: &str,
    convener_id: Uuid,
    actor: MeetingActor<'_>,
) -> Result<()> {
    let authority = &ty.convener_authority;
    let listed = |field: &str, value: &str| -> bool {
        match authority.get(field).and_then(Value::as_array) {
            Some(items) if !items.is_empty() => {
                items.iter().any(|item| item.as_str() == Some(value))
            }
            _ => true,
        }
    };
    if !listed("kinds", convener_kind) {
        return Err(MeetingStoreError::Illegal {
            what: "convener",
            why: format!("this meeting type does not permit a {convener_kind} convener"),
        });
    }
    if !listed("ids", &convener_id.to_string()) {
        return Err(MeetingStoreError::Illegal {
            what: "convener",
            why: "this convener is not named by the type's authority row".into(),
        });
    }
    if !listed("roles", actor.role) {
        return Err(MeetingStoreError::Illegal {
            what: "convener",
            why: format!("role {} may not convene this type", actor.role),
        });
    }
    match convener_kind {
        "governance_body" => {
            let body: Option<(String, Value)> = sqlx::query_as(
                "SELECT status, members FROM governance_body WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(convener_id)
            .fetch_optional(&mut *conn)
            .await?;
            let (status, members) = body.ok_or(MeetingStoreError::NotFound("governance body"))?;
            if status != "active" {
                return Err(MeetingStoreError::Illegal {
                    what: "convener",
                    why: format!("the governance body is {status}; only an active body convenes"),
                });
            }
            if let Some(items) = members.as_array() {
                if !items.is_empty() {
                    let actor_id = actor.id.to_string();
                    let member = items.iter().any(|item| match item {
                        Value::String(id) => id == &actor_id,
                        Value::Object(map) => ["id", "user_id", "app_user_id"]
                            .iter()
                            .any(|key| map.get(*key).and_then(Value::as_str) == Some(&actor_id)),
                        _ => false,
                    });
                    if !member {
                        return Err(MeetingStoreError::Illegal {
                            what: "convener",
                            why: "the caller is not a member of the governance body".into(),
                        });
                    }
                }
            }
        }
        "job_binding" => {
            let binding: Option<(Value, String)> = sqlx::query_as(
                "SELECT meta, status FROM job_binding WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(convener_id)
            .fetch_optional(&mut *conn)
            .await?;
            let (meta, status) = binding.ok_or(MeetingStoreError::NotFound("job binding"))?;
            if status != "active" {
                return Err(MeetingStoreError::Illegal {
                    what: "convener",
                    why: format!("the job binding is {status}; only an active binding convenes"),
                });
            }
            let allowed = meta.get("authority");
            let permitted = match allowed {
                Some(Value::String(value)) => value == "*" || value == &ty.key,
                Some(Value::Array(items)) => {
                    items.iter().any(|item| item.as_str() == Some(&ty.key))
                }
                _ => false,
            };
            if !permitted {
                return Err(MeetingStoreError::Illegal {
                    what: "convener",
                    why: "the job binding's meta.authority does not name this meeting type".into(),
                });
            }
        }
        "app_user" => {
            // `app_user` is global; the tenant link is `membership`.
            let user: Option<(Uuid,)> = sqlx::query_as(
                "SELECT u.id FROM app_user u
                   JOIN membership m ON m.user_id = u.id AND m.tenant_id = $1
                  WHERE u.id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(convener_id)
            .fetch_optional(&mut *conn)
            .await?;
            if user.is_none() {
                return Err(MeetingStoreError::NotFound("app user"));
            }
            let membership: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM membership WHERE tenant_id = $1 AND user_id = $2")
                    .bind(tenant.as_uuid())
                    .bind(actor.id)
                    .fetch_optional(&mut *conn)
                    .await?;
            if membership.is_none() {
                return Err(MeetingStoreError::Illegal {
                    what: "convener",
                    why: "the caller is not a member of this tenant".into(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

async fn ensure_participant_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    id: Uuid,
) -> Result<()> {
    let found: Option<(Uuid,)> =
        match kind {
            "job_binding" => sqlx::query_as(
                "SELECT id FROM job_binding WHERE tenant_id = $1 AND id = $2 AND status = 'active'",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
            "app_user" => {
                sqlx::query_as(
                    "SELECT u.id FROM app_user u
                   JOIN membership m ON m.user_id = u.id AND m.tenant_id = $1
                  WHERE u.id = $2",
                )
                .bind(tenant.as_uuid())
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
            }
            _ => None,
        };
    found
        .map(|_| ())
        .ok_or(MeetingStoreError::NotFound("participant"))
}

// ── close_item — the outcome arithmetic ────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CloseItemResult {
    pub item: AgendaItemRecord,
    pub approval_status: Option<String>,
    pub decisions_written: usize,
    pub counted_positions: usize,
    pub quorum: usize,
}

struct ApprovalGate {
    id: Uuid,
    status: String,
    required_decisions: i32,
    meta: Value,
    requested_by: Option<Uuid>,
    gate: String,
    card_assignee: Option<Uuid>,
    card_job: Option<Uuid>,
    run_job: Option<Uuid>,
}

struct SeatDecision {
    approver_id: Option<Uuid>,
    job_binding_id: Option<Uuid>,
    participant_kind: &'static str,
    decision: &'static str,
    note: String,
    via: Value,
    decided_by_kind: &'static str,
}

/// Close one agenda item: quorum first, then the meeting's rule, then the linked approval's
/// stricter policy; the stricter result wins. A deciding close writes one `approval_decision`
/// row per qualifying seat and sets the gate's status once. A refusal writes nothing.
pub async fn close_item_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
    non_decision: Option<&NonDecision<'_>>,
) -> Result<CloseItemResult> {
    let meeting: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT status, meeting_type_id FROM meeting
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (meeting_status, meeting_type_id) =
        meeting.ok_or(MeetingStoreError::NotFound("meeting"))?;
    if meeting_status != "live" {
        return Err(MeetingStoreError::Illegal {
            what: "meeting",
            why: format!("a {meeting_status} meeting cannot close an item"),
        });
    }
    let ty = get_meeting_type_tx(&mut *conn, tenant, meeting_type_id)
        .await?
        .ok_or(MeetingStoreError::NotFound("meeting type"))?;

    let item: Option<(Option<String>, String, Uuid, Value, Vec<Uuid>)> = sqlx::query_as(
        "SELECT outcome, resolved_kind, resolved_id, evidence_refs, participating_departments
           FROM meeting_agenda_item
          WHERE tenant_id = $1 AND id = $2 AND meeting_id = $3 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(agenda_item_id)
    .bind(meeting_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (outcome, resolved_kind, resolved_id, item_evidence, item_departments) =
        item.ok_or(MeetingStoreError::NotFound("agenda item"))?;
    if outcome.is_some() {
        return Err(MeetingStoreError::Illegal {
            what: "agenda item",
            why: "the item is already closed".into(),
        });
    }
    if resolved_kind != "approval" {
        let non_decision = non_decision.ok_or(MeetingStoreError::Unprocessable {
            code: "outcome_required",
            detail: "a non-decision item closes with an outcome: commitment, action or information"
                .into(),
        })?;
        return close_non_decision_tx(
            &mut *conn,
            tenant,
            meeting_id,
            agenda_item_id,
            &ty,
            non_decision,
        )
        .await;
    }
    if non_decision.is_some_and(|other| other.outcome != "decision") {
        return Err(MeetingStoreError::Unprocessable {
            code: "invalid_outcome",
            detail: "a deciding item closes as a decision; its result is derived from the seats"
                .into(),
        });
    }

    let attendance = list_attendance_tx(&mut *conn, tenant, meeting_id).await?;
    let accepted: Vec<&AttendanceRecord> = attendance
        .iter()
        .filter(|row| row.response_state == "accepted" && row.seat != "observer")
        .collect();

    // 1. Quorum and required participation.
    let mut missing: Vec<String> = Vec::new();
    if (accepted.len() as i32) < ty.quorum {
        missing.push(format!(
            "quorum {} attending, {} required",
            accepted.len(),
            ty.quorum
        ));
    }
    for (department, minimum) in required_departments(&ty.required_participation, &item_departments)
    {
        let present = accepted
            .iter()
            .filter(|row| row.department_id == Some(department))
            .count() as i64;
        if present < minimum {
            missing.push(format!(
                "department {department} has {present}, {minimum} required"
            ));
        }
    }
    for (seat, minimum) in required_seats(&ty.required_participation) {
        let present = accepted.iter().filter(|row| row.seat == seat).count() as i64;
        if present < minimum {
            missing.push(format!("seat {seat} has {present}, {minimum} required"));
        }
    }
    if !missing.is_empty() {
        return defer_item_tx(
            conn,
            tenant,
            meeting_id,
            agenda_item_id,
            "inquorate",
            accepted.len(),
            None,
        )
        .await;
    }

    // 2. The cast positions, with observers and P4-excluded seats removed.
    let positions = list_positions_tx(&mut *conn, tenant, meeting_id).await?;
    let gate = if resolved_kind == "approval" {
        Some(load_gate_tx(&mut *conn, tenant, resolved_id).await?)
    } else {
        None
    };
    let self_approval_allowed = match &gate {
        Some(gate) => crate::approval_policy::allows_self_approval(&mut *conn, tenant, &gate.gate)
            .await
            .unwrap_or(false),
        None => false,
    };
    let mut decisions: Vec<SeatDecision> = Vec::new();
    let mut cast_approve = 0_i32;
    let mut cast_reject = 0_i32;
    let mut counted = 0_usize;
    let mut cast_departments: Vec<Uuid> = Vec::new();
    let mut chair_stance: Option<&str> = None;
    for position in positions
        .iter()
        .filter(|position| position.agenda_item_id == agenda_item_id)
    {
        let Some(seat) = attendance.iter().find(|row| {
            row.participant_kind == position.participant_kind
                && row.participant_id == position.participant_id
        }) else {
            continue;
        };
        if seat.seat == "observer" {
            continue;
        }
        let excluded = match &gate {
            Some(gate) => participant_excluded(position, seat, gate, self_approval_allowed),
            None => false,
        };
        if excluded {
            continue;
        }
        if seat.seat == "chair" {
            chair_stance = Some(&position.stance);
        }
        if let Some(department) = seat.department_id {
            cast_departments.push(department);
        }
        match position.stance.as_str() {
            "approve" | "approve_with_conditions" | "reject" => {
                counted += 1;
                match position.stance.as_str() {
                    "reject" => cast_reject += 1,
                    _ => cast_approve += 1,
                }
                let job = position.participant_kind == "job_binding";
                let kind = if job { "job" } else { "human" };
                decisions.push(SeatDecision {
                    approver_id: if job {
                        None
                    } else {
                        Some(position.participant_id)
                    },
                    job_binding_id: if job {
                        Some(position.participant_id)
                    } else {
                        None
                    },
                    participant_kind: kind,
                    decision: if position.stance == "reject" {
                        "reject"
                    } else {
                        "approve"
                    },
                    note: position.rationale.clone(),
                    via: json!({
                        "surface": "meeting",
                        "channel": position.surface,
                        "meeting_id": meeting_id,
                    }),
                    decided_by_kind: if job { "agent" } else { "human" },
                });
            }
            // Abstain and request_evidence count toward quorum only; they are never decision rows.
            _ => {}
        }
    }
    if decisions.is_empty() {
        return defer_item_tx(
            conn,
            tenant,
            meeting_id,
            agenda_item_id,
            "no_positions",
            accepted.len(),
            None,
        )
        .await;
    }

    // 3. The meeting's rule.
    let approved = match ty.decision_rule.as_str() {
        "majority" => {
            if cast_approve == cast_reject {
                return defer_item_tx(
                    conn,
                    tenant,
                    meeting_id,
                    agenda_item_id,
                    "tie",
                    accepted.len(),
                    None,
                )
                .await;
            }
            cast_approve > cast_reject
        }
        "unanimous" => {
            if cast_reject > 0 {
                false
            } else if cast_approve > 0 {
                true
            } else {
                return defer_item_tx(
                    conn,
                    tenant,
                    meeting_id,
                    agenda_item_id,
                    "no_positions",
                    accepted.len(),
                    None,
                )
                .await;
            }
        }
        "chair" => {
            // The chair carries once quorum exists and every required department has stated a
            // position. A missing seat is a deferral, not an invented approval.
            let every_department =
                required_departments(&ty.required_participation, &item_departments)
                    .into_iter()
                    .all(|(department, _)| cast_departments.contains(&department));
            if !every_department {
                return defer_item_tx(
                    conn,
                    tenant,
                    meeting_id,
                    agenda_item_id,
                    "inquorate",
                    accepted.len(),
                    None,
                )
                .await;
            }
            match chair_stance {
                Some("approve") | Some("approve_with_conditions") => true,
                Some("reject") => false,
                _ => {
                    return defer_item_tx(
                        conn,
                        tenant,
                        meeting_id,
                        agenda_item_id,
                        "chair_position_missing",
                        accepted.len(),
                        None,
                    )
                    .await;
                }
            }
        }
        _ => {
            return Err(MeetingStoreError::Illegal {
                what: "meeting type",
                why: format!("unknown decision rule {}", ty.decision_rule),
            });
        }
    };

    // 4. The linked approval's stricter policy: required decisions (dual control), evidence and
    //    P4. A meeting never lowers any of them, and writes no rows when it would.
    let approval_status = if let Some(gate) = &gate {
        if gate.status != "pending" {
            return Err(MeetingStoreError::Conflict {
                what: "approval",
                why: format!("the approval is {}; the first decision wins", gate.status),
            });
        }
        if evidence_count(&gate.meta) < i64::from(ty.evidence_minimum)
            || evidence_count(&item_evidence) < i64::from(ty.evidence_minimum)
        {
            return Err(MeetingStoreError::Stricter {
                detail: format!(
                    "the linked approval's evidence is below the meeting type's minimum of {}",
                    ty.evidence_minimum
                ),
            });
        }
        if approved && cast_approve < gate.required_decisions {
            return Err(MeetingStoreError::Stricter {
                detail: format!(
                    "the approval requires {} distinct approvals (dual control); the meeting has {}",
                    gate.required_decisions, cast_approve
                ),
            });
        }
        let rows: Vec<crate::approval::MeetingDecisionRow<'_>> = decisions
            .iter()
            .map(|decision| crate::approval::MeetingDecisionRow {
                approver_id: decision.approver_id,
                job_binding_id: decision.job_binding_id,
                participant_kind: decision.participant_kind,
                decision: decision.decision,
                note: &decision.note,
                via: &decision.via,
                decided_by_kind: decision.decided_by_kind,
            })
            .collect();
        let approval = crate::approval::record_meeting_decisions_tx(
            &mut *conn, tenant, gate.id, &rows, approved,
        )
        .await
        .map_err(|error| MeetingStoreError::Conflict {
            what: "approval",
            why: error.to_string(),
        })?;
        Some(approval.status.to_string())
    } else {
        None
    };

    let item =
        close_item_row_tx(&mut *conn, tenant, agenda_item_id, "decision", None, None).await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting_item.closed".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({
                "meeting_id": meeting_id,
                "agenda_item_id": agenda_item_id,
                "outcome": "decision",
                "approval_status": approval_status,
                "positions": counted,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    Ok(CloseItemResult {
        item,
        approval_status,
        decisions_written: decisions.len(),
        counted_positions: counted,
        quorum: accepted.len(),
    })
}

async fn defer_item_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    meeting_id: Uuid,
    agenda_item_id: Uuid,
    reason: &str,
    quorum: usize,
    approval_status: Option<String>,
) -> Result<CloseItemResult> {
    let item = close_item_row_tx(
        &mut *conn,
        tenant,
        agenda_item_id,
        "deferred",
        None,
        Some(reason),
    )
    .await?;
    outbox::append(
        &mut *conn,
        tenant,
        &NewEvent {
            topic: format!("meeting:{meeting_id}"),
            event_type: "meeting_item.closed".into(),
            aggregate_type: "meeting".into(),
            aggregate_id: meeting_id,
            payload: json!({
                "meeting_id": meeting_id,
                "agenda_item_id": agenda_item_id,
                "outcome": "deferred",
                "defer_reason": reason,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await?;
    Ok(CloseItemResult {
        item,
        approval_status,
        decisions_written: 0,
        counted_positions: 0,
        quorum,
    })
}

async fn close_item_row_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    agenda_item_id: Uuid,
    outcome: &str,
    outcome_ref: Option<Uuid>,
    defer_reason: Option<&str>,
) -> Result<AgendaItemRecord> {
    let sql = format!(
        "UPDATE meeting_agenda_item
            SET outcome = $3, outcome_ref = $4, defer_reason = $5, closed_at = now(),
                updated_at = now(), version = version + 1
          WHERE tenant_id = $1 AND id = $2
          RETURNING {AGENDA_COLUMNS}"
    );
    sqlx::query_as::<_, AgendaItemRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(agenda_item_id)
        .bind(outcome)
        .bind(outcome_ref)
        .bind(defer_reason)
        .fetch_one(&mut *conn)
        .await
        .map_err(MeetingStoreError::from)
}

async fn load_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<ApprovalGate> {
    let row: Option<(
        Uuid,
        String,
        i32,
        Value,
        Option<Uuid>,
        String,
        Option<Uuid>,
        Option<Uuid>,
        Option<Uuid>,
    )> = sqlx::query_as(
        "SELECT a.id, a.status, a.required_decisions, a.meta, a.requested_by, a.gate,
                c.assignee_id AS card_assignee, c.job_binding_id AS card_job,
                r.job_binding_id AS run_job
           FROM approval a
           LEFT JOIN card c ON c.id = a.card_id AND c.tenant_id = a.tenant_id
           LEFT JOIN run r ON r.id = a.run_id AND r.tenant_id = a.tenant_id
          WHERE a.tenant_id = $1 AND a.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (
        id,
        status,
        required_decisions,
        meta,
        requested_by,
        gate,
        card_assignee,
        card_job,
        run_job,
    ) = row.ok_or(MeetingStoreError::NotFound("approval"))?;
    Ok(ApprovalGate {
        id,
        status,
        required_decisions,
        meta,
        requested_by,
        gate,
        card_assignee,
        card_job,
        run_job,
    })
}

/// P4: the executor never approves its own work. A human seat is excluded when it is the card's
/// assignee; a job seat when it is the card's or run's binding; and (unless the approval policy
/// allows self-approval) a human who requested the gate is excluded.
fn participant_excluded(
    position: &PositionRecord,
    _seat: &AttendanceRecord,
    gate: &ApprovalGate,
    self_approval_allowed: bool,
) -> bool {
    match position.participant_kind.as_str() {
        "app_user" => {
            gate.card_assignee == Some(position.participant_id)
                || (!self_approval_allowed && gate.requested_by == Some(position.participant_id))
        }
        "job_binding" => {
            gate.card_job == Some(position.participant_id)
                || gate.run_job == Some(position.participant_id)
        }
        _ => false,
    }
}

fn required_departments(value: &Value, item_departments: &[Uuid]) -> Vec<(Uuid, i64)> {
    let mut out: Vec<(Uuid, i64)> = Vec::new();
    let mut push = |entry: &Value| {
        let (id, minimum) = match entry {
            Value::String(raw) => (raw.parse::<Uuid>().ok(), 1),
            Value::Object(map) => {
                let raw = map
                    .get("id")
                    .or_else(|| map.get("department_id"))
                    .or_else(|| map.get("department"))
                    .and_then(Value::as_str);
                let minimum = map.get("minimum").and_then(Value::as_i64).unwrap_or(1);
                (raw.and_then(|raw| raw.parse::<Uuid>().ok()), minimum)
            }
            _ => (None, 0),
        };
        if let Some(id) = id {
            match out.iter_mut().find(|(existing, _)| *existing == id) {
                Some((_, current)) => *current = (*current).max(minimum),
                None => out.push((id, minimum.max(1))),
            }
        }
    };
    match value {
        Value::Array(items) => {
            for entry in items {
                push(entry);
            }
        }
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("departments") {
                for entry in items {
                    push(entry);
                }
            }
            if let Some(Value::Array(items)) = map.get("department_ids") {
                for entry in items {
                    push(entry);
                }
            }
        }
        _ => {}
    }
    for department in item_departments {
        match out.iter_mut().find(|(existing, _)| existing == department) {
            Some((_, current)) => *current = (*current).max(1),
            None => out.push((*department, 1)),
        }
    }
    out
}

fn required_seats(value: &Value) -> Vec<(String, i64)> {
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let Some(Value::Array(items)) = map.get("seats") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|entry| match entry {
            Value::String(seat) => Some((seat.clone(), 1)),
            Value::Object(fields) => fields.get("seat").and_then(Value::as_str).map(|seat| {
                (
                    seat.to_owned(),
                    fields.get("minimum").and_then(Value::as_i64).unwrap_or(1),
                )
            }),
            _ => None,
        })
        .collect()
}
