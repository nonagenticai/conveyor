#![forbid(unsafe_code)]
//! `uq_appr_eval_pending`: a second pending sign-off for one `(tenant, eval, gate)` is refused,
//! and it is refused **by name** (S19 Part A step 3, plan trap 4; CONTRACT §20).
//!
//! `uq_appr_pending` is `(tenant_id, run_id, gate)` and an eval gate has `run_id IS NULL`;
//! Postgres treats NULLs as distinct, so that index accepts two pending eval gates. This test
//! pins the index that actually holds, and it asserts the constraint name rather than "the insert
//! failed" — a probe that only checks the outcome passes for the wrong reason (a missing column,
//! a missing grant, a typo) and this one failed exactly that way on a throwaway Postgres 18 on
//! 2026-11-09.
//!
//! The last section is what proves the index is **partial**: once the first gate is decided, a
//! fresh pending row for the same `(tenant, eval, gate)` is accepted again — a re-request reopens
//! the gate while the decided row stays as evidence.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{ApprovalStatus, Decision, TenantId};
use conveyor_store::approval::ApprovalStoreError;
use conveyor_store::begin_scoped;
use conveyor_store::eval::NewEval;
use conveyor_store::eval::insert_eval_tx;
use conveyor_store::{NewApproval, insert_approval_tx, record_decision_tx};
use uuid::Uuid;

/// The gate's columns, as the direct INSERT below spells them.
const DIRECT_INSERT: &str = "INSERT INTO approval (tenant_id, gate, kind, status, eval_id)
     VALUES ($1, 'eval_inputs', 'eval', 'pending', $2)";

/// A minimal eval, so the two evals in this test differ only by name.
fn new_eval<'a>(name: &'a str, schema: &'a serde_json::Value, created_by: Uuid) -> NewEval<'a> {
    NewEval {
        name,
        purpose: "",
        subject_kind: "completions",
        entry_action: "evals.get",
        model_ref: None,
        prompt_ref: None,
        inputs_schema: schema,
        trace_source: "synthesized",
        created_by: Some(created_by),
    }
}

#[tokio::test]
async fn a_second_pending_signoff_is_refused() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('signoff-idx', 'Signoff') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let approver: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('approver') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed app_user");
    let schema = serde_json::json!({});

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval_a = insert_eval_tx(&mut tx, tenant, &new_eval("signoff-a", &schema, approver))
        .await
        .expect("insert eval A");
    let eval_b = insert_eval_tx(&mut tx, tenant, &new_eval("signoff-b", &schema, approver))
        .await
        .expect("insert eval B");
    tx.commit().await.expect("commit the evals");

    // ── (a) the first pending gate inserts, through the store ───────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut gate = NewApproval::new("eval_inputs", "eval");
    gate.eval_id = Some(eval_a.id);
    gate.requested_by = Some(approver);
    gate.meta = serde_json::json!({"artifact_sha256": "digest-a"});
    let first = insert_approval_tx(&mut tx, tenant, &gate)
        .await
        .expect("the first pending eval_inputs gate inserts");
    tx.commit().await.expect("commit the first gate");
    assert_eq!(first.status, ApprovalStatus::Pending);
    assert_eq!(first.gate, "eval_inputs");

    // ── (b) the store path reports Conflict for the duplicate ───────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = insert_approval_tx(&mut tx, tenant, &gate).await;
    tx.rollback().await.expect("roll back the refusal");
    match refused {
        Err(ApprovalStoreError::Conflict) => {}
        other => {
            panic!("the store must report Conflict for a duplicate pending gate, got {other:?}")
        }
    }

    // ── (c) the DIRECT insert names the index ───────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let error = sqlx::query(DIRECT_INSERT)
        .bind(tenant_id)
        .bind(eval_a.id)
        .execute(&mut *tx)
        .await
        .expect_err("a second pending eval_inputs gate must be refused");
    let constraint = error
        .as_database_error()
        .and_then(|database| database.constraint())
        .map(str::to_owned);
    println!("second pending gate refused by {constraint:?}: {error}");
    assert_eq!(
        constraint.as_deref(),
        Some("uq_appr_eval_pending"),
        "the refusal must NAME the partial index; a generic failure would also be produced by a \
         missing column or a missing grant: {error}"
    );
    tx.rollback().await.expect("roll back the refusal");

    // ── (d) a different gate on the same eval is a different sign-off ───────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut grading = NewApproval::new("eval_grading", "eval");
    grading.eval_id = Some(eval_a.id);
    grading.requested_by = Some(approver);
    insert_approval_tx(&mut tx, tenant, &grading)
        .await
        .expect("eval_grading is a different pending gate and must be accepted");
    tx.commit().await.expect("commit the grading gate");

    // ── (e) the same gate on a different eval is a different sign-off ───────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut other_eval = NewApproval::new("eval_inputs", "eval");
    other_eval.eval_id = Some(eval_b.id);
    other_eval.requested_by = Some(approver);
    insert_approval_tx(&mut tx, tenant, &other_eval)
        .await
        .expect("eval_inputs on another eval must be accepted");
    tx.commit().await.expect("commit the second eval's gate");

    // ── (f) the index is partial: a decided row frees the slot ──────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let decided = record_decision_tx(
        &mut tx,
        tenant,
        first.id,
        &Decision::approve(approver),
        true,
    )
    .await
    .expect("approve the first gate");
    assert_eq!(decided.status, ApprovalStatus::Approved);
    let mut re_request = NewApproval::new("eval_inputs", "eval");
    re_request.eval_id = Some(eval_a.id);
    re_request.requested_by = Some(approver);
    let reopened = insert_approval_tx(&mut tx, tenant, &re_request)
        .await
        .expect("the index is partial (status = 'pending'), so a decided row frees the slot");
    tx.commit().await.expect("commit the re-request");
    assert_eq!(reopened.status, ApprovalStatus::Pending);
    assert_ne!(
        reopened.id, first.id,
        "a re-request is a new row; the decided one stays as evidence"
    );
}
