#![forbid(unsafe_code)]
//! Approval actions: `approvals.decide`, `approvals.list` (S06 §5.2).
//!
//! A decision is one transaction under the gate's row lock
//! (`store::approval::record_decision_tx`); the gate is opened by a run moving to
//! `waiting_approval` (`store::approval::open_gate_tx`) and voided by `runs.cancel`
//! (`store::approval::void_pending_for_run_tx`). Dual control is `required_decisions`, and the
//! `(approval_id, approver_id)` unique index is what stops one approver satisfying it twice.
//!
//! **The decider kind is derived, never client-supplied** (S06 decision 8): `ctx.actor_kind`
//! maps `user → human`, `agent → agent` and `service → rule`, and the transport is recorded in
//! `via`. An HTTP caller therefore cannot claim to be an agent (the HTTP route hardcodes
//! `actor_kind = "user"`).
//!
//! A terminal event is emitted only when the gate reached a terminal state: the first approve
//! under `required_decisions = 2` leaves the gate `pending` and announces nothing, because the
//! gate has not been resolved and a watcher told otherwise would act on a false resolution.

use std::str::FromStr;

use conveyor_domain::{
    Approval, ApprovalError, ApprovalStatus, DeciderKind, Decision, DecisionKind, RunStatus,
    TenantId,
};
use conveyor_store as store;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{self, RawEvent};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::Page;

use super::views::{
    ApprovalDecideIn, ApprovalDecisionOut, ApprovalListIn, ApprovalOut, ApprovalRequestIn,
};
use super::{approval_error, invalid};

/// `approval.approved` (S06 §5.6) — the gate reached its quorum.
pub const APPROVAL_APPROVED: &str = "approval.approved";
/// `approval.rejected` (S06 §5.6) — one reject resolves the gate.
pub const APPROVAL_REJECTED: &str = "approval.rejected";

/// Record one approver's decision.
///
/// The gate is decided by a quorum count, so this is status-guarded and takes no `If-Match`
/// (CONTRACT §3). The second decision by the same approver is a `409` from the unique index,
/// never a silent second count.
#[crate::action(
    name = "approvals.decide",
    version = 1,
    kind = "internal",
    scopes = ["approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approvals_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApprovalDecideIn,
) -> Result<ApprovalOut, ActionError> {
    let kind = parse_decision(&input.decision)?;
    // The transport is DERIVED from the authenticated authority, never from the input
    // (S06 decision 8, extended by S06d). `ctx.channel` is `Some` only for a machine connector;
    // a human session records `web`, and the engine records `internal`. The sender reference is
    // audit evidence (which chat member pressed approve), never an authorization input.
    let via = match &ctx.channel {
        Some(channel) => {
            let mut via = serde_json::json!({ "channel": channel.kind() });
            if let Some(sender) = channel.sender_ref() {
                via["sender_ref"] = serde_json::Value::String(sender.to_owned());
            }
            via
        }
        None => serde_json::json!({
            "channel": if ctx.actor_kind == "user" { "web" } else { "internal" }
        }),
    };
    let decision = Decision {
        approver_id: ctx.actor_id,
        kind,
        note: input.note.unwrap_or_default(),
        decided_by_kind: decider_kind(&ctx.actor_kind),
        via: Some(via),
        item_decisions: input.item_decisions,
    };

    // The eval gate's second door (W1): a human may decide it only with `evals:approve`.
    // The store writer enforces this for every path, including this one.
    let eval_authorized = ctx.channel.is_none()
        && ctx.actor_kind == "user"
        && ctx.holds_scope(crate::spec::Scope::EvalsApprove);
    let approval = store::approval::record_decision_tx(
        &mut *tx,
        ctx.tenant,
        input.approval_id,
        &decision,
        eval_authorized,
    )
    .await
    .map_err(|error| decide_error(error, "approvals.decide"))?;

    // T3: an approved PM gate moves the row it names — the charter, the baseline or the change
    // request — and leaves a trace edge from each typed evidence item to the project. The
    // effect rides the decision's transaction, so a failed effect rolls the decision back.
    if approval.status == ApprovalStatus::Approved {
        let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval.id)
            .await
            .map_err(|error| decide_error(error, "approvals.decide"))?
            .ok_or_else(ActionError::internal)?;
        crate::pm::apply_gate_effect_tx(&mut *tx, ctx.tenant, &record, ctx.actor_id).await?;
    }

    // Only a gate that actually resolved announces it: a first approve under a two-person
    // quorum leaves the gate pending, and a "pending" gate has no terminal event.
    match approval.status {
        ApprovalStatus::Approved => {
            emit_terminal(&mut *tx, &ctx, &approval, APPROVAL_APPROVED).await?
        }
        ApprovalStatus::Rejected => {
            emit_terminal(&mut *tx, &ctx, &approval, APPROVAL_REJECTED).await?
        }
        _ => {}
    }

    // Closing the gate resolves the run in the same transaction: the gate was the run's only
    // blocker. An approval resumes it; a rejection cancels it, because the domain's run
    // machine has no `waiting_approval → failed` edge (data-arch §3.2) and leaving the run
    // live would hold `uq_run_live` for the card forever. A run a concurrent caller already
    // cancelled has nothing to resume — the gate still stands on its own record.
    if let Some(run_id) = approval.run_id {
        if approval.kind == "process" {
            // S11: a process gate resolves to the gated STEP's resume, not the generic
            // settle/cancel below. `resolve_gate` moves the run itself and, on a rejection,
            // applies the step's `on_result` (which may continue the run). A pending quorum is a
            // no-op; the same call is made by the `approval.expire` worker for `expired`.
            crate::process::step::resolve_gate(&mut *tx, ctx.tenant, approval.id)
                .await
                .map_err(|failure| {
                    tracing::error!(
                        approval_id = %approval.id,
                        error = %failure.message,
                        retryable = failure.retryable,
                        "resolve a process gate"
                    );
                    ActionError::internal()
                })?;
        } else if approval.kind == "workflow" {
            // S13: an approved workflow gate resumes the run and re-arms its tick — the next tick
            // replays the decision into the trace. A rejection is terminal, so the gate's row is
            // recorded here (the run is settled by the same branch `runs.cancel` uses).
            match approval.status {
                ApprovalStatus::Approved => {
                    settle_run(&mut *tx, ctx.tenant, run_id, RunStatus::Running).await?;
                    store::workflow::run::enqueue_workflow_node_tx(&mut *tx, ctx.tenant, run_id, 0)
                        .await
                        .map_err(|error| {
                            tracing::error!(%error, approval_id = %approval.id, "enqueue the workflow tick");
                            ActionError::internal()
                        })?;
                }
                ApprovalStatus::Rejected => {
                    if let Some(gate) =
                        store::approval::workflow_gate_tx(&mut *tx, ctx.tenant, approval.id)
                            .await
                            .map_err(|error| {
                                tracing::error!(%error, approval_id = %approval.id, "read the workflow gate");
                                ActionError::internal()
                            })?
                        && let (Some(node_id), Some(attempt)) = (gate.node_id, gate.attempt)
                    {
                        crate::workflow::node::record_gate_row(
                            &mut *tx, ctx.tenant, run_id, &node_id, attempt, false,
                        )
                        .await
                        .map_err(|failure| {
                            tracing::error!(error = %failure.message, "record the refused gate");
                            ActionError::internal()
                        })?;
                    }
                    cancel_run_for_rejection(&mut *tx, &ctx, run_id).await?;
                }
                _ => {}
            }
        } else {
            match approval.status {
                ApprovalStatus::Approved => {
                    settle_run(&mut *tx, ctx.tenant, run_id, RunStatus::Running).await?;
                }
                ApprovalStatus::Rejected => {
                    cancel_run_for_rejection(&mut *tx, &ctx, run_id).await?;
                }
                _ => {}
            }
        }
    }

    let record = reload_record(&mut *tx, ctx.tenant, &approval).await?;
    Ok(approval_out(&record))
}

/// One page of gates, newest first.
///
/// The filters are the store's; the cursor is the `(created_at, id)` of the previous page's
/// last row, so a concurrent insert cannot make a later page skip a row.
#[crate::action(
    name = "approvals.list",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approvals_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApprovalListIn,
) -> Result<Page<ApprovalOut>, ActionError> {
    let status = match input.status.as_deref() {
        None => None,
        Some(raw) => Some(
            ApprovalStatus::from_str(raw)
                .map_err(|_| invalid("status", "invalid_value", "not an approval status"))?,
        ),
    };
    let status = status.map(ApprovalStatus::as_str);
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::actions::decode_time_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid_cursor",
                "not a cursor this endpoint issued",
            )
        })?),
    };
    let limit = store::actions::clamp_limit(input.page.limit);

    // One extra row decides whether a next page exists; it is never returned.
    let mut rows = store::approval::list_approvals_tx(
        &mut *tx,
        ctx.tenant,
        status,
        input.run_id,
        input.card_id,
        input.project_id,
        input.eval_id,
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| approval_error(error, "approvals.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.approval.id))
    } else {
        None
    };
    let mut decisions = decisions_by_approval(&mut *tx, ctx.tenant, &rows).await?;
    Ok(Page {
        items: rows
            .iter()
            .map(|row| {
                let mut out = approval_out(row);
                out.decisions = decisions.remove(&row.approval.id).unwrap_or_default();
                out
            })
            .collect(),
        next_cursor,
    })
}

/// Pose a question for a human to decide (S06b §5).
///
/// The agent's only path to a person. This cannot be `open_gate_tx`: that primitive requires a
/// `running` run and moves it to `waiting_approval`, and an escalation out of a project has no
/// run to move. Inserting directly is the deliberate choice — the gate's parent is the
/// project, the card or the run the caller names, and a standalone gate is exactly what
/// `uq_appr_pending`'s nullable `run_id` exists to allow.
///
/// A parentless gate is accepted only when it names a destructive action whose target the call
/// computes (#285): then `meta.target`, not a parent, is the gate's identity. Without a parent
/// and without a target nothing could ever read it, so the request is refused.
///
/// No terminal event is emitted (a request is not a resolution), and the requester is the
/// authenticated principal — never a body field.
#[crate::action(
    name = "approvals.request",
    version = 1,
    kind = "internal",
    scopes = ["approvals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn approvals_request(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApprovalRequestIn,
) -> Result<ApprovalOut, ActionError> {
    let gate = input.gate.trim();
    if gate.is_empty() {
        return Err(invalid("gate", "missing", "a gate name is required"));
    }
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a gate kind is required"));
    }
    let required_decisions = input.required_decisions.unwrap_or(1);
    // The same bound the process gate parser applies (`process/step.rs`): a quorum above 255
    // cannot be reached in practice, so it is a typo or an attempt to make a gate undecidable.
    // Zero already had its own refusal; the type (`u16`) left 256..=65535 open.
    if !(1..=255).contains(&required_decisions) {
        return Err(invalid(
            "required_decisions",
            "invalid_value",
            "a gate needs between 1 and 255 decisions",
        ));
    }

    // #282: a gate opened for a destructive action computes its own target, so the string in
    // `meta.target` and the string the invoke boundary will compare against cannot disagree.
    if let Some(action) = input.action.as_deref()
        && action != gate
    {
        return Err(invalid(
            "gate",
            "invalid_value",
            "a gate opened for an action must carry that action's name as its gate",
        ));
    }
    let target =
        resolve_request_target(input.action.as_deref(), input.input.as_ref(), input.target)?;

    // A gate with no parent is legitimate in exactly one shape: it *is* the approval for a named
    // destructive action, and its `meta.target` says what that call will mutate. That string is
    // what the invoke boundary compares (`require_approval_tx`), so the gate has an identity and
    // a reader — the approvals screen — without belonging to a project, a card or a run. The
    // console's destructive flow (conveyor#285) requests exactly these, because the actions it
    // gates (`documents.delete`, `models.delete`, `secrets.delete`, …) have no board to hang on.
    //
    // Everything else still needs a parent: a standalone gate that authorises nothing belongs to
    // no list anybody reads, and could only ever expire.
    if input.project_id.is_none()
        && input.card_id.is_none()
        && input.run_id.is_none()
        && target.is_none()
    {
        return Err(invalid(
            "project_id",
            "missing",
            "an approval must belong to a project, a card or a run, or name a destructive action \
             (with its input) whose computed target it authorises",
        ));
    }

    let mut meta = serde_json::Map::new();
    for (key, value) in [
        ("description", input.description),
        ("proposed_answer", input.proposed_answer),
        ("target", target),
    ] {
        if let Some(value) = value {
            meta.insert(key.to_owned(), Value::String(value));
        }
    }
    if let Some(allowed) = input.allowed_decisions {
        meta.insert("allowed_decisions".to_owned(), allowed);
    }
    if let Some(proposed) = input.proposed_input {
        meta.insert("proposed_input".to_owned(), proposed);
    }
    // T3: a gate resolves its typed evidence before it is opened, so an absent, draft,
    // superseded or non-typed reference never becomes a pending gate.
    let evidence = crate::pm::resolve_evidence(
        &mut *tx,
        ctx.tenant,
        &input.evidence,
        input.require_evidence,
    )
    .await?;
    if !evidence.is_empty() {
        meta.insert("evidence".to_owned(), Value::Array(evidence));
    }

    let new = store::approval::NewApproval {
        run_id: input.run_id,
        card_id: input.card_id,
        project_id: input.project_id,
        gate: gate.to_owned(),
        kind: kind.to_owned(),
        required_decisions,
        requested_by: Some(ctx.actor_id),
        expires_at: input.expires_at,
        // Built above and, until now, dropped: `..NewApproval::new(..)` supplies an empty
        // `meta`, so the agent's description and proposed answer never reached the row
        // (S06b §5: `description`, `proposed_answer` go "into `meta`").
        meta: Value::Object(meta),
        ..store::approval::NewApproval::new(gate, kind)
    };
    let approval = store::approval::insert_approval_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| approval_error(error, "approvals.request"))?;

    let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval.id)
        .await
        .map_err(|error| approval_error(error, "approvals.request"))?
        .ok_or_else(|| {
            tracing::error!(approval = %approval.id, "the gate just inserted is absent");
            ActionError::internal()
        })?;
    Ok(approval_out(&record))
}

/// The gate a destructive action passes before it reaches a backend.
///
/// SPEC_V2 §11.4: `destructive` = approval + dry-run + dual control. The approval row is the
/// control-plane primitive this crate already owns; this function is the one place a handler
/// asks it, so `ops.restart`, `ops.scale` and `ops.rollback` cannot disagree about what an
/// approval authorises. It runs in the caller's transaction and **before any cluster call**.
///
/// Refused (403) when the id is absent, unknown in this tenant, not approved, opened for a
/// different action, or naming a different target. The target is the exact thing the call will
/// mutate (`namespace/deployment/name`, plus the replicas or revision where they matter), so an
/// approval for one deployment can never authorise another.
pub async fn require_approval_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    action: &'static str,
    target: &str,
    approval_id: Option<Uuid>,
) -> Result<Uuid, ActionError> {
    let Some(id) = approval_id else {
        return Err(ActionError::new(
            403,
            "approval_required",
            format!(
                "{action} is destructive: an approved approval for `{target}` must be named in `approval_id`"
            ),
        ));
    };
    let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| approval_error(error, action))?
        .ok_or_else(|| {
            ActionError::new(
                403,
                "approval_not_found",
                format!("no approval {id} in this tenant"),
            )
        })?;
    if record.approval.status != ApprovalStatus::Approved {
        return Err(ActionError::new(
            403,
            "approval_not_approved",
            format!(
                "approval {id} is {}; only an approved gate authorises {action}",
                record.approval.status.as_str()
            ),
        ));
    }
    if record.approval.gate != action {
        return Err(ActionError::new(
            403,
            "approval_wrong_action",
            format!(
                "approval {id} was opened for {:?}, not {action}",
                record.approval.gate
            ),
        ));
    }
    let meta = store::approval::approval_meta_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| approval_error(error, action))?
        .unwrap_or(Value::Null);
    let approved_target = meta
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if approved_target != target {
        return Err(ActionError::new(
            403,
            "approval_wrong_target",
            format!("approval {id} authorises {approved_target:?}, not {target:?}"),
        ));
    }
    Ok(id)
}

/// The target a gate will authorise, computed the way the invoke boundary will compute it.
///
/// Either the caller names the action **and** the input it will later invoke with — and the
/// action's own target function answers — or it names a literal target. Naming both is allowed
/// only when they agree; a gate whose target cannot match the call it was opened for authorises
/// nothing, and a caller that believes otherwise has a bug worth refusing here.
fn resolve_request_target(
    action: Option<&str>,
    input: Option<&Value>,
    literal: Option<String>,
) -> Result<Option<String>, ActionError> {
    let Some(action) = action else {
        return Ok(literal);
    };
    let spec = crate::registry::registry().get(action).ok_or_else(|| {
        invalid(
            "action",
            "invalid_value",
            format!("no action named {action:?}"),
        )
    })?;
    let Some(compute) = spec.approval_target else {
        return Err(invalid(
            "action",
            "invalid_value",
            format!("{action} declares no approval target, so a gate cannot authorise it"),
        ));
    };
    let Some(input) = input else {
        return Err(invalid(
            "input",
            "missing",
            "naming an action requires the input it will be invoked with",
        ));
    };
    let computed = compute(input).map_err(|detail| invalid("input", "invalid_value", &detail))?;
    if let Some(literal) = &literal
        && literal != &computed
    {
        return Err(invalid(
            "target",
            "invalid_value",
            format!("the target {literal:?} disagrees with the {action} target {computed:?}"),
        ));
    }
    Ok(Some(computed))
}

/// The destructive gate, enforced once at the invoke boundary (conveyor#282).
///
/// SPEC_V2 §11.4 says a destructive action runs only behind an approval; until #282 that rule was
/// kept by three handlers remembering to ask. They can forget, and most never asked at all. This
/// function is the one place the engine applies it, so a destructive action cannot be written
/// without a decision about its gate:
///
/// * `Effect::Read` / `Effect::Write` — nothing to gate, return immediately;
/// * `Effect::Destructive` **without** a declared target — ungated, and named in the burn-down
///   constant (`crates/actions/tests/destructive_gate.rs`) rather than silently ignored; #283
///   empties that list;
/// * `Effect::Destructive` **with** a target — compute the canonical target from the input the
///   caller actually sent, read this tenant's policy for the action, and either waive or demand
///   an approved approval naming exactly that target.
///
/// The policy read is the only way an un-approved destructive call is admitted, and a missing
/// policy row is `required` (see `store::approval_policy`). A `dry_run` passes this gate too: it
/// changes nothing, but it still answers with what a real call would do, and the §11.4 rule does
/// not carve dry runs out.
pub async fn enforce_destructive_gate(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    spec: &crate::spec::ActionSpec,
    input: &Value,
) -> Result<(), ActionError> {
    if spec.effect != crate::spec::Effect::Destructive {
        return Ok(());
    }
    let Some(compute) = spec.approval_target else {
        // Ungated by design until #283 declares its target; the burn-down test is the ledger.
        return Ok(());
    };
    let target = compute(input).map_err(|detail| {
        ActionError::invalid_input(vec![crate::spec::FieldError {
            field: "input".to_owned(),
            code: "invalid_value".to_owned(),
            detail: format!("this action declares no approval target for this input: {detail}"),
        }])
    })?;
    let waived = store::approval_policy::waives(&mut *tx, ctx.tenant, spec.name)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = spec.name, "read approval policy");
            ActionError::internal()
        })?;
    if waived {
        return Ok(());
    }
    require_approval_tx(tx, ctx, spec.name, &target, ctx.approval_id)
        .await
        .map(|_| ())
}

fn parse_decision(raw: &str) -> Result<DecisionKind, ActionError> {
    match raw {
        "approve" => Ok(DecisionKind::Approve),
        "reject" => Ok(DecisionKind::Reject),
        _ => Err(invalid(
            "decision",
            "invalid_value",
            "must be approve or reject",
        )),
    }
}

/// The decider kind for an authenticated caller (S06 decision 8).
///
/// Unknown kinds (which the auth layer never produces — it sanitises them to `user`) fall
/// back to `human`: the safe direction is recording a human, never granting an agent's
/// authority to an unclassifiable caller.
fn decider_kind(actor_kind: &str) -> DeciderKind {
    match actor_kind {
        "agent" => DeciderKind::Agent,
        // S06d: a connector's service account is a machine identity, not a deterministic rule.
        // The old mapping hid which transport answered; `service` is the honest value.
        "service" => DeciderKind::Service,
        _ => DeciderKind::Human,
    }
}

/// A store failure on the decide path.
///
/// Everything is [`approval_error`]'s, with one exception: `InvalidRequirement` (a zero
/// quorum) is a *schema-shaped* refusal of the gate, so it is `422`, not the `409` that means
/// "the state moved underneath you".
fn decide_error(error: store::ApprovalStoreError, action: &str) -> ActionError {
    match error {
        store::ApprovalStoreError::Illegal(ApprovalError::InvalidRequirement) => invalid(
            "required_decisions",
            "invalid_value",
            "a gate needs at least one decision",
        ),
        other => approval_error(other, action),
    }
}

/// Re-read the gate's read projection (the decision result carries no `created_at`).
///
/// `record_decision_tx` returns the domain value, which has no row timestamp; the output
/// document declares one. The gate is filtered by its run (always present for a
/// run-opened gate) or its card, so this is a small, index-served read — never a table scan.
async fn reload_record(
    tx: &mut PgConnection,
    tenant: TenantId,
    approval: &Approval,
) -> Result<store::approval::ApprovalRecord, ActionError> {
    let rows = store::approval::list_approvals_tx(
        &mut *tx,
        tenant,
        None,
        approval.run_id,
        approval.card_id,
        approval.project_id,
        None,
        None,
        store::actions::MAX_LIMIT,
    )
    .await
    .map_err(|error| approval_error(error, "approvals.decide"))?;
    match rows.into_iter().find(|row| row.approval.id == approval.id) {
        Some(record) => Ok(record),
        None => {
            // A gate written in this transaction is visible to this transaction; not finding
            // it means the projection and the write disagree, which is a bug, not a 404.
            tracing::error!(
                approval = %approval.id,
                "the decided gate is absent from its own read projection"
            );
            Err(ActionError::internal())
        }
    }
}

/// Append the terminal `approval.*` event in the decision's transaction (CONTRACT §7).
///
/// The topic is the approval's own aggregate topic (S06 §5.6); the payload carries the run so
/// a `run:{id}` subscriber can correlate without a second read.
async fn emit_terminal(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    approval: &Approval,
    event_type: &'static str,
) -> Result<(), ActionError> {
    let payload = serde_json::json!({
        "approval_id": approval.id,
        "run_id": approval.run_id,
        "actor": ctx.actor_id,
    });
    events::append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: format!("approval:{}", approval.id),
            board_id: None,
            aggregate_type: "approval",
            aggregate_id: approval.id,
            event_type,
            payload,
            data: Value::Null,
            prev_data: Value::Null,
        },
    )
    .await
}

/// Move the run behind a decided gate, tolerating a run that is no longer waiting.
///
/// `Ok(false)` means a concurrent `runs.cancel` (or `run.reap`/`approval.expire`) settled the
/// run first. The gate's own decision is still valid and must not be rolled back because the
/// run moved — the two are separate aggregates and the gate's record is the approver's, not
/// the run's.
async fn settle_run(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    run_id: Uuid,
    to: RunStatus,
) -> Result<bool, ActionError> {
    match store::run::transition_run_tx(conn, tenant, run_id, to).await {
        Ok(_) => Ok(true),
        Err(store::RunStoreError::Illegal(_) | store::RunStoreError::NotFound) => Ok(false),
        Err(error) => {
            tracing::error!(%error, "resolve the run behind a decided gate");
            Err(ActionError::internal())
        }
    }
}

/// A rejection withdraws the run: the gate was its only blocker and the domain has no
/// `waiting_approval → failed` edge, so the run is cancelled and announced on its own topic.
async fn cancel_run_for_rejection(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
) -> Result<(), ActionError> {
    if settle_run(conn, ctx.tenant, run_id, RunStatus::Cancelled).await? {
        emit_run_event(
            conn,
            ctx,
            run_id,
            "run.cancelled",
            serde_json::json!({ "run_id": run_id, "reason": "approval rejected" }),
        )
        .await?;
    }
    Ok(())
}

/// `run.cancelled` on the run's own aggregate topic (CONTRACT §7).
async fn emit_run_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    run_id: Uuid,
    event_type: &'static str,
    payload: Value,
) -> Result<(), ActionError> {
    events::append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: format!("run:{run_id}"),
            board_id: None,
            aggregate_type: "run",
            aggregate_id: run_id,
            event_type,
            payload,
            data: Value::Null,
            prev_data: Value::Null,
        },
    )
    .await
}

/// Map one stored gate onto the wire document.
pub fn approval_out(record: &store::approval::ApprovalRecord) -> ApprovalOut {
    let approval = &record.approval;
    ApprovalOut {
        id: approval.id.to_string(),
        run_id: approval.run_id.map(|id| id.to_string()),
        card_id: approval.card_id.map(|id| id.to_string()),
        project_id: approval.project_id.map(|id| id.to_string()),
        eval_id: record.eval_id.map(|id| id.to_string()),
        gate: approval.gate.clone(),
        kind: approval.kind.clone(),
        status: approval.status.as_str().to_owned(),
        target: record
            .meta
            .get("target")
            .and_then(Value::as_str)
            .map(str::to_owned),
        description: record
            .meta
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        proposed_answer: record
            .meta
            .get("proposed_answer")
            .and_then(Value::as_str)
            .map(str::to_owned),
        allowed_decisions: record.meta.get("allowed_decisions").cloned(),
        proposed_input: record.meta.get("proposed_input").cloned(),
        requested_by: record.requested_by.map(|id| id.to_string()),
        requested_by_label: record.requested_by_label.clone(),
        required_decisions: approval.required_decisions,
        approvals: approval.approvals,
        expires_at: approval.expires_at,
        version: i64::try_from(approval.version.get()).unwrap_or(i64::MAX),
        created_at: record.created_at,
        decisions: Vec::new(),
    }
}

/// Read every decision for a page of gates, grouped by gate id.
///
/// One query for the whole page: the cockpit and the run detail both render the history, and a
/// per-gate read would be N+1 on every list.
pub(crate) async fn decisions_by_approval(
    conn: &mut PgConnection,
    tenant: TenantId,
    approvals: &[store::approval::ApprovalRecord],
) -> Result<std::collections::HashMap<Uuid, Vec<ApprovalDecisionOut>>, ActionError> {
    let ids: Vec<Uuid> = approvals.iter().map(|row| row.approval.id).collect();
    let decisions = store::approval::list_decisions_for_approvals_tx(&mut *conn, tenant, &ids)
        .await
        .map_err(|error| approval_error(error, "approvals.list"))?;
    let mut grouped: std::collections::HashMap<Uuid, Vec<ApprovalDecisionOut>> =
        std::collections::HashMap::new();
    for decision in decisions {
        let approver = if decision.approver_display_name.trim().is_empty() {
            decision.approver_email
        } else {
            decision.approver_display_name
        };
        grouped
            .entry(decision.approval_id)
            .or_default()
            .push(ApprovalDecisionOut {
                approver_id: decision.approver_id.to_string(),
                approver,
                decision: decision.decision,
                note: decision.note,
                decided_by_kind: decision.decided_by_kind,
                via: decision.via,
                decided_at: decision.decided_at,
            });
    }
    Ok(grouped)
}

/// The publication gate a derived artefact passes (T16, SPEC_V2 §5.6 "Gates").
///
/// A gate over one released version, not over a run: the target is the row id the approval must
/// name, the gate name is the publishing action's own name, and the approval must carry **at
/// least one `human` decision** — an agent, rule or triage decision is recorded evidence, never a
/// person's act. A tenant's `approval_policy` row may waive the gate; a missing row is `required`
/// (the #282 default). The requester-cannot-decide rule is enforced by the store's one decision
/// writer, so this function never re-checks it.
pub async fn require_human_gate_tx(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    gate: &'static str,
    target: Uuid,
) -> Result<Option<Uuid>, ActionError> {
    let waived = store::approval_policy::waives(&mut *tx, ctx.tenant, gate)
        .await
        .map_err(|error| {
            tracing::error!(%error, gate, "read the tenant's publication policy");
            ActionError::internal()
        })?;
    if waived {
        return Ok(None);
    }
    let Some(id) = ctx.approval_id else {
        return Err(ActionError::new(
            403,
            "approval_required",
            format!(
                "{gate} on a derived artefact needs an approved approval for {target} named in `approval_id`"
            ),
        ));
    };
    let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the publication approval");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                403,
                "approval_not_found",
                format!("no approval {id} in this tenant"),
            )
        })?;
    if record.approval.status != ApprovalStatus::Approved {
        return Err(ActionError::new(
            403,
            "approval_not_approved",
            format!(
                "approval {id} is {}; only an approved gate releases the artefact",
                record.approval.status
            ),
        ));
    }
    if record.approval.gate != gate {
        return Err(ActionError::new(
            403,
            "approval_wrong_action",
            format!(
                "approval {id} was opened for {:?}, not {gate}",
                record.approval.gate
            ),
        ));
    }
    let meta = store::approval::approval_meta_tx(&mut *tx, ctx.tenant, id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the publication approval's target");
            ActionError::internal()
        })?
        .unwrap_or(Value::Null);
    let approved_target = meta
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if approved_target != target.to_string() {
        return Err(ActionError::new(
            403,
            "approval_wrong_target",
            format!("approval {id} authorises {approved_target:?}, not {target}"),
        ));
    }
    let decisions = store::approval::list_decisions_for_approvals_tx(&mut *tx, ctx.tenant, &[id])
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the publication approval's decisions");
            ActionError::internal()
        })?;
    let human = decisions
        .iter()
        .any(|decision| decision.decision == "approve" && decision.decided_by_kind == "human");
    if !human {
        return Err(ActionError::new(
            403,
            "human_decision_required",
            format!(
                "approval {id} carries no human approval decision; a gate on a derived artefact is a person's act"
            ),
        ));
    }
    Ok(Some(id))
}
