#![forbid(unsafe_code)]
//! The case driver — one `(case, rep)` end to end (S19 Part B steps 8–12; CONTRACT §20).
//!
//! One job, one slot. The four properties the slice names are enforced here rather than
//! documented here:
//!
//! 1. **`deadline_at` is a total ceiling** ([`CaseDeadline`]), fixed when the attempt starts.
//! 2. **Jittered backoff with the attempt visible** — the returned [`JobFailure`] carries the
//!    eval's own [`RetryPolicy::backoff_jittered`], and the attempt number is written to
//!    `eval_attempt_error` before the job is retried.
//! 3. **Strict-per-attempt scoring** — one attempt's numbers reach the row.
//! 4. **A failure class on every failed attempt** — [`FailureClass`], by whose fault it was.
//!
//! Two rules are structural rather than careful:
//!
//! - **A failed attempt never writes the slot.** The result row is only written once a scorable
//!   output and its grade exist; every other path writes `eval_attempt_error` and leaves the
//!   `(case, rep)` slot empty, so a resume re-runs exactly the attempts that produced nothing.
//! - **The answer key is read after the call, by the grader.** The case is loaded with
//!   [`get_case_tx`], whose row type has no `expected`; the key arrives separately, once the
//!   subject has already answered.
//!
//! The subject is invoked through the ordinary engine as a **service** principal holding
//! `KataExecutor` — the one tenant-admin role that deliberately lacks `evals:approve`. An eval
//! whose `entry_action` were a sign-off action is therefore refused by the same predicate that
//! stops any machine from signing, not by a rule written here.

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use conveyor_actions::eval::runs::{CASE_JOB_KIND, REGRADE_JOB_KIND};
use conveyor_actions::invoke::{Authority, HandlerCtx, InvokeRequest, UsageContext, invoke};
use conveyor_actions::registry::registry;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::eval::{
    CaseDeadline, FailureClass, GraderKind, RetryPolicy, SubjectInput, grade_programmatic,
    served_model_matches, subject_request,
};
use conveyor_domain::inference::CallerKind;
use conveyor_store as store;
use conveyor_store::eval::{NewAttemptError, NewCaseResult, NewTrace};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// The wall-clock ceiling for one attempt.
///
/// ⛔ A **total** ceiling, not an inactivity timer: a stream that emits keepalives forever still
/// expires when it was always going to. The action's own `timeout_ms` bounds the engine call
/// inside it; this bounds the whole attempt, including grading.
pub const EVAL_CASE_TIMEOUT: Duration = Duration::from_secs(300);

/// How many attempts a slot gets.
/// The default policy is 5 attempts; the literal is here because `default()` is not `const`.
const MAX_ATTEMPTS: u32 = 5;

/// Run one `(case, rep)` of one run.
///
/// Returns `Ok(())` when the slot is filled, already filled, or the run is no longer running —
/// the job succeeded in every one of those cases. A failure is a [`JobFailure`] carrying the
/// class's message and the eval's own jittered backoff.
pub async fn run_case(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<(), JobFailure> {
    let attempt = next_attempt(pool, tenant, run_id, case_id, rep).await?;

    // ── read the slot, before anything is called ────────────────────────────────────────
    let (run, eval, case) = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let run = store::eval::get_run_tx(&mut tx, tenant, run_id)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        let eval = store::eval::get_eval_tx(&mut tx, tenant, run.eval_id)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        let case = store::eval::get_case_tx(&mut tx, tenant, case_id)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        (run, eval, case)
    };

    // A cancelled or finished run drains its queued jobs as no-ops.
    if run.status.stops_cases() {
        tracing::debug!(run = %run_id, "the run is no longer accepting cases");
        return Ok(());
    }
    if already_filled(pool, tenant, run_id, case_id, rep).await? {
        return Ok(());
    }

    let deadline = CaseDeadline::starting(Utc::now(), EVAL_CASE_TIMEOUT);
    let started = std::time::Instant::now();

    // ── build the request from the input alone ─────────────────────────────────────────
    let request = match subject_request(
        &SubjectInput { input: &case.input },
        &eval.inputs_schema,
        eval.model_ref.as_deref(),
    ) {
        Ok(request) => request,
        Err(error) => {
            return fail(
                pool,
                tenant,
                &Slot {
                    run_id,
                    case_id,
                    rep,
                    attempt,
                },
                FailureClass::Harness,
                error.to_string(),
                None,
                None,
                None,
                deadline,
            )
            .await;
        }
    };

    // ── invoke the subject through the ordinary engine ─────────────────────────────────
    let outcome = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: eval.entry_action.clone(),
            input: request.clone(),
            precondition: None,
            idempotency_key: Some(format!("eval.case.run:{run_id}:{case_id}:{rep}")),
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(format!("eval.case.run:{run_id}:{case_id}:{rep}")),
            trace_id: Some(format!("eval:{run_id}")),
            depth: 1,
            app_key: app_key.to_owned(),
            // D23's amendment (S19 §9): an eval's model call is attributed to the eval run, so the
            // ledger and `inference_usage.report` see the instrument that is measuring everything
            // else. The engine records it only when the subject action captures usage.
            usage: Some(UsageContext {
                caller_kind: CallerKind::Eval,
                run_id: None,
                run_step_id: None,
                conversation_turn_id: None,
                eval_run_id: Some(run_id),
                prompt_ref: Some(format!("eval:{}", eval.entry_action)),
                prompt_version: None,
                correlation_id: None,
            }),
        },
    )
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            let class = FailureClass::from_status(error.status);
            return fail(
                pool,
                tenant,
                &Slot {
                    run_id,
                    case_id,
                    rep,
                    attempt,
                },
                class,
                format!("{}: {}", error.code, error.detail),
                None,
                None,
                None,
                deadline,
            )
            .await;
        }
    };
    if !(200..300).contains(&outcome.status) {
        let class = FailureClass::from_status(outcome.status);
        return fail(
            pool,
            tenant,
            &Slot {
                run_id,
                case_id,
                rep,
                attempt,
            },
            class,
            format!("the subject answered {}", outcome.status),
            None,
            None,
            None,
            deadline,
        )
        .await;
    }

    let output = outcome.body.get("output").cloned().unwrap_or(Value::Null);

    // ⛔ The model comes from the RESPONSE, never from the config.
    let served = output
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    // One let-chain, not a nested `if`: the config names a model AND the response names another.
    // The bindings here shadow the outer `served` only for this block, so the `let Some(served)`
    // below still reads the response's own value.
    if let (Some(configured), Some(served)) = (eval.model_ref.as_deref(), served.as_deref())
        && !served_model_matches(configured, served)
    {
        return fail(
            pool,
            tenant,
            &Slot {
                run_id,
                case_id,
                rep,
                attempt,
            },
            FailureClass::ServedModelMismatch,
            format!("the response was served by {served:?}, not {configured:?}"),
            Some(served),
            None,
            None,
            deadline,
        )
        .await;
    }
    let Some(served) = served else {
        return fail(
            pool,
            tenant,
            &Slot {
                run_id,
                case_id,
                rep,
                attempt,
            },
            FailureClass::Harness,
            "the response names no model, so the row cannot say what produced it".to_owned(),
            None,
            None,
            None,
            deadline,
        )
        .await;
    };

    if deadline.is_expired(Utc::now()) {
        return fail(
            pool,
            tenant,
            &Slot {
                run_id,
                case_id,
                rep,
                attempt,
            },
            FailureClass::Timeout,
            format!(
                "the attempt ran past its {}s ceiling",
                EVAL_CASE_TIMEOUT.as_secs()
            ),
            Some(&served),
            None,
            Some(deadline),
            deadline,
        )
        .await;
    }

    // ── grade, reading the answer key only now ─────────────────────────────────────────
    //
    // The loop lives in [`grade_and_record`] so that a regrade — which grades a *stored* output
    // against the eval's current version — runs the same four grader kinds and writes the same
    // two rows. This call is the only path a fresh measurement takes.
    let turns = synthesise_turns(&request, &output);
    let usage = output.get("usage").cloned().unwrap_or_else(|| json!({}));
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    grade_and_record(
        pool,
        app_key,
        tenant,
        GradeRequest {
            run_id,
            case_id,
            rep,
            attempt,
            version_id: run.version_id,
            case_input: &case.input,
            output: &output,
            turns: &turns,
            model: &served,
            stop_reason: output.get("stop_reason").and_then(Value::as_str),
            usage: &usage,
            latency_ms,
            tool_calls: count_tool_calls(&output),
            action_invocation_id: outcome.invocation_id,
            subject_run_id: output
                .get("run_id")
                .and_then(Value::as_str)
                .and_then(parse_uuid),
            action: CASE_JOB_KIND,
        },
        deadline,
    )
    .await
}

/// Everything one slot needs in order to be graded and written.
///
/// The fields that differ between a measurement and a regrade are **provenance**, not behaviour:
/// a regrade's [`output`](Self::output) is the one the source run captured, its
/// [`usage`](Self::usage) is empty (the subject call was already billed on the source run, and
/// spend is summed from these rows), and its [`action`](Self::action) names the job that wrote it.
struct GradeRequest<'a> {
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    attempt: i32,
    /// The instrument whose metrics grade this slot.
    version_id: Uuid,
    /// The case's input, as a judge prompt sees it.
    case_input: &'a str,
    /// The subject's output — the value every grader reads.
    output: &'a Value,
    /// The transcript stored for this slot.
    turns: &'a Value,
    /// `eval_case_result.model`; the column is NOT NULL.
    model: &'a str,
    /// Why the subject stopped, when it said.
    stop_reason: Option<&'a str>,
    usage: &'a Value,
    latency_ms: i32,
    tool_calls: i32,
    action_invocation_id: Option<Uuid>,
    subject_run_id: Option<Uuid>,
    /// `eval.case.run` or `eval.regrade`.
    action: &'static str,
}

/// Grade one slot and write its trace, its result and its `eval_result.recorded` event.
///
/// Every path that cannot produce a scorable grade goes through [`fail`], which appends an
/// `eval_attempt_error` and leaves the slot empty — a failed attempt never writes the result row,
/// so a resume re-runs exactly the slots that produced nothing.
async fn grade_and_record(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    request: GradeRequest<'_>,
    deadline: CaseDeadline,
) -> Result<(), JobFailure> {
    let GradeRequest {
        run_id,
        case_id,
        rep,
        attempt,
        version_id,
        case_input,
        output,
        turns,
        model: served,
        stop_reason,
        usage,
        latency_ms,
        tool_calls,
        action_invocation_id,
        subject_run_id,
        action,
    } = request;

    let metrics = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        store::eval::list_metrics_tx(&mut tx, tenant, version_id)
            .await
            .map_err(|error| permanent(error.to_string()))?
    };

    // The answer key is read after the subject answered, and once — not once per metric. A
    // metric that needs no key never sees it.
    let expected = read_expected(pool, tenant, case_id).await?;
    let grades = match grade_metrics(
        pool,
        app_key,
        tenant,
        &metrics,
        case_input,
        expected.as_ref(),
        output,
        Some(served),
        action,
        &format!("eval.judge:{run_id}:{case_id}:{rep}"),
        &format!("eval:{run_id}"),
    )
    .await
    {
        Ok(grades) => grades,
        Err(failure) => {
            return fail(
                pool,
                tenant,
                &Slot {
                    run_id,
                    case_id,
                    rep,
                    attempt,
                },
                failure.class,
                failure.message,
                Some(served),
                None,
                None,
                deadline,
            )
            .await;
        }
    };
    let MetricGrades {
        grade,
        detail: explanation,
        pending_label,
        judge_model,
        judge_usage,
    } = grades;

    // ── write the slot: trace and result together, in one transaction ──────────────────
    let status = if pending_label { "pending_label" } else { "ok" };
    let grade = Value::Object(grade);

    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    store::eval::insert_trace_tx(
        &mut tx,
        tenant,
        &NewTrace {
            eval_run_id: run_id,
            case_id,
            rep,
            turns,
        },
    )
    .await
    .map_err(|error| permanent(format!("the transcript was refused: {error}")))?;
    let result_id = store::eval::insert_case_result_tx(
        &mut tx,
        tenant,
        &NewCaseResult {
            eval_run_id: run_id,
            case_id,
            rep,
            status,
            stop_reason,
            grade: &grade,
            explanation: &Value::Object(explanation),
            model: served,
            usage,
            latency_ms,
            tool_calls,
            action_invocation_id,
            subject_run_id,
            judge_model: judge_model.as_deref(),
            judge_usage: judge_usage.as_ref(),
            // A human label is a person's judgement of one result; a regrade writes
            // `pending_label` and a person labels the new row (`eval_results.label`).
            human_score: None,
        },
    )
    .await
    .map_err(|error| permanent(format!("the result was refused: {error}")))?;

    // `eval_result.recorded` rides the same transaction as the row it announces.
    let ctx = service_ctx(tenant, app_key, action);
    let _ = conveyor_actions::board::events::append_raw_event(
        &mut tx,
        &ctx,
        &conveyor_actions::board::events::RawEvent {
            topic: format!("eval_run:{run_id}"),
            board_id: None,
            aggregate_type: "eval_run",
            aggregate_id: run_id,
            event_type: conveyor_actions::eval::EVAL_RESULT_RECORDED,
            payload: json!({"result_id": result_id, "case_id": case_id, "rep": rep, "status": status}),
            data: json!({"result_id": result_id, "status": status}),
            prev_data: Value::Null,
        },
    )
    .await;

    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    tracing::info!(run = %run_id, case = %case_id, rep, action, score = ?grade, "eval case recorded");
    Ok(())
}

/// What one pass of the grader loop produced: the two maps the rows carry, and the judge's
/// provenance.
pub(crate) struct MetricGrades {
    /// `{metric_key: score}` — the object `eval_case_result.grade` / `eval_grade.grade` holds.
    pub grade: serde_json::Map<String, Value>,
    /// `{metric_key: detail}` — the grader's per-metric evidence.
    pub detail: serde_json::Map<String, Value>,
    /// A `human` metric parked the row for a person's label.
    pub pending_label: bool,
    /// The judge's model, when a judge graded a metric (the last one wins).
    pub judge_model: Option<String>,
    /// The judge's token usage.
    pub judge_usage: Option<Value>,
}

/// Why the grader loop could not produce a complete grade.
#[derive(Clone, Debug)]
pub(crate) struct GradeFailure {
    /// Whose fault it was — a `grader` failure is not retried, a `serving` one is.
    pub class: FailureClass,
    /// The metric and the reason, in prose (`judge_is_subject` is carried verbatim).
    pub message: String,
}

impl GradeFailure {
    fn grader(message: String) -> Self {
        Self {
            class: FailureClass::Grader,
            message,
        }
    }
}

/// Grade one output against every metric of one instrument.
///
/// ⭐ **One loop, two callers.** [`grade_and_record`] (a run's `(case, rep)` slot) and the
/// `eval.grade` job (a process step's one output, migration 0036) must never disagree about what
/// a grade is, so the four grader kinds, the answer-key rule and the judge guard live here and
/// both callers own only their failure policy: a case appends an `eval_attempt_error` and
/// retries, a one-shot grade writes `failed` on its own row.
///
/// The judge is invoked through the ordinary engine **outside** the caller's transaction — a
/// model call inside it would hold row locks for the length of an inference.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn grade_metrics(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    metrics: &[store::eval::EvalMetric],
    case_input: &str,
    expected: Option<&Value>,
    output: &Value,
    subject_model: Option<&str>,
    action: &str,
    judge_request_id: &str,
    judge_trace_id: &str,
) -> Result<MetricGrades, GradeFailure> {
    let mut grades = MetricGrades {
        grade: serde_json::Map::new(),
        detail: serde_json::Map::new(),
        pending_label: false,
        judge_model: None,
        judge_usage: None,
    };

    for metric in metrics {
        let kind = GraderKind::from_str(&metric.grader_kind).unwrap_or(GraderKind::None);
        match kind {
            GraderKind::None => continue,
            GraderKind::Human => {
                grades.pending_label = true;
            }
            GraderKind::Programmatic => {
                match grade_programmatic(&metric.key, &metric.grader, expected, output) {
                    Ok(scored) => {
                        grades.grade.insert(metric.key.clone(), json!(scored.score));
                        grades.detail.insert(metric.key.clone(), scored.detail);
                    }
                    Err(error) => {
                        return Err(GradeFailure::grader(format!("{}: {error}", metric.key)));
                    }
                }
            }
            judge @ (GraderKind::Rubric | GraderKind::Pairwise) => {
                let Some(model) = conveyor_actions::eval::graders::judge_model(&metric.grader)
                else {
                    return Err(GradeFailure::grader(format!(
                        "the metric {:?} calls a judge but names no judge_model",
                        metric.key
                    )));
                };
                if let Err(error) = conveyor_actions::eval::graders::assert_judge_is_not_subject(
                    subject_model,
                    Some(model),
                    action,
                ) {
                    // The machine code is carried into the evidence, not only the prose:
                    // `judge_is_subject` is what an operator greps for, and a message that says
                    // "the same model" in words alone is not a code.
                    return Err(GradeFailure::grader(format!(
                        "{}: {}: {}",
                        metric.key, error.code, error.detail
                    )));
                }
                let request = match conveyor_actions::eval::graders::judge_request(
                    judge,
                    &metric.key,
                    metric.rubric.as_deref(),
                    case_input,
                    expected,
                    output,
                    model,
                ) {
                    Ok(request) => request,
                    Err(error) => {
                        return Err(GradeFailure::grader(format!(
                            "{}: {}",
                            metric.key, error.detail
                        )));
                    }
                };
                match invoke_judge(
                    pool,
                    app_key,
                    tenant,
                    request,
                    judge_request_id,
                    judge_trace_id,
                )
                .await
                {
                    Ok(body) => {
                        grades.judge_model = Some(model.to_owned());
                        grades.judge_usage = body.get("usage").cloned();
                        match conveyor_actions::eval::graders::grade_from_judge(&body) {
                            Ok(scored) => {
                                grades.grade.insert(metric.key.clone(), json!(scored.score));
                                grades.detail.insert(metric.key.clone(), scored.detail);
                            }
                            Err(error) => {
                                return Err(GradeFailure::grader(format!(
                                    "{}: {error}",
                                    metric.key
                                )));
                            }
                        }
                    }
                    Err(class) => {
                        return Err(GradeFailure {
                            class,
                            message: format!("{}: the judge call failed", metric.key),
                        });
                    }
                }
            }
        }
    }

    Ok(grades)
}

/// Regrade one **stored** slot under the target run's version.
///
/// The output is the one the source run captured, never a reconstruction: a slot with no
/// transcript, or with a transcript written before output capture, is skipped. The caller has
/// already counted those out (`not_regradable`) before enqueueing, so one arriving here means a
/// slot changed under us — it finishes quietly rather than grading a guess.
///
/// Nothing is re-invoked: no pipeline call, no subject spend. The only new cost is a judge call,
/// recorded in `judge_usage`.
#[allow(clippy::too_many_arguments)]
pub async fn regrade_case(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    source_run_id: Uuid,
    source_case_id: Uuid,
) -> Result<(), JobFailure> {
    let attempt = next_attempt(pool, tenant, run_id, case_id, rep).await?;

    let (run, case, source, trace) = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let run = store::eval::get_run_tx(&mut tx, tenant, run_id)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        let case = store::eval::get_case_tx(&mut tx, tenant, case_id)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        let source =
            store::eval::get_result_tx(&mut tx, tenant, source_run_id, source_case_id, rep)
                .await
                .map_err(|error| permanent(error.to_string()))?;
        let trace = store::eval::get_trace_tx(&mut tx, tenant, source_run_id, source_case_id, rep)
            .await
            .map_err(|error| permanent(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        (run, case, source, trace)
    };

    if run.status.stops_cases() {
        tracing::debug!(run = %run_id, "the run is no longer accepting cases");
        return Ok(());
    }
    if already_filled(pool, tenant, run_id, case_id, rep).await? {
        return Ok(());
    }

    let Some(source) = source else {
        tracing::warn!(run = %run_id, case = %case_id, rep, "no stored result to regrade");
        return Ok(());
    };
    let Some(trace) = trace else {
        tracing::warn!(run = %run_id, case = %case_id, rep, "no source transcript to regrade");
        return Ok(());
    };
    let Some(output) = captured_output(&trace.turns) else {
        tracing::warn!(
            run = %run_id,
            case = %case_id,
            rep,
            "the source transcript did not capture an output; slot left empty"
        );
        return Ok(());
    };

    let deadline = CaseDeadline::starting(Utc::now(), EVAL_CASE_TIMEOUT);
    // The subject's tokens were billed on the source run; `usage` stays empty so the climb's
    // summed spend does not count them twice. `judge_usage` carries the regrade's own cost.
    let usage = json!({});
    grade_and_record(
        pool,
        app_key,
        tenant,
        GradeRequest {
            run_id,
            case_id,
            rep,
            attempt,
            version_id: run.version_id,
            case_input: &case.input,
            output: &output,
            turns: &trace.turns,
            model: &source.model,
            stop_reason: source.stop_reason.as_deref(),
            usage: &usage,
            latency_ms: source.latency_ms,
            tool_calls: source.tool_calls,
            action_invocation_id: source.action_invocation_id,
            subject_run_id: source.subject_run_id,
            action: REGRADE_JOB_KIND,
        },
        deadline,
    )
    .await
}

/// The output a transcript captured for its last assistant turn, if it captured one.
fn captured_output(turns: &Value) -> Option<Value> {
    turns
        .as_array()?
        .iter()
        .rev()
        .find(|turn| turn.get("role").and_then(Value::as_str) == Some("assistant"))
        .and_then(|turn| turn.get("output"))
        .cloned()
}

/// One `(run, case, rep)` slot.
struct Slot {
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    attempt: i32,
}

/// Append the failure row and return the `JobFailure` the worker retries on.
#[allow(clippy::too_many_arguments)]
async fn fail(
    pool: &PgPool,
    tenant: TenantId,
    slot: &Slot,
    class: FailureClass,
    message: String,
    model: Option<&str>,
    reported_judge: Option<&str>,
    deadline: Option<CaseDeadline>,
    _ceiling: CaseDeadline,
) -> Result<(), JobFailure> {
    let _ = reported_judge;
    let retry_after = RetryPolicy::default().backoff_jittered(
        u32::try_from(slot.attempt).unwrap_or(1),
        entropy(slot.run_id, slot.case_id, slot.rep),
    );

    let written = async {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| error.to_string())?;
        store::eval::insert_attempt_error_tx(
            &mut tx,
            tenant,
            &NewAttemptError {
                eval_run_id: slot.run_id,
                case_id: slot.case_id,
                rep: slot.rep,
                attempt: slot.attempt,
                failure_class: class.as_str(),
                retry_count: slot.attempt - 1,
                model,
                usage: None,
                message: &message,
                deadline_at: deadline.map(|deadline| deadline.deadline_at()),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        tx.commit().await.map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = written {
        tracing::error!(run = %slot.run_id, case = %slot.case_id, %error, "recording an eval attempt failed");
    }

    let exhausted = slot.attempt >= i32::try_from(MAX_ATTEMPTS).unwrap_or(i32::MAX);
    Err(JobFailure {
        message: format!("{}: {message}", class.as_str()),
        permanent: !class.is_retryable() || exhausted,
        retry_after,
    })
}

async fn next_attempt(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<i32, JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    store::eval::next_attempt_tx(&mut tx, tenant, run_id, case_id, rep)
        .await
        .map_err(|error| permanent(error.to_string()))
}

async fn already_filled(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
) -> Result<bool, JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let filled = store::eval::get_result_tx(&mut tx, tenant, run_id, case_id, rep)
        .await
        .map_err(|error| permanent(error.to_string()))?;
    Ok(filled.is_some())
}

async fn read_expected(
    pool: &PgPool,
    tenant: TenantId,
    case_id: Uuid,
) -> Result<Option<Value>, JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    store::eval::get_case_expected_tx(&mut tx, tenant, case_id)
        .await
        .map_err(|error| permanent(error.to_string()))
}

/// Call the judge through the engine, as the same service principal.
async fn invoke_judge(
    pool: &PgPool,
    app_key: &str,
    tenant: TenantId,
    request: Value,
    request_id: &str,
    trace_id: &str,
) -> Result<Value, FailureClass> {
    let outcome = invoke(
        pool,
        registry(),
        tenant,
        InvokeRequest {
            action: "completions.create".to_owned(),
            input: request,
            precondition: None,
            idempotency_key: None,
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some(request_id.to_owned()),
            trace_id: Some(trace_id.to_owned()),
            depth: 1,
            app_key: app_key.to_owned(),
            // The judge is an eval call too: without this the grader's spend is invisible to the
            // ledger, which is the one place the product's cost is supposed to be readable.
            usage: Some(UsageContext {
                caller_kind: CallerKind::Eval,
                run_id: None,
                run_step_id: None,
                conversation_turn_id: None,
                // This seam grades one output and has no eval-run id in scope; the call is still
                // attributed to `CallerKind::Eval` and its spend lands on the ledger.
                eval_run_id: None,
                prompt_ref: Some("eval_judge".to_owned()),
                prompt_version: None,
                correlation_id: None,
            }),
        },
    )
    .await
    .map_err(|error| FailureClass::from_status(error.status))?;
    if !(200..300).contains(&outcome.status) {
        return Err(FailureClass::from_status(outcome.status));
    }
    Ok(outcome.body)
}

/// The transcript: what the subject was asked, and what it answered.
///
/// The turn list is the method's five roles. `trace_source` selects where a *stored* transcript
/// would be read from when the subject produces a conversation; for a synthesised subject the
/// exchange itself is the transcript, which is the only shape that always exists.
fn synthesise_turns(request: &Value, output: &Value) -> Value {
    let mut turns = Vec::new();
    if let Some(messages) = request.get("messages").and_then(Value::as_array) {
        for message in messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .filter(|role| {
                    matches!(
                        *role,
                        "system" | "user" | "assistant" | "tool_call" | "tool_result"
                    )
                })
                .unwrap_or("user");
            turns.push(json!({
                "role": role,
                "content": message.get("content").cloned().unwrap_or(Value::Null),
            }));
        }
    } else if let Some(input) = request.get("input") {
        turns.push(json!({"role": "user", "content": input}));
    }

    let answer = output
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .cloned()
        .or_else(|| output.get("content").cloned())
        .unwrap_or(Value::Null);
    // ⭐ The assistant turn carries the **whole output**, not only the answer text, because a
    // regrade re-grades a stored output and the graders read paths into the object
    // (`eval::grade`'s `field`). Without it a regrade could only grade the answer and would
    // silently score a different value. A trace written before this capture is not regradable;
    // it is skipped and counted, never guessed.
    //
    // The trace row is capped at 1 MiB, so the capture is skipped — not the measurement — when
    // the output alone would eat most of that budget. An oversized output is still graded and
    // recorded; it simply cannot be regraded later.
    let mut assistant = json!({"role": "assistant", "content": answer});
    if let Ok(encoded) = serde_json::to_string(output)
        && encoded.len() <= CAPTURED_OUTPUT_CAP
    {
        assistant["output"] = output.clone();
    }
    turns.push(assistant);
    Value::Array(turns)
}

/// The largest serialised output a transcript will carry for a later regrade.
const CAPTURED_OUTPUT_CAP: usize = 256 * 1024;

fn count_tool_calls(output: &Value) -> i32 {
    output
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
        .map_or(0, |calls| i32::try_from(calls.len()).unwrap_or(i32::MAX))
}

fn parse_uuid(raw: &str) -> Option<Uuid> {
    Uuid::parse_str(raw).ok()
}

/// A pure function of the slot, so a replayed attempt gets the same backoff.
fn entropy(run_id: Uuid, case_id: Uuid, rep: i32) -> u64 {
    let bytes = run_id.as_bytes();
    let mut seed = u64::from(rep as u32);
    for (index, byte) in bytes.iter().chain(case_id.as_bytes()).enumerate() {
        seed = seed
            .wrapping_mul(31)
            .wrapping_add(u64::from(*byte))
            .wrapping_add(index as u64);
    }
    seed
}

fn service_ctx(tenant: TenantId, app_key: &str, action: &'static str) -> HandlerCtx {
    HandlerCtx {
        tenant,
        actor_id: Uuid::nil(),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        channel: None,
        request_id: None,
        version: None,
        if_match: None,
        action,
        app_key: app_key.to_owned(),
    }
}

fn permanent(message: String) -> JobFailure {
    JobFailure {
        message,
        permanent: true,
        retry_after: Duration::ZERO,
    }
}
