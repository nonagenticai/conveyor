#![forbid(unsafe_code)]
//! `eval_signoffs.*` — the instrument's two literal pauses (S19 step 7; CONTRACT §20).
//!
//! A sign-off is an ordinary `approval` row carrying `eval_id`, with the instrument digest in
//! `meta.artifact_sha256`. There is no second store and no second status:
//!
//! - `eval_signoffs.request` freezes **the instrument as it is now** — `version_digest_tx`, not
//!   the version's stored `digest` — so signing what the caller was shown is the only thing
//!   that can be signed. For the two gating pauses (`eval_inputs`, `eval_grading`) a supplied
//!   `subject_sha256` is ignored: you sign what is there. For the two recorded-but-not-gating
//!   pauses (`eval_cost`, `eval_harness`) it is accepted, because the thing being signed is a
//!   cost ceiling or a harness that is not part of the instrument digest.
//! - `eval_signoffs.decide` is the one action a machine may never call: `evals:approve` is held
//!   by `Owner | Admin` only, and the handler refuses any principal whose `actor_kind` is not
//!   `user` with `403 signoff_requires_human`. A scope a machine holds cannot be a sign-off
//!   (CONTRACT §20 trap 1).
//! - Neither action has a run to resolve: these gates pause the *authoring* of an instrument,
//!   not a run, so there is deliberately no `run_id` on the row and no run-resolution path here.
//!
//! Readiness is *not* computed here. `store::eval::eval_readiness_tx` takes the latest approval
//! per gate and requires `status = 'approved'` **and** a digest match; editing a case therefore
//! voids a sign-off with no status write, which is why nothing in this module ever writes one.

use std::str::FromStr;

use conveyor_domain::eval::EvalGate;
use conveyor_domain::{DeciderKind, Decision, DecisionKind};
use conveyor_store as store;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::approvals::approval_out;
use crate::control::invalid;
use crate::control::views::ApprovalOut;
use crate::invoke::{ActionError, HandlerCtx};

use super::{eval_error, not_ready};

/// `eval_signoffs.request` — which pause, and what the requester is asking.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SignoffRequestIn {
    /// The eval being signed off.
    pub eval_id: Uuid,
    /// `eval_inputs | eval_grading | eval_cost | eval_harness`.
    pub gate: String,
    /// The artifact hash, for `eval_cost`/`eval_harness` only. Ignored by the two gating
    /// pauses, which always sign the instrument as it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_sha256: Option<String>,
    /// What the approver is being asked to confirm.
    pub ask: String,
}

/// `eval_signoffs.decide` — one approver's decision on an eval gate.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SignoffDecideIn {
    /// The approval id, which must be an eval gate.
    pub approval_id: Uuid,
    /// `approve` or `reject`.
    pub decision: String,
    /// A free-text note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Pose one eval gate for a human to decide.
#[crate::action(
    name = "eval_signoffs.request",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_signoffs_request(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SignoffRequestIn,
) -> Result<ApprovalOut, ActionError> {
    let gate = EvalGate::from_str(input.gate.trim()).map_err(|_| {
        invalid(
            "gate",
            "invalid_value",
            "must be eval_inputs, eval_grading, eval_cost or eval_harness",
        )
    })?;

    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let Some(version_id) = eval.current_version_id else {
        return Err(not_ready(
            ctx.action,
            "the eval has no version; there is no instrument to sign off",
        ));
    };

    // The instrument AS IT IS NOW: a sign-off covers what the approver can be shown, not the
    // digest frozen when the version was minted.
    let digest = store::eval::version_digest_tx(&mut *tx, ctx.tenant, version_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let artifact_sha256 = match gate {
        // You sign what is there. A supplied hash cannot make a gating sign-off cover an
        // instrument other than the one that will run.
        EvalGate::Inputs | EvalGate::Grading => digest,
        // The cost ceiling and the harness are not part of the instrument digest, so the
        // caller's artifact is the only thing those rows can name; the digest is the fallback.
        EvalGate::Cost | EvalGate::Harness => input
            .subject_sha256
            .as_deref()
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .map(str::to_owned)
            .unwrap_or(digest),
    };

    let new = store::approval::NewApproval {
        eval_id: Some(eval.id),
        gate: gate.as_str().to_owned(),
        kind: "eval".to_owned(),
        required_decisions: 1,
        requested_by: Some(ctx.actor_id),
        // Approvals never expire (owner ruling): no `expires_at` is set, ever.
        meta: json!({ "artifact_sha256": artifact_sha256, "ask": input.ask }),
        ..store::approval::NewApproval::new(gate.as_str(), "eval")
    };
    // `uq_appr_eval_pending` is the guarantee that one gate has one pending sign-off; the
    // conflict is a `409`, never a check-then-insert.
    let approval = store::approval::insert_approval_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| signoff_error(error, ctx.action))?;

    reload(&mut *tx, &ctx, approval.id).await
}

/// Record one human decision on an eval gate.
#[crate::action(
    name = "eval_signoffs.decide",
    version = 1,
    kind = "internal",
    scopes = ["evals:approve"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_signoffs_decide(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SignoffDecideIn,
) -> Result<ApprovalOut, ActionError> {
    // A machine cannot sign off, whatever scope it happens to hold. This is checked before the
    // lookup so a machine gets the same refusal whether or not the id exists: the refusal is
    // about the principal, not about the row.
    if ctx.actor_kind != "user" {
        return Err(ActionError::new(
            403,
            "signoff_requires_human",
            format!(
                "{}: an eval sign-off must be decided by a person, not by a {} principal",
                ctx.action, ctx.actor_kind
            ),
        ));
    }

    let kind = match input.decision.trim() {
        "approve" => DecisionKind::Approve,
        "reject" => DecisionKind::Reject,
        _ => {
            return Err(invalid(
                "decision",
                "invalid_value",
                "must be approve or reject",
            ));
        }
    };

    // 404 for an absent row AND for any approval that is not an eval gate: this action only
    // decides eval gates, and pointing it at a run's gate must not decide that run.
    store::eval::get_eval_approval_tx(&mut *tx, ctx.tenant, input.approval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let decision = Decision {
        approver_id: ctx.actor_id,
        kind,
        note: input.note.unwrap_or_default(),
        // The decider is derived from the authenticated principal, never from the body: a
        // scope a machine holds cannot be a sign-off, and the row records the human.
        decided_by_kind: DeciderKind::Human,
        via: Some(json!({ "channel": "web" })),
        item_decisions: None,
    };
    // This action already requires `evals:approve` and a human, so it is the one caller that
    // may pass `true`; the writer still enforces both.
    let approval = store::approval::record_decision_tx(
        &mut *tx,
        ctx.tenant,
        input.approval_id,
        &decision,
        true,
    )
    .await
    .map_err(|error| signoff_error(error, ctx.action))?;

    reload(&mut *tx, &ctx, approval.id).await
}

/// Re-read the gate's read projection and map it onto the shared `ApprovalOut`.
///
/// `record_decision_tx` returns the domain value, which carries no row timestamp; the output
/// document declares one. This is the same reload `approvals.decide` performs, and it is why
/// this module needs no view of its own.
async fn reload(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    approval_id: Uuid,
) -> Result<ApprovalOut, ActionError> {
    let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval_id)
        .await
        .map_err(|error| signoff_error(error, ctx.action))?
        .ok_or_else(|| {
            tracing::error!(approval = %approval_id, "the eval gate just written is absent");
            ActionError::internal()
        })?;
    Ok(approval_out(&record))
}

/// The approval store's errors, with the one eval-specific refusal named.
fn signoff_error(error: store::ApprovalStoreError, action: &str) -> ActionError {
    match error {
        store::ApprovalStoreError::Conflict => ActionError::new(
            409,
            "signoff_pending",
            format!("{action:?}: a pending sign-off already exists for this eval gate"),
        ),
        other => crate::control::approval_error(other, action),
    }
}
