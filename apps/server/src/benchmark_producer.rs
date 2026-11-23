#![forbid(unsafe_code)]
//! `benchmark.case.produce` — the produced arms (S18 step 1 and P3c).
//!
//! **One job, one case.** The driver's tick may not make a model call or start a process: a tick
//! is bounded and re-delivered, while either is minutes long. So the driver enqueues one producer
//! per case (deduped by the case id) and this job is the only thing that produces.
//!
//! **Two paths.** The `conveyor` and `baseline` model arms drive one `agent.run` inline. The
//! `process` subject is card-scoped (P3c): the producer hosts the run's cards on its own
//! project/board/list, creates the case's `benchmark` card, starts the published process the
//! benchmark run pinned, and waits across ticks — reading the patch from the run's typed output
//! is P3d's, not this module's, job.
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

use conveyor_actions::invoke::{ActionError, Authority, InvokeRequest, invoke};
use conveyor_actions::registry::registry;
use conveyor_actions::workflow::agent::run_step;
use conveyor_auth::Role;
use conveyor_domain::{ListKind, TenantId};
use conveyor_store as store;
use conveyor_store::benchmark::MAX_PATCH_BYTES;
use conveyor_store::project::NewProject;
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
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let run = store::benchmark::get_run_tx(&mut tx, tenant, run_id)
        .await
        .map_err(db)?;
    if !matches!(run.arm.as_str(), "conveyor" | "baseline") {
        return Err(JobFailure::permanent(format!(
            "run {run_id} is arm {}; only the model arms are produced",
            run.arm
        )));
    }
    if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Ok(());
    }
    let Some(instance) = store::benchmark::producer_instances_tx(
        &mut tx,
        tenant,
        run.suite_id,
        Some(&[instance_id.to_owned()]),
    )
    .await
    .map_err(db)?
    .into_iter()
    .next() else {
        return Err(JobFailure::permanent(format!(
            "instance {instance_id} is not in the run's suite"
        )));
    };
    let (case_id, produce_status): (Uuid, String) = sqlx::query_as(
        "SELECT id, produce_status FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(instance_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| JobFailure::permanent(format!("the case row: {error}")))?;
    // A re-delivered job (or P3d's collector) may have finished the case already.
    if produce_status != "pending" {
        return Ok(());
    }
    // The process arm is card-scoped (S18 P3c): it starts a published process on a card and
    // waits across ticks, so its attempt is spent by that path when it starts — never here,
    // where a wait would consume the budget.
    if run.subject_kind == "process" {
        tx.commit().await.map_err(db)?;
        return produce_process(pool, tenant, app_key, &run, &instance, case_id).await;
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
                .map_err(db)?
                .ok_or_else(|| JobFailure::permanent("the capability agent is gone"))?,
        )
    } else {
        None
    };
    // Spend one attempt now, so a producer that dies mid-call still consumes its budget and the
    // driver can bound the run (a re-delivered job cannot loop for ever).
    store::benchmark::begin_produce_tx(&mut tx, tenant, case_id)
        .await
        .map_err(db)?;
    tx.commit().await.map_err(db)?;

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

/// The process arm (S18 P3c): one case is one `benchmark` card and one run of the process the
/// benchmark run pinned.
///
/// **The card is the case's home.** The process subjects execute through `process_runs.start`,
/// which needs a card on a board; the run's cards live on one host project/board/list keyed by
/// the benchmark run, so a re-delivered tick finds the same host and the same card.
///
/// **Waiting is free.** A live process is a tick that returns: the driver re-enqueues the next
/// tick (the job dedupe key only holds a live producer) and the case's attempt budget is left
/// untouched. The attempt is spent exactly once, on the tick that finds no process yet and
/// starts it — so a broken start is bounded, and a process that runs for minutes is not.
///
/// **Collection is P3d's.** A terminal process writes nothing here; the collector reads the
/// run's typed output and records the case.
async fn produce_process(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    run: &store::benchmark::Run,
    instance: &store::benchmark::ProducerInstance,
    case_id: Uuid,
) -> Result<(), JobFailure> {
    let Some(process_id) = run.process_id else {
        return Err(JobFailure::permanent(
            "a process run must name the pinned swebench-fix process",
        ));
    };

    // ── 1. the host, the case's card, and this case's process run ────────────────────────
    let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
    let (board_id, list_id) = host_tx(&mut tx, tenant, run).await?;
    let existing =
        card_for_case_tx(&mut tx, tenant, board_id, list_id, &instance.instance_id).await?;
    if let Some(card_id) = existing
        && let Some(state) = root_run_status_tx(&mut *tx, tenant, card_id).await?
    {
        // Live: wait for the next tick. Terminal: P3d collects; either way nothing is spent.
        tracing::debug!(case = %case_id, %card_id, state, "benchmark process arm waits");
        tx.commit().await.map_err(db)?;
        return Ok(());
    }
    // The case was produced between the caller's read and here: never start a second process.
    let produce_status: String = sqlx::query_scalar(
        "SELECT produce_status FROM benchmark_case WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(case_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    if produce_status != "pending" {
        return Ok(());
    }
    // No process exists yet, so this tick is the one that produces: spend the one attempt.
    store::benchmark::begin_produce_tx(&mut tx, tenant, case_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    tx.commit().await.map_err(db)?;

    // ── 2. the card, through the same public action a person calls ───────────────────────
    let card_id = match existing {
        Some(card_id) => card_id,
        None => {
            let card = invoke_action(
                pool,
                tenant,
                app_key,
                "cards.create",
                json!({
                    "board_id": board_id,
                    "list_id": list_id,
                    "name": instance.instance_id,
                    // The model's only material: the card facts reach the compiled prompt, so
                    // the problem statement goes here and the answer key never does.
                    "description": instance.problem_statement,
                    "card_type_code": "benchmark",
                    "meta": {
                        "instance_id": instance.instance_id,
                        "base_commit": instance.base_commit,
                        "benchmark_run_id": run.id,
                        "benchmark_case_id": case_id,
                    },
                }),
            )
            .await;
            match card {
                Ok(card) => card
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|raw| Uuid::parse_str(raw).ok())
                    .ok_or_else(|| JobFailure::permanent("cards.create returned no card id"))?,
                Err(error) if error.status >= 500 => {
                    return Err(JobFailure::retryable(format!("cards.create: {error}")));
                }
                Err(error) => {
                    return record(
                        pool,
                        tenant,
                        case_id,
                        "no_attempt",
                        None,
                        &format!("cards.create: {error}"),
                    )
                    .await;
                }
            }
        }
    };

    // ── 3. the process, pinned to the version the benchmark run measured ─────────────────
    let started = invoke_action(
        pool,
        tenant,
        app_key,
        "process_runs.start",
        json!({"card_id": card_id, "process_id": process_id, "inputs": {}}),
    )
    .await;
    match started {
        Ok(_) => Ok(()),
        Err(error) if error.status >= 500 => Err(JobFailure::retryable(format!(
            "process_runs.start: {error}"
        ))),
        Err(error) => {
            // `uq_run_live` may have lost the card's slot to a concurrent start: wait, never
            // record a refusal for a process that did start.
            let mut tx = store::begin_scoped(pool, tenant).await.map_err(db)?;
            let running = root_run_status_tx(&mut *tx, tenant, card_id).await?;
            tx.commit().await.map_err(db)?;
            if running.is_some() {
                return Ok(());
            }
            record(
                pool,
                tenant,
                case_id,
                "no_attempt",
                None,
                &format!("process_runs.start: {error}"),
            )
            .await
        }
    }
}

/// The host project/board/list one benchmark run's cards live on.
///
/// The project key is the run id, so every tick of every case of one run resolves the same
/// host, and a different run can never share it. Concurrent producer jobs (one per case) take
/// a transaction-scoped advisory lock on the key, so the first-create is a one-winner race and
/// the loser reads the winner's rows.
async fn host_tx(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    run: &store::benchmark::Run,
) -> Result<(Uuid, Uuid), JobFailure> {
    let key = format!("benchmark-{}", run.id);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&key)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let project = match store::project::get_project_by_key_tx(&mut *tx, tenant, &key)
        .await
        .map_err(db)?
    {
        Some(project) => project,
        None => store::project::insert_project_tx(
            &mut *tx,
            tenant,
            &NewProject::new(key, format!("Benchmark {}", run.label)),
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?,
    };
    let board = match store::board::list_boards_tx(&mut *tx, tenant, project.id)
        .await
        .map_err(db)?
        .into_iter()
        .next()
    {
        Some(board) => board,
        None => store::board::insert_board_tx(
            &mut *tx,
            tenant,
            &store::board::NewBoard::new(project.id, "Benchmark"),
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?,
    };
    let list = match store::list::list_of_kind_tx(&mut *tx, tenant, board.id, ListKind::Intake)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
    {
        Some(record) => record.list,
        None => store::list::insert_list_tx(
            &mut *tx,
            tenant,
            &store::list::NewList::new(
                board.id,
                "Cases",
                rust_decimal::Decimal::ZERO,
                ListKind::Intake,
            ),
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?,
    };
    Ok((board.id, list.id))
}

/// The case's card on the run's host list, matched by the instance it pins.
async fn card_for_case_tx(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    list_id: Uuid,
    instance_id: &str,
) -> Result<Option<Uuid>, JobFailure> {
    sqlx::query_scalar(
        "SELECT id FROM card
          WHERE tenant_id = $1 AND board_id = $2 AND list_id = $3
            AND archived_at IS NULL AND meta->>'instance_id' = $4
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(list_id)
    .bind(instance_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db)
}

/// The card's latest ROOT run status, when it has one.
///
/// Root-only: a child run shares its parent's card, so it must never be mistaken for the
/// process run the producer started.
async fn root_run_status_tx(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<String>, JobFailure> {
    sqlx::query_scalar(
        "SELECT status FROM run
          WHERE tenant_id = $1 AND card_id = $2 AND parent_run_id IS NULL
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db)
}

/// Invoke one action as the engine's service principal and return its output document.
async fn invoke_action(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    action: &str,
    input: Value,
) -> Result<Value, ActionError> {
    let outcome = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            approval_id: None,
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("benchmark.produce:{action}")),
            trace_id: None,
            depth: 1,
            app_key: app_key.to_owned(),
            usage: None,
        },
    )
    .await?;
    if !(200..300).contains(&outcome.status) {
        return Err(ActionError::new(
            outcome.status,
            "action_not_ok",
            format!("{action} answered {}", outcome.status),
        ));
    }
    Ok(outcome.body.get("output").cloned().unwrap_or(outcome.body))
}

/// A database read/write failed: retryable, never a dead letter.
fn db(error: sqlx::Error) -> JobFailure {
    JobFailure::retryable(format!("database: {error}"))
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
