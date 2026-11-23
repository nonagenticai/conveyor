//! #282 — the tenant decides how its destructive actions are gated, and the defaults are the
//! strict §11.4 / §5.4 rule.
//!
//! `approval` is DATA: nothing in the code waives an approval for a particular action name, so a
//! tenant that sets nothing keeps the behaviour it had, and a tenant that sets something can be
//! shown exactly what it set. `self_approval` is the opposite — §5.4 / P4 make the requester's
//! exclusion an invariant, so `allowed` cannot be stored and a historical `allowed` row
//! authorises nothing (ledger v1-w4b-4).

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (holds `ops:write` and `approvals:write`).
fn request_as(actor: Uuid, action: &str, input: Value, approval_id: Option<Uuid>) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-approval-policies".to_owned()),
        trace_id: Some("trace-approval-policies".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// A dry-run scale: the gate runs, and a pass writes nothing and needs no cluster.
fn scale_input() -> Value {
    json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3, "dry_run": true})
}

/// An APPROVED gate for `gate` whose `meta.target` is `target`.
async fn approved(pg: &support::Pg, tenant: TenantId, gate: &str, target: &str) -> Uuid {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the approval insert");
    let new = store::approval::NewApproval {
        run_id: None,
        card_id: None,
        project_id: None,
        eval_id: None,
        gate: gate.to_owned(),
        kind: "gate".to_owned(),
        required_decisions: 1,
        requested_by: None,
        expires_at: None,
        callback_url: None,
        correlation_id: None,
        meta: json!({"target": target}),
    };
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the gate");
    sqlx::query("UPDATE approval SET status = 'approved' WHERE id = $1")
        .bind(approval.id)
        .execute(&mut *tx)
        .await
        .expect("approve the gate");
    tx.commit().await.expect("commit the gate");
    approval.id
}

/// A PENDING gate requested by `requested_by`, on `gate`.
async fn pending(pg: &support::Pg, tenant: TenantId, gate: &str, requested_by: Uuid) -> Uuid {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the approval insert");
    let new = store::approval::NewApproval {
        run_id: None,
        card_id: None,
        project_id: None,
        eval_id: None,
        gate: gate.to_owned(),
        kind: "gate".to_owned(),
        required_decisions: 1,
        requested_by: Some(requested_by),
        expires_at: None,
        callback_url: None,
        correlation_id: None,
        meta: json!({}),
    };
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the gate");
    tx.commit().await.expect("commit the gate");
    approval.id
}

/// Record `approval`/`self_approval` for `action` through the real action, as `actor`.
async fn set_policy(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    action: &str,
    approval: &str,
    self_approval: &str,
) {
    invoke(
        &pg.runtime,
        registry(),
        tenant,
        request_as(
            actor,
            "approval_policies.set",
            json!({"action": action, "approval": approval, "self_approval": self_approval}),
            None,
        ),
    )
    .await
    .unwrap_or_else(|error| panic!("approval_policies.set refused: {}", error.detail));
}

/// Record the complete policy, including the approver kind the tenant declares non-governance.
async fn set_policy_kind(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    action: &str,
    approval: &str,
    self_approval: &str,
    approver_kind: &str,
) {
    invoke(
        &pg.runtime,
        registry(),
        tenant,
        request_as(
            actor,
            "approval_policies.set",
            json!({
                "action": action,
                "approval": approval,
                "self_approval": self_approval,
                "approver_kind": approver_kind,
            }),
            None,
        ),
    )
    .await
    .unwrap_or_else(|error| panic!("approval_policies.set refused: {}", error.detail));
}

/// A request from a principal of `actor_kind`: the decide path turns `agent`/`service` into a
/// machine [`DeciderKind`](conveyor_domain::DeciderKind), never a caller's own claim.
fn request_as_kind(actor: Uuid, actor_kind: &str, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: actor_kind.to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-approval-policies".to_owned()),
        trace_id: Some("trace-approval-policies".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// SPEC_V2 §5.4 — a human approver is the DEFAULT. A machine decision on a gate the tenant has
/// not declared non-governance is refused `403 human_only_gate`, and writes no decision row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_decision_on_an_undeclared_gate_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let approval_id = pending(&pg, seeded.tenant, "ops.scale", seeded.user_id).await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as_kind(
            reviewer,
            "agent",
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
        ),
    )
    .await
    .expect_err("an undeclared gate does not accept a machine decision");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "human_only_gate");
    assert!(
        refused.detail.contains("ops.scale"),
        "the refusal names the gate: {}",
        refused.detail
    );

    let mut tx = store::scope::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the decision count");
    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(approval_id)
    .fetch_one(&mut *tx)
    .await
    .expect("count the decision rows");
    assert_eq!(decisions, 0, "a refused decision writes no row");
    tx.rollback().await.expect("roll back");
}

/// SPEC_V2 §5.4 — a tenant may authorise one non-human approver kind for a gate it explicitly
/// declares non-governance; that row admits the job/agent decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tenant_declared_non_governance_gate_accepts_a_job_decision() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let approval_id = pending(&pg, seeded.tenant, "ops.scale", seeded.user_id).await;
    set_policy_kind(
        &pg,
        seeded.tenant,
        seeded.user_id,
        "ops.scale",
        "required",
        "forbidden",
        "agent",
    )
    .await;

    let decided = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as_kind(
            reviewer,
            "agent",
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
        ),
    )
    .await
    .expect("the tenant declared the agent approver for this gate");
    assert_eq!(decided.body["output"]["status"], json!("approved"));
}

/// SPEC_V2 §5.4 — with no policy row at all, a human decision is accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_human_decision_is_accepted_by_default() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let approval_id = pending(&pg, seeded.tenant, "ops.scale", seeded.user_id).await;

    let decided = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            reviewer,
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect("a human approver is the default");
    assert_eq!(decided.body["output"]["status"], json!("approved"));
}

/// P2.1 — with no policy rows, the default is `required`: an approved approval is honoured and
/// its absence is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_default_policy_is_required() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(seeded.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect_err("the default is required");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "approval_required");

    let approval_id = approved(
        &pg,
        seeded.tenant,
        "ops.scale",
        "conveyor/deployment/conveyor/scale/3",
    )
    .await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "ops.scale",
            scale_input(),
            Some(approval_id),
        ),
    )
    .await
    .expect("an approved approval authorises the call");
    assert_eq!(outcome.body["output"]["dry_run"], json!(true));
}

/// P2.2 — a waiver covers exactly the action it names.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waived_policy_covers_only_its_action() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");
    set_policy(
        &pg,
        seeded.tenant,
        seeded.user_id,
        "ops.scale",
        "waived",
        "forbidden",
    )
    .await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(seeded.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect("the waiver admits the call with no approval");
    assert_eq!(outcome.body["output"]["dry_run"], json!(true));

    // The neighbour is untouched: a waiver is per action, never per effect class or per tenant.
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "ops.restart",
            json!({"namespace": "conveyor", "deployment": "conveyor", "dry_run": true}),
            None,
        ),
    )
    .await
    .expect_err("a waiver for scale must not admit restart");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "approval_required");
}

/// Write a historical `self_approval = 'allowed'` row straight to the table, as a deployment
/// that ran before ledger v1-w4b-4 still carries one. `approval_policies.set` now refuses the
/// value, so only SQL can model the old data the reader must keep parsing.
async fn seed_historical_allow(pg: &support::Pg, tenant: TenantId, action: &str) {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the historical policy write");
    sqlx::query(
        "INSERT INTO approval_policy (tenant_id, action, approval, self_approval)
         VALUES ($1, $2, 'required', 'allowed')
         ON CONFLICT (tenant_id, action) DO UPDATE SET self_approval = 'allowed'",
    )
    .bind(tenant.as_uuid())
    .bind(action)
    .execute(&mut *tx)
    .await
    .expect("seed the historical policy");
    tx.commit().await.expect("commit the historical policy");
}

/// P2.3 (ledger v1-w4b-4) — `self_approval = allowed` is unstorable, while a historical row
/// still reads: old data must not break the reader, but it must never be writable again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn allowed_self_approval_cannot_be_stored() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            json!({"action": "ops.restart", "approval": "required", "self_approval": "allowed"}),
            None,
        ),
    )
    .await
    .expect_err("§5.4 is an invariant, so `allowed` cannot be stored");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.errors.len(), 1, "{:?}", refused.errors);
    assert_eq!(refused.errors[0].field, "self_approval");
    assert_eq!(refused.errors[0].code, "invalid_value");
    assert!(
        refused.errors[0].detail.contains("5.4"),
        "the refusal must name SPEC_V2 §5.4: {}",
        refused.errors[0].detail
    );

    // The refused write recorded no row: the strict default still answers.
    let got = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.get",
            json!({"action": "ops.restart"}),
            None,
        ),
    )
    .await
    .expect("read the default policy");
    assert_eq!(got.body["output"]["self_approval"], json!("forbidden"));

    // A row written before the invariant still parses — the compatibility that keeps old data
    // readable, proven by this read succeeding. The answer is the invariant, and the stored
    // value has not been rewritten; the next test proves it authorises nothing.
    seed_historical_allow(&pg, seeded.tenant, "ops.restart").await;
    let got = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.get",
            json!({"action": "ops.restart"}),
            None,
        ),
    )
    .await
    .expect("a historical policy still reads");
    assert_eq!(got.body["output"]["self_approval"], json!("forbidden"));

    let mut tx = store::scope::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the historical read");
    let stored: String = sqlx::query_scalar(
        "SELECT self_approval FROM approval_policy WHERE tenant_id = $1 AND action = $2",
    )
    .bind(seeded.tenant.as_uuid())
    .bind("ops.restart")
    .fetch_one(&mut *tx)
    .await
    .expect("read the historical row");
    tx.commit().await.expect("commit the historical read");
    assert_eq!(stored, "allowed", "the old row must not be rewritten");
}

/// P2.3 (ledger v1-w4b-4) — the requester cannot decide its own gate even when a historical
/// policy row says `allowed`, and a distinct approver still can.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_requester_cannot_decide_its_own_gate_even_with_a_historical_allowed_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    seed_historical_allow(&pg, seeded.tenant, "ops.restart").await;

    let own = pending(&pg, seeded.tenant, "ops.restart", reviewer).await;
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            reviewer,
            "approvals.decide",
            json!({"approval_id": own, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect_err("a stored `allowed` row must not authorise a self-decision");
    assert_eq!(refused.status, 409, "{}", refused.detail);
    assert!(
        refused.detail.contains("own"),
        "the refusal must name the self-decision rule: {}",
        refused.detail
    );

    // A distinct person's decision is exactly what the invariant still permits.
    let decided = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.decide",
            json!({"approval_id": own, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect("a distinct approver can decide the gate");
    assert_eq!(decided.body["output"]["status"], json!("approved"));
}

/// P2.4 — `approvals.request` computes the target from the action, so no caller types it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_computes_the_target_it_will_need() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` reads its lanes from the `standard` board_template row, which is a
    // platform catalogue seed rather than tenant data (`migrate --seed`, not `--dev-seed`).
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("seed the catalogue");

    let project = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "projects.create",
            json!({"title": "Gate target"}),
            None,
        ),
    )
    .await
    .expect("create a project");
    let project_id = project.body["output"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "project_id": project_id,
                "gate": "ops.scale",
                "kind": "gate",
                "action": "ops.scale",
                "input": {"namespace": "conveyor", "deployment": "conveyor", "replicas": 3},
                "evidence_waiver_reason": "test fixture: the gate's subject is the target",
            }),
            None,
        ),
    )
    .await
    .expect("request the gate");
    let approval_id: Uuid = outcome.body["output"]["id"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a uuid");

    // The caller supplied no target string; the row carries the one the invoke boundary will
    // compare against. Reading it back is the whole claim.
    let mut tx = store::scope::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the read");
    let target: Option<String> =
        sqlx::query_scalar("SELECT meta->>'target' FROM approval WHERE id = $1")
            .bind(approval_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the gate");
    tx.commit().await.expect("commit");
    assert_eq!(
        target.as_deref(),
        Some("conveyor/deployment/conveyor/scale/3")
    );

    // A literal target that disagrees with the computed one is a caller bug, refused rather than
    // stored as a gate that can never authorise the call it was opened for.
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "project_id": project_id,
                "gate": "ops.scale",
                "kind": "gate",
                "action": "ops.scale",
                "target": "conveyor/deployment/elsewhere/scale/3",
                "input": {"namespace": "conveyor", "deployment": "conveyor", "replicas": 3},
                "evidence_waiver_reason": "test fixture: the gate's subject is the target",
            }),
            None,
        ),
    )
    .await
    .expect_err("a target that disagrees with the action's is refused");
    assert_eq!(refused.status, 422, "{}", refused.detail);
}

/// P2.5 — a policy is tenant data: another tenant cannot read it, and never inherits it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn policies_are_tenant_scoped() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;

    set_policy(
        &pg,
        alice.tenant,
        alice.user_id,
        "ops.scale",
        "waived",
        "forbidden",
    )
    .await;

    let listed = invoke(
        &pg.runtime,
        registry(),
        alice.tenant,
        request_as(alice.user_id, "approval_policies.list", json!({}), None),
    )
    .await
    .expect("list the first tenant's policies");
    assert_eq!(listed.body["output"].as_array().map(Vec::len), Some(1));
    assert_eq!(listed.body["output"][0]["action"], json!("ops.scale"));
    assert_eq!(listed.body["output"][0]["approval"], json!("waived"));

    // The second tenant set nothing, so it sees nothing — and its `get` answers with the strict
    // default rather than with a neighbour's row.
    let listed = invoke(
        &pg.runtime,
        registry(),
        bob.tenant,
        request_as(bob.user_id, "approval_policies.list", json!({}), None),
    )
    .await
    .expect("list the second tenant's policies");
    assert_eq!(listed.body["output"].as_array().map(Vec::len), Some(0));

    let got = invoke(
        &pg.runtime,
        registry(),
        bob.tenant,
        request_as(
            bob.user_id,
            "approval_policies.get",
            json!({"action": "ops.scale"}),
            None,
        ),
    )
    .await
    .expect("read the second tenant's default");
    assert_eq!(got.body["output"]["approval"], json!("required"));
    assert_eq!(got.body["output"]["self_approval"], json!("forbidden"));

    // And the waiver does not leak into the other tenant's gate.
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");
    let refused = invoke(
        &pg.runtime,
        registry(),
        bob.tenant,
        request_as(bob.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect_err("a neighbour's waiver must not admit this call");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "approval_required");
}
