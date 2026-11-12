#![forbid(unsafe_code)]
//! The `(run, case, rep)` slot: one result per slot, and a failed attempt that never occupies it
//! (S19 Part A step 3; CONTRACT §20).
//!
//! Two properties, both schema facts rather than driver conventions:
//!
//! - `eval_case_result_slot` refuses a second result for the same `(tenant, run, case, rep)`,
//!   **by name** — a re-run is a new `rep` or a new run, never an overwrite;
//! - `eval_attempt_error` is append-only and independent of the slot, so the resume path re-runs
//!   exactly the attempts that produced no scorable output and the evidence of the failure
//!   survives the later success.
//!
//! Each write gets its own transaction: a failed statement aborts its transaction, so a refusal
//! sharing one with the next assertion would report the wrong error.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::eval::{EvalStoreError, NewAttemptError, NewCaseResult};
use conveyor_store::eval::{insert_attempt_error_tx, insert_case_result_tx};
use serde_json::{Value, json};
use uuid::Uuid;

/// The result for one slot, with a grade **object** (a scalar would be refused by the schema's
/// own CHECK before the slot uniqueness was ever evaluated).
fn result_at<'a>(
    run: Uuid,
    case: Uuid,
    rep: i32,
    grade: &'a Value,
    explanation: &'a Value,
    usage: &'a Value,
) -> NewCaseResult<'a> {
    NewCaseResult {
        eval_run_id: run,
        case_id: case,
        rep,
        status: "ok",
        stop_reason: None,
        grade,
        explanation,
        model: "test-model",
        usage,
        latency_ms: 42,
        tool_calls: 0,
        action_invocation_id: None,
        subject_run_id: None,
        judge_model: None,
        judge_usage: None,
        human_score: None,
    }
}

#[tokio::test]
async fn a_result_slot_is_unique_and_a_failed_attempt_never_fills_it() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-slot', 'Slot') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let run = Uuid::now_v7();
    let case = Uuid::now_v7();
    let grade = json!({"pass": 1});
    let explanation = json!({});
    let usage = json!({"input_tokens": 3, "output_tokens": 5});

    // ── (a) the first result at (run, case, 0) is written ───────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let first = insert_case_result_tx(
        &mut tx,
        tenant,
        &result_at(run, case, 0, &grade, &explanation, &usage),
    )
    .await
    .expect("the first result for a slot inserts");
    tx.commit().await.expect("commit the first result");

    // ── (b) a second result at the same slot is refused BY NAME ─────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = insert_case_result_tx(
        &mut tx,
        tenant,
        &result_at(run, case, 0, &grade, &explanation, &usage),
    )
    .await;
    tx.rollback().await.expect("roll back the refusal");
    match refused {
        Err(EvalStoreError::Conflict { constraint }) => {
            assert_eq!(
                constraint, "eval_case_result_slot",
                "the conflict must name the slot constraint"
            );
        }
        other => panic!("a second result for one slot must be a Conflict, got {other:?}"),
    }

    // ── (c) a failed attempt at a DIFFERENT slot writes an error row, not a result ──────
    let attempt_usage = json!({"input_tokens": 1});
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    insert_attempt_error_tx(
        &mut tx,
        tenant,
        &NewAttemptError {
            eval_run_id: run,
            case_id: case,
            rep: 1,
            attempt: 1,
            failure_class: "serving",
            retry_count: 0,
            model: Some("test-model"),
            usage: Some(&attempt_usage),
            message: "the serving endpoint returned 503",
            deadline_at: None,
        },
    )
    .await
    .expect("an attempt error is append-only and needs no result row");
    tx.commit().await.expect("commit the attempt error");

    // ── (d) the slot the attempt failed in is still empty ───────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let results_at_rep1: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_case_result
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = 1",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(case)
    .fetch_one(&mut *tx)
    .await
    .expect("count results at the failed slot");
    assert_eq!(
        results_at_rep1, 0,
        "a failed attempt must not write an eval_case_result"
    );
    let errors_at_rep1: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = 1",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(case)
    .fetch_one(&mut *tx)
    .await
    .expect("count the attempt errors");
    assert_eq!(errors_at_rep1, 1, "the failure is evidence and is kept");
    tx.rollback().await.expect("roll back the read");

    // ── (e) the resume fills the slot, and the error row survives ───────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let resumed = insert_case_result_tx(
        &mut tx,
        tenant,
        &result_at(run, case, 1, &grade, &explanation, &usage),
    )
    .await
    .expect("the slot a failed attempt left empty accepts the resumed result");
    tx.commit().await.expect("commit the resumed result");
    assert_ne!(
        resumed, first,
        "the resume is a new row, not the old one reused"
    );

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let results: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_case_result
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(case)
    .fetch_one(&mut *tx)
    .await
    .expect("count results");
    let errors: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval_attempt_error
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3",
    )
    .bind(tenant_id)
    .bind(run)
    .bind(case)
    .fetch_one(&mut *tx)
    .await
    .expect("count attempts");
    tx.rollback().await.expect("roll back the read");
    assert_eq!(results, 2, "both filled slots hold their own result");
    assert_eq!(
        errors, 1,
        "the failed attempt is superseded by the success, never deleted"
    );
}
