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

/// The baseline's pinned system prompt. It is the direct pipeline: one model call with the
/// problem statement and the files the fix touches, answering in the same unified-diff contract
/// the conveyor arm's patch uses.
const BASELINE_SYSTEM: &str = "\
You are a software engineer fixing a bug in a repository. You are given the problem statement and \
the current content of the files the fix is expected to touch. Answer with a unified diff in \
`git diff` format only — no prose, no explanation, no code fence. If you cannot produce a patch, \
answer with the single word NONE.";
/// The baseline's one model call is bounded.
const BASELINE_TIMEOUT_MS: i64 = 300_000;
/// The source fetch is bounded, so a huge file cannot become the prompt.
const MAX_SOURCE_FILES: usize = 8;
const MAX_SOURCE_BYTES: usize = 64 * 1024;

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
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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
    if !matches!(run.arm.as_str(), "conveyor" | "baseline") {
        return Err(JobFailure::permanent(format!(
            "run {run_id} is arm {}; only the model arms are produced",
            run.arm
        )));
    }
    if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Ok(());
    }
    // A conveyor run started before the snapshot existed falls back to the agent row it names.
    // A baseline run never names an agent: its model is on the run options.
    let agent = if run.arm == "conveyor" && run.options.get("agent_snapshot").is_none() {
        let Some(agent_id) = run.capability_agent_id else {
            return Err(JobFailure::permanent(
                "a conveyor run must name the capability agent it measures",
            ));
        };
        Some(
            store::capability::get_agent_tx(&mut tx, tenant, agent_id)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
                .ok_or_else(|| JobFailure::permanent("the capability agent is gone"))?,
        )
    } else {
        None
    };
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
    let (request, bound_agent) = match run.arm.as_str() {
        "conveyor" => {
            let Some(agent_id) = run.capability_agent_id else {
                return Err(JobFailure::permanent(
                    "a conveyor run must name the capability agent it measures",
                ));
            };
            let (model, max_iterations, timeout_secs, bound) =
                match (run.options.get("agent_snapshot"), agent) {
                    (Some(snapshot), _) => {
                        let model = snapshot
                            .get("model_ref")
                            .and_then(Value::as_str)
                            .filter(|model| !model.trim().is_empty())
                            .ok_or_else(|| {
                                JobFailure::permanent("the pinned agent snapshot has no model_ref")
                            })?;
                        let max_iterations = snapshot
                            .get("max_iterations")
                            .and_then(Value::as_i64)
                            .ok_or_else(|| {
                                JobFailure::permanent(
                                    "the pinned agent snapshot has no max_iterations",
                                )
                            })?;
                        let timeout_secs = snapshot
                            .get("timeout_secs")
                            .and_then(Value::as_i64)
                            .ok_or_else(|| {
                                JobFailure::permanent(
                                    "the pinned agent snapshot has no timeout_secs",
                                )
                            })?;
                        (model.to_owned(), max_iterations, timeout_secs, false)
                    }
                    (None, Some(agent)) => (
                        agent.model_ref,
                        i64::from(agent.max_iterations),
                        i64::from(agent.timeout_secs),
                        true,
                    ),
                    (None, None) => {
                        return Err(JobFailure::permanent(
                            "the run has neither a pinned agent snapshot nor a live agent row",
                        ));
                    }
                };
            let mut request = json!({
                "model": model,
                "messages": [
                    { "role": "user", "content": format!("{}\n\n{}", INSTRUCTION, instance.problem_statement) }
                ],
                "tools": [],
                "max_turns": max_iterations,
                "timeout_ms": timeout_secs * 1000,
            });
            if bound {
                request["capability_agent_id"] = json!(agent_id);
            }
            (request, if bound { Some(agent_id) } else { None })
        }
        "baseline" => {
            let Some(model) = run
                .options
                .get("model_ref")
                .and_then(Value::as_str)
                .filter(|model| !model.trim().is_empty())
            else {
                return record(
                    pool,
                    tenant,
                    case_id,
                    "no_attempt",
                    None,
                    "the baseline run has no model_ref",
                )
                .await;
            };
            let source = match source_context(&instance).await {
                Ok(source) => source,
                Err(note) => {
                    return record(pool, tenant, case_id, "no_attempt", None, &note).await;
                }
            };
            let request = json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": BASELINE_SYSTEM },
                    {
                        "role": "user",
                        "content": format!(
                            "{}\n\n## Source files at the base commit\n\n{}",
                            instance.problem_statement, source
                        )
                    }
                ],
                "tools": [],
                "max_turns": 1,
                "timeout_ms": BASELINE_TIMEOUT_MS,
            });
            (request, None)
        }
        _ => unreachable!("validated above"),
    };
    let outcome = run_step(
        pool,
        tenant,
        run_id,
        None,
        bound_agent,
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

/// Fetch the instance's source files at its base commit for the baseline prompt.
///
/// The base is configurable so a test can serve the files without GitHub; production uses the
/// raw content host. A fetch failure is `no_attempt`, never a model failure.
async fn source_context(instance: &store::benchmark::ProducerInstance) -> Result<String, String> {
    let base = std::env::var("BENCHMARK_SOURCE_BASE")
        .unwrap_or_else(|_| "https://raw.githubusercontent.com".to_owned());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("the source client: {error}"))?;
    let mut files = Vec::new();
    for path in instance.oracle_files.iter().take(MAX_SOURCE_FILES) {
        let url = format!(
            "{}/{}/{}/{}",
            base.trim_end_matches('/'),
            instance.repo,
            instance.base_commit,
            path.trim_start_matches('/')
        );
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("fetch {path}: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("fetch {path}: {}", response.status()));
        }
        let text = response
            .text()
            .await
            .map_err(|error| format!("read {path}: {error}"))?;
        if text.len() > MAX_SOURCE_BYTES {
            return Err(format!("{path} is over {MAX_SOURCE_BYTES} bytes"));
        }
        files.push(format!("### {path}\n```\n{text}\n```"));
    }
    if files.is_empty() {
        Ok(String::from("(the instance names no source files)"))
    } else {
        Ok(files.join("\n\n"))
    }
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
