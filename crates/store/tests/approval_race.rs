//! Gate property (c): one pending approval per `(run, gate)` under concurrency.
//!
//! Sixteen tasks insert the same pending gate at once. The partial unique index
//! `uq_appr_pending` (not a check-then-insert) decides: one `Ok`, fifteen
//! `ApprovalConflict`, and exactly one pending row afterwards.
//!
//! `cargo test -p conveyor-store --test approval_race -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{ApprovalError, ApprovalStatus, Decision, TenantId};
use conveyor_store::approval::{
    ApprovalStoreError, NewApproval, insert_approval_tx, record_decision_tx,
};
use conveyor_store::begin_scoped;
use tokio::task::JoinSet;

#[tokio::test]
async fn sixteen_racers_create_exactly_one_pending_approval() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "appr").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let mut racers = JoinSet::new();
    for _ in 0..16 {
        let pool = db.runtime.clone();
        let run = seed.run;
        let project = seed.project;
        racers.spawn(async move {
            let mut input = NewApproval::new("race", "gate");
            input.run_id = Some(run);
            input.project_id = Some(project);
            let mut tx = begin_scoped(&pool, tenant).await.expect("scope");
            match insert_approval_tx(&mut tx, tenant, &input).await {
                Ok(approval) => {
                    tx.commit().await.expect("commit the winner");
                    Ok(approval)
                }
                Err(ApprovalStoreError::Conflict) => {
                    tx.rollback().await.expect("roll back the loser");
                    Err(ApprovalStoreError::Conflict)
                }
                Err(other) => panic!("unexpected insert error: {other}"),
            }
        });
    }

    let mut winners = 0;
    let mut conflicts = 0;
    while let Some(joined) = racers.join_next().await {
        match joined.expect("a racer must not panic") {
            Ok(_) => winners += 1,
            Err(ApprovalStoreError::Conflict) => conflicts += 1,
            Err(other) => panic!("unexpected racer error: {other}"),
        }
    }
    println!("16 racers: {winners} ok, {conflicts} conflict");
    assert_eq!(winners, 1, "exactly one pending approval may be created");
    assert_eq!(conflicts, 15, "every loser must see ApprovalConflict");

    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND run_id = $2
           AND gate = 'race' AND status = 'pending'",
    )
    .bind(seed.tenant)
    .bind(seed.run)
    .fetch_one(&db.super_pool)
    .await
    .expect("count pending");
    println!("pending approvals for (run, 'race'): {pending}");
    assert_eq!(pending, 1, "the database holds exactly one pending gate");

    // The existing seeded `review` gate is untouched and still pending.
    let review_pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND status = 'pending'",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count all pending");
    assert_eq!(
        review_pending, 2,
        "the race gate plus the seeded review gate"
    );
}

#[tokio::test]
async fn dual_control_needs_two_distinct_approvers_and_one_decision_each() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dual").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let approval = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let mut input = NewApproval::new("dual", "gate");
        input.run_id = Some(seed.run);
        input.project_id = Some(seed.project);
        input.required_decisions = 2;
        let approval = insert_approval_tx(&mut tx, tenant, &input)
            .await
            .expect("insert the gate");
        tx.commit().await.expect("commit");
        approval
    };

    let after_first = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let decided = record_decision_tx(
            &mut tx,
            tenant,
            approval.id,
            &Decision::approve(seed.user),
            false,
        )
        .await
        .expect("first decision");
        tx.commit().await.expect("commit");
        decided
    };
    println!(
        "after one approval: status={} approvals={}",
        after_first.status, after_first.approvals
    );
    assert_eq!(after_first.status.as_str(), "pending");
    assert_eq!(after_first.approvals, 1);

    // The same approver cannot decide twice (unique (approval_id, approver_id)).
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let duplicate = record_decision_tx(
        &mut tx,
        tenant,
        approval.id,
        &Decision::approve(seed.user),
        false,
    )
    .await;
    tx.rollback().await.expect("roll back");
    assert!(
        matches!(duplicate, Err(ApprovalStoreError::AlreadyDecided)),
        "one approver cannot satisfy dual control twice: {duplicate:?}"
    );

    let after_second = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let decided = record_decision_tx(
            &mut tx,
            tenant,
            approval.id,
            &Decision::approve(seed.other_user),
            false,
        )
        .await
        .expect("second decision");
        tx.commit().await.expect("commit");
        decided
    };
    println!(
        "after two approvals: status={} approvals={}",
        after_second.status, after_second.approvals
    );
    assert_eq!(after_second.status.as_str(), "approved");

    let stored: (String, i64) = sqlx::query_as(
        "SELECT status, (SELECT count(*) FROM approval_decision WHERE approval_id = $2)
           FROM approval WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seed.tenant)
    .bind(approval.id)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the gate");
    assert_eq!(stored, ("approved".to_owned(), 2));
}

/// A refused decision must not insert a row first.
///
/// The second call is COMMITTED, not rolled back, on purpose: the decision has to be absent
/// because the store validated the gate before writing, never because the caller undid a
/// write it should not have made.
#[tokio::test]
async fn a_refused_decision_writes_no_decision_row_even_when_committed() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "refuse").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");
    // The seeded fixture already recorded one decision (by `other_user`), so the refused
    // approver must be a THIRD user: with a duplicate approver the unique index would hide
    // the ordering bug behind `AlreadyDecided`.
    let late: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('late approver') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a third approver");
    let tenant_id = seed.tenant;
    let approval_id = seed.approval;
    let count = |approver: Option<uuid::Uuid>| {
        let pool = db.super_pool.clone();
        async move {
            let rows: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM approval_decision
                  WHERE tenant_id = $1 AND approval_id = $2
                    AND ($3::uuid IS NULL OR approver_id = $3)",
            )
            .bind(tenant_id)
            .bind(approval_id)
            .bind(approver)
            .fetch_one(&pool)
            .await
            .expect("count decisions");
            rows
        }
    };

    // The seeded `review` gate is pending; `user` rejects it.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let rejected = record_decision_tx(
        &mut tx,
        tenant,
        seed.approval,
        &Decision::reject(seed.user),
        false,
    )
    .await
    .expect("the first decision resolves the gate");
    assert_eq!(rejected.status, ApprovalStatus::Rejected);
    tx.commit().await.expect("commit the rejection");
    let before = count(None).await;
    println!("decision rows before the refused call: {before}");

    // A fresh approver now tries to approve the resolved gate. The domain refuses it; the
    // caller COMMITS whatever the call left behind.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = record_decision_tx(
        &mut tx,
        tenant,
        seed.approval,
        &Decision::approve(late),
        false,
    )
    .await;
    tx.commit()
        .await
        .expect("commit the refused call's transaction");
    assert!(
        matches!(
            refused,
            Err(ApprovalStoreError::Illegal(ApprovalError::NotPending {
                status: ApprovalStatus::Rejected
            }))
        ),
        "a resolved gate must refuse the decision: {refused:?}"
    );

    let after = count(None).await;
    let by_late = count(Some(late)).await;
    println!("decision rows after the refused call: {after} (by the refused approver: {by_late})");
    assert_eq!(
        by_late, 0,
        "the refused decision inserted no row for its approver"
    );
    assert_eq!(
        after, before,
        "the refused call changed nothing; only a caller rollback would have hidden an insert"
    );
}
