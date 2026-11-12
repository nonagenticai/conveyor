#![forbid(unsafe_code)]
//! The Part A proof: readiness is computed from digest-matched sign-offs, and an edit voids them
//! with no status write (S19 Part A step 3; CONTRACT §20 traps 4 and 5).
//!
//! The sequence is the whole claim, one step at a time:
//!
//! 1. a fresh version with cases and a metric is missing **both** sign-off gates;
//! 2. approving `eval_inputs` with the recomputed digest satisfies inputs and leaves grading
//!    missing;
//! 3. approving `eval_grading` with the same digest satisfies both;
//! 4. editing a case input changes the digest, so both gates reopen **while the approval rows
//!    still say `approved`** — there is no void write anywhere;
//! 5. a re-request reopens the gate even though the older approved row exists: readiness reads
//!    the latest row per gate, so a pending one is not a sign-off;
//! 6. re-approving both gates against the new digest satisfies them again, and locking a split
//!    leaves them satisfied (the split is not part of the instrument digest).
//!
//! ⚠️ `Readiness::evaluate` also requires a locked, complete, matching split, so `is_ready()` is
//! asserted at the only point where it can be true without one: after the split is locked in
//! step 6. Before that the split gates are the only ones missing, which is itself the assertion
//! that both sign-offs are satisfied.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::eval::{
    EvalGate, MissingGate, Readiness, ReadinessFacts, SplitCase, draw_split,
};
use conveyor_domain::{Approval, ApprovalStatus, DeciderKind, Decision, TenantId};
use conveyor_store::eval::{
    NewEval, NewEvalCase, NewEvalMetric, UpdateEvalCase, cases_for_split_tx, create_version_tx,
    eval_readiness_tx, insert_case_tx, insert_eval_tx, insert_metric_tx, latest_eval_approval_tx,
    lock_split_tx, update_case_tx, version_digest_tx,
};
use conveyor_store::{ApprovalStoreError, begin_scoped};
use conveyor_store::{NewApproval, insert_approval_tx, record_decision_tx};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// Request one eval sign-off carrying `digest` in `meta.artifact_sha256`.
async fn request_signoff(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    gate: &str,
    digest: &str,
    requested_by: Uuid,
) -> Uuid {
    let mut approval = NewApproval::new(gate, "eval");
    approval.eval_id = Some(eval_id);
    approval.requested_by = Some(requested_by);
    approval.meta = json!({ "artifact_sha256": digest });
    insert_approval_tx(conn, tenant, &approval)
        .await
        .expect("request an eval sign-off")
        .id
}

/// Approve one sign-off as `approver` and return the decided row.
async fn approve_signoff(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    approver: Uuid,
) -> Approval {
    let decided = record_decision_tx(
        conn,
        tenant,
        approval_id,
        &Decision::approve(approver),
        true,
    )
    .await
    .expect("approve the eval sign-off");
    assert_eq!(
        decided.status,
        ApprovalStatus::Approved,
        "a single-approver gate must be approved by one decision"
    );
    decided
}

/// The readiness facts for one `(eval, version)`, read in the caller's transaction.
async fn facts_for(
    conn: &mut PgConnection,
    tenant: TenantId,
    eval_id: Uuid,
    version_id: Uuid,
) -> ReadinessFacts {
    eval_readiness_tx(conn, tenant, eval_id, version_id)
        .await
        .expect("read readiness facts")
}

/// A minimal eval, so the fixture's shape does not distract from the sequence.
fn new_eval<'a>(name: &'a str, schema: &'a Value, created_by: Uuid) -> NewEval<'a> {
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
async fn an_edit_voids_both_signoffs_and_locking_a_split_does_not() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-gate', 'Gate') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('signer') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed app_user");
    let schema = json!({});
    let tags = vec!["suite".to_owned()];
    let attachments = json!([]);
    let meta = json!({});
    let grader = json!({"kind": "exact"});

    // ── the instrument: eval → v1 → 2 cases + 1 quality metric ─────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval = insert_eval_tx(&mut tx, tenant, &new_eval("gate-eval", &schema, user))
        .await
        .expect("insert the eval");
    let version = create_version_tx(&mut tx, tenant, eval.id, Some(user))
        .await
        .expect("v1");
    tx.commit().await.expect("commit eval + v1");

    let expected_alpha = json!({"answer": "alpha"});
    let expected_beta = json!({"answer": "beta"});
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let case_a = insert_case_tx(
        &mut tx,
        tenant,
        &NewEvalCase {
            version_id: version.id,
            input: "alpha",
            tags: &tags,
            expected: Some(&expected_alpha),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("case A");
    insert_case_tx(
        &mut tx,
        tenant,
        &NewEvalCase {
            version_id: version.id,
            input: "beta",
            tags: &tags,
            expected: Some(&expected_beta),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("case B");
    insert_metric_tx(
        &mut tx,
        tenant,
        &NewEvalMetric {
            version_id: version.id,
            key: "pass",
            role: "quality",
            kind: "binary",
            better: "higher",
            label: "pass",
            unit: None,
            grader_kind: "programmatic",
            grader: &grader,
            rubric: None,
            ordinal: 0,
        },
    )
    .await
    .expect("the quality metric");
    let digest_before = version_digest_tx(&mut tx, tenant, version.id)
        .await
        .expect("recompute the digest");
    tx.commit().await.expect("commit the instrument");

    // ── 1. no approvals: both sign-off gates are missing ───────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(!facts.inputs_approved && !facts.grading_approved);
    assert_eq!(facts.metric_count, 1, "the metric row is counted");
    assert_eq!(facts.unsplit_cases, 2, "both cases are still unsplit");
    assert_eq!(
        Readiness::evaluate(facts).missing(),
        &[
            MissingGate::InputsSignoff,
            MissingGate::GradingSignoff,
            MissingGate::SplitMissing,
            MissingGate::SplitIncomplete,
        ],
        "a fresh instrument is missing both sign-offs and its split — in that order"
    );
    tx.rollback().await.expect("roll back the read");

    // ── 2. eval_inputs approved against the digest: grading is still missing ───────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let inputs_approval = request_signoff(
        &mut tx,
        tenant,
        eval.id,
        "eval_inputs",
        &digest_before,
        user,
    )
    .await;
    approve_signoff(&mut tx, tenant, inputs_approval, user).await;
    tx.commit().await.expect("commit the inputs sign-off");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(
        facts.inputs_approved,
        "inputs are approved against the digest"
    );
    assert!(!facts.grading_approved, "grading has no sign-off yet");
    assert_eq!(
        Readiness::evaluate(facts).missing(),
        &[
            MissingGate::GradingSignoff,
            MissingGate::SplitMissing,
            MissingGate::SplitIncomplete,
        ],
        "only the grading sign-off and the split remain"
    );
    tx.rollback().await.expect("roll back the read");

    // ── 3. eval_grading approved against the same digest: both are satisfied ───────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let grading_approval = request_signoff(
        &mut tx,
        tenant,
        eval.id,
        "eval_grading",
        &digest_before,
        user,
    )
    .await;
    approve_signoff(&mut tx, tenant, grading_approval, user).await;
    tx.commit().await.expect("commit the grading sign-off");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(facts.inputs_approved && facts.grading_approved);
    assert_eq!(
        Readiness::evaluate(facts).missing(),
        &[MissingGate::SplitMissing, MissingGate::SplitIncomplete],
        "both sign-offs are satisfied; only the split is outstanding — which is why \
         is_ready() cannot be true until step 6"
    );
    tx.rollback().await.expect("roll back the read");

    // ── 4. an edit changes the digest and voids BOTH gates, with no status write ───────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    update_case_tx(
        &mut tx,
        tenant,
        case_a.id,
        &UpdateEvalCase {
            input: "alpha, edited",
            tags: &tags,
            expected: Some(&expected_alpha),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("edit case A");
    let digest_after = version_digest_tx(&mut tx, tenant, version.id)
        .await
        .expect("recompute the digest after the edit");
    assert_ne!(
        digest_after, digest_before,
        "editing a case input must change the instrument digest"
    );
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(
        !facts.inputs_approved && !facts.grading_approved,
        "both gates reopen when the digest changes"
    );

    // …and the rows themselves still say `approved`: the void is the mismatch, not a status.
    let still_inputs = latest_eval_approval_tx(&mut tx, tenant, eval.id, EvalGate::Inputs.as_str())
        .await
        .expect("read the inputs gate")
        .expect("the inputs gate exists");
    assert_eq!(still_inputs.status, "approved");
    assert_eq!(
        still_inputs.artifact_sha256.as_deref(),
        Some(digest_before.as_str()),
        "the approved row still carries the OLD digest — that is the whole void mechanism"
    );
    assert_ne!(
        still_inputs.artifact_sha256.as_deref(),
        Some(digest_after.as_str())
    );
    let still_grading =
        latest_eval_approval_tx(&mut tx, tenant, eval.id, EvalGate::Grading.as_str())
            .await
            .expect("read the grading gate")
            .expect("the grading gate exists");
    assert_eq!(still_grading.status, "approved");
    tx.commit().await.expect("commit the edit");

    // ── 5. a re-request reopens the gate: readiness reads the LATEST row ───────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let reopened_inputs =
        request_signoff(&mut tx, tenant, eval.id, "eval_inputs", &digest_after, user).await;
    tx.commit().await.expect("commit the re-request");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(
        !facts.inputs_approved,
        "the latest eval_inputs row is pending, so the gate is open again even though an older \
         approved row exists"
    );
    let latest = latest_eval_approval_tx(&mut tx, tenant, eval.id, EvalGate::Inputs.as_str())
        .await
        .expect("read the latest inputs gate")
        .expect("a latest row exists");
    assert_eq!(latest.id, reopened_inputs);
    assert_eq!(latest.status, "pending");
    tx.rollback().await.expect("roll back the read");

    // ── 6. re-approve both gates against the new digest, then lock the split ───────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    approve_signoff(&mut tx, tenant, reopened_inputs, user).await;
    let fresh_grading = request_signoff(
        &mut tx,
        tenant,
        eval.id,
        "eval_grading",
        &digest_after,
        user,
    )
    .await;
    approve_signoff(&mut tx, tenant, fresh_grading, user).await;
    tx.commit().await.expect("commit the re-approvals");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert!(facts.inputs_approved && facts.grading_approved);

    let split_cases = cases_for_split_tx(&mut tx, tenant, version.id)
        .await
        .expect("the cases the draw may see");
    assert_eq!(split_cases.len(), 2);
    let borrows: Vec<SplitCase<'_>> = split_cases
        .iter()
        .map(|(case_id, case_tags)| SplitCase {
            id: *case_id,
            tags: case_tags,
        })
        .collect();
    let assignment = draw_split(&borrows, 0.5, b"s19-gate").expect("draw a split");
    lock_split_tx(
        &mut tx,
        tenant,
        version.id,
        &assignment.split_sha256,
        &assignment.per_case,
    )
    .await
    .expect("lock the split");

    let facts = facts_for(&mut tx, tenant, eval.id, version.id).await;
    assert_eq!(
        facts.unsplit_cases, 0,
        "the lock writes every case's side in the same transaction"
    );
    assert!(
        facts.split_matches,
        "the stored hash equals the recomputed one"
    );
    assert!(
        Readiness::evaluate(facts).is_ready(),
        "locking a split must not disturb the sign-offs: {facts:?}"
    );
    tx.commit().await.expect("commit the split lock");
}

/// W1 (S06d): the shared writer is the one boundary every decision path traverses. A machine
/// decision on an eval gate is refused even when the caller claims `eval_authorized`; a human
/// decision is refused when it does not hold `evals:approve`. Neither refusal writes a row.
#[tokio::test]
async fn a_machine_or_an_unscoped_human_cannot_decide_an_eval_gate() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ('w1', 'W1') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('w1-signer') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed app_user");
    let schema = json!({});

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval = insert_eval_tx(&mut tx, tenant, &new_eval("w1-eval", &schema, user))
        .await
        .expect("insert the eval");
    let approval = request_signoff(&mut tx, tenant, eval.id, "eval_inputs", "sha", user).await;
    tx.commit().await.expect("commit");

    // A machine's decision is refused even if the caller passes `eval_authorized = true`.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let machine = Decision {
        decided_by_kind: DeciderKind::Agent,
        ..Decision::approve(user)
    };
    let refused = record_decision_tx(&mut tx, tenant, approval, &machine, true)
        .await
        .expect_err("a machine cannot sign an eval gate");
    assert!(matches!(refused, ApprovalStoreError::HumanOnlyGate));
    tx.rollback().await.expect("roll back");

    // A human without `evals:approve` is refused by the same boundary.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = record_decision_tx(&mut tx, tenant, approval, &Decision::approve(user), false)
        .await
        .expect_err("a human needs the sign-off scope");
    assert!(matches!(refused, ApprovalStoreError::EvalScopeRequired));
    tx.rollback().await.expect("roll back");

    // The gate is still pending: neither refusal wrote a decision row.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let pending: String =
        sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(approval)
            .fetch_one(&mut *tx)
            .await
            .expect("read the gate");
    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval)
    .fetch_one(&mut *tx)
    .await
    .expect("count decisions");
    tx.commit().await.expect("commit");
    assert_eq!(pending, "pending");
    assert_eq!(decisions, 0);
}
