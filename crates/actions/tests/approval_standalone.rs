//! #285 — a gate with no parent is a gate for an ACTION, and its target is its identity.
//!
//! The console's destructive flow asks for an approval before it invokes. Most destructive
//! actions (`documents.delete`, `models.delete`, `secrets.delete`, …) belong to no project, no
//! card and no run, so a gate for one has no parent to hang on. Three claims:
//!
//! * a parentless request that names a destructive action and the input it will be invoked with
//!   is accepted, and its `meta.target` is the string the invoke boundary will compare;
//! * that gate, once approved by someone other than its requester, authorises exactly that call;
//! * a parentless request that names nothing is still refused — it authorises nothing and would
//!   belong to no list anybody reads.

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (holds `approvals:write` and `ops:write`).
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
        request_id: Some("rid-approval-standalone".to_owned()),
        trace_id: Some("trace-approval-standalone".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// A dry-run scale: the gate runs, and a pass writes nothing and needs no cluster.
fn scale_input() -> Value {
    json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3, "dry_run": true})
}

/// P1.1 — request → decide → invoke, with no parent anywhere in the chain.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_standalone_gate_authorises_the_call_it_names() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    // 1. The requester asks for a gate that names the action and the input it will use.
    let requested = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "gate": "ops.scale",
                "kind": "action",
                "action": "ops.scale",
                "input": scale_input(),
                "description": "scale conveyor to 3",
            }),
            None,
        ),
    )
    .await
    .expect("a gate that names a destructive action needs no parent");
    let approval_id = requested.body["output"]["id"]
        .as_str()
        .expect("the gate id")
        .to_owned();
    assert_eq!(
        requested.body["output"]["target"],
        json!("conveyor/deployment/conveyor/scale/3"),
        "the target is computed by the action, not typed by the caller"
    );
    assert_eq!(
        requested.body["output"]["requested_by"],
        json!(seeded.user_id.to_string()),
        "the requester is the authenticated principal"
    );
    assert_eq!(
        requested.body["output"]["requested_by_label"],
        json!("Alice"),
        "the approvals screen renders a name, not an id"
    );
    assert_eq!(
        requested.body["output"]["description"],
        json!("scale conveyor to 3")
    );

    // 2. The call is refused while the gate is pending.
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "ops.scale",
            scale_input(),
            Some(Uuid::parse_str(&approval_id).expect("uuid")),
        ),
    )
    .await
    .expect_err("a pending gate authorises nothing");
    assert_eq!(refused.code, "approval_not_approved");

    // 3. Someone other than the requester approves it (SPEC_V2 §5.4).
    let reviewer = support::reviewer_id(&pg).await;
    invoke(
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
    .expect("the reviewer decides the gate");

    // 4. The same call, now naming the gate, passes — and does what it says.
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "ops.scale",
            scale_input(),
            Some(Uuid::parse_str(&approval_id).expect("uuid")),
        ),
    )
    .await
    .expect("an approved standalone gate authorises its action");
    assert_eq!(outcome.body["output"]["dry_run"], json!(true));
}

/// P1.1's refusal — nothing to read, nothing to authorise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_standalone_gate_needs_an_action() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({"gate": "something", "kind": "gate"}),
            None,
        ),
    )
    .await
    .expect_err("a gate with no parent and no action belongs to no list");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "action-invalid-input");
    assert_eq!(error.errors[0].field, "project_id");
    assert_eq!(error.errors[0].code, "missing");
}

/// An action that declares no target cannot be the subject of a standalone gate: the gate would
/// carry no identity for the invoke boundary to compare (#283 declares the rest).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_untargeted_action_cannot_open_a_standalone_gate() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "gate": "documents.delete",
                "kind": "action",
                "action": "documents.delete",
                "input": {"id": Uuid::nil()},
            }),
            None,
        ),
    )
    .await
    .expect_err("an action with no declared target authorises nothing");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors[0].field, "action");
    assert!(
        error.errors[0].detail.contains("declares no approval target"),
        "{}",
        error.errors[0].detail
    );
}

/// The tenant's own policy still governs: `waived` means no gate is needed, so a standalone gate
/// is never required for an action the tenant waived.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waived_action_needs_no_gate_at_all() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            json!({"action": "ops.scale", "approval": "waived", "self_approval": "forbidden"}),
            None,
        ),
    )
    .await
    .expect("waive the action for this tenant");

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(seeded.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect("a waived action runs with no gate");
    assert_eq!(outcome.body["output"]["dry_run"], json!(true));
}
