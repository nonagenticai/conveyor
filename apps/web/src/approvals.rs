//! The console's destructive-invoke path (#285).
//!
//! A destructive action is refused by the engine unless the call names an **approved** approval
//! whose target equals the target the action computes (SPEC_V2 §11.4, conveyor#282). Until this
//! module existed every destructive button in the console answered a bare `403 approval_required`
//! — the gate was real and the way through it was not.
//!
//! One path serves every call site, so no screen can invent its own:
//!
//! 1. read the tenant's policy for the action (`approval_policies.get`, whose unset answer is the
//!    strict default — never a waiver);
//! 2. `waived` → invoke, no gate;
//! 3. otherwise **request** a gate that names the action and the input, so the server computes
//!    `meta.target` from the same function the invoke boundary compares;
//! 4. `self_approval: allowed` → decide it and invoke with the id; `forbidden` (the default) →
//!    stop and say so, because someone else has to approve it.

use std::cell::RefCell;
use std::collections::HashMap;

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::state::AppCtx;

thread_local! {
    /// The gate each action is waiting on, for this browser session.
    ///
    /// A gate approved by someone else is not executed for you: nothing replays the call. So the
    /// requester's retry is what completes it, and this is what that retry looks up — otherwise
    /// every retry would open a SECOND gate for the same object and the approver's first answer
    /// would authorise nothing. Session-scoped on purpose: a reload forgets, and a forgotten
    /// gate is a stale row on the approvals screen, never a wrong execution.
    static WAITING: RefCell<HashMap<&'static str, String>> = RefCell::new(HashMap::new());
}

/// What a destructive attempt did.
#[derive(Clone, Debug)]
pub enum Destructive {
    /// The action ran.
    Done,
    /// The gate is waiting for someone else; the approver decides it on `/approvals`.
    Waiting { approval_id: String },
    /// The request, the decision or the call was refused.
    Failed(ApiError),
}

impl Destructive {
    /// Whether the action actually ran — the only outcome a screen should treat as success.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done)
    }
}

/// Ask, decide when the tenant allows it, then invoke — with the approval id on the call.
///
/// Mirrors the screens' existing `run_write` shape so a call site keeps its own refresh and
/// notice callbacks: `done` receives the outcome, `finish` always runs (clear the busy flag).
pub fn run_destructive(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    options: InvokeOptions,
    done: impl FnOnce(Destructive) + 'static,
    finish: impl FnOnce() + 'static,
) {
    leptos::task::spawn_local(async move {
        let outcome = attempt(ctx, action, input, options).await;
        match &outcome {
            Destructive::Done => {}
            Destructive::Waiting { approval_id } => {
                // Never a silent no-op: the person is told what is waiting and where to look.
                ctx.toast.set(Some(format!(
                    "Waiting for approval — someone else must decide gate {} on the approvals \
                     screen.",
                    short(approval_id)
                )));
            }
            Destructive::Failed(error) => ctx.report(error),
        }
        done(outcome);
        finish();
    });
}

async fn attempt(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    options: InvokeOptions,
) -> Destructive {
    let (approval, self_approval) = policy(ctx, action).await;
    if approval == "waived" {
        return call(ctx, action, input, options).await;
    }

    // A retry after someone approved the gate this session is waiting on: finish the call with
    // it instead of asking for a second gate. A refusal here falls through to a fresh ask.
    if let Some(approval_id) = WAITING.with(|waiting| waiting.borrow().get(action).cloned()) {
        let retried = call(
            ctx,
            action,
            input.clone(),
            InvokeOptions {
                approval_id: Some(approval_id),
                ..options.clone()
            },
        )
        .await;
        if matches!(retried, Destructive::Done) {
            WAITING.with(|waiting| waiting.borrow_mut().remove(action));
            return retried;
        }
    }

    let requested = ctx
        .invoke(
            "approvals.request",
            json!({
                "gate": action,
                "kind": "action",
                "action": action,
                // The input the target is computed from, and the same document kept for the
                // approver to read. They are intentionally the same value: what is approved is
                // what runs.
                "input": input,
                "proposed_input": input,
                "description": format!("{action} from the console"),
            }),
            InvokeOptions::default(),
        )
        .await;
    let requested = match requested {
        Ok(invocation) => invocation,
        Err(error) => return Destructive::Failed(error),
    };
    let Some(approval_id) = requested
        .output
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return Destructive::Failed(ApiError::Network(
            "the approval response carried no id".to_owned(),
        ));
    };

    if self_approval != "allowed" {
        // Remember what this session is waiting on so the retry completes the call.
        WAITING.with(|waiting| waiting.borrow_mut().insert(action, approval_id.clone()));
        return Destructive::Waiting { approval_id };
    }
    if let Err(error) = ctx
        .invoke(
            "approvals.decide",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            InvokeOptions::default(),
        )
        .await
    {
        return Destructive::Failed(error);
    }
    let outcome = call(
        ctx,
        action,
        input,
        InvokeOptions {
            approval_id: Some(approval_id),
            ..options
        },
    )
    .await;
    if matches!(outcome, Destructive::Done) {
        WAITING.with(|waiting| waiting.borrow_mut().remove(action));
    }
    outcome
}

/// Why a caller may not decide their own gate, when the tenant forbids self-approval.
///
/// A screen shows the decide control **disabled and says why** — an enabled control the engine
/// then refuses reads as a bug. The reason is one sentence so every screen says the same thing.
pub fn self_gate_refusal(mine: bool, self_approval: &str) -> Option<&'static str> {
    (mine && self_approval != "allowed")
        .then_some("You requested this gate; another person must decide it.")
}

/// The tenant's policy for one gate: `(approval, self_approval)`.
///
/// The engine's own defaults when the tenant has no row — and when the read is refused — are
/// `required`/`forbidden`; a reader that cannot see the policy must never guess a waiver.
pub async fn policy_for(client: &crate::api::Client, gate: &str) -> (String, String) {
    match client
        .invoke(
            "approval_policies.get",
            json!({ "action": gate }),
            InvokeOptions::default(),
        )
        .await
    {
        Ok(invocation) => (
            text(&invocation.output, "approval", "required"),
            text(&invocation.output, "self_approval", "forbidden"),
        ),
        Err(_) => ("required".to_owned(), "forbidden".to_owned()),
    }
}

/// The tenant's policy for one action, read through this context's client.
async fn policy(ctx: AppCtx, action: &str) -> (String, String) {
    let client = ctx.client();
    policy_for(&client, action).await
}

fn text(output: &Value, field: &str, fallback: &str) -> String {
    output
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

async fn call(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    options: InvokeOptions,
) -> Destructive {
    match ctx.invoke(action, input, options).await {
        Ok(_) => Destructive::Done,
        Err(error) => Destructive::Failed(error),
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
