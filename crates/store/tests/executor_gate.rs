#![forbid(unsafe_code)]
//! S21 step 5 — an `executor` gate is a person's act (issue #590, ADR 0011).
//!
//! The approval writer is the one boundary every decision path traverses (`approvals.decide`,
//! MCP `tasks/update`, a connector reply). A machine decision on an `executor` gate is refused
//! there, before the gate can resolve; a person's decision still approves it.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{ApprovalStatus, DeciderKind, Decision, TenantId};
use conveyor_store::{
    ApprovalStoreError, NewApproval, begin_scoped, insert_approval_tx, record_decision_tx,
};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_executor_gate_refuses_a_machine_decision() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('exec-gate', 'Exec Gate') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let approver: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('exec-approver') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed approver");
    let requester: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('exec-requester') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed requester");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut approval = NewApproval::new("gate.precheck", "executor");
    approval.requested_by = Some(requester);
    let approval = insert_approval_tx(&mut tx, tenant, &approval)
        .await
        .expect("insert the executor gate");
    tx.commit().await.expect("commit");

    // A machine's decision is refused, and the gate stays pending.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let machine = Decision {
        decided_by_kind: DeciderKind::Agent,
        ..Decision::approve(approver)
    };
    let refused = record_decision_tx(&mut tx, tenant, approval.id, &machine, false)
        .await
        .expect_err("a machine cannot decide an executor gate");
    assert!(
        matches!(refused, ApprovalStoreError::ExecutorGateHumanOnly),
        "{refused}"
    );
    tx.rollback().await.expect("rollback");

    // A person's decision is the gate's normal path.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let decided = record_decision_tx(
        &mut tx,
        tenant,
        approval.id,
        &Decision::approve(approver),
        false,
    )
    .await
    .expect("a person decides the executor gate");
    assert_eq!(decided.status, ApprovalStatus::Approved);
    tx.commit().await.expect("commit");
}
