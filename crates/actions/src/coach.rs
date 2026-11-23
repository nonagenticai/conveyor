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
//! # The remedy is APPLIED, not only recorded
//!
//! `coach.remedy` writes its one row **after** the class's effect has succeeded, in the same
//! transaction: `retry` starts the failed process again on the interrupted card, `repull`
//! queues the board's `wip.pull`, `move` and `block` move the card through the registered
//! `cards.move` handler, `ask` starts `ask_fr` and parks the human gate, and a systemic class
//! opens `kata.open`. A failed effect rolls the remedy back, so an interruption is never
//! `remedied` by a write alone. The effect's run/job ids travel in the remedy's `detail`
//! (`detail.effect`) and in the `coach.remedied` event.
//!
//! # Policy, not constants
//!
//! The repeat window, the confidence floor and the systemic classes are read from the tenant's
//! `decision_policy` row of kind `coach.window` (decision Q3) — the T2 store, never a second
//! one. An absent row is the safe default, never "no policy": 24 h, 0.7 and
//! `conveyor_defect` systemic.

use chrono::{Duration, Utc};
use conveyor_domain::{ListKind, RunStatus, TenantId};
use conveyor_store as store;
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::board::pull::service_request;
use crate::board::views::CardCreateIn;
use crate::invoke::{ActionError, HandlerCtx, invoke};
use crate::process::views::ProcessRunStartIn;
use crate::registry::registry;
use crate::spec::{HandlerOutput, HandlerRef};

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

/// The seeded human-question route an `ask` remedy starts (`ask_fr`).
pub const ASK_FR_PROCESS: &str = "ask_fr";

/// The gate a systemic class opens when the policy cannot name a kata instrument.
pub const KATA_ESCALATION_GATE: &str = "coach.kata";

/// The platform job that re-pulls a board's queue: the `repull` remedy's trigger.
pub const WIP_PULL_KIND: &str = "wip.pull";

// ── the inputs and outputs ───────────────────────────────────────────────────────────────

/// The §6.2 flow inputs that are not run failures: each carries its own repeat key and no run.
pub const FLOW_TRIGGER_KINDS: [&str; 4] =
    ["wip_breach", "stale_lease", "gate_timeout", "board_event"];

/// `coach.interrupt`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CoachInterruptIn {
    /// The failed run to coach. Absent for the four flow inputs (§6.2).
    #[serde(default)]
    pub run_id: Option<Uuid>,
    /// The card the interruption is about, required when `run_id` is absent.
    #[serde(default)]
    pub card_id: Option<Uuid>,
    /// The flow input that opened it — one of [`FLOW_TRIGGER_KINDS`]. Absent for a failed run.
    #[serde(default)]
    pub trigger_kind: Option<String>,
    /// The producer's own repeat key, required when `run_id` is absent.
    #[serde(default)]
    pub signature: Option<String>,
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
    /// The Definition-of-Done override reason the coach read for the interrupted card, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_reason: Option<String>,
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
    /// The measured kata opened for a systemic class, if any (a systemic class without a
    /// policy-pinned instrument escalates instead and names no kata).
    pub kata_id: Option<String>,
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
    /// The measured kata pin a systemic class opens (SPEC_V2 §6.3). Absent means the policy
    /// cannot name an instrument, and a systemic candidate escalates to a person instead.
    kata: Option<KataPin>,
}

/// The instrument and pre-registered prediction `kata.open` requires.
#[derive(Clone, Debug)]
struct KataPin {
    instrument_kind: String,
    instrument_id: Option<Uuid>,
    instrument_version: String,
    split: String,
    split_sha256: String,
    prediction: Value,
    base_ref: Value,
}

/// Parse the policy's `kata` block. Every field the open contract requires must be present and
/// well-formed; a partial block is treated as no pin, never guessed at.
fn kata_pin_of(meta: &Value) -> Option<KataPin> {
    let block = meta.get("kata")?.as_object()?;
    let text = |key: &str| {
        block
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let instrument_id = block
        .get("instrument_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let prediction = block.get("prediction")?.clone();
    if !prediction.is_object() || prediction.as_object().is_some_and(|map| map.is_empty()) {
        return None;
    }
    Some(KataPin {
        instrument_kind: text("instrument_kind")?,
        instrument_id,
        instrument_version: text("instrument_version")?,
        split: text("split")?,
        split_sha256: text("split_sha256")?,
        prediction,
        base_ref: block.get("base_ref")?.clone(),
    })
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
        kata: kata_pin_of(&meta),
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

/// Start a ROOT process run on the interrupted card, in the remedy's transaction.
/// The project behind an interrupted card: the kata's project (a kata is a project type).
async fn project_of_card_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    card_id: Uuid,
) -> Result<Uuid, ActionError> {
    let card = store::card::get_card_record_tx(&mut *conn, tenant, card_id)
        .await
        .map_err(|error| store_failure("coach: read the interrupted card", error))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *conn, tenant, card.card.board_id)
        .await
        .map_err(|error| store_failure("coach: read the card's board", error))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    Ok(board.project_id)
}

async fn start_process_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card_id: Uuid,
    process_key: &str,
    inputs: Value,
) -> Result<String, ActionError> {
    // A model-calling run declares its spend ceiling before it may start (D45). The coach's
    // machine-owned runs are one bounded turn each; the ceiling is explicit here rather than
    // inherited from a job release the coach does not have.
    let mut inputs = inputs;
    if inputs.get("limits").is_none() {
        inputs["limits"] = json!({ "cost_budget_usd": 1.0 });
    }
    let started = crate::process::runs::process_runs_start(
        &mut *tx,
        ctx.clone(),
        ProcessRunStartIn {
            card_id,
            process_id: None,
            process_key: Some(process_key.to_owned()),
            inputs,
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            job_binding_id: None,
            release_hash: None,
        },
    )
    .await?;
    Ok(started.value.id)
}

/// Move one card through the registered `cards.move` handler, in this transaction.
///
/// The handler is the action's own body: it applies the precondition, the project-type matrix,
/// the position arithmetic and the `card.moved` event exactly as a person's move does. The
/// coach passes the version it just read as the precondition.
async fn move_card_action_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card_id: Uuid,
    list_id: Uuid,
    version: u64,
) -> Result<(), ActionError> {
    let spec = crate::registry::registry()
        .get("cards.move")
        .ok_or_else(ActionError::internal)?;
    let HandlerRef::Internal(handler) = spec.handler else {
        return Err(ActionError::internal());
    };
    let mut move_ctx = ctx.clone();
    move_ctx.version = i64::try_from(version).ok();
    handler(
        &mut *tx,
        move_ctx,
        json!({ "id": card_id, "list_id": list_id, "internal_coach": true }),
    )
    .await
    .map(|_| ())
}

/// Apply one remedy's effect and return the document recorded on the remedy row.
///
/// Every effect is a real mutation through an existing surface — a process run, a job, or the
/// `cards.move` handler — not a marker. `note` is the one class that changes nothing: its
/// contract is to record the diagnosis and not act on a third party's behalf.
async fn apply_effect_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    interruption: &store::coach::CoachInterruption,
    remedy_class: &str,
    triage_class: &str,
    confidence: f64,
) -> Result<Value, ActionError> {
    if remedy_class == "note" {
        return Ok(json!({ "kind": "note" }));
    }
    let card =
        store::card::get_card_record_tx(&mut *tx, ctx.tenant, interruption.interrupted_card_id)
            .await
            .map_err(|error| store_failure("coach.remedy: read the interrupted card", error))?
            .ok_or_else(|| ActionError::resource_not_found("card"))?;
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, card.card.board_id)
        .await
        .map_err(|error| store_failure("coach.remedy: read the board", error))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    match remedy_class {
        // The rerun guard is `process_runs.start`'s own `uq_run_live`: a retry while another
        // execution is live is refused, and the remedy is not written.
        "retry" => {
            // §6.2 flow inputs carry no run; there is nothing to rerun, and inventing one would
            // put a stranger's execution behind the remedy.
            let Some(failed_run_id) = interruption.failed_run_id else {
                return Err(ActionError::new(
                    409,
                    "coach.no_run",
                    "a flow interruption has no failed run to retry",
                ));
            };
            let failed = store::run::get_run_detail_tx(&mut *tx, ctx.tenant, failed_run_id)
                .await
                .map_err(|error| store_failure("coach.remedy: read the failed run", error))?
                .ok_or_else(|| ActionError::resource_not_found("run"))?;
            let process_key =
                store::run::process_key_of_run_tx(&mut *tx, ctx.tenant, failed_run_id)
                    .await
                    .map_err(|error| {
                        store_failure("coach.remedy: read the failed run's process", error)
                    })?
                    .ok_or_else(|| {
                        ActionError::new(
                            409,
                            "coach.no_process",
                            "the failed run has no published process to rerun",
                        )
                    })?;
            let run_id =
                start_process_tx(&mut *tx, ctx, card.card.id, &process_key, failed.inputs).await?;
            Ok(json!({
                "kind": "retry",
                "run_id": run_id,
                "process_key": process_key,
            }))
        }
        "repull" => {
            let job = store::jobs::enqueue_tx(
                &mut *tx,
                ctx.tenant,
                WIP_PULL_KIND,
                &format!("{WIP_PULL_KIND}:{}", board.id),
                json!({ "board_id": board.id }),
                None,
                3,
            )
            .await
            .map_err(|error| store_failure("coach.remedy: queue the re-pull", error))?;
            Ok(json!({
                "kind": "repull",
                "board_id": board.id,
                "job_id": job.map(|id| id.to_string()),
            }))
        }
        "move" => {
            let active =
                store::list::list_of_kind_tx(&mut *tx, ctx.tenant, board.id, ListKind::Active)
                    .await
                    .map_err(|error| store_failure("coach.remedy: read the active lane", error))?
                    .ok_or_else(|| {
                        ActionError::new(
                            409,
                            "coach.no_active_lane",
                            "the board has no active lane to move the card to",
                        )
                    })?;
            move_card_action_tx(
                &mut *tx,
                ctx,
                card.card.id,
                active.list.id,
                card.card.version.get(),
            )
            .await?;
            Ok(json!({
                "kind": "move",
                "card_id": card.card.id,
                "list_id": active.list.id,
            }))
        }
        "block" => {
            let waiting =
                store::list::list_of_kind_tx(&mut *tx, ctx.tenant, board.id, ListKind::Waiting)
                    .await
                    .map_err(|error| store_failure("coach.remedy: read the waiting lane", error))?
                    .ok_or_else(|| {
                        ActionError::new(
                            409,
                            "coach.no_waiting_lane",
                            "the board has no waiting lane to block the card in",
                        )
                    })?;
            move_card_action_tx(
                &mut *tx,
                ctx,
                card.card.id,
                waiting.list.id,
                card.card.version.get(),
            )
            .await?;
            Ok(json!({
                "kind": "block",
                "card_id": card.card.id,
                "list_id": waiting.list.id,
            }))
        }
        "ask" => {
            let run_id = start_process_tx(
                &mut *tx,
                ctx,
                card.card.id,
                ASK_FR_PROCESS,
                json!({ "interruption_id": interruption.id }),
            )
            .await?;
            let approval_id = open_gate_tx(
                &mut *tx,
                ctx,
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
            Ok(json!({
                "kind": "ask",
                "run_id": run_id,
                "approval_id": approval_id,
            }))
        }
        other => {
            // The caller validated the closed set; a value outside it here is a build defect.
            tracing::error!(remedy_class = other, "coach.remedy: unknown remedy class");
            Err(ActionError::internal())
        }
    }
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
        override_reason: None,
        kata_candidate_id: interruption.kata_candidate_id.map(|id| id.to_string()),
    }
}

/// The interruption view with the card's Definition-of-Done override reason, when one is
/// recorded. SPEC_V2 §5.4: the coach reads the reason the approval recorded.
async fn view_with_reason(
    tx: &mut PgConnection,
    tenant: TenantId,
    interruption: &store::coach::CoachInterruption,
) -> Result<InterruptionOut, ActionError> {
    let mut out = view(interruption);
    out.override_reason = store::approval::approved_card_gate_meta_tx(
        &mut *tx,
        tenant,
        interruption.interrupted_card_id,
        crate::board::CARD_DONE_GATE,
    )
    .await
    .map_err(|error| {
        tracing::error!(
            %error,
            interruption = %interruption.id,
            "read the Definition-of-Done gate"
        );
        ActionError::internal()
    })?
    .and_then(|meta| {
        meta.get("evidence_waiver")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    Ok(out)
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
    // §6.2 names four flow inputs that are not run failures. Exactly one cause is named: a
    // failed run, or one flow trigger carrying its own card and its own repeat key.
    let (failed_run_id, flow_trigger) = match (input.run_id, input.trigger_kind.as_deref()) {
        (Some(run_id), None) => (Some(run_id), None),
        (None, Some(kind)) if FLOW_TRIGGER_KINDS.contains(&kind) => (None, Some(kind.to_owned())),
        (None, Some(kind)) => {
            return Err(ActionError::new(
                400,
                "coach.trigger_unknown",
                format!("{kind:?} is not a §6.2 flow input"),
            ));
        }
        (None, None) => {
            return Err(ActionError::new(
                400,
                "coach.no_cause",
                "an interruption names a failed run or a flow trigger",
            ));
        }
        (Some(_), Some(_)) => {
            return Err(ActionError::new(
                400,
                "coach.two_causes",
                "an interruption names a failed run or one flow trigger, never both",
            ));
        }
    };
    let trigger_kind = flow_trigger
        .clone()
        .unwrap_or_else(|| "failed_run".to_owned());

    let (interrupted_card_id, error_class, process_key, failing_step) = match failed_run_id {
        Some(run_id) => {
            let run = store::run::get_run_tx(&mut *tx, ctx.tenant, run_id)
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
            let Some(card_id) = run.card_id else {
                return Err(ActionError::new(
                    409,
                    "coach.no_card",
                    "a project-scoped run has no card to coach",
                ));
            };
            let process_key = store::run::process_key_of_run_tx(&mut *tx, ctx.tenant, run_id)
                .await
                .map_err(|error| {
                    store_failure("coach.interrupt: read the failed run's process", error)
                })?;
            let failing_step = store::coach::failing_step_position_tx(&mut *tx, ctx.tenant, run_id)
                .await
                .map_err(|error| store_failure("coach.interrupt: read the failing step", error))?;
            (
                card_id,
                run.error_class
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
                process_key,
                failing_step,
            )
        }
        None => {
            let card_id = input.card_id.ok_or_else(|| {
                ActionError::new(
                    400,
                    "coach.no_card",
                    "a flow interruption names the card it is about",
                )
            })?;
            (card_id, trigger_kind.clone(), None, None)
        }
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

    // There is deliberately no per-card breadth cap: §6.2 makes every interruption exactly one
    // coach card, and the (card_id, signature) window below is the recursion guard — a repeat
    // inside the window escalates to a person, and a marker never opens a second coach card.
    // A distinct signature is always admitted (v1-p6-4).

    let (signature, signature_parts) = match failed_run_id {
        Some(_) => (
            format!(
                "{error_class}|{}|{}",
                process_key.as_deref().unwrap_or("-"),
                failing_step
                    .map(|position| position.to_string())
                    .unwrap_or_else(|| "-".to_owned())
            ),
            json!({
                "error_class": error_class,
                "process_key": process_key,
                "step_position": failing_step,
            }),
        ),
        None => {
            let supplied = input
                .signature
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if supplied.is_empty() {
                return Err(ActionError::new(
                    400,
                    "coach.no_signature",
                    "a flow interruption carries its own repeat key",
                ));
            }
            (
                supplied.to_owned(),
                json!({ "trigger_kind": trigger_kind.clone(), "supplied": supplied }),
            )
        }
    };

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
            job_binding_id: None,
            meta: Some(json!({ "interrupted_card_id": interrupted_card_id })),
            // The one caller allowed to create a coach card (SPEC_V2 §6.2): the interruption
            // that references it is inserted in the same transaction (v1-p6-6).
            internal_coach_card: true,
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
            failed_run_id,
            trigger_kind: trigger_kind.clone(),
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
            inputs: json!({
                "interruption_id": interruption.id,
                // One triage turn is the coach's whole spend; declare it, never inherit it.
                "limits": { "cost_budget_usd": 1.0 },
            }),
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
            job_binding_id: None,
            release_hash: None,
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
            "failed_run_id": failed_run_id,
            "trigger_kind": trigger_kind,
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

// ── the flow producers (§6.2 inputs that are not a failed run) ─────────────────────────────

/// Open an interruption from a §6.2 flow input, inside the caller's transaction.
///
/// The producer supplies its own repeat key; everything after it is the one admission path, so
/// the `(card_id, signature)` window, the escalation and one-card-per-interruption apply to a
/// flow exactly as they do to a failed run.
pub(crate) async fn open_flow_interruption_tx(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    card_id: Uuid,
    trigger_kind: &str,
    signature: &str,
) -> Result<HandlerOutput<CoachInterruptOut>, ActionError> {
    coach_interrupt(
        tx,
        ctx,
        CoachInterruptIn {
            run_id: None,
            card_id: Some(card_id),
            trigger_kind: Some(trigger_kind.to_owned()),
            signature: Some(signature.to_owned()),
        },
    )
    .await
}

/// Open an interruption from a §6.2 flow input, owning its transaction — the sweepers' shape.
///
/// A deadline sweep does not hold a handler context, so it reaches the same admission path the
/// way `err.card` does: through the registry, as a service actor. `signature` must already carry
/// the producer's own repeat key.
pub async fn open_flow_interruption(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    card_id: Uuid,
    trigger_kind: &str,
    signature: &str,
) -> Result<(), ActionError> {
    let request = service_request(
        app_key,
        "coach.interrupt",
        json!({
            "card_id": card_id,
            "trigger_kind": trigger_kind,
            "signature": signature,
        }),
        format!("{trigger_kind}:{card_id}:{signature}"),
    );
    invoke(pool, registry(), tenant, request).await?;
    Ok(())
}

/// Observe one flow input without endangering the mutation that produced it.
///
/// A move must never fail because the coach's substrate is unconfigured — a tenant that has not
/// published the coach process has nothing to triage with. The savepoint is what makes that
/// safe: a failed statement inside `coach.interrupt` would otherwise poison the caller's
/// transaction, so the observation rolls back to its own mark and the move stands.
async fn observe_flow(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card_id: Uuid,
    trigger_kind: &str,
    signature: &str,
) -> bool {
    if let Err(error) = sqlx::query("SAVEPOINT coach_flow").execute(&mut *tx).await {
        tracing::error!(%error, "coach: open the flow savepoint");
        return false;
    }
    match open_flow_interruption_tx(tx, ctx.clone(), card_id, trigger_kind, signature).await {
        Ok(_) => {
            if let Err(error) = sqlx::query("RELEASE SAVEPOINT coach_flow")
                .execute(&mut *tx)
                .await
            {
                tracing::error!(%error, "coach: release the flow savepoint");
            }
            true
        }
        Err(error) => {
            if let Err(rollback) = sqlx::query("ROLLBACK TO SAVEPOINT coach_flow")
                .execute(&mut *tx)
                .await
            {
                tracing::error!(%rollback, "coach: roll back the flow savepoint");
            }
            tracing::warn!(
                %error,
                trigger_kind,
                %card_id,
                "coach: the flow interruption was not opened"
            );
            false
        }
    }
}

/// The §6.2 flow inputs a card move can raise.
///
/// A move that takes its destination past `wip_limit` is a breach; a card landing in a blocked
/// (`waiting`) lane is a board event. **One move yields at most one interruption** — a breach is
/// the more specific signal, so it pre-empts the blocked-lane event. A move inside one lane is a
/// reorder, not a flow event, and raises neither.
pub(crate) async fn flow_inputs_after_move(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    card_id: Uuid,
    from_kind: ListKind,
    to_kind: ListKind,
    wip_limit: Option<i32>,
    list_id: Uuid,
) {
    if let Some(limit) = wip_limit {
        let live =
            match store::card::count_live_cards_in_list_tx(&mut *tx, ctx.tenant, list_id).await {
                Ok(count) => count,
                Err(error) => {
                    tracing::error!(%error, %list_id, "coach: count the destination lane");
                    return;
                }
            };
        if live > i64::from(limit) {
            observe_flow(
                tx,
                &ctx,
                card_id,
                "wip_breach",
                &format!("wip_breach|list:{list_id}|limit:{limit}"),
            )
            .await;
            return;
        }
    }
    if to_kind == ListKind::Waiting && from_kind != ListKind::Waiting {
        observe_flow(
            tx,
            &ctx,
            card_id,
            "board_event",
            &format!("board_event|card.moved|to:{list_id}"),
        )
        .await;
    }
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
    Ok(HandlerOutput::ok(
        view_with_reason(&mut *tx, ctx.tenant, &interruption).await?,
    ))
}

// ── coach.remedy ─────────────────────────────────────────────────────────────────────────

/// Apply the interruption's one remedy — then record it — and open exactly one kata for a
/// systemic class.
///
/// The remedy is derived from the triage class and the confidence floor when the caller does not
/// name one; the computed step never does. The class's effect runs through a real surface before
/// the row is written (`retry` → `process_runs.start`, `repull` → a `wip.pull` job, `move` /
/// `block` → `cards.move`, `ask` → `ask_fr` plus the human gate). A class the tenant policy marks
/// systemic opens one `kata.open` run and records it as the interruption's `kata_candidate_id`.
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
    // Only an open interruption has a remedy to apply; a closed one is history.
    if interruption.state != "open" {
        return Err(ActionError::new(
            409,
            "coach.not_open",
            "only an open interruption carries a remedy",
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
    let mut detail = input.detail.unwrap_or_else(|| json!({}));
    if !detail.is_object() {
        return Err(invalid(
            "detail",
            "invalid_value",
            "the remedy detail is a JSON object",
        ));
    }

    // The effect is applied FIRST, inside this transaction: a failure rolls the whole remedy
    // back, so the interruption is never marked `remedied` by a row alone.
    let mut effect = apply_effect_tx(
        &mut *tx,
        &ctx,
        &interruption,
        remedy_class,
        &triage_class,
        confidence,
    )
    .await?;

    // P4: a systemic class opens exactly one MEASURED kata; every other class opens none.
    // §6.3: an unmeasurable kata is not opened — the instrument, its held-out split and the
    // pre-registered prediction come from the coach policy, and a policy without a complete
    // pin escalates the candidate to a person instead of starting a documentation run
    // (v1-p6-2: the old path terminated at a process-run id).
    let mut kata_id: Option<String> = None;
    if policy.systemic.iter().any(|class| class == &triage_class) {
        let project_id =
            project_of_card_tx(&mut *tx, ctx.tenant, interruption.interrupted_card_id).await?;
        match policy.kata.clone() {
            Some(pin) => {
                let opened = crate::kata::kata_open(
                    &mut *tx,
                    ctx.clone(),
                    crate::kata::KataOpenIn {
                        project_id,
                        signature: interruption.signature.clone(),
                        instrument_kind: pin.instrument_kind,
                        instrument_id: pin.instrument_id,
                        instrument_version: pin.instrument_version,
                        split: pin.split,
                        split_sha256: pin.split_sha256,
                        prediction: pin.prediction,
                        base_ref: pin.base_ref,
                    },
                )
                .await?
                .value;
                let id = Uuid::parse_str(&opened.id).map_err(|error| {
                    tracing::error!(%error, "coach.remedy: the kata id is not a uuid");
                    ActionError::internal()
                })?;
                store::coach::set_kata_candidate_tx(&mut *tx, ctx.tenant, interruption.id, id)
                    .await
                    .map_err(|error| {
                        store_failure("coach.remedy: record the kata candidate", error)
                    })?;
                effect["kata_id"] = json!(opened.id);
                kata_id = Some(opened.id);
            }
            None => {
                let approval = open_gate_tx(
                    &mut *tx,
                    &ctx,
                    KATA_ESCALATION_GATE,
                    interruption.interrupted_card_id,
                    project_id,
                    json!({
                        "interruption_id": interruption.id,
                        "signature": interruption.signature,
                        "reason": "a systemic class needs a kata; the coach policy names no instrument",
                    }),
                )
                .await?;
                effect["kata_escalation"] = json!(approval);
            }
        }
    }
    if let Some(map) = detail.as_object_mut() {
        map.insert("effect".to_owned(), effect.clone());
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

    emit(
        &mut *tx,
        &ctx,
        "coach.remedied",
        interruption.id,
        json!({
            "interruption_id": interruption.id,
            "remedy_id": remedy.id,
            "remedy_class": remedy.remedy_class,
            "effect": effect,
            "kata_id": kata_id,
        }),
    )
    .await?;

    Ok(HandlerOutput::created(RemedyOut {
        id: remedy.id.to_string(),
        remedy_class: remedy.remedy_class,
        kata_id,
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
    // A closed interruption is settled history: escalating it would reopen a window the store
    // already closed and raise a gate nothing will decide.
    if interruption.state != "open" {
        return Err(ActionError::new(
            409,
            "coach.not_open",
            "only an open interruption escalates",
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
    Ok(HandlerOutput::ok(
        view_with_reason(&mut *tx, ctx.tenant, &escalated).await?,
    ))
}
