#![forbid(unsafe_code)]
//! The `internal_agent` run executor (SPEC_V2 §4.4/§4.5, §7.2–§7.7; S21 step 4, ADR 0010).
//!
//! One bound run becomes one `agent.run` step: the frozen release supplies the model, the prompt
//! recipe and the caps; the one compiler renders the four sources; the one loop drives the
//! generation; the output is an unattached `document` with `meta.generated = true`; and the
//! effective `gate.precheck` policy decides the draft's disposition (no policy → no laya call).
//! Applying the draft is `ladder.apply_draft` with an accepted decision — generation never moves
//! a card.
//!
//! **Refusals and retries.** A readiness refusal (`capability_not_ready`, `tool_not_ready`) is
//! terminal and recorded before any model call. A 5xx from the router or laya leaves the run and
//! its step `running` and returns a retryable failure under the same idempotency prefix; the next
//! attempt replays the stored model/tool responses rather than buying them twice.

use std::future::Future;
use std::pin::Pin;

use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::executor::ExecutorKind;
use conveyor_domain::ladder::{Outcome, unavailable};
use conveyor_store as store;
use conveyor_workflow::trace::hash_value;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::invoke::{ActionError, Authority, InvokeRequest, invoke};
use crate::process::judge;
use crate::registry::registry;

use super::{
    ExecutorFailure, RunExecutor, RunRequest, finish_failed, finish_succeeded, service_ctx,
};

/// The case kind the gate pre-check is keyed by (SPEC_V2 §7.3).
const GATE_KIND: &str = "gate.precheck";

/// The built-in `internal_agent` executor.
pub struct InternalAgentExecutor;

impl RunExecutor for InternalAgentExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::internal_agent()
    }

    fn execute<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorFailure>> + Send + 'a>> {
        Box::pin(run(request))
    }
}

/// The draft identity the run step and the document meta both record.
fn draft_meta(
    run_id: Uuid,
    binding_id: Uuid,
    release_hash: &str,
    model_ref: &str,
    out: &crate::workflow::agent::AgentRunOut,
) -> Value {
    json!({
        "generated": true,
        "run_id": run_id,
        "job_binding_id": binding_id,
        "release_hash": release_hash,
        "model_ref": model_ref,
        "prompt_ref": out.prompt_ref,
        "prompt_version": out.prompt_version,
        "prompt_sha256": out.prompt_sha256,
        "turns": out.turns,
    })
}

/// One typed refusal ready to finish the run with.
struct Refusal {
    class: String,
    detail: String,
}

/// What the gate decided about the draft.
struct Gate {
    /// The exact `run_step.judge` document.
    judge: Value,
    /// `None` to proceed, or the typed class and detail that fail the run.
    refusal: Option<Refusal>,
    /// A `handover`: park the run and open the approval that applies the draft.
    escalation: Option<Escalation>,
}

/// The facts the `executor` approval needs when a gate hands the draft to a person.
struct Escalation {
    /// The policy threshold the answer missed.
    threshold: f64,
    /// The policy that produced the decision.
    policy_id: Uuid,
    /// The laya config the policy pinned.
    laya_config_id: Uuid,
}

async fn run(request: RunRequest<'_>) -> Result<(), ExecutorFailure> {
    let RunRequest {
        pool,
        tenant,
        app_key,
        run,
        binding,
        anatomy,
        actor_id,
    } = request;
    let run_id = run.id;
    let Some(agent_id) = binding.capability_agent_id else {
        // The runtime validated the binding, so this is unreachable through `run.execute`.
        return Err(ExecutorFailure::permanent(
            "an internal_agent binding names no capability agent",
        ));
    };
    let prefix = store::process::process_step_idempotency_key(run_id, 1, 1);

    // ── readiness, before any model call ────────────────────────────────────────────────────
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let agent = store::capability::get_agent_tx(&mut tx, tenant, agent_id)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?
        .ok_or_else(|| ExecutorFailure::permanent("the binding's capability agent is gone"))?;
    if agent.status != "active" {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "capability_not_ready",
            &format!(
                "capability_agent {agent_id} is {:?}, not active",
                agent.status
            ),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    let Some(reference) = anatomy
        .effective_prompt_ref(agent.prompt_ref.as_deref())
        .map(str::to_owned)
    else {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "capability_not_ready",
            "the release pins no prompt_recipe and the capability agent names no prompt_ref",
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    };
    let model_ref = anatomy.effective_model_ref(&agent.model_ref).to_owned();
    if model_ref.trim().is_empty() {
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "capability_not_ready",
            "the release and the capability agent name no model",
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    }
    // The prompt facts read the run's capability; a `runs.create` run has none until now.
    let capability_id = binding.capability_id.or(Some(agent.capability_id));
    store::run::bind_executor_tx(&mut tx, tenant, run_id, capability_id, Some(agent_id))
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;

    // A configured gate that cannot decide must refuse before the run spends a model call.
    let policy =
        store::ladder::effective_policy_tx(&mut tx, tenant, GATE_KIND, anatomy.gate.as_deref())
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    if let Some(policy) = &policy {
        if policy.threshold().is_none() {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "capability_not_ready",
                "the effective gate.precheck policy names no threshold",
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        if policy.laya_config.is_none() {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "capability_not_ready",
                "the effective gate.precheck policy has no laya inference config",
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
        if let Err(error) = policy.validate_vocabulary() {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                "capability_not_ready",
                &format!("the effective gate.precheck policy is unusable: {error}"),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    }

    // ── the one step ────────────────────────────────────────────────────────────────────────
    let envelope = json!({
        "executor": "internal_agent",
        "job_binding_id": binding.id.to_string(),
        "release_hash": binding.release_hash,
        "model_ref": model_ref,
        "prompt_ref": reference,
        "limits": {
            "max_iterations": anatomy.limits.max_iterations,
            "timeout_secs": anatomy.limits.timeout_secs,
            "max_tool_calls": anatomy.limits.max_tool_calls,
            "max_tokens": anatomy.limits.max_tokens,
            "budget_usd": anatomy.limits.budget_usd,
        },
        "gate": { "kind": GATE_KIND, "gate": anatomy.gate },
        "card_id": run.card_id,
        "project_id": run.project_id,
    });
    let step_id = match store::workflow::run::start_step_tx(
        &mut tx,
        tenant,
        run_id,
        &store::workflow::run::NewWorkflowStep {
            position: 1,
            attempt: 1,
            node_id: None,
            action: "agent.run".to_owned(),
            input: envelope.clone(),
            input_hash: Some(hash_value(&envelope)),
        },
    )
    .await
    {
        Ok(step_id) => step_id,
        Err(error) => {
            return Err(ExecutorFailure::retryable(format!("database: {error}")));
        }
    };
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;

    // ── the one loop over the one compiler ──────────────────────────────────────────────────
    let started = std::time::Instant::now();
    let out = match crate::workflow::agent::run_release_step(
        pool,
        tenant,
        run_id,
        Some(step_id),
        Some(agent_id),
        app_key,
        Some(&prefix),
        0,
        &anatomy,
        &json!({}),
    )
    .await
    {
        Ok(out) => out,
        // A 5xx is an outage: nothing was decided, so the run stays `running` and the job is
        // retried under the same prefix.
        Err(error) if error.status >= 500 => {
            return Err(ExecutorFailure::retryable(format!(
                "{}: {}",
                error.code, error.detail
            )));
        }
        Err(error) => {
            let detail = format!("{}: {}", error.code, error.detail);
            finish_step_failed(pool, tenant, run_id, &detail).await?;
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                &internal_class(&error),
                &detail,
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    };

    // ── the draft ───────────────────────────────────────────────────────────────────────────
    let meta = draft_meta(run_id, binding.id, &binding.release_hash, &model_ref, &out);
    let Some(actor_id) = actor_id else {
        let detail = "the tenant has no active member for an agent run to act as".to_owned();
        finish_step_failed(pool, tenant, run_id, &detail).await?;
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        finish_failed(
            &mut tx,
            tenant,
            app_key,
            run_id,
            "capability_not_ready",
            &detail,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
        return Ok(());
    };
    let title = if anatomy.title.trim().is_empty() {
        format!("{} — run {run_id}", anatomy.job_key)
    } else {
        format!("{} — run {run_id}", anatomy.title)
    };
    let composed = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: "documents.compose".to_owned(),
            input: json!({
                "title": title,
                "project_id": run.project_id,
                "body": out.output,
                "meta": meta,
            }),
            precondition: None,
            idempotency_key: Some(format!("{prefix}:draft")),
            approval_id: None,
            actor_id,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("internal_agent:{}:draft", run_id)),
            trace_id: Some(format!("run:{run_id}")),
            depth: 1,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await;
    let document_id = match composed {
        Ok(outcome) => outcome
            .body
            .get("output")
            .and_then(|output| output.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        Err(error) if error.status >= 500 => {
            return Err(ExecutorFailure::retryable(format!(
                "documents.compose: {}: {}",
                error.code, error.detail
            )));
        }
        Err(error) => {
            let class = if error.status == 413 {
                "draft_too_large"
            } else {
                "internal_agent.empty_output"
            };
            let detail = format!("documents.compose: {}: {}", error.code, error.detail);
            finish_step_failed(pool, tenant, run_id, &detail).await?;
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            finish_failed(&mut tx, tenant, app_key, run_id, class, &detail).await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            return Ok(());
        }
    };

    // ── the gate ────────────────────────────────────────────────────────────────────────────
    let gate = decide(
        pool,
        tenant,
        app_key,
        &run,
        &anatomy,
        &binding,
        &out,
        document_id.as_deref(),
        &prefix,
    )
    .await?;

    // ── finish: the step, its judge, and the run ────────────────────────────────────────────
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let output = json!({
        "body": out.output,
        "draft_document_id": document_id,
        "prompt_ref": out.prompt_ref,
        "prompt_version": out.prompt_version,
        "prompt_sha256": out.prompt_sha256,
        "turns": out.turns,
        "tool_calls": out.tool_calls.len(),
    });
    let duration_ms = i32::try_from(started.elapsed().as_millis()).ok();
    store::workflow::run::finish_step_tx(
        &mut tx,
        tenant,
        run_id,
        1,
        1,
        "succeeded",
        &output,
        Some(&hash_value(&output)),
        duration_ms,
        None,
    )
    .await
    .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    store::process::record_run_step_judge_tx(&mut tx, tenant, step_id, &gate.judge)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    match (gate.refusal, gate.escalation) {
        (Some(refusal), _) => {
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run_id,
                &refusal.class,
                &refusal.detail,
            )
            .await?;
        }
        (None, Some(escalation)) => {
            let Some(document) = document_id.as_deref() else {
                // `documents.compose` succeeded, so a handover without a draft is a runtime bug;
                // record it as a terminal refusal rather than parking an empty gate.
                finish_failed(
                    &mut tx,
                    tenant,
                    app_key,
                    run_id,
                    "gate_requires_approval",
                    "the gate handed over no draft",
                )
                .await?;
                tx.commit()
                    .await
                    .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
                return Ok(());
            };
            let meta = json!({
                "run_id": run_id,
                "gate": GATE_KIND,
                "draft_document_id": document,
                "prompt_sha256": out.prompt_sha256,
                "position": 1,
                "attempt": 1,
                "threshold": escalation.threshold,
                "policy_id": escalation.policy_id,
                "laya_config_id": escalation.laya_config_id,
            });
            store::approval::open_gate_with_meta_tx(
                &mut tx,
                tenant,
                run_id,
                GATE_KIND,
                "executor",
                1,
                None,
                Some(actor_id),
                meta,
            )
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            let ctx = service_ctx(tenant, Some(actor_id), app_key, "run.execute");
            crate::board::events::append_raw_event(
                &mut tx,
                &ctx,
                &crate::board::events::RawEvent {
                    topic: format!("run:{run_id}"),
                    board_id: None,
                    aggregate_type: "approval",
                    aggregate_id: run_id,
                    event_type: "approval.requested",
                    payload: json!({
                        "run_id": run_id,
                        "gate": GATE_KIND,
                        "kind": "executor",
                        "draft_document_id": document,
                        "threshold": escalation.threshold,
                    }),
                    data: Value::Null,
                    prev_data: Value::Null,
                },
            )
            .await
            .map_err(|error| ExecutorFailure::retryable(error.detail))?;
            // The run stays `waiting_approval`; the step is already `succeeded` with the draft.
        }
        (None, None) => finish_succeeded(&mut tx, tenant, app_key, run_id).await?,
    }
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

/// The class a typed refusal from the loop records.
fn internal_class(error: &ActionError) -> String {
    match error.code {
        "agent.turn_limit"
        | "agent.tool_limit"
        | "agent.timeout"
        | "agent.depth_limit"
        | "capability_not_ready"
        | "tool_not_ready" => error.code.to_owned(),
        _ => "internal_agent.failed".to_owned(),
    }
}

async fn finish_step_failed(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    detail: &str,
) -> Result<(), ExecutorFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    store::workflow::run::finish_step_tx(
        &mut tx,
        tenant,
        run_id,
        1,
        1,
        "failed",
        &json!({}),
        None,
        None,
        Some(detail),
    )
    .await
    .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

/// Evaluate the configured `gate.precheck` policy over the draft.
///
/// No policy → no laya call and `run_step.judge = {decision: "no_policy"}`. A policy → one
/// `laya.judge` call and one `ladder.evaluate` decision row; an outage follows the policy's
/// `fail_policy` and never records a counterfactual decision.
#[allow(clippy::too_many_arguments)]
async fn decide(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run: &store::run::RunContext,
    anatomy: &conveyor_domain::organisation::ReleaseAnatomy,
    binding: &store::organisation::BindingRow,
    out: &crate::workflow::agent::AgentRunOut,
    document_id: Option<&str>,
    prefix: &str,
) -> Result<Gate, ExecutorFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let policy =
        store::ladder::effective_policy_tx(&mut tx, tenant, GATE_KIND, anatomy.gate.as_deref())
            .await
            .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let Some(policy) = policy else {
        return Ok(Gate {
            judge: json!({ "decision": "no_policy", "kind": GATE_KIND }),
            refusal: None,
            escalation: None,
        });
    };
    // Readiness proved both of these above; re-reading them here cannot fail.
    let Some(threshold) = policy.threshold() else {
        return Err(ExecutorFailure::permanent(
            "the effective gate.precheck policy lost its threshold",
        ));
    };
    let laya_config = policy
        .laya_config
        .as_ref()
        .ok_or_else(|| ExecutorFailure::permanent("the policy lost its laya config"))?;
    let fail_policy = policy
        .fail_policy()
        .map_err(|error| ExecutorFailure::permanent(format!("the policy is unusable: {error}")))?;
    let action = policy
        .action()
        .map_err(|error| ExecutorFailure::permanent(format!("the policy is unusable: {error}")))?;
    let actor_id = store::identity::tenant_actor(pool, tenant)
        .await
        .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
    let Some(actor_id) = actor_id else {
        return Err(ExecutorFailure::permanent(
            "the tenant has no active member for the laya call to act as",
        ));
    };

    let criteria = policy
        .policy
        .meta
        .get("criteria")
        .cloned()
        .unwrap_or_else(|| json!("the draft satisfies the request"));
    let case_input = json!({
        "run_id": run.id,
        "card_id": run.card_id,
        "job_key": anatomy.job_key,
        "release_hash": binding.release_hash,
    });
    let request = judge::request(
        Some(&laya_config.model_ref),
        &json!({ "output": out.output, "turns": out.turns }),
        &criteria,
        Some(&case_input),
    );
    let started = std::time::Instant::now();
    let judged = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: "laya.judge".to_owned(),
            input: request,
            precondition: None,
            idempotency_key: Some(format!("{prefix}:judge")),
            approval_id: None,
            actor_id,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("internal_agent:{}:judge", run.id)),
            trace_id: Some(format!("run:{}", run.id)),
            depth: 1,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await;
    let latency_ms = i32::try_from(started.elapsed().as_millis()).ok();
    let verdict = match judged {
        Err(error) if error.status >= 500 => None,
        Err(error) => {
            // A 4xx from the judge is an outage too: only a readable verdict can decide.
            tracing::warn!(code = %error.code, "internal_agent: the laya judge refused the request");
            None
        }
        Ok(outcome) => {
            let answer = outcome.body.get("output").cloned().unwrap_or(Value::Null);
            match judge::parse_verdict(&answer, Some(threshold)) {
                Ok(verdict) => Some(verdict),
                Err(error) => {
                    tracing::warn!(%error, "internal_agent: unreadable laya verdict");
                    None
                }
            }
        }
    };

    let Some(verdict) = verdict else {
        let outcome = unavailable(fail_policy, action);
        let judge_doc = json!({
            "decision": "unavailable",
            "outcome": outcome_str(outcome),
            "fail_policy": fail_policy.as_str(),
            "action": action.as_str(),
            "threshold": threshold,
            "policy_id": policy.policy.id.to_string(),
            "laya_config_id": laya_config.id.to_string(),
            "latency_ms": latency_ms,
        });
        // An open gate whose configured action is handover parks the draft for a person; a
        // refused outcome (closed, or block) fails closed with `gate_unavailable`.
        let escalation = matches!(outcome, Outcome::Escalated).then_some(Escalation {
            threshold,
            policy_id: policy.policy.id,
            laya_config_id: laya_config.id,
        });
        return Ok(Gate {
            judge: judge_doc,
            refusal: if escalation.is_some() {
                None
            } else {
                refusal_for(outcome, "gate_unavailable", "the laya judge did not decide")
            },
            escalation,
        });
    };

    let answer = if verdict.accepted { "accept" } else { "refuse" };
    let confidence = verdict.score.unwrap_or(0.0);
    let evaluated = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: "ladder.evaluate".to_owned(),
            input: json!({
                "kind": GATE_KIND,
                "gate": anatomy.gate,
                "project_id": run.project_id,
                "card_id": run.card_id,
                "question": judge::JUDGE_QUESTION,
                "answer": answer,
                "confidence": confidence,
                "input": {
                    "output": out.output,
                    "release_hash": binding.release_hash,
                },
                "latency_ms": latency_ms,
                "meta": {
                    "run_id": run.id,
                    "job_binding_id": binding.id,
                    "release_hash": binding.release_hash,
                    "draft_document_id": document_id,
                    "prompt_ref": out.prompt_ref,
                    "prompt_sha256": out.prompt_sha256,
                    "model_ref": anatomy.model_ref,
                },
            }),
            precondition: None,
            idempotency_key: Some(format!("{prefix}:decision")),
            approval_id: None,
            actor_id,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("internal_agent:{}:decision", run.id)),
            trace_id: Some(format!("run:{}", run.id)),
            depth: 1,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await;
    let (decision_id, outcome) = match evaluated {
        Ok(outcome) => {
            let output = outcome.body.get("output").cloned().unwrap_or(Value::Null);
            let decision_id = output.get("id").and_then(Value::as_str).map(str::to_owned);
            let recorded = output
                .get("outcome")
                .and_then(Value::as_str)
                .map(str::to_owned);
            (decision_id, recorded)
        }
        Err(error) => {
            // The verdict was readable but the ladder refused to record it: an infrastructure
            // fault, not a decision. Retry rather than invent an outcome.
            return Err(ExecutorFailure::retryable(format!(
                "ladder.evaluate: {}: {}",
                error.code, error.detail
            )));
        }
    };
    let outcome = match outcome.as_deref() {
        Some("accepted") => Outcome::Accepted,
        Some("refused") => Outcome::Refused,
        Some("escalated") => Outcome::Escalated,
        other => {
            return Err(ExecutorFailure::retryable(format!(
                "ladder.evaluate answered an unknown outcome {other:?}"
            )));
        }
    };
    let judge_doc = json!({
        "decision": outcome_str(outcome),
        "decision_id": decision_id,
        "answer": answer,
        "confidence": confidence,
        "threshold": threshold,
        "reason": verdict.reason,
        "policy_id": policy.policy.id.to_string(),
        "laya_config_id": laya_config.id.to_string(),
        "latency_ms": latency_ms,
    });
    let refusal = match outcome {
        Outcome::Accepted => None,
        Outcome::Refused => Some(Refusal {
            class: "laya_refused".to_owned(),
            detail: format!(
                "the gate refused the draft (confidence {confidence:.3} < threshold {threshold:.3})"
            ),
        }),
        Outcome::Escalated => None,
    };
    let escalation = matches!(outcome, Outcome::Escalated).then_some(Escalation {
        threshold,
        policy_id: policy.policy.id,
        laya_config_id: laya_config.id,
    });
    Ok(Gate {
        judge: judge_doc,
        refusal,
        escalation,
    })
}

fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Accepted => "accepted",
        Outcome::Refused => "refused",
        Outcome::Escalated => "escalated",
    }
}

fn refusal_for(outcome: Outcome, class: &str, detail: &str) -> Option<Refusal> {
    match outcome {
        Outcome::Accepted => None,
        Outcome::Refused | Outcome::Escalated => Some(Refusal {
            class: class.to_owned(),
            detail: detail.to_owned(),
        }),
    }
}
