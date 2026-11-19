#![forbid(unsafe_code)]
//! The kanban coach — one interruption, one coach card, one triage, at most one remedy
//! (SPEC_V2 §6.2 · §10.4; T4, issue #75).
//!
//! # The shape
//!
//! `err.card` calls [`coach.interrupt`] once per failed run. That action creates the **coach
//! card** — an ordinary card of the catalogue type `coach`, on the interrupted card's board
//! (decision Q1) — records the interruption and starts the coach run on it. The coach run's
//! agent step calls [`coach.triage`] with one class from the closed set and the diagnosis
//! narrative; its computed step calls [`coach.remedy`], which writes the one remedy (and opens
//! exactly one kata when the class is systemic).
//!
//! # What this module refuses, and why
//!
//! * A triage or remedy outside the closed sets is a `422` **before** any write (§6.2: the sets
//!   are the contract, not a suggestion).
//! * A repeat signature inside the window never opens a second card: [`coach.interrupt`] closes
//!   the interruption `escalated` and raises a human approval instead (P2).
//! * A coach card's own failure never opens another coach card (P3) — refused here and at the
//!   enqueue boundary (`store::run::enqueue_run_failed_trigger_tx`).
//! * The remedy is never the model's choice: the coach agent's `allowed_actions` carries
//!   `coach.triage` and not `coach.remedy`, and the computed step supplies the class.
//!
//! # Policy, not constants
//!
//! The repeat window, the confidence floor and the systemic classes are read from the tenant's
//! `decision_policy` row of kind `coach.window` (decision Q3) — the T2 store, never a second
//! one. An absent row is the safe default, never "no policy": 24 h, 0.7 and
//! `conveyor_defect` systemic.

use chrono::{Duration, Utc};
use conveyor_domain::RunStatus;
use conveyor_store as store;
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::views::CardCreateIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::process::views::ProcessRunStartIn;
use crate::spec::HandlerOutput;

/// The decision-policy kind the coach reads its window, floor and systemic classes from.
pub const COACH_POLICY_KIND: &str = "coach.window";

/// The default repeat window when the tenant has no policy row.
pub const DEFAULT_WINDOW_HOURS: i64 = 24;

/// The default confidence floor: below it the coach records `ask` rather than a remedy.
pub const DEFAULT_CONFIDENCE_FLOOR: f64 = 0.7;

/// The default systemic class when the tenant has no policy row: a conveyor defect is the
/// conveyor's own, so the evidence says systemic.
pub const DEFAULT_SYSTEMIC_CLASS: &str = "conveyor_defect";

/// The escalation gate's name.
pub const ESCALATION_GATE: &str = "coach.escalation";

/// The low-confidence human gate's name.
pub const ASK_GATE: &str = "coach.ask";

// ── the inputs and outputs ───────────────────────────────────────────────────────────────

/// `coach.interrupt`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CoachInterruptIn {
    /// The failed run to coach.
    pub run_id: Uuid,
}

/// What one interruption did.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct CoachInterruptOut {
    /// The interruption row.
    pub interruption_id: String,
    /// The coach card carrying it (`None` on a repeat: no second card is created).
    pub coach_card_id: Option<String>,
    /// The coach run started on the coach card (`None` on a repeat).
    pub coach_run_id: Option<String>,
    /// True when this call created the interruption.
    pub created: bool,
    /// True when this call escalated a repeat instead of creating one.
    pub escalated: bool,
    /// The escalation approval, when one was raised.
    pub approval_id: Option<String>,
}

/// `coach.triage`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CoachTriageIn {
    /// The interruption being triaged.
    pub interruption_id: Uuid,
    /// One of `transient | conveyor_defect | job_defect | project_decision | external`.
    pub triage_class: String,
    /// The triage's confidence, 0..=1.
    pub confidence: f64,
    /// The diagnosis narrative. The model writes this; it never writes the remedy.
    pub narrative: String,
}

/// `coach.remedy`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CoachRemedyIn {
    /// The interruption to remedy.
    pub interruption_id: Uuid,
    /// The remedy class. Omitted by the computed step, which lets the action derive it from the
    /// triage class and the confidence floor; a supplied value must be in the closed set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remedy_class: Option<String>,
    /// The remedy's own detail document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

/// One interruption, as the coach actions return it.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct InterruptionOut {
    /// The row id.
    pub id: String,
    /// The coach card.
    pub coach_card_id: String,
    /// The card whose run failed.
    pub interrupted_card_id: String,
    /// The repeated fault's signature.
    pub signature: String,
    /// The closed-set triage, once triaged.
    pub triage_class: Option<String>,
    /// The triage's confidence.
    pub confidence: Option<f64>,
    /// The diagnosis narrative.
    pub narrative: Option<String>,
    /// `open | remedied | escalated | closed`.
    pub state: String,
    /// The escalation or ask approval.
    pub approval_id: Option<String>,
    /// The kata run a systemic class opened.
    pub kata_candidate_id: Option<String>,
}

/// The remedy that was written.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct RemedyOut {
    /// The remedy row.
    pub id: String,
    /// The class recorded.
    pub remedy_class: String,
    /// The kata run opened for a systemic class, if any.
    pub kata_run_id: Option<String>,
}

/// `coach.escalate`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CoachEscalateIn {
    /// The interruption to escalate.
    pub interruption_id: Uuid,
    /// Why a human is needed. Recorded on the approval.
    pub reason: String,
}

// ── helpers ──────────────────────────────────────────────────────────────────────────────

fn store_failure(what: &str, error: impl std::fmt::Display) -> ActionError {
    tracing::error!(%error, what, "coach: store call failed");
    ActionError::internal()
}

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::new(422, "action-invalid-input", format!("{field}: {code}")).with_errors(vec![
        crate::spec::FieldError {
            field: field.to_owned(),
            code: code.to_owned(),
            detail: detail.into(),
        },
    ])
}

/// The coach's policy in force: window, confidence floor and systemic classes.
#[derive(Clone, Debug)]
struct CoachPolicy {
    window: Duration,
    floor: f64,
    systemic: Vec<String>,
}

async fn policy_tx(conn: &mut PgConnection, tenant: conveyor_domain::TenantId) -> CoachPolicy {
    let policy = store::ladder::effective_policy_tx(&mut *conn, tenant, COACH_POLICY_KIND, None)
        .await
        .ok()
        .flatten();
    let floor = policy
        .as_ref()
        .and_then(|policy| policy.threshold())
        .unwrap_or(DEFAULT_CONFIDENCE_FLOOR);
    let meta = policy
        .as_ref()
        .map(|policy| policy.policy.meta.clone())
        .unwrap_or_else(|| json!({}));
    let hours = meta
        .get("window_hours")
        .and_then(Value::as_i64)
        .filter(|hours| *hours > 0)
        .unwrap_or(DEFAULT_WINDOW_HOURS);
    let systemic: Vec<String> = meta
        .get("systemic_classes")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_else(|| vec![DEFAULT_SYSTEMIC_CLASS.to_owned()]);
    CoachPolicy {
        window: Duration::hours(hours),
        floor,
        systemic,
    }
}

/// The remedy a triage class implies, before the confidence floor is applied.
fn remedy_for(triage_class: &str) -> &'static str {
    match triage_class {
        "transient" => "retry",
        "conveyor_defect" => "block",
        "job_defect" => "repull",
        "project_decision" => "ask",
        // `external` and anything the policy admits later: note it, do not act on a third
        // party's behalf.
        _ => "note",
    }
}

/// A human gate for the interruption: the escalation and the low-confidence ask share one shape.
async fn open_gate_tx(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    gate: &str,
    card_id: Uuid,
    project_id: Uuid,
    meta: Value,
) -> Result<Uuid, ActionError> {
    let mut approval = store::approval::NewApproval::new(gate, "human_approval");
    approval.card_id = Some(card_id);
    approval.project_id = Some(project_id);
    approval.requested_by = if ctx.actor_kind == "user" {
        Some(ctx.actor_id)
    } else {
        None
    };
    approval.meta = meta;
    store::approval::insert_approval_tx(&mut *conn, ctx.tenant, &approval)
        .await
        .map(|approval| approval.id)
        .map_err(|error| store_failure("coach: open the human gate", error))
}

async fn emit(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    interruption_id: Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    store::outbox::append(
        &mut *conn,
        ctx.tenant,
        &store::NewEvent {
            topic: format!("coach_interruption:{interruption_id}"),
            event_type: event_type.to_owned(),
            aggregate_type: "coach_interruption".to_owned(),
            aggregate_id: interruption_id,
            payload,
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| store_failure("coach: append the event", error))
}

fn view(interruption: &store::coach::CoachInterruption) -> InterruptionOut {
    InterruptionOut {
        id: interruption.id.to_string(),
        coach_card_id: interruption.coach_card_id.to_string(),
        interrupted_card_id: interruption.interrupted_card_id.to_string(),
        signature: interruption.signature.clone(),
        triage_class: interruption.triage_class.clone(),
        confidence: interruption
            .confidence
            .map(|value| value.to_string().parse().unwrap_or(0.0)),
        narrative: interruption.narrative.clone(),
        state: interruption.state.clone(),
        approval_id: interruption.approval_id.map(|id| id.to_string()),
        kata_candidate_id: interruption.kata_candidate_id.map(|id| id.to_string()),
    }
}

// ── coach.interrupt ──────────────────────────────────────────────────────────────────────

/// Admit one interruption: one coach card, one interruption row, one coach run.
///
/// A repeat signature inside the window **escalates** instead: no second card, no second run.
/// The check runs inside this transaction, before any card write, so a redelivery reaches the
/// same answer.
#[crate::action(
    name = "coach.interrupt",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn coach_interrupt(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CoachInterruptIn,
) -> Result<HandlerOutput<CoachInterruptOut>, ActionError> {
    let run = store::run::get_run_tx(&mut *tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the run", error))?
        .ok_or_else(|| ActionError::resource_not_found("run"))?;
    if run.status != RunStatus::Failed {
        return Err(ActionError::new(
            409,
            "coach.run_not_failed",
            "only a failed run has an interruption to coach",
        ));
    }
    let Some(interrupted_card_id) = run.card_id else {
        return Err(ActionError::new(
            409,
            "coach.no_card",
            "a project-scoped run has no card to coach",
        ));
    };
    // P3: a coach card is terminal for the interruption path. The enqueue boundary refuses
    // this too; the handler refuses again so an already-queued job cannot open a second card.
    if store::coach::is_coach_card_tx(&mut *tx, ctx.tenant, interrupted_card_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the card type", error))?
    {
        return Err(ActionError::new(
            409,
            "coach.about_a_coach",
            "a coach card's failure never opens another coach card",
        ));
    }
    let card = store::card::get_card_record_tx(&mut *tx, ctx.tenant, interrupted_card_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the card", error))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.card.board_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the board", error))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    // The per-card depth cap: three interruptions without convergence is the whole answer.
    let prior =
        store::coach::count_interruptions_for_card_tx(&mut *tx, ctx.tenant, interrupted_card_id)
            .await
            .map_err(|error| {
                store_failure("coach.interrupt: count the card's interruptions", error)
            })?;
    if prior >= crate::board::pull::COACH_CHAIN_MAX {
        return Err(ActionError::new(
            409,
            "coach.chain_capped",
            "the card has already had three interruptions; a fourth would only repeat them",
        ));
    }

    let process_key = store::run::process_key_of_run_tx(&mut *tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the failed run's process", error))?;
    let failing_step = store::coach::failing_step_position_tx(&mut *tx, ctx.tenant, input.run_id)
        .await
        .map_err(|error| store_failure("coach.interrupt: read the failing step", error))?;
    let error_class = run
        .error_class
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    let signature = format!(
        "{error_class}|{}|{}",
        process_key.as_deref().unwrap_or("-"),
        failing_step
            .map(|position| position.to_string())
            .unwrap_or_else(|| "-".to_owned())
    );
    let signature_parts = json!({
        "error_class": error_class,
        "process_key": process_key,
        "step_position": failing_step,
    });

    let policy = policy_tx(&mut *tx, ctx.tenant).await;
    let now = Utc::now();
    // A window that has passed closes first: the partial unique index is immutable, so this is
    // what makes a post-window repeat a new interruption instead of a second card.
    store::coach::close_expired_tx(&mut *tx, ctx.tenant, interrupted_card_id, &signature, now)
        .await
        .map_err(|error| store_failure("coach.interrupt: close the expired window", error))?;

    if let Some(open) =
        store::coach::open_interruption_tx(&mut *tx, ctx.tenant, interrupted_card_id, &signature)
            .await
            .map_err(|error| store_failure("coach.interrupt: read the open interruption", error))?
    {
        // P2: a repeat inside the window escalates. An already-escalated signature is not
        // escalated twice and never gets a remedy.
        if open.state == "escalated" {
            return Ok(HandlerOutput::ok(CoachInterruptOut {
                interruption_id: open.id.to_string(),
                coach_card_id: Some(open.coach_card_id.to_string()),
                coach_run_id: None,
                created: false,
                escalated: true,
                approval_id: open.approval_id.map(|id| id.to_string()),
            }));
        }
        let approval_id = open_gate_tx(
            &mut *tx,
            &ctx,
            ESCALATION_GATE,
            interrupted_card_id,
            board.project_id,
            json!({
                "interruption_id": open.id,
                "signature": signature,
                "reason": "the signature repeated inside its window",
            }),
        )
        .await?;
        let escalated =
            store::coach::mark_escalated_tx(&mut *tx, ctx.tenant, open.id, Some(approval_id))
                .await
                .map_err(|error| store_failure("coach.interrupt: escalate", error))?;
        emit(
            &mut *tx,
            &ctx,
            "coach.escalated",
            escalated.id,
            json!({
                "interruption_id": escalated.id,
                "approval_id": approval_id,
                "signature": signature,
            }),
        )
        .await?;
        return Ok(HandlerOutput::ok(CoachInterruptOut {
            interruption_id: escalated.id.to_string(),
            coach_card_id: Some(escalated.coach_card_id.to_string()),
            coach_run_id: None,
            created: false,
            escalated: true,
            approval_id: Some(approval_id.to_string()),
        }));
    }

    // The coach card: an ordinary card of type `coach`, in the interrupted card's lane, on its
    // board (decision Q1). Created through `cards.create` so the board's own rules apply.
    let coach_card = crate::board::cards::cards_create(
        &mut *tx,
        ctx.clone(),
        CardCreateIn {
            board_id: card.card.board_id,
            list_id: card.card.list_id,
            name: format!("Coach: {}", card.name),
            description: None,
            card_type_code: store::coach::COACH_CARD_TYPE.to_owned(),
            due_at: None,
            parent_card_id: None,
            meta: Some(json!({ "interrupted_card_id": interrupted_card_id })),
        },
    )
    .await?
    .value;
    let coach_card_id = Uuid::parse_str(&coach_card.id).map_err(|error| {
        tracing::error!(%error, "coach.interrupt: the created card id is not a uuid");
        ActionError::internal()
    })?;

    let interruption = store::coach::insert_interruption_tx(
        &mut *tx,
        ctx.tenant,
        &store::coach::NewInterruption {
            coach_card_id,
            interrupted_card_id,
            failed_run_id: input.run_id,
            signature,
            signature_parts,
            window_ends_at: now + policy.window,
        },
    )
    .await
    .map_err(|error| store_failure("coach.interrupt: insert the interruption", error))?;

    // The coach run, on the coach card. Its agent step triages; its computed step remedies.
    let started = crate::process::runs::process_runs_start(
        &mut *tx,
        ctx.clone(),
        ProcessRunStartIn {
            card_id: coach_card_id,
            process_id: None,
            process_key: Some(crate::board::pull::COACH_ERROR_PROCESS.to_owned()),
            inputs: json!({ "interruption_id": interruption.id }),
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
        },
    )
    .await?;
    let coach_run_id = Some(started.value.id.clone());

    emit(
        &mut *tx,
        &ctx,
        "coach.interrupted",
        interruption.id,
        json!({
            "interruption_id": interruption.id,
            "coach_card_id": coach_card_id,
            "interrupted_card_id": interrupted_card_id,
            "failed_run_id": input.run_id,
            "signature": interruption.signature,
        }),
    )
    .await?;

    Ok(HandlerOutput::created(CoachInterruptOut {
        interruption_id: interruption.id.to_string(),
        coach_card_id: Some(coach_card_id.to_string()),
        coach_run_id,
        created: true,
        escalated: false,
        approval_id: None,
    }))
}

// ── coach.triage ─────────────────────────────────────────────────────────────────────────

/// Record the closed-set triage and the model's diagnosis narrative.
#[crate::action(
    name = "coach.triage",
    version = 1,
    kind = "internal",
    scopes = ["runs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn coach_triage(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CoachTriageIn,
) -> Result<HandlerOutput<InterruptionOut>, ActionError> {
    let triage_class = input.triage_class.trim();
    if !store::coach::TRIAGE_CLASSES.contains(&triage_class) {
        return Err(invalid(
            "triage_class",
            "invalid_value",
            format!(
                "{triage_class:?} is not one of {}",
                store::coach::TRIAGE_CLASSES.join(", ")
            ),
        ));
    }
    if !(0.0..=1.0).contains(&input.confidence) {
        return Err(invalid(
            "confidence",
            "out_of_range",
            "confidence is a number between 0 and 1",
        ));
    }
    let narrative = input.narrative.trim();
    if narrative.is_empty() {
        return Err(invalid(
            "narrative",
            "required",
            "the triage owes the human a diagnosis",
        ));
    }
    let confidence = Decimal::try_from(input.confidence)
        .map_err(|error| store_failure("coach.triage: convert the confidence", error))?;
    let interruption = store::coach::record_triage_tx(
        &mut *tx,
        ctx.tenant,
        input.interruption_id,
        triage_class,
        confidence,
        narrative,
    )
    .await
    .map_err(|error| match error {
        store::coach::CoachStoreError::AlreadySet { .. } => ActionError::new(
            409,
            "coach.already_triaged",
            "the interruption already carries its one triage",
        ),
        store::coach::CoachStoreError::NotFound(_) => {
            ActionError::resource_not_found("coach interruption")
        }
        other => store_failure("coach.triage: record", other),
    })?;
    emit(
        &mut *tx,
        &ctx,
        "coach.triaged",
        interruption.id,
        json!({
            "interruption_id": interruption.id,
            "triage_class": interruption.triage_class,
            "confidence": input.confidence,
        }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&interruption)))
}

// ── coach.remedy ─────────────────────────────────────────────────────────────────────────

/// Write the interruption's one remedy — and open exactly one kata for a systemic class.
///
/// The remedy is derived from the triage class and the confidence floor when the caller does not
/// name one; the computed step never does. A class the tenant policy marks systemic opens one
/// kata run and records it as the interruption's `kata_candidate_id`.
#[crate::action(
    name = "coach.remedy",
    version = 1,
    kind = "internal",
    scopes = ["runs:write", "processes:write", "approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn coach_remedy(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CoachRemedyIn,
) -> Result<HandlerOutput<RemedyOut>, ActionError> {
    let interruption =
        store::coach::get_interruption_tx(&mut *tx, ctx.tenant, input.interruption_id)
            .await
            .map_err(|error| store_failure("coach.remedy: read the interruption", error))?
            .ok_or_else(|| ActionError::resource_not_found("coach interruption"))?;
    let Some(triage_class) = interruption.triage_class.clone() else {
        return Err(ActionError::new(
            409,
            "coach.not_triaged",
            "a remedy follows a triage; none has been recorded",
        ));
    };
    // P2.2: an escalated signature records no further remedy.
    if interruption.state == "escalated" {
        return Err(ActionError::new(
            409,
            "coach.escalated",
            "an escalated interruption stays blocked for its human",
        ));
    }
    if store::coach::get_remedy_tx(&mut *tx, ctx.tenant, interruption.id)
        .await
        .map_err(|error| store_failure("coach.remedy: read the remedy", error))?
        .is_some()
    {
        return Err(ActionError::new(
            409,
            "coach.already_remedied",
            "the interruption already carries its one remedy",
        ));
    }

    let policy = policy_tx(&mut *tx, ctx.tenant).await;
    let confidence = interruption
        .confidence
        .map(|value| value.to_string().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);
    // P1.3: below the floor the coach asks a human, never retries.
    let derived = if confidence < policy.floor {
        "ask"
    } else {
        remedy_for(&triage_class)
    };
    let remedy_class = input
        .remedy_class
        .as_deref()
        .map(str::trim)
        .unwrap_or(derived);
    if !store::coach::REMEDY_CLASSES.contains(&remedy_class) {
        return Err(invalid(
            "remedy_class",
            "invalid_value",
            format!(
                "{remedy_class:?} is not one of {}",
                store::coach::REMEDY_CLASSES.join(", ")
            ),
        ));
    }
    if confidence < policy.floor && remedy_class != "ask" {
        return Err(ActionError::new(
            409,
            "coach.low_confidence",
            "a triage below the confidence floor routes to ask, never to an action",
        ));
    }
    let detail = input.detail.unwrap_or_else(|| json!({}));
    if !detail.is_object() {
        return Err(invalid(
            "detail",
            "invalid_value",
            "the remedy detail is a JSON object",
        ));
    }

    let remedy = store::coach::insert_remedy_tx(
        &mut *tx,
        ctx.tenant,
        interruption.id,
        remedy_class,
        &detail,
    )
    .await
    .map_err(|error| store_failure("coach.remedy: insert", error))?
    .ok_or_else(|| {
        ActionError::new(
            409,
            "coach.already_remedied",
            "the interruption already carries its one remedy",
        )
    })?;
    store::coach::mark_remedied_tx(&mut *tx, ctx.tenant, interruption.id)
        .await
        .map_err(|error| store_failure("coach.remedy: mark remedied", error))?;

    // P1.3: the ask parks a human gate on the coach card.
    if remedy_class == "ask" {
        let card =
            store::card::get_card_record_tx(&mut *tx, ctx.tenant, interruption.interrupted_card_id)
                .await
                .map_err(|error| store_failure("coach.remedy: read the interrupted card", error))?
                .ok_or_else(|| ActionError::resource_not_found("card"))?;
        let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.card.board_id)
            .await
            .map_err(|error| store_failure("coach.remedy: read the board", error))?
            .ok_or_else(|| ActionError::resource_not_found("board"))?;
        let approval_id = open_gate_tx(
            &mut *tx,
            &ctx,
            ASK_GATE,
            interruption.interrupted_card_id,
            board.project_id,
            json!({
                "interruption_id": interruption.id,
                "triage_class": triage_class,
                "confidence": confidence,
            }),
        )
        .await?;
        store::coach::set_approval_tx(&mut *tx, ctx.tenant, interruption.id, approval_id)
            .await
            .map_err(|error| store_failure("coach.remedy: record the ask gate", error))?;
    }

    // P4: a systemic class opens exactly one kata; every other class opens none.
    let mut kata_run_id = None;
    if policy.systemic.iter().any(|class| class == &triage_class) {
        let started = crate::process::runs::process_runs_start(
            &mut *tx,
            ctx.clone(),
            ProcessRunStartIn {
                card_id: interruption.interrupted_card_id,
                process_id: None,
                process_key: Some("kata.open".to_owned()),
                inputs: json!({ "interruption_id": interruption.id }),
                parent_run_id: None,
                step_id: None,
                workflow_step_id: None,
            },
        )
        .await?;
        let started = started.value;
        kata_run_id = Some(started.id.clone());
        if let Ok(run_id) = Uuid::parse_str(&started.id) {
            store::coach::set_kata_candidate_tx(&mut *tx, ctx.tenant, interruption.id, run_id)
                .await
                .map_err(|error| store_failure("coach.remedy: record the kata candidate", error))?;
        }
    }

    emit(
        &mut *tx,
        &ctx,
        "coach.remedied",
        interruption.id,
        json!({
            "interruption_id": interruption.id,
            "remedy_id": remedy.id,
            "remedy_class": remedy.remedy_class,
            "kata_run_id": kata_run_id,
        }),
    )
    .await?;

    Ok(HandlerOutput::created(RemedyOut {
        id: remedy.id.to_string(),
        remedy_class: remedy.remedy_class,
        kata_run_id,
    }))
}

// ── coach.escalate ───────────────────────────────────────────────────────────────────────

/// Escalate an interruption to a human explicitly (the repeat path escalates automatically).
#[crate::action(
    name = "coach.escalate",
    version = 1,
    kind = "internal",
    scopes = ["approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn coach_escalate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CoachEscalateIn,
) -> Result<HandlerOutput<InterruptionOut>, ActionError> {
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(invalid(
            "reason",
            "required",
            "an escalation names its reason",
        ));
    }
    let interruption =
        store::coach::get_interruption_tx(&mut *tx, ctx.tenant, input.interruption_id)
            .await
            .map_err(|error| store_failure("coach.escalate: read the interruption", error))?
            .ok_or_else(|| ActionError::resource_not_found("coach interruption"))?;
    if interruption.state == "escalated" {
        return Err(ActionError::new(
            409,
            "coach.already_escalated",
            "the interruption already waits on a human",
        ));
    }
    let card =
        store::card::get_card_record_tx(&mut *tx, ctx.tenant, interruption.interrupted_card_id)
            .await
            .map_err(|error| store_failure("coach.escalate: read the card", error))?
            .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.card.board_id)
        .await
        .map_err(|error| store_failure("coach.escalate: read the board", error))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    let approval_id = open_gate_tx(
        &mut *tx,
        &ctx,
        ESCALATION_GATE,
        interruption.interrupted_card_id,
        board.project_id,
        json!({ "interruption_id": interruption.id, "reason": reason }),
    )
    .await?;
    let escalated =
        store::coach::mark_escalated_tx(&mut *tx, ctx.tenant, interruption.id, Some(approval_id))
            .await
            .map_err(|error| store_failure("coach.escalate: mark", error))?;
    emit(
        &mut *tx,
        &ctx,
        "coach.escalated",
        escalated.id,
        json!({ "interruption_id": escalated.id, "approval_id": approval_id, "reason": reason }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&escalated)))
}
