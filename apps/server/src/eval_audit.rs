//! `eval.audit.run` — one health audit of an eval instrument (S19 Part C step 14; SPEC D25).
//!
//! The job is enqueued by `eval_audits.start`. It reads the frozen version and its cases, runs
//! the `claude-api` `eval-audit` method's mechanical checks, runs the controls the method
//! requires (the answer key through the instrument's own programmatic graders, an empty answer
//! through the same, and — when a judge metric exists — the judge's three known negatives), and
//! writes the findings under the audit row.
//!
//! **The audit is not a second runner.** It grades *stored or constructed* answers with the same
//! `grade_programmatic` the case runner uses; it never re-implements scoring, never writes a
//! result row and never touches the subject. The oracle and null are run over every
//! `(case, programmatic metric)` pair the version declares, which is what makes "≈100% / ≈0%"
//! a statement about the instrument rather than about one case.
//!
//! A failed job leaves the audit `running` and retries; a permanent failure (a malformed payload,
//! a missing row) dead-letters with the audit `failed` and the reason on the row.

use conveyor_actions::eval::graders::{grade_from_judge, judge_model, judge_request};
use conveyor_actions::invoke::{Authority, InvokeRequest, invoke};
use conveyor_actions::registry::registry;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::eval::audit::{
    CaseFacts, Controls, InstrumentFacts, MetricFacts, RunFacts, Severity, findings,
    resolution_line, resolution_shortfall,
};
use conveyor_domain::eval::{GraderKind, grade_programmatic};
use conveyor_store as store;
use conveyor_store::eval::{AuditRow, EvalCase, EvalMetric, ResultRow};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// How many cases or results one page reads.
const PAGE: i64 = 500;

/// The three known-bad answers a judge must reject (the method's "tested on known negatives").
const KNOWN_NEGATIVES: [&str; 3] = ["", "I don't know", "The result is correct; all tests pass."];

/// Run one `eval.audit.run` job.
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let audit_id = payload
        .get("audit_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent("eval.audit.run needs an audit_id"))?;

    let audit = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let Some(audit) = store::eval::get_audit_tx(&mut tx, tenant, audit_id)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
        else {
            // The tenant was restored or deleted: there is no audit to run.
            return Ok(());
        };
        if audit.status == "succeeded" || audit.status == "failed" {
            return Ok(());
        }
        let running = store::eval::transition_audit_tx(
            &mut tx, tenant, audit_id, "running", None, None, None, None, None, None,
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        running
    };

    match audit_work(pool, tenant, app_key, &audit).await {
        Ok((numbers, drafts)) => {
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            for draft in &drafts {
                store::eval::insert_finding_tx(
                    &mut tx,
                    tenant,
                    audit.id,
                    &store::eval::NewFinding {
                        section: draft.section.as_str(),
                        item: &draft.item,
                        severity: draft.severity.as_str(),
                        evidence: &draft.evidence,
                        suggestion: &draft.suggestion,
                    },
                )
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            }
            store::eval::transition_audit_tx(
                &mut tx,
                tenant,
                audit.id,
                "succeeded",
                numbers.oracle,
                numbers.null,
                numbers.noise_floor,
                numbers.headroom,
                Some(&numbers.summary),
                None,
            )
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            let event = store::NewEvent {
                topic: format!("eval:{}", audit.eval_id),
                event_type: "eval_audit.completed".to_owned(),
                aggregate_type: "eval".to_owned(),
                aggregate_id: audit.eval_id,
                payload: json!({
                    "id": audit.id,
                    "status": "succeeded",
                    "findings": drafts.len(),
                }),
                correlation_id: None,
                causation_id: None,
            };
            store::outbox::append(&mut tx, tenant, &event)
                .await
                .map_err(|error| JobFailure::retryable(format!("outbox: {error}")))?;
            tx.commit()
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            Ok(())
        }
        Err(failure) => {
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
            let _ = store::eval::transition_audit_tx(
                &mut tx,
                tenant,
                audit.id,
                "failed",
                None,
                None,
                None,
                None,
                None,
                Some(failure.message.as_str()),
            )
            .await;
            let _ = tx.commit().await;
            Err(failure)
        }
    }
}

/// The numbers an audit writes on its row.
struct AuditNumbers {
    oracle: Option<rust_decimal::Decimal>,
    null: Option<rust_decimal::Decimal>,
    noise_floor: Option<rust_decimal::Decimal>,
    headroom: Option<rust_decimal::Decimal>,
    summary: Value,
}

fn decimal(value: f64) -> Option<rust_decimal::Decimal> {
    rust_decimal::Decimal::try_from(value).ok()
}

/// Everything the audit does between `running` and its terminal state.
async fn audit_work(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    audit: &AuditRow,
) -> Result<
    (
        AuditNumbers,
        Vec<conveyor_domain::eval::audit::FindingDraft>,
    ),
    JobFailure,
> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let eval = store::eval::get_eval_tx(&mut tx, tenant, audit.eval_id)
        .await
        .map_err(|error| JobFailure::permanent(format!("no eval: {error}")))?;
    let version = store::eval::get_version_tx(&mut tx, tenant, audit.version_id)
        .await
        .map_err(|error| JobFailure::permanent(format!("no version: {error}")))?;
    let metrics: Vec<EvalMetric> = store::eval::list_metrics_tx(&mut tx, tenant, audit.version_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let cases = all_cases(&mut tx, tenant, audit.version_id).await?;
    let expected: Vec<Option<Value>> = {
        let mut out = Vec::with_capacity(cases.len());
        for case in &cases {
            out.push(
                store::eval::get_case_expected_tx(&mut tx, tenant, case.id)
                    .await
                    .map_err(|error| JobFailure::retryable(format!("database: {error}")))?,
            );
        }
        out
    };
    let run = match audit.target_run_id {
        Some(run_id) => Some(
            store::eval::get_run_tx(&mut tx, tenant, run_id)
                .await
                .map_err(|error| JobFailure::permanent(format!("no target run: {error}")))?,
        ),
        None => None,
    };
    let results = match audit.target_run_id {
        Some(run_id) => all_results(&mut tx, tenant, run_id).await?,
        None => Vec::new(),
    };
    let attempts = match audit.target_run_id {
        Some(run_id) => store::eval::attempt_errors_tx(&mut tx, tenant, run_id)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?,
        None => Vec::new(),
    };
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    let case_facts = case_facts(&cases);
    let metric_facts = metric_facts(&metrics);
    let facts = InstrumentFacts {
        model_ref: eval.model_ref.as_deref(),
        split_locked: version.split_locked_at.is_some(),
        prices: &version.prices,
        cases: &case_facts,
        metrics: &metric_facts,
        run: run.as_ref().map(|run| RunFacts {
            reps: run.reps,
            ok: results.iter().filter(|row| row.status == "ok").count(),
            truncated: results
                .iter()
                .filter(|row| row.status == "truncated")
                .count(),
            failed: attempts.len(),
            scored_cases: scored_cases(
                &results,
                metrics.iter().find(|metric| metric.role == "quality"),
            ),
        }),
    };

    // ── the controls: the answer key and an empty answer through the real graders ───────
    let (oracle, null, oracle_checked) = controls(&facts, &cases, &expected);
    let (judge_checked, judge_accepted, judge_problems, judge_error) =
        judge_controls(pool, tenant, app_key, &metrics, &cases, &expected).await?;
    let controls = Controls {
        oracle,
        null,
        judge_checked,
        judge_accepted,
        judge_problems: judge_problems.clone(),
    };

    // ── the resolution numbers, from the target run when one was named ──────────────────
    let (noise_floor, headroom) = resolution(&facts, &results, run.as_ref());

    let mut drafts = findings(&facts, &controls);
    if let Some(finding) = resolution_shortfall(noise_floor, headroom) {
        drafts.push(finding);
    }
    let sections: Vec<&str> = conveyor_domain::eval::audit::Section::all()
        .iter()
        .map(|section| section.as_str())
        .collect();
    let summary = json!({
        "sections": sections,
        "finding_counts": {
            "misleading": drafts.iter().filter(|d| d.severity == Severity::Misleading).count(),
            "noise": drafts.iter().filter(|d| d.severity == Severity::Noise).count(),
            "generality": drafts.iter().filter(|d| d.severity == Severity::Generality).count(),
        },
        "controls": {
            "oracle": oracle,
            "null": null,
            "pairs_checked": oracle_checked,
            "judge_checked": judge_checked,
            "judge_accepted": judge_accepted,
            "judge_problems": judge_problems,
            "judge_error": judge_error,
        },
        "resolution": resolution_line(noise_floor, headroom),
        "target_run_id": audit.target_run_id,
    });
    Ok((
        AuditNumbers {
            oracle: oracle.and_then(decimal),
            null: null.and_then(decimal),
            noise_floor: noise_floor.and_then(decimal),
            headroom: headroom.and_then(decimal),
            summary,
        },
        drafts,
    ))
}

/// Every case of a version, read to exhaustion.
async fn all_cases(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    version_id: Uuid,
) -> Result<Vec<EvalCase>, JobFailure> {
    let mut out = Vec::new();
    let mut cursor: Option<(i32, Uuid)> = None;
    loop {
        let page = store::eval::list_cases_tx(tx, tenant, version_id, cursor, PAGE)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let short = (page.len() as i64) < PAGE;
        if let Some(last) = page.last() {
            cursor = Some((last.ordinal, last.id));
        }
        out.extend(page);
        if short {
            return Ok(out);
        }
    }
}

/// Every result of a run, read to exhaustion.
async fn all_results(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<ResultRow>, JobFailure> {
    let mut out = Vec::new();
    let mut cursor: Option<(Uuid, i32)> = None;
    loop {
        let page = store::eval::list_results_tx(
            tx,
            tenant,
            run_id,
            store::eval::ResultFilter::default(),
            cursor,
            PAGE,
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let short = (page.len() as i64) < PAGE;
        if let Some(last) = page.last() {
            cursor = Some((last.case_id, last.rep));
        }
        out.extend(page);
        if short {
            return Ok(out);
        }
    }
}

fn case_facts(cases: &[EvalCase]) -> Vec<CaseFacts> {
    cases
        .iter()
        .map(|case| CaseFacts {
            id: case.id,
            ordinal: case.ordinal,
            input: case.input.clone(),
            input_len: case.input.chars().count(),
            tag: case.tags.first().cloned(),
            split: case.split.clone(),
            gold_provenance: case.gold_provenance.clone(),
            gold_model: case.gold_model.clone(),
        })
        .collect()
}

fn metric_facts(metrics: &[EvalMetric]) -> Vec<MetricFacts> {
    metrics
        .iter()
        .map(|metric| MetricFacts {
            key: metric.key.clone(),
            role: metric.role.clone(),
            kind: metric.kind.clone(),
            better: metric.better.clone(),
            grader_kind: metric.grader_kind.clone(),
            grader: metric.grader.clone(),
            rubric: metric.rubric.clone(),
        })
        .collect()
}

/// How many cases contributed a scorable value to `metric`.
fn scored_cases(results: &[ResultRow], metric: Option<&EvalMetric>) -> usize {
    let Some(metric) = metric else {
        return 0;
    };
    let mut cases: std::collections::BTreeSet<Uuid> = std::collections::BTreeSet::new();
    for row in results {
        if row.status == "ok"
            && row
                .grade
                .get(metric.key.as_str())
                .and_then(Value::as_f64)
                .is_some()
        {
            cases.insert(row.case_id);
        }
    }
    cases.len()
}

/// The shapes a reference answer is offered to a grader in.
///
/// A programmatic grader reads a field of the **subject's response** (`choices.0.message.content`
/// for a completion), and some read the answer itself (`label`). The oracle control presents the
/// key in both shapes and passes when either scores 1.0; the null control presents an empty
/// answer in both and passes when **neither** does.
fn answer_shapes(answer: &Value) -> [Value; 2] {
    let text = answer
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| answer.to_string());
    [
        answer.clone(),
        json!({"choices": [{"message": {"role": "assistant", "content": text}}]}),
    ]
}

/// The oracle and null controls: every `(case, programmatic metric)` pair, graded by the same
/// function the case runner uses.
///
/// Returns `(oracle, null, pairs_checked)`. A pair whose grade call errors contributes `0.0`:
/// the oracle that cannot be graded is not an oracle, and a null that makes the grader crash is
/// not evidence of leniency either — both are visible in the evidence string of a finding.
fn controls(
    facts: &InstrumentFacts<'_>,
    cases: &[EvalCase],
    expected: &[Option<Value>],
) -> (Option<f64>, Option<f64>, usize) {
    let programmatic: Vec<&MetricFacts> = facts
        .metrics
        .iter()
        .filter(|metric| metric.grader_kind == "programmatic")
        .collect();
    if programmatic.is_empty() {
        return (None, None, 0);
    }
    let mut pairs = 0_usize;
    let mut oracle_pass = 0_usize;
    let mut null_pass = 0_usize;
    for (_case, expected) in cases.iter().zip(expected.iter()) {
        let Some(expected) = expected else {
            continue;
        };
        let oracle_shapes = answer_shapes(expected);
        let null_shapes = answer_shapes(&json!(""));
        for metric in &programmatic {
            pairs += 1;
            if oracle_shapes.iter().any(|output| {
                grade_programmatic(&metric.key, &metric.grader, Some(expected), output)
                    .map(|grade| grade.score >= 1.0)
                    .unwrap_or(false)
            }) {
                oracle_pass += 1;
            }
            if null_shapes.iter().any(|output| {
                grade_programmatic(&metric.key, &metric.grader, Some(expected), output)
                    .map(|grade| grade.score >= 1.0)
                    .unwrap_or(false)
            }) {
                null_pass += 1;
            }
        }
    }
    if pairs == 0 {
        return (None, None, 0);
    }
    let oracle = oracle_pass as f64 / pairs as f64;
    let null = null_pass as f64 / pairs as f64;
    (Some(oracle), Some(null), pairs)
}

/// The judge's three known negatives.
///
/// One metric is probed — the first judge metric that names a model — because the control is
/// about the judge prompt, not about every metric, and three calls per metric would spend a
/// run's budget on an audit. A failed call is reported as `judge_error` and leaves the control
/// unmeasured rather than counting an outage as leniency.
async fn judge_controls(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    metrics: &[EvalMetric],
    cases: &[EvalCase],
    expected: &[Option<Value>],
) -> Result<(usize, usize, Vec<String>, Option<String>), JobFailure> {
    // The first judge metric that names a model; one metric's three negatives are the control.
    let mut probe = None;
    for metric in metrics {
        if !matches!(metric.grader_kind.as_str(), "rubric" | "pairwise") {
            continue;
        }
        if judge_model(&metric.grader).is_none() {
            continue;
        }
        let index = if metric.grader_kind == "pairwise" {
            match expected.iter().position(Option::is_some) {
                Some(index) => index,
                None => continue,
            }
        } else {
            0
        };
        if let (Some(case), Some(expected)) = (cases.get(index), expected.get(index).cloned()) {
            probe = Some((metric, case, expected));
            break;
        }
    }
    let Some((metric, case, expected)) = probe else {
        return Ok((0, 0, Vec::new(), None));
    };
    let Some(judge) = judge_model(&metric.grader) else {
        return Ok((0, 0, Vec::new(), None));
    };
    let kind = match metric.grader_kind.as_str() {
        "rubric" => GraderKind::Rubric,
        _ => GraderKind::Pairwise,
    };

    let mut checked = 0_usize;
    let mut accepted = 0_usize;
    let mut problems = Vec::new();
    for negative in KNOWN_NEGATIVES {
        let output = json!(negative);
        let request = judge_request(
            kind,
            &metric.key,
            metric.rubric.as_deref(),
            &case.input,
            expected.as_ref(),
            &output,
            judge,
        )
        .map_err(|error| JobFailure::permanent(format!("judge request: {error}")))?;
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
                request_id: Some(format!("eval.audit:judge:{:?}", metric.key)),
                trace_id: None,
                depth: 1,
                app_key: app_key.to_owned(),
                usage: None,
            },
        )
        .await
        .map_err(|error| JobFailure::retryable(format!("judge call: {error}")))?;
        checked += 1;
        if !(200..300).contains(&outcome.status) {
            return Ok((
                checked,
                accepted,
                problems,
                Some(format!("the judge call answered {}", outcome.status)),
            ));
        }
        match grade_from_judge(&outcome.body) {
            Ok(grade) if grade.score >= 0.5 => {
                accepted += 1;
                problems.push(format!(
                    "{} scored {:?} for {:?}",
                    metric.key, grade.score, negative
                ));
            }
            Ok(_) => {}
            Err(error) => {
                problems.push(format!("the judge's answer did not parse: {error}"));
                accepted += 1;
            }
        }
    }
    Ok((checked, accepted, problems, None))
}

/// The headline metric's noise floor and headroom, from the target run when there is one.
fn resolution(
    facts: &InstrumentFacts<'_>,
    results: &[ResultRow],
    run: Option<&store::eval::Run>,
) -> (Option<f64>, Option<f64>) {
    let (Some(run), false) = (run, results.is_empty()) else {
        return (None, None);
    };
    let Some(headline) = facts.metrics.iter().find(|metric| metric.role == "quality") else {
        return (None, None);
    };
    // The held-out numbers: the test-split cases when the run has any, else every graded case.
    let test: Vec<&ResultRow> = results.iter().filter(|row| row.split == "test").collect();
    let scope: Vec<&ResultRow> = if test.is_empty() {
        results.iter().collect()
    } else {
        test
    };
    let mut per_case: std::collections::BTreeMap<Uuid, Vec<f64>> =
        std::collections::BTreeMap::new();
    for row in scope {
        if row.status != "ok" {
            continue;
        }
        if let Some(value) = row.grade.get(headline.key.as_str()).and_then(Value::as_f64) {
            per_case.entry(row.case_id).or_default().push(value);
        }
    }
    let means: Vec<f64> = per_case
        .values()
        .map(|values| values.iter().sum::<f64>() / values.len() as f64)
        .collect();
    if means.is_empty() {
        return (None, None);
    }
    let floor = conveyor_domain::eval::climb::noise_floor(means.len(), run.reps.max(0) as u32);
    let noise_floor = floor.is_finite().then_some(floor);
    let mean = means.iter().sum::<f64>() / means.len() as f64;
    let headroom = if headline.kind == "binary" {
        Some(if headline.better == "lower" {
            mean
        } else {
            1.0 - mean
        })
    } else {
        None
    };
    (noise_floor, headroom)
}
