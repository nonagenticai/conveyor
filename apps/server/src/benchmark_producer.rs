#![forbid(unsafe_code)]
//! `benchmark.case.produce` — the `conveyor` arm's producer (S18 step 1).
//!
//! **One job, one case, one `agent.run`.** The driver's tick may not make a model call: a tick is
//! bounded and re-delivered, while a run is minutes long. So the driver enqueues one producer per
//! case (deduped by the case id) and this job is the only thing that talks to the model.
//!
//! **The producer is a consumer of the public surface** (S18 §3). It reads the instance through
//! `producer_instances_tx`, which carries no `gold_patch`, no `test_patch`, no `fail_to_pass` — the
//! answer key is never in the model's context — and it drives `agent.run`'s one loop through the
//! same `run_step` seam a procedure or workflow step uses, bounded by the agent row's caps.
//! ⛔ The arm is deliberately **tool-less** and sends no `prompt_ref` (a benchmark run has no run
//! facts to render one): it measures the model over the problem statement, not a checkout and not
//! the agent's configured prompt.
//!
//! **A refusal is not a model failure.** CONTRACT §19: an outage, an unready binding or an empty
//! reply is written as `produce_status = 'no_attempt'`, which the graded denominator withholds.
//! Only a genuine infrastructure fault (a 5xx, a transport error) is a retryable job failure.

use conveyor_actions::workflow::agent::run_step;
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::benchmark::MAX_PATCH_BYTES;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// The instruction the agent answers. The reply **is** the patch: there is no second parser, and
/// the sandbox applies it exactly as it applies a stored gold patch.
const INSTRUCTION: &str = "\
You are fixing a bug in a repository you cannot see. Answer with a unified diff (`git diff` \
format) only and nothing else — no prose, no explanation, no code fence. If you cannot produce a \
patch, answer with the single word NONE.";

fn want_uuid(payload: &Value, field: &str) -> Result<Uuid, JobFailure> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent(format!("the job payload has no {field}")))
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Produce one case's patch, then write it to the case.
pub async fn produce(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let run_id = want_uuid(payload, "run_id")?;
    let instance_id = payload
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or_else(|| JobFailure::permanent("the job payload has no instance_id"))?;

    // ── 1. the run, the case and the subject, in one short scoped transaction ───────────
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let run = store::benchmark::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    if run.arm != "conveyor" {
        return Err(JobFailure::permanent(format!(
            "run {run_id} is arm {}; only a conveyor run is produced",
            run.arm
        )));
    }
    if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Ok(());
    }
    let Some(agent_id) = run.capability_agent_id else {
        return Err(JobFailure::permanent(
            "a conveyor run must name the capability agent it measures",
        ));
    };
    let agent = store::capability::get_agent_tx(&mut tx, tenant, agent_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
        .ok_or_else(|| JobFailure::permanent("the capability agent is gone"))?;
    let Some(instance) = store::benchmark::producer_instances_tx(
        &mut tx,
        tenant,
        run.suite_id,
        Some(&[instance_id.to_owned()]),
    )
    .await
    .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
    .into_iter()
    .next() else {
        return Err(JobFailure::permanent(format!(
            "instance {instance_id} is not in the run's suite"
        )));
    };
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(instance_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| JobFailure::permanent(format!("the case row: {error}")))?;
    // Spend one attempt now, so a producer that dies mid-call still consumes its budget and the
    // driver can bound the run (a re-delivered job cannot loop for ever).
    store::benchmark::begin_produce_tx(&mut tx, tenant, case_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    // ── 2. the model, through the ordinary engine ───────────────────────────────────────
    //
    // `agent.run`'s handler refuses a direct invocation by construction (it holds the
    // invocation transaction and no pool), so the producer drives the one seam the step
    // executors drive: `run_step`, with an explicit step input. The input is deliberately
    // tool-less, and it carries the agent row's caps so the loop is bounded by the
    // configuration the run pinned. `prompt_ref` is not sent: a benchmark run has no
    // run/procedure/capability facts, so the catalogue's `agent@1` cannot be rendered, and
    // the inference ledger must not claim a prompt the loop never applied.
    let request = json!({
        "model": agent.model_ref,
        "messages": [
            { "role": "user", "content": format!("{}\n\n{}", INSTRUCTION, instance.problem_statement) }
        ],
        "tools": [],
        "max_turns": agent.max_iterations,
        "timeout_ms": i64::from(agent.timeout_secs) * 1000,
        "capability_agent_id": agent_id,
    });
    let outcome = run_step(
        pool,
        tenant,
        run_id,
        None,
        Some(agent_id),
        app_key,
        Some(&format!("benchmark.produce:{case_id}")),
        1,
        &request,
    )
    .await;

    let out = match outcome {
        Ok(out) => out,
        // A transport-level fault: the call may or may not have happened, so retry.
        Err(error) if error.status >= 500 => {
            return Err(JobFailure::retryable(format!("agent.run: {error}")));
        }
        // A typed refusal (an unready binding, a scope, a cap): no model call happened, so this
        // is `no_attempt`, never a model failure and never a retried job.
        Err(error) => {
            return record(
                pool,
                tenant,
                case_id,
                "no_attempt",
                None,
                &error.to_string(),
            )
            .await;
        }
    };

    let text = out.output.trim().to_owned();
    if text.is_empty() || text == "NONE" {
        return record(
            pool,
            tenant,
            case_id,
            "no_attempt",
            None,
            "the agent produced no patch",
        )
        .await;
    }
    if text.len() > MAX_PATCH_BYTES {
        return record(
            pool,
            tenant,
            case_id,
            "no_attempt",
            None,
            &format!("the produced patch is {} bytes, over the cap", text.len()),
        )
        .await;
    }
    let hash = sha256_hex(&text);
    record(
        pool,
        tenant,
        case_id,
        "ok",
        Some((text.as_str(), hash.as_str())),
        "",
    )
    .await
}

/// Write the produced outcome and finish the job.
async fn record(
    pool: &PgPool,
    tenant: TenantId,
    case_id: Uuid,
    produce_status: &str,
    patch: Option<(&str, &str)>,
    note: &str,
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    store::benchmark::record_produced_tx(
        &mut tx,
        tenant,
        case_id,
        produce_status,
        patch.map(|(text, _)| text),
        patch.map(|(_, hash)| hash),
    )
    .await
    .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tracing::info!(case = %case_id, produce_status, note, "benchmark case produced");
    Ok(())
}
