#![forbid(unsafe_code)]
//! The `run.resume` worker: apply the draft an approved `executor` gate decided
//! (S21 step 5, issue #590; ADR 0011).
//!
//! The approval transaction already recorded the bound L3 `gate.draft_apply` decision and
//! settled the run `waiting_approval → running`. This worker only applies the draft through the
//! one [`crate::ladder::apply_draft_tx`] gate and finishes the run. It never calls the model, so
//! the attached draft is the draft the person approved, bought once.

use conveyor_domain::{ApprovalStatus, RunStatus, TenantId};
use conveyor_store as store;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{ExecutorFailure, finish_failed, finish_succeeded, service_ctx};

/// Apply the approved gate's draft and finish the run.
///
/// A terminal run or an unapproved gate is a quiet no-op (a reclaimed job is safe). Any stored
/// state that cannot be the approval's output finishes the run `failed` with
/// `draft_apply_invalid`, releasing the card's `uq_run_live` slot; a database failure is
/// retryable and leaves the run `running`.
pub async fn resume_run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run_id: Uuid,
    approval_id: Uuid,
    decision_id: Uuid,
) -> Result<(), ExecutorFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let Some(run) = store::run::get_run_context_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?
    else {
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        tracing::warn!(run = %run_id, "run.resume: the run no longer exists");
        return Ok(());
    };
    if matches!(
        run.status,
        RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
    ) {
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        tracing::info!(
            run = %run_id,
            status = run.status.as_str(),
            "run.resume: the run is terminal"
        );
        return Ok(());
    }
    if run.status != RunStatus::Running {
        let detail = format!(
            "the run is {} when its resume ran; the approval must settle it to running",
            run.status.as_str()
        );
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "draft_apply_invalid",
            &detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }

    let approval = store::approval::get_approval_tx(&mut tx, tenant, approval_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let approved_gate = approval.is_some_and(|record| {
        record.approval.kind == "executor"
            && record.approval.run_id == Some(run_id)
            && record.approval.status == ApprovalStatus::Approved
    });
    if !approved_gate {
        let detail = "the run.resume job does not name an approved executor gate for this run";
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "draft_apply_invalid",
            detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }

    let decision = store::ladder::get_decision_tx(&mut tx, tenant, decision_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let decision_ok = decision.as_ref().is_some_and(|decision| {
        decision.approval_id == Some(approval_id)
            && decision.kind == crate::ladder::DRAFT_APPLY_GATE
            && decision.outcome.as_deref() == Some("accepted")
    });
    if !decision_ok {
        let detail =
            "the resume's decision is not the accepted gate.draft_apply decision of its approval";
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "draft_apply_invalid",
            detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }

    let Some(card_id) = run.card_id else {
        let detail = "the internal_agent run has no card to apply its draft to";
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "draft_apply_invalid",
            detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    };
    let output = store::run::last_step_output_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let document_id = output
        .as_ref()
        .and_then(|output| output.get("draft_document_id"))
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok());
    let Some(document_id) = document_id else {
        let detail = "the resumed run's step names no draft document";
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "draft_apply_invalid",
            detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    };

    let actor_id = decision.as_ref().and_then(|decision| decision.actor_id);
    let ctx = service_ctx(tenant, actor_id, app_key, "run.resume");
    match crate::ladder::apply_draft_tx(&mut tx, &ctx, document_id, card_id, decision_id).await {
        Ok(_) => {}
        Err(error) if error.status >= 500 => {
            return Err(ExecutorFailure::retryable(format!(
                "{}: {}",
                error.code, error.detail
            )));
        }
        Err(error) => {
            let detail = format!("{}: {}", error.code, error.detail);
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "draft_apply_invalid",
                &detail,
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    }
    finish_succeeded(&mut tx, tenant, app_key, run_id).await?;
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    Ok(())
}
