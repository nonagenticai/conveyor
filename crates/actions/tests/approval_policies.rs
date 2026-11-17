//! #282 — the tenant decides how its destructive actions are gated, and the defaults are the
//! strict §11.4 / §5.4 rule.
//!
//! The point of every test here is the same: the two settings are DATA. Nothing in the code
//! waives an approval or allows a self-decision for a particular action name, so a tenant that
//! sets nothing keeps the behaviour it had, and a tenant that sets something can be shown exactly
//! what it set.

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

/// P2.3 — §5.4's self-approval exclusion is the default, and the opt-out is a recorded policy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn self_approval_is_a_policy_never_a_hardcode() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;

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
    .expect_err("the default forbids a self-decision");
    assert_eq!(refused.status, 409, "{}", refused.detail);
    assert!(
        refused.detail.contains("own"),
        "the refusal must name the self-decision rule: {}",
        refused.detail
    );

    set_policy(
        &pg,
        seeded.tenant,
        seeded.user_id,
        "ops.restart",
        "required",
        "allowed",
    )
    .await;
    let own = pending(&pg, seeded.tenant, "ops.restart", reviewer).await;
    invoke(
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
    .expect("the policy allows the requester to decide its own gate");

    // The permission is per action: another action's gate is still un-decidable by its requester.
    let other = pending(&pg, seeded.tenant, "ops.scale", reviewer).await;
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            reviewer,
            "approvals.decide",
            json!({"approval_id": other, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect_err("the self-approval policy names one action");
    assert_eq!(refused.status, 409, "{}", refused.detail);
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
        "allowed",
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
