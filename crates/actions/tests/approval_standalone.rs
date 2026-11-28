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
use conveyor_domain::development::ApproverKind;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (holds `approvals:write` and `ops:write`).
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

/// The earned-waiver shape D34 requires (§9.3): an `auto` approver, typed conditions and a future
/// expiry. A bare `waived` row admits nothing any more, so the fixture seeds the real shape.
async fn seed_earned_waiver(pg: &support::Pg, tenant: conveyor_domain::TenantId, action: &str) {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the earned waiver");
    store::approval_policy::set(
        &mut tx,
        tenant,
        &store::approval_policy::ApprovalPolicy {
            action: action.to_owned(),
            approval: store::approval_policy::ApprovalRequirement::Waived,
            self_approval: store::approval_policy::SelfApproval::Forbidden,
            approver_kind: ApproverKind::Auto,
            conditions: json!({"max_reverts": 0}),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(1)),
            required_decisions: 1,
        },
        None,
    )
    .await
    .expect("seed the earned waiver");
    tx.commit().await.expect("commit the earned waiver");
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
                // T7 / v1-w1b-4: ops.scale is in the §6.5 dual-control set, so the gate opens
                // with a two-decision quorum and two distinct humans have to decide it.
                "required_decisions": 2,
                "description": "scale conveyor to 3",
                // This test is about the computed target and the requester, not evidence;
                // a destructive gate now requires evidence or an explicit waiver.
                "evidence_waiver_reason": "test fixture: the gate's subject is the target",
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

    // 3b. The second distinct human meets the §6.5 dual-control floor; the gate approves.
    let second = support::reviewer_two_id(&pg).await;
    let approved = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            second,
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect("the second approver decides the gate");
    assert_eq!(
        approved.body["output"]["status"],
        json!("approved"),
        "two distinct human decisions meet the quorum"
    );

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

/// P2.1 — the dev seed's second human: the principal who did NOT request the gate decides it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_principal_decides_a_gate_the_first_requested() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // 1. Alice requests the gate; she is its requester.
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
                // This test is about the computed target and the requester, not evidence;
                // a destructive gate now requires evidence or an explicit waiver.
                "evidence_waiver_reason": "test fixture: the gate's subject is the target",
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
        requested.body["output"]["requested_by"],
        json!(seeded.user_id.to_string()),
        "the requester is the authenticated principal"
    );

    // 2. Her own decision is refused: §5.4's requester exclusion is the default.
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
        ),
    )
    .await
    .expect_err("the requester cannot decide the gate that judges their own work");
    assert_eq!(refused.status, 409, "{}", refused.detail);
    assert_eq!(refused.code, "conflict");
    assert_eq!(
        refused.detail, "the requester cannot decide their own gate",
        "the refusal must be the store's RequesterCannotDecide mapping"
    );

    // 3. The second human in the same tenant decides it, and the gate approves.
    let reviewer = support::reviewer_id(&pg).await;
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
    .expect("a second principal decides the gate");
    assert_eq!(
        decided.body["output"]["status"],
        json!("approved"),
        "one decision from the other principal resolves the gate"
    );
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

/// A gate that cannot compute a target cannot stand alone: the gate would carry no identity for
/// the invoke boundary to compare.
///
/// #283 gave every destructive action a target, so the old shape of this test — naming an action
/// that declares none — no longer exists. What survives is the property: an action whose own
/// target function REFUSES this input yields no target, and the request is a 422 naming the field.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_whose_target_cannot_be_computed_is_refused() {
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
                "gate": "ops.restart",
                "kind": "action",
                "action": "ops.restart",
                // `target_restart` refuses a blank namespace before any cluster call.
                "input": {"namespace": "", "deployment": "conveyor"},
            }),
            None,
        ),
    )
    .await
    .expect_err("an action whose target refuses the input authorises nothing");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors[0].field, "input");
}

/// The tenant's own policy still governs: an earned `waived` row means no gate is needed, so a
/// standalone gate is never required for an action the tenant waived.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waived_action_needs_no_gate_at_all() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    seed_earned_waiver(&pg, seeded.tenant, "ops.scale").await;

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
