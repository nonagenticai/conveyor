#![forbid(unsafe_code)]
//! `eval_grade` — one output graded once (migration 0034).
//!
//! Two properties, both schema facts rather than caller conventions:
//!
//! - the insert writes `pending` and returns the row id the caller polls;
//! - the terminal write is conditional on `status = 'pending'`, so a replayed `eval.grade` job
//!   settles nothing a second time — the row is the exactly-once guard.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::eval::{
    EvalStoreError, NewEval, NewEvalGrade, TerminalGrade, get_grade_tx, insert_eval_tx,
    insert_pending_tx, set_terminal_tx,
};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn a_grade_is_inserted_pending_and_settled_exactly_once() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-grade', 'Grade') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");

    let (eval_id, version_id) = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let inputs_schema = json!({"type": "object"});
        let eval = insert_eval_tx(
            &mut tx,
            tenant,
            &NewEval {
                name: "one output",
                purpose: "grade a step's product",
                subject_kind: "process",
                entry_action: "process_runs.start",
                model_ref: None,
                prompt_ref: None,
                inputs_schema: &inputs_schema,
                trace_source: "synthesized",
                created_by: None,
            },
        )
        .await
        .expect("insert the eval");
        let version = conveyor_store::eval::create_version_tx(&mut tx, tenant, eval.id, None)
            .await
            .expect("insert the version");
        tx.commit().await.expect("commit the instrument");
        (eval.id, version.id)
    };

    let threshold = Decimal::try_from(0.5_f64).expect("0.5 is a decimal");
    let grade_id = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let id = insert_pending_tx(
            &mut tx,
            tenant,
            &NewEvalGrade {
                eval_id,
                version_id,
                output_hash: "0f1e2d",
                threshold,
            },
        )
        .await
        .expect("a pending grade inserts");
        tx.commit().await.expect("commit the pending row");
        id
    };

    // ── the row is pending, with the identity and the bar ───────────────────────────────
    let pending = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let row = get_grade_tx(&mut tx, tenant, grade_id)
            .await
            .expect("read the grade")
            .expect("the row exists");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.eval_id, eval_id);
    assert_eq!(pending.version_id, version_id);
    assert_eq!(pending.output_hash, "0f1e2d");
    assert_eq!(pending.threshold, threshold);
    assert_eq!(
        pending.grade,
        json!({}),
        "an ungraded row carries no scores"
    );
    assert_eq!(pending.score, None);

    // ── the terminal write settles it ───────────────────────────────────────────────────
    let grade = json!({"accuracy": 1.0});
    let detail = json!({"accuracy": {"expect": "exact", "matched": true}});
    let settled = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let row = set_terminal_tx(
            &mut tx,
            tenant,
            grade_id,
            &TerminalGrade {
                status: "accepted",
                grade: &grade,
                detail: &detail,
                score: Some(Decimal::ONE),
                reason: Some("every quality metric is at or above 0.5"),
                error: None,
            },
        )
        .await
        .expect("the terminal write runs")
        .expect("the pending row is updated");
        tx.commit().await.expect("commit the verdict");
        row
    };
    assert_eq!(settled.status, "accepted");
    assert_eq!(settled.grade, grade);
    assert_eq!(settled.detail, detail);
    assert_eq!(settled.score, Some(Decimal::ONE));

    // ── a second terminal write is a no-op: the row is the exactly-once guard ──────────
    let replayed = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let row = set_terminal_tx(
            &mut tx,
            tenant,
            grade_id,
            &TerminalGrade {
                status: "rejected",
                grade: &json!({"accuracy": 0.0}),
                detail: &json!({}),
                score: Some(Decimal::ZERO),
                reason: Some("a replay must not rewrite a settled verdict"),
                error: None,
            },
        )
        .await
        .expect("a replayed write runs");
        tx.commit().await.expect("commit");
        row
    };
    assert!(replayed.is_none(), "a settled grade is never rewritten");

    let unchanged = {
        let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
        let row = get_grade_tx(&mut tx, tenant, grade_id)
            .await
            .expect("read")
            .expect("the row exists");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(unchanged.status, "accepted");
    assert_eq!(unchanged.score, Some(Decimal::ONE));

    // ── a status outside the three terminal ones is refused before SQL ────────────────
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let refused = set_terminal_tx(
        &mut tx,
        tenant,
        grade_id,
        &TerminalGrade {
            status: "pending",
            grade: &json!({}),
            detail: &json!({}),
            score: None,
            reason: None,
            error: None,
        },
    )
    .await
    .expect_err("`pending` is not a terminal status");
    assert!(
        matches!(refused, EvalStoreError::Invalid { .. }),
        "{refused:?}"
    );

    // A grade that does not exist reads as absent, not as an error.
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let missing = get_grade_tx(&mut tx, tenant, Uuid::now_v7())
        .await
        .expect("an absent id is not a failure");
    assert!(missing.is_none());
}
