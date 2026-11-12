//! Migration 0034 — the one-shot per-output grade, executed.
//!
//! S19's run measures a stored case set. An `inspection.kind = 'eval'` SOP has no case: the
//! output is the step's own, so `eval_grades.create` writes one `pending` row and queues one
//! `eval.grade` job, and the job grades it with the **same** loop the case driver uses.
//!
//! Three things are proved here:
//!
//! 1. a programmatic metric accepts an output that matches the caller's answer key and rejects
//!    one that does not, with the score the mean over the quality metrics;
//! 2. a rubric metric is graded by a real judge call through the engine (the stub inference),
//!    so the model-calling half of the path is exercised, not described;
//! 3. the job writes the terminal row **exactly once** — a replay with a different payload
//!    settles nothing, because the row is the guard.
//!
//! The stub and the fixture live in `common::eval_support`, because
//! `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global.

mod common;

use conveyor_actions::InvokeRequest;
use conveyor_actions::invoke::Authority;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "eval_support/mod.rs"]
mod fx;

/// Invoke one action as a machine principal holding `evals:write` (the role a process step's
/// judge runs as), which is also what `eval_runs.start` is invoked with in the driver's tests.
async fn invoke(
    db: &common::TestDb,
    tenant: TenantId,
    action: &str,
    input: Value,
) -> conveyor_actions::InvokeOutcome {
    match conveyor_actions::invoke(
        &db.runtime,
        conveyor_actions::registry(),
        tenant,
        InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            actor_id: Uuid::nil(),
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Internal,
            request_id: Some("eval-grade-test".to_owned()),
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => panic!("{action} refused: {} {}", error.status, error.detail),
    }
}

/// Create one grade and return its id plus the job payload the action enqueued.
async fn create_grade(
    db: &common::TestDb,
    fixture: &fx::Fixture,
    output: Value,
    expected: Value,
    threshold: f64,
) -> (Uuid, Value) {
    let outcome = invoke(
        db,
        fixture.tenant,
        "eval_grades.create",
        json!({
            "eval_id": fixture.eval_id,
            "output": output,
            "case_input": {"messages": [{"role": "user", "content": "classify: refund"}]},
            "expected": expected,
            "threshold": threshold,
        }),
    )
    .await;
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], "pending");
    let grade_id = Uuid::parse_str(
        outcome.body["output"]["grade_id"]
            .as_str()
            .expect("the create returns a grade id"),
    )
    .expect("a uuid");

    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let payload: Value = sqlx::query_scalar(
        "SELECT payload FROM job
          WHERE tenant_id = $1 AND kind = 'eval.grade' AND dedupe_key = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(grade_id.to_string())
    .fetch_one(&mut *tx)
    .await
    .expect("the create enqueued exactly one eval.grade job");
    tx.commit().await.expect("commit");
    assert_eq!(payload["grade_id"], json!(grade_id.to_string()));
    (grade_id, payload)
}

async fn read_grade(
    db: &common::TestDb,
    tenant: TenantId,
    grade_id: Uuid,
) -> store::eval::EvalGrade {
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let row = store::eval::get_grade_tx(&mut tx, tenant, grade_id)
        .await
        .expect("read the grade")
        .expect("the grade exists");
    tx.commit().await.expect("commit");
    row
}

#[tokio::test]
async fn a_programmatic_metric_accepts_and_rejects_and_a_replay_settles_nothing() {
    let db = common::TestDb::fresh().await;
    let fixture = fx::fixture_with(&db, &["classify: refund"], &[fx::exact_metric()]).await;
    let output = json!({"choices": [{"message": {"content": "billing"}}]});

    // ── the answer key matches: accepted, score 1.0 ──────────────────────────────────────
    let (accepted_id, payload) =
        create_grade(&db, &fixture, output.clone(), json!("billing"), 0.5).await;
    conveyor_server::eval_worker::grade_output(&db.runtime, "conveyor", fixture.tenant, &payload)
        .await
        .expect("the job graded the output");

    let accepted = read_grade(&db, fixture.tenant, accepted_id).await;
    assert_eq!(accepted.status, "accepted", "{accepted:?}");
    assert_eq!(accepted.grade, json!({"label_accuracy": 1.0}));
    assert_eq!(
        accepted
            .score
            .map(|score| score.to_f64().unwrap_or_default()),
        Some(1.0)
    );
    assert!(
        accepted
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("0.5")),
        "the reason names the bar it was judged at: {accepted:?}"
    );
    assert_eq!(accepted.error, None);

    // The SOP's poll: `eval_grades.get` returns the row the verdict lives on.
    let polled = invoke(
        &db,
        fixture.tenant,
        "eval_grades.get",
        json!({"grade_id": accepted_id}),
    )
    .await;
    assert_eq!(polled.status, 200, "{}", polled.body);
    assert_eq!(polled.body["output"]["status"], "accepted");
    assert_eq!(polled.body["output"]["threshold"], json!(0.5));
    assert_eq!(
        polled.body["output"]["grade"],
        json!({"label_accuracy": 1.0})
    );
    assert_eq!(
        polled.body["output"]["output_hash"].as_str().map(str::len),
        Some(64)
    );

    // ── a replay settles nothing: the row is the exactly-once guard ────────────────────
    let mut replay = payload.clone();
    replay["output"] = json!({"choices": [{"message": {"content": "refund"}}]});
    conveyor_server::eval_worker::grade_output(&db.runtime, "conveyor", fixture.tenant, &replay)
        .await
        .expect("a replay is a no-op, not a failure");
    let unchanged = read_grade(&db, fixture.tenant, accepted_id).await;
    assert_eq!(
        unchanged.status, "accepted",
        "a second job must not rewrite a settled verdict: {unchanged:?}"
    );

    // ── the key does not match: rejected, score 0.0 ────────────────────────────────────
    let (rejected_id, payload) = create_grade(&db, &fixture, output, json!("refund"), 0.5).await;
    conveyor_server::eval_worker::grade_output(&db.runtime, "conveyor", fixture.tenant, &payload)
        .await
        .expect("the job graded the output");
    let rejected = read_grade(&db, fixture.tenant, rejected_id).await;
    assert_eq!(rejected.status, "rejected", "{rejected:?}");
    assert_eq!(rejected.grade, json!({"label_accuracy": 0.0}));
    assert_eq!(
        rejected
            .score
            .map(|score| score.to_f64().unwrap_or_default()),
        Some(0.0)
    );
    assert!(
        rejected
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("label_accuracy")),
        "a rejection names the metric that lost: {rejected:?}"
    );
    assert_eq!(rejected.error, None);
}

#[tokio::test]
async fn a_rubric_metric_is_graded_by_the_stub_judge() {
    let db = common::TestDb::fresh().await;
    // ⛔ One stub per binary: `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global, so the claim
    // must happen before any grade that calls a judge (the audit tests' pattern).
    let _ = fx::stub_base();
    let rubric = fx::MetricSpec {
        key: "tone",
        grader_kind: "rubric",
        grader: json!({"judge_model": "judge-model"}),
        // The stub's only per-test seam: a rubric containing `LENIENT` makes it score 0.9.
        rubric: Some("LENIENT: names the failing component."),
    };
    let fixture = fx::fixture_with(&db, &["classify: refund"], &[rubric]).await;
    let output = json!({"choices": [{"message": {"content": "the cache"}}]});

    // 0.9 ≥ 0.8: the judge's verdict is the grade.
    let (accepted_id, payload) =
        create_grade(&db, &fixture, output.clone(), json!("billing"), 0.8).await;
    conveyor_server::eval_worker::grade_output(&db.runtime, "conveyor", fixture.tenant, &payload)
        .await
        .expect("the judge graded the output");
    let accepted = read_grade(&db, fixture.tenant, accepted_id).await;
    assert_eq!(accepted.status, "accepted", "{accepted:?}");
    assert_eq!(accepted.grade["tone"], json!(0.9));
    assert_eq!(accepted.detail["tone"]["judge"], json!(true));
    assert_eq!(
        accepted
            .score
            .map(|score| score.to_f64().unwrap_or_default()),
        Some(0.9)
    );

    // The same verdict under a higher bar is a rejection, not a second judge call's accident.
    let (rejected_id, payload) = create_grade(&db, &fixture, output, json!("billing"), 0.95).await;
    conveyor_server::eval_worker::grade_output(&db.runtime, "conveyor", fixture.tenant, &payload)
        .await
        .expect("the judge graded the output");
    let rejected = read_grade(&db, fixture.tenant, rejected_id).await;
    assert_eq!(rejected.status, "rejected", "{rejected:?}");
    assert!(
        rejected
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("tone")),
        "a rejection names the metric that lost: {rejected:?}"
    );
}
