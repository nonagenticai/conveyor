//! The meetings action family (SPEC_V2 §6.8, §10.3–§10.5, D66; T19, issue #606).
//!
//! One declaration per action — HTTP, MCP and OpenAPI are projections of the same
//! `#[crate::action]`. The store owns every invariant; this module validates input, maps the
//! store's refusals onto the action error vocabulary, and (where the lifecycle has an event)
//! leaves the event to the store's transaction.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::meetings::{
    AgendaItemRecord, AttendanceRecord, MeetingActor, MeetingDetail, MeetingRecord,
    MeetingTypeRecord, NewAgendaItem, NewMeeting, NewMeetingType, NewPosition, PositionRecord,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{card_error, invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

pub(crate) fn meeting_error(
    error: store::meetings::MeetingStoreError,
    action: &str,
) -> ActionError {
    use store::meetings::MeetingStoreError as E;
    match error {
        E::Database(error) => store_error(error, action),
        E::NotFound(what) => ActionError::resource_not_found(what),
        E::Stale(what) => ActionError::new(412, "stale", format!("the {what} moved; re-read it")),
        E::Conflict { what, why } => {
            ActionError::new(409, "conflict", format!("the {what}: {why}"))
        }
        E::Illegal { what, why } => {
            ActionError::new(409, "meeting_not_permitted", format!("{what}: {why}"))
        }
        E::Unprocessable { code, detail } => ActionError::new(422, code, detail),
        E::Stricter { detail } => ActionError::new(409, "meeting_approval_stricter", detail),
        E::Refused { code, detail } => ActionError::new(409, code, detail),
        E::Card(error) => card_error(error, action),
        E::Outbox(store::outbox::OutboxError::Database(error)) => store_error(error, action),
        E::Outbox(error) => {
            tracing::error!(%error, action, "meeting event");
            ActionError::internal()
        }
    }
}

fn actor(ctx: &HandlerCtx) -> MeetingActor<'static> {
    // `Role::as_str` is a &'static str, so the borrow lives as long as the actor value.
    MeetingActor {
        id: ctx.actor_id,
        role: ctx.role.as_str(),
    }
}

// ── policy input ───────────────────────────────────────────────────────────────────────

fn default_quorum() -> i32 {
    1
}
fn default_rule() -> String {
    "majority".to_owned()
}
fn default_window() -> i64 {
    7 * 24 * 60 * 60
}
fn default_timebox() -> i64 {
    60 * 60
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PolicyIn {
    /// The display name; defaults to the key.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    /// The L0 sources: `approval`, `register_item`, `asset`, `cadence`.
    #[serde(default)]
    pub agenda_sources: Value,
    /// Required participation by department/position with minimum counts.
    #[serde(default)]
    pub required_participation: Value,
    #[serde(default = "default_quorum")]
    pub quorum: i32,
    #[serde(default = "default_rule")]
    pub decision_rule: String,
    #[serde(default)]
    pub evidence_minimum: i32,
    #[serde(default = "default_window")]
    pub decision_window_seconds: i64,
    #[serde(default = "default_timebox")]
    pub timebox_seconds: i64,
    #[serde(default)]
    pub allowed_outcomes: Vec<String>,
    /// Who may convene: `{kinds[], ids[], roles[]}`; empty lists are open.
    #[serde(default)]
    pub convener_authority: Value,
}

impl PolicyIn {
    fn normalised(&self) -> PolicyIn {
        let mut policy = self.clone();
        if policy.agenda_sources.is_null() {
            policy.agenda_sources = json!([]);
        }
        if policy.required_participation.is_null() {
            policy.required_participation = json!([]);
        }
        if policy.convener_authority.is_null() {
            policy.convener_authority = json!({});
        }
        if policy.allowed_outcomes.is_empty() {
            policy.allowed_outcomes = store::meetings::OUTCOMES
                .iter()
                .map(|value| (*value).to_owned())
                .collect();
        }
        policy
    }

    fn validate(&self) -> Result<(), ActionError> {
        if !matches!(
            self.decision_rule.as_str(),
            "unanimous" | "majority" | "chair"
        ) {
            return Err(invalid(
                "decision_rule",
                "invalid_value",
                "decision_rule must be unanimous, majority or chair",
            ));
        }
        if self.quorum < 1 {
            return Err(invalid(
                "quorum",
                "invalid_value",
                "quorum must be at least 1",
            ));
        }
        if self.evidence_minimum < 0 {
            return Err(invalid(
                "evidence_minimum",
                "invalid_value",
                "evidence_minimum cannot be negative",
            ));
        }
        if self.decision_window_seconds <= 0 || self.timebox_seconds <= 0 {
            return Err(invalid(
                "decision_window_seconds",
                "invalid_value",
                "the decision window and timebox must be positive",
            ));
        }
        for outcome in &self.allowed_outcomes {
            if !store::meetings::OUTCOMES.contains(&outcome.as_str()) {
                return Err(invalid(
                    "allowed_outcomes",
                    "invalid_value",
                    format!("{outcome:?} is not an outcome kind"),
                ));
            }
        }
        Ok(())
    }
}

// ── views ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeetingOut {
    pub id: Uuid,
    pub card_id: Uuid,
    pub card_name: String,
    pub meeting_type: String,
    pub meeting_type_name: String,
    pub meeting_type_version: i64,
    pub decision_rule: String,
    pub status: String,
    pub convener_kind: String,
    pub convener_id: Uuid,
    pub cadence_slot: Option<Uuid>,
    pub opens_at: Option<DateTime<Utc>>,
    pub decision_window_end: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub version: i64,
    /// A closed forum with no decision: eligible for retirement (P10).
    pub retirement_candidate: bool,
}

impl From<MeetingRecord> for MeetingOut {
    fn from(record: MeetingRecord) -> Self {
        Self {
            id: record.id,
            card_id: record.card_id,
            card_name: record.card_name,
            meeting_type: record.meeting_type_key,
            meeting_type_name: record.meeting_type_name,
            meeting_type_version: record.meeting_type_version,
            decision_rule: record.decision_rule,
            status: record.status,
            convener_kind: record.convener_kind,
            convener_id: record.convener_id,
            cadence_slot: record.cadence_slot,
            opens_at: record.opens_at,
            decision_window_end: record.decision_window_end,
            closed_at: record.closed_at,
            version: record.version,
            retirement_candidate: record.retirement_candidate,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeetingTypeOut {
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

impl From<MeetingTypeRecord> for MeetingTypeOut {
    fn from(record: MeetingTypeRecord) -> Self {
        Self {
            id: record.id,
            key: record.key,
            name: record.name,
            purpose: record.purpose,
            agenda_sources: record.agenda_sources,
            required_participation: record.required_participation,
            quorum: record.quorum,
            decision_rule: record.decision_rule,
            evidence_minimum: record.evidence_minimum,
            decision_window_seconds: record.decision_window_seconds,
            timebox_seconds: record.timebox_seconds,
            allowed_outcomes: record.allowed_outcomes,
            convener_authority: record.convener_authority,
            status: record.status,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgendaItemOut {
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

impl From<AgendaItemRecord> for AgendaItemOut {
    fn from(record: AgendaItemRecord) -> Self {
        Self {
            id: record.id,
            meeting_id: record.meeting_id,
            sequence: record.sequence,
            resolved_kind: record.resolved_kind,
            resolved_id: record.resolved_id,
            participating_departments: record.participating_departments,
            evidence_refs: record.evidence_refs,
            outcome: record.outcome,
            outcome_ref: record.outcome_ref,
            defer_reason: record.defer_reason,
            closed_at: record.closed_at,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AttendanceOut {
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

impl From<AttendanceRecord> for AttendanceOut {
    fn from(record: AttendanceRecord) -> Self {
        Self {
            id: record.id,
            meeting_id: record.meeting_id,
            participant_kind: record.participant_kind,
            participant_id: record.participant_id,
            seat: record.seat,
            department_id: record.department_id,
            invitation_state: record.invitation_state,
            response_state: record.response_state,
            delegated_from: record.delegated_from,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PositionOut {
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
    pub recorded_at: DateTime<Utc>,
    pub version: i64,
}

impl From<PositionRecord> for PositionOut {
    fn from(record: PositionRecord) -> Self {
        Self {
            id: record.id,
            meeting_id: record.meeting_id,
            agenda_item_id: record.agenda_item_id,
            participant_kind: record.participant_kind,
            participant_id: record.participant_id,
            stance: record.stance,
            rationale: record.rationale,
            conditions: record.conditions,
            commitment: record.commitment,
            surface: record.surface,
            recorded_at: record.recorded_at,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeetingInstrumentsOut {
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeetingDetailOut {
    pub meeting: MeetingOut,
    pub meeting_type: MeetingTypeOut,
    pub agenda: Vec<AgendaItemOut>,
    pub attendance: Vec<AttendanceOut>,
    pub positions: Vec<PositionOut>,
    pub instruments: MeetingInstrumentsOut,
}

impl From<MeetingDetail> for MeetingDetailOut {
    fn from(detail: MeetingDetail) -> Self {
        Self {
            meeting: detail.meeting.into(),
            meeting_type: detail.meeting_type.into(),
            agenda: detail.agenda.into_iter().map(Into::into).collect(),
            attendance: detail.attendance.into_iter().map(Into::into).collect(),
            positions: detail.positions.into_iter().map(Into::into).collect(),
            instruments: MeetingInstrumentsOut {
                decisions: detail.instruments.decisions,
                items: detail.instruments.items,
                open_items: detail.instruments.open_items,
                decision_latency_seconds: detail.instruments.decision_latency_seconds,
                agenda_conversion: detail.instruments.agenda_conversion,
                quorum_miss_rate: detail.instruments.quorum_miss_rate,
                commitment_slippage: detail.instruments.commitment_slippage,
                action_closure: detail.instruments.action_closure,
                participant_hours: detail.instruments.participant_hours,
                person_hours_per_decision: detail.instruments.person_hours_per_decision,
                decisions_per_meeting: detail.instruments.decisions_per_meeting,
            },
        }
    }
}

// ── D1: policy, convening ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleIn {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub list_id: Uuid,
    #[serde(default)]
    pub board_id: Option<Uuid>,
    /// The forum policy key. A key that exists is pinned at its latest version.
    pub meeting_type_key: String,
    /// Required only when the key does not exist yet; a change to an existing policy is the
    /// versioned governance route.
    #[serde(default)]
    pub policy: Option<PolicyIn>,
    pub convener_kind: String,
    pub convener_id: Uuid,
    #[serde(default)]
    pub cadence_slot: Option<Uuid>,
}

#[crate::action(
    name = "meetings.schedule",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_schedule(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ScheduleIn,
) -> Result<HandlerOutput<MeetingOut>, ActionError> {
    if input.title.trim().is_empty() {
        return Err(invalid("title", "invalid_value", "a meeting needs a title"));
    }
    let normalised = input.policy.as_ref().map(PolicyIn::normalised);
    if let Some(policy) = &normalised {
        policy.validate()?;
    }
    let policy_name = normalised
        .as_ref()
        .map(|policy| {
            if policy.name.trim().is_empty() {
                input.meeting_type_key.clone()
            } else {
                policy.name.clone()
            }
        })
        .unwrap_or_default();
    let new_policy = if let Some(policy) = &normalised {
        Some(NewMeetingType {
            key: &input.meeting_type_key,
            name: &policy_name,
            purpose: &policy.purpose,
            agenda_sources: &policy.agenda_sources,
            required_participation: &policy.required_participation,
            quorum: policy.quorum,
            decision_rule: &policy.decision_rule,
            evidence_minimum: policy.evidence_minimum,
            decision_window_seconds: policy.decision_window_seconds,
            timebox_seconds: policy.timebox_seconds,
            allowed_outcomes: &policy.allowed_outcomes,
            convener_authority: &policy.convener_authority,
        })
    } else {
        None
    };
    let record = store::meetings::schedule_tx(
        &mut *tx,
        ctx.tenant,
        actor(&ctx),
        &NewMeeting {
            meeting_type_key: &input.meeting_type_key,
            policy: new_policy.as_ref(),
            title: &input.title,
            description: &input.description,
            list_id: input.list_id,
            board_id: input.board_id,
            convener_kind: &input.convener_kind,
            convener_id: input.convener_id,
            cadence_slot: input.cadence_slot,
        },
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::created(record.into()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OpenIn {
    pub meeting_id: Uuid,
}

#[crate::action(
    name = "meetings.open",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_open(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OpenIn,
) -> Result<HandlerOutput<MeetingOut>, ActionError> {
    let record = store::meetings::open_tx(&mut *tx, ctx.tenant, input.meeting_id, ctx.version)
        .await
        .map_err(|error| meeting_error(error, ctx.action))?;
    let version = record.version;
    Ok(HandlerOutput::ok(record.into()).with_etag(version.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InviteIn {
    pub meeting_id: Uuid,
    pub participant_kind: String,
    pub participant_id: Uuid,
    pub seat: String,
    #[serde(default)]
    pub department_id: Option<Uuid>,
    #[serde(default)]
    pub delegated_from: Option<Uuid>,
}

#[crate::action(
    name = "meetings.invite",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_invite(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: InviteIn,
) -> Result<HandlerOutput<AttendanceOut>, ActionError> {
    let record = store::meetings::invite_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        &store::meetings::NewAttendance {
            participant_kind: &input.participant_kind,
            participant_id: input.participant_id,
            seat: &input.seat,
            department_id: input.department_id,
            delegated_from: input.delegated_from,
        },
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::created(record.into()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RespondIn {
    pub meeting_id: Uuid,
    /// Absent on a human channel: the caller answers for their own seat.
    #[serde(default)]
    pub participant_kind: Option<String>,
    #[serde(default)]
    pub participant_id: Option<Uuid>,
    pub response: String,
}

#[crate::action(
    name = "meetings.respond",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_respond(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RespondIn,
) -> Result<HandlerOutput<AttendanceOut>, ActionError> {
    let participant_kind = input
        .participant_kind
        .unwrap_or_else(|| "app_user".to_owned());
    let participant_id = input.participant_id.unwrap_or(ctx.actor_id);
    let record = store::meetings::respond_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        &participant_kind,
        participant_id,
        &input.response,
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(record.into()))
}

// ── D2: agenda ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgendaAddIn {
    pub meeting_id: Uuid,
    pub resolved_kind: String,
    pub resolved_id: Uuid,
    #[serde(default)]
    pub participating_departments: Vec<Uuid>,
    #[serde(default)]
    pub evidence_refs: Value,
}

#[crate::action(
    name = "meetings.agenda_add",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_agenda_add(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AgendaAddIn,
) -> Result<HandlerOutput<AgendaItemOut>, ActionError> {
    let evidence = if input.evidence_refs.is_null() {
        json!([])
    } else {
        input.evidence_refs
    };
    if !evidence.is_array() {
        return Err(invalid(
            "evidence_refs",
            "invalid_value",
            "evidence_refs must be an array",
        ));
    }
    let record = store::meetings::agenda_add_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        &NewAgendaItem {
            resolved_kind: &input.resolved_kind,
            resolved_id: input.resolved_id,
            participating_departments: &input.participating_departments,
            evidence_refs: &evidence,
        },
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::created(record.into()))
}

// ── D3: positions ──────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PositionIn {
    pub meeting_id: Uuid,
    pub agenda_item_id: Uuid,
    /// Absent on a human channel: the position is the caller's own seat.
    #[serde(default)]
    pub participant_kind: Option<String>,
    #[serde(default)]
    pub participant_id: Option<Uuid>,
    pub stance: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub conditions: Value,
    #[serde(default)]
    pub commitment: Option<Value>,
    #[serde(default = "default_surface")]
    pub surface: String,
}

fn default_surface() -> String {
    "web".to_owned()
}

#[crate::action(
    name = "meetings.position",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_position(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PositionIn,
) -> Result<HandlerOutput<PositionOut>, ActionError> {
    let conditions = if input.conditions.is_null() {
        json!([])
    } else {
        input.conditions
    };
    let participant_kind = input
        .participant_kind
        .unwrap_or_else(|| "app_user".to_owned());
    let participant_id = match input.participant_id {
        Some(id) => id,
        None if participant_kind == "app_user" => ctx.actor_id,
        None => {
            return Err(invalid(
                "participant_id",
                "required",
                "a job-binding seat must name its binding",
            ));
        }
    };
    let record = store::meetings::position_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        &NewPosition {
            agenda_item_id: input.agenda_item_id,
            participant_kind: &participant_kind,
            participant_id,
            stance: &input.stance,
            rationale: &input.rationale,
            conditions: &conditions,
            commitment: input.commitment.as_ref(),
            surface: &input.surface,
        },
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::created(record.into()))
}

// ── D4: the outcome arithmetic ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloseItemIn {
    pub meeting_id: Uuid,
    pub agenda_item_id: Uuid,
    /// For a non-decision item: `commitment | action | information`. A deciding item derives
    /// its outcome from the seats and must leave this absent.
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub commitment: Option<Value>,
    #[serde(default)]
    pub action: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloseItemOut {
    pub item: AgendaItemOut,
    pub approval_status: Option<String>,
    pub decisions_written: usize,
    pub counted_positions: usize,
    pub quorum: usize,
}

#[crate::action(
    name = "meetings.close_item",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_close_item(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CloseItemIn,
) -> Result<HandlerOutput<CloseItemOut>, ActionError> {
    let non_decision = input
        .outcome
        .as_deref()
        .map(|outcome| store::meetings::NonDecision {
            outcome,
            commitment: input.commitment.as_ref(),
            action: input.action.as_ref(),
        });
    let result = store::meetings::close_item_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        input.agenda_item_id,
        non_decision.as_ref(),
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(CloseItemOut {
        item: result.item.into(),
        approval_status: result.approval_status,
        decisions_written: result.decisions_written,
        counted_positions: result.counted_positions,
        quorum: result.quorum,
    }))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeferIn {
    pub meeting_id: Uuid,
    pub agenda_item_id: Uuid,
    pub reason: String,
}

#[crate::action(
    name = "meetings.defer",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_defer(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DeferIn,
) -> Result<HandlerOutput<AgendaItemOut>, ActionError> {
    let item = store::meetings::defer_agenda_item_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        input.agenda_item_id,
        &input.reason,
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(item.into()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CloseMeetingIn {
    pub meeting_id: Uuid,
}

#[crate::action(
    name = "meetings.close",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CloseMeetingIn,
) -> Result<HandlerOutput<MeetingOut>, ActionError> {
    let record = store::meetings::close_meeting_tx(&mut *tx, ctx.tenant, input.meeting_id)
        .await
        .map_err(|error| meeting_error(error, ctx.action))?;
    let version = record.version;
    Ok(HandlerOutput::ok(record.into()).with_etag(version.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MinutesLineIn {
    pub agenda_item_id: Uuid,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MinutesPublishIn {
    pub meeting_id: Uuid,
    /// The chair's approval: approved, and naming this meeting (its card or its meta).
    pub approval_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<MinutesLineIn>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MinutesOut {
    pub asset_id: Uuid,
    pub status: String,
    pub title: String,
}

#[crate::action(
    name = "meetings.minutes_publish",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_minutes_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MinutesPublishIn,
) -> Result<HandlerOutput<MinutesOut>, ActionError> {
    if input.title.trim().is_empty() {
        return Err(invalid("title", "invalid_value", "minutes need a title"));
    }
    let lines: Vec<(Uuid, String)> = input
        .lines
        .iter()
        .map(|line| (line.agenda_item_id, line.text.clone()))
        .collect();
    let asset = store::meetings::minutes_publish_tx(
        &mut *tx,
        ctx.tenant,
        input.meeting_id,
        &store::meetings::NewMinutes {
            approval_id: input.approval_id,
            title: &input.title,
            lines: &lines,
        },
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::created(MinutesOut {
        asset_id: asset,
        status: "approved".into(),
        title: input.title,
    }))
}

// ── reads ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListIn {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[crate::action(
    name = "meetings.list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ListIn,
) -> Result<HandlerOutput<Page<MeetingOut>>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(status, "scheduled" | "live" | "closed" | "cancelled")
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "status must be scheduled, live, closed or cancelled",
        ));
    }
    let rows = store::meetings::list_meetings_tx(
        &mut *tx,
        ctx.tenant,
        input.status.as_deref(),
        input.limit.unwrap_or(50),
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor: None,
    }))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TypeListIn {
    /// Exact-match policy key.
    #[serde(default)]
    pub key: Option<String>,
    /// `draft | active | retired`; every status when omitted.
    #[serde(default)]
    pub status: Option<String>,
    /// Page size; 50 by default, at most 200.
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TypeGetIn {
    /// The policy row.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// The policy key; used when `id` is absent.
    #[serde(default)]
    pub key: Option<String>,
    /// The exact version; the latest non-retired version when omitted.
    #[serde(default)]
    pub version: Option<i64>,
}

/// `meetings.type_list` — the tenant's forum policies, newest version first per key.
#[crate::action(
    name = "meetings.type_list",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_type_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TypeListIn,
) -> Result<HandlerOutput<Page<MeetingTypeOut>>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(status, "draft" | "active" | "retired")
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "status must be draft, active or retired",
        ));
    }
    let rows = store::meetings::list_meeting_types_tx(
        &mut *tx,
        ctx.tenant,
        input.key.as_deref(),
        input.status.as_deref(),
        input.limit.unwrap_or(50),
    )
    .await
    .map_err(|error| meeting_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(Page {
        items: rows.into_iter().map(Into::into).collect(),
        next_cursor: None,
    }))
}

/// `meetings.type_get` — one policy by id, or by key at a version (latest when omitted).
#[crate::action(
    name = "meetings.type_get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_type_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TypeGetIn,
) -> Result<HandlerOutput<MeetingTypeOut>, ActionError> {
    let record = match (input.id, input.key.as_deref()) {
        (Some(id), _) => store::meetings::get_meeting_type_tx(&mut *tx, ctx.tenant, id).await,
        (None, Some(key)) => {
            store::meetings::get_meeting_type_by_key_tx(&mut *tx, ctx.tenant, key, input.version)
                .await
        }
        (None, None) => {
            return Err(invalid(
                "id",
                "required",
                "supply the policy's id or its key",
            ));
        }
    }
    .map_err(|error| meeting_error(error, ctx.action))?
    .ok_or_else(|| ActionError::resource_not_found("meeting type"))?;
    let version = record.version;
    Ok(HandlerOutput::ok(record.into()).with_etag(version.to_string()))
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetIn {
    pub meeting_id: Uuid,
}

#[crate::action(
    name = "meetings.get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn meetings_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: GetIn,
) -> Result<HandlerOutput<MeetingDetailOut>, ActionError> {
    let detail = store::meetings::detail_tx(&mut *tx, ctx.tenant, input.meeting_id)
        .await
        .map_err(|error| meeting_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("meeting"))?;
    let version = detail.meeting.version;
    Ok(HandlerOutput::ok(detail.into()).with_etag(version.to_string()))
}
