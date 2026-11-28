//! conveyor#284 — a destructive proposal a person approves actually executes.
//!
//! Before this unit the proposal row carried `action` and `input` but no `target`, and the
//! executor replayed the sealed input with no `approval_id`. The gate that #282 put at the invoke
//! boundary therefore refused the very execution the approval had just permitted: a question
//! answered for nothing.
//!
//! The property under test is symmetric, and both halves matter:
//!
//! * the proposal records the target the **same function** the gate will use computes, so the
//!   approval and the call agree by construction rather than by a caller copying a string;
//! * the executor binds **that** approval — an approval for another object is refused by the
//!   gate, never retried into the object the approver did not name.

mod copilot;

use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_api::copilot::proposal::{Execution, execute};

/// The action every test here proposes: destructive, declares a target, and its `dry_run` writes
/// nothing — so the tests exercise the gate and the binding, not a cluster.
const ACTION: &str = "ops.scale";

fn scale_input(replicas: u32) -> Value {
    json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": replicas, "dry_run": true})
}

/// The target the gate will compute for [`scale_input`].
fn scale_target(replicas: u32) -> String {
    format!("conveyor/deployment/conveyor/scale/{replicas}")
}

/// Boot the copilot with an admin member, propose `ops.scale`, and return the proposal.
///
/// Returns `(running, pg, tenant, approval_id, turn_id, model)` so a test can approve, reject or
/// execute the proposal it was given.
async fn propose_scale(
    replicas: u32,
) -> (
    copilot::support::Running,
    copilot::support::Pg,
    conveyor_domain::TenantId,
    Uuid,
    Uuid,
    copilot::Model,
) {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    // `ops.scale` is `ops:write`, which only an admin holds. The copilot re-checks the
    // proposer's scopes at execute time, so the proposer must genuinely hold them.
    copilot::member(&pg, seeded.tenant_id, "ada", "admin").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let token = "dev:ada";

    model.tool_call("ops_scale", scale_input(replicas));
    let conversation = copilot::conversation(&running, token).await;
    let (status, body) = copilot::turn(&running, token, conversation, "scale it").await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["turn"]["meta"]["status"], "proposed", "{body}");
    let approval_id =
        Uuid::parse_str(body["proposal_id"].as_str().expect("a proposal id")).expect("uuid");
    let turn_id = Uuid::parse_str(body["turn"]["id"].as_str().expect("turn id")).expect("uuid");
    (running, pg, seeded.tenant, approval_id, turn_id, model)
}

/// The `meta.target` recorded on a gate.
async fn recorded_target(
    pg: &copilot::support::Pg,
    tenant: conveyor_domain::TenantId,
    id: Uuid,
) -> Option<String> {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the read");
    let target: Option<String> =
        sqlx::query_scalar("SELECT meta->>'target' FROM approval WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the gate");
    tx.commit().await.expect("commit");
    target
}

/// The quorum recorded on a gate.
async fn recorded_quorum(
    pg: &copilot::support::Pg,
    tenant: conveyor_domain::TenantId,
    id: Uuid,
) -> i64 {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the read");
    let quorum: i64 =
        sqlx::query_scalar("SELECT required_decisions::bigint FROM approval WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the quorum");
    tx.commit().await.expect("commit");
    quorum
}

/// P1.1 — the proposal records the target its gate will compare against, at the §6.5 quorum.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proposal_records_the_target_it_authorises() {
    let (_running, pg, tenant, approval_id, _turn, _model) = propose_scale(3).await;
    assert_eq!(
        recorded_target(&pg, tenant, approval_id).await.as_deref(),
        Some(scale_target(3).as_str()),
        "the proposal must carry the target the invoke boundary will compute"
    );
    assert_eq!(
        recorded_quorum(&pg, tenant, approval_id).await,
        2,
        "an ops-controlled proposal opens the §6.5 dual-control quorum"
    );
}

/// P1.2 — a destructive input that yields no target is refused, not stored unfireable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_proposal_without_a_target_is_refused() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    copilot::member(&pg, seeded.tenant_id, "ada", "admin").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    // "BAD" is a perfectly valid JSON string, so it passes the input schema; what refuses it is
    // the action's own target function, which requires a lowercase Kubernetes name. That is the
    // P1.2 case: the schema cannot catch it, and a stored gate would be unfireable.
    model.tool_call(
        "ops_scale",
        json!({"namespace": "BAD", "deployment": "conveyor", "replicas": 3, "dry_run": true}),
    );
    let conversation = copilot::conversation(&running, "dev:ada").await;
    let (status, body) = copilot::turn(&running, "dev:ada", conversation, "scale it").await;
    assert_ne!(
        status, 201,
        "an untargetable destructive proposal must be refused: {body}"
    );

    // And nothing was stored: no gate, so the turn did not leave a question behind either.
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the read");
    let gates: i64 = sqlx::query_scalar("SELECT count(*) FROM approval WHERE tenant_id = $1")
        .bind(seeded.tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("count the gates");
    tx.commit().await.expect("commit");
    assert_eq!(gates, 0, "no approval row may survive a refused proposal");
}

/// P2.1 — an approved proposal executes its destructive action.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_proposal_executes_its_destructive_action() {
    let (running, pg, tenant, approval_id, turn_id, _model) = propose_scale(3).await;

    // SPEC_V2 §6.5: ops.scale carries dual control, so the proposal opened a two-decision gate
    // and two distinct humans approve it before anything executes.
    for bearer in [copilot::support::REVIEWER, copilot::support::REVIEWER_TWO] {
        let (status, decided) = running
            .invoke(
                "approvals.decide",
                bearer,
                json!({"approval_id": approval_id, "decision": "approve"}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "approvals.decide: {decided}");
    }

    let outcome = execute(
        &running.state.pool,
        &running.state.config.app_key,
        tenant,
        approval_id,
    )
    .await
    .expect("execute the proposal");
    assert!(
        matches!(outcome, Execution::Executed(_)),
        "the approved destructive proposal must execute: {outcome:?}"
    );

    // Exactly one invocation, and the turn cites it — the same shape `copilot_approve` asserts
    // for a non-gated action.
    assert_eq!(copilot::invocations(&pg, tenant.as_uuid(), ACTION).await, 1);
    let meta = copilot::turn_meta(&pg, turn_id).await;
    assert_eq!(meta["status"], "completed", "{meta}");
    let calls = meta["tool_calls"].as_array().expect("tool_calls");
    assert_eq!(calls[0]["action"], ACTION);
    assert!(calls[0]["invocation_id"].is_string(), "{meta}");
}

/// P2.2 — a rejected proposal executes nothing and consumes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_proposal_executes_nothing() {
    let (running, pg, tenant, approval_id, _turn, _model) = propose_scale(3).await;

    let (status, decided) = running
        .invoke(
            "approvals.decide",
            copilot::support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "reject"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "approvals.decide: {decided}");

    let outcome = execute(
        &running.state.pool,
        &running.state.config.app_key,
        tenant,
        approval_id,
    )
    .await
    .expect("execute returns a verdict, not an error");
    assert!(
        !matches!(outcome, Execution::Executed(_)),
        "a rejected proposal must not execute: {outcome:?}"
    );
    assert_eq!(copilot::invocations(&pg, tenant.as_uuid(), ACTION).await, 0);
}

/// P2.3 — an approval for another object cannot execute this one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_for_another_object_cannot_execute() {
    let (running, pg, tenant, approval_id, _turn, _model) = propose_scale(3).await;

    for bearer in [copilot::support::REVIEWER, copilot::support::REVIEWER_TWO] {
        let (status, decided) = running
            .invoke(
                "approvals.decide",
                bearer,
                json!({"approval_id": approval_id, "decision": "approve"}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "approvals.decide: {decided}");
    }

    // The approval is now APPROVED, and it names replicas 9 while the sealed input asks for 3.
    // That is what a gate that only checked "is it approved?" would miss.
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the tamper");
    sqlx::query(
        "UPDATE approval SET meta = jsonb_set(meta, '{target}', to_jsonb($2::text)) WHERE id = $1",
    )
    .bind(approval_id)
    .bind(scale_target(9))
    .execute(&mut *tx)
    .await
    .expect("retarget the approval");
    tx.commit().await.expect("commit");

    let error = execute(
        &running.state.pool,
        &running.state.config.app_key,
        tenant,
        approval_id,
    )
    .await
    .expect_err("an approval for another object must refuse the execution");
    let conveyor_api::copilot::proposal::ExecutionError::Action(action) = &error else {
        panic!("the refusal must come from the gate, not the store: {error:?}");
    };
    assert_eq!(action.status, 403, "{}", action.detail);
    assert_eq!(action.code, "approval_wrong_target");
    // The refusal IS recorded — that is why the gate lives inside the invocation's transaction
    // rather than before it — so the row exists and says `refused`. What must NOT happen is an
    // invocation that ran.
    assert_eq!(copilot::invocations(&pg, tenant.as_uuid(), ACTION).await, 1);
    let outcome: String = sqlx::query_scalar(
        "SELECT outcome FROM action_invocation WHERE tenant_id = $1 AND action = $2",
    )
    .bind(tenant.as_uuid())
    .bind(ACTION)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the invocation outcome");
    assert_eq!(
        outcome, "refused",
        "the gate's refusal is what was recorded"
    );
}

/// v1-u5-copilot-gate-quorum — the tenant's policy quorum reaches both doors.
///
/// The proposal is the gate-opening door a copilot tool call uses; the invoke boundary is the
/// door that authorises the destructive call. The policy row sets THREE decisions for
/// `ops.scale`, while §6.5's dual-control floor is two — so a three-decision gate proves the
/// policy was read, and an approval opened at two must authorise nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_policy_quorum_reaches_the_proposal_and_the_gate() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    copilot::member(&pg, seeded.tenant_id, "ada", "admin").await;
    sqlx::query(
        "INSERT INTO approval_policy
             (tenant_id, action, approval, self_approval, approver_kind, conditions,
              required_decisions, updated_by)
         VALUES ($1, 'ops.scale', 'required', 'forbidden', 'human', '{}'::jsonb, 3, $2)",
    )
    .bind(seeded.tenant_id)
    .bind(seeded.user_id)
    .execute(&pg.super_pool)
    .await
    .expect("the tenant's policy raises the quorum");

    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    model.tool_call("ops_scale", scale_input(3));
    let conversation = copilot::conversation(&running, "dev:ada").await;
    let (status, body) = copilot::turn(&running, "dev:ada", conversation, "scale it").await;
    assert_eq!(status, 201, "{body}");
    let approval_id =
        Uuid::parse_str(body["proposal_id"].as_str().expect("a proposal id")).expect("uuid");

    // The proposal opens at the POLICY's three decisions: the dual-control floor alone is two.
    assert_eq!(
        recorded_quorum(&pg, seeded.tenant, approval_id).await,
        3,
        "a copilot proposal must open at the tenant policy's quorum"
    );

    // One decision leaves the gate pending and executes nothing.
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            copilot::support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    let outcome = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("execute the proposal");
    assert!(
        matches!(outcome, Execution::Skipped),
        "one decision must not authorise a three-decision gate: {outcome:?}"
    );

    // An approval opened below the policy is refused at the call even when the gate is approved:
    // the invoke boundary re-reads the floor rather than trusting whoever opened the gate.
    sqlx::query("UPDATE approval SET required_decisions = 2 WHERE id = $1")
        .bind(approval_id)
        .execute(&pg.super_pool)
        .await
        .expect("open the gate below the policy");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            copilot::support::REVIEWER_TWO,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    let error = execute(
        &running.state.pool,
        &running.state.config.app_key,
        seeded.tenant,
        approval_id,
    )
    .await
    .expect_err("an approval below the policy must refuse the execution");
    let conveyor_api::copilot::proposal::ExecutionError::Action(action) = &error else {
        panic!("the refusal must come from the gate, not the store: {error:?}");
    };
    assert_eq!(action.status, 403, "{}", action.detail);
    assert_eq!(action.code, "approval_below_policy");
}
