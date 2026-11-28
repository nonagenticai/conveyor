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
use conveyor_domain::{DeciderKind, Decision, DecisionKind};
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

/// T7 / v1-w1b-4 — an approved gate for an ops-controlled action. `require_approval_tx` refuses
/// the call unless the row's quorum is two and two distinct humans approved it, so this helper
/// writes the quorum and both decisions. The single-decision `approved` stays for the gates whose
/// floor is one (the policy writes, which are not in the ops-controlled set).
async fn approved_dual(pg: &support::Pg, tenant: TenantId, gate: &str, target: &str) -> Uuid {
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
        required_decisions: 2,
        requested_by: None,
        expires_at: None,
        callback_url: None,
        correlation_id: None,
        meta: json!({"target": target}),
    };
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the dual-control gate");
    for approver in [
        support::reviewer_id(pg).await,
        support::reviewer_two_id(pg).await,
    ] {
        store::approval::record_decision_tx(
            &mut tx,
            tenant,
            approval.id,
            &Decision {
                approver_id: approver,
                kind: DecisionKind::Approve,
                note: "dual control under SPEC_V2 §6.5".to_owned(),
                decided_by_kind: DeciderKind::Human,
                via: None,
                item_decisions: None,
            },
            false,
        )
        .await
        .expect("a human decision counts towards the quorum");
    }
    tx.commit().await.expect("commit the dual-control gate");
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

/// The earned-waiver shape D34 requires: an `auto` approver, a typed condition set and a future
/// expiry. `max_reverts = 0` is a real term — any revert revokes the policy.
fn earned_waiver_input(action: &str) -> Value {
    json!({
        "action": action,
        "approval": "waived",
        "self_approval": "forbidden",
        "approver_kind": "auto",
        "conditions": {"max_reverts": 0},
        "expires_at": (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339(),
    })
}

/// The same earned row written straight to the table, for fixtures whose subject is the gate's
/// read rather than the policy write.
async fn seed_earned_waiver(
    pg: &support::Pg,
    tenant: TenantId,
    action: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) {
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
            approver_kind: conveyor_domain::development::ApproverKind::Auto,
            conditions: json!({"max_reverts": 0}),
            expires_at: Some(expires_at),
            required_decisions: 1,
        },
        None,
    )
    .await
    .expect("seed the earned waiver");
    tx.commit().await.expect("commit the earned waiver");
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
        authority: conveyor_actions::Authority::Internal,
        request_id: Some("rid-approval-policies".to_owned()),
        trace_id: Some("trace-approval-policies".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
        credential_alias: None,
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

    let approval_id = approved_dual(
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

/// P2.2 — a waiver covers exactly the action it names. The row is D34's earned shape; this test
/// is the gate's read, so it is seeded rather than written through the (now gated) action.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waived_policy_covers_only_its_action() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");
    seed_earned_waiver(
        &pg,
        seeded.tenant,
        "ops.scale",
        chrono::Utc::now() + chrono::Duration::days(1),
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

/// D34 — a bare `waived` row is not a waiver. Without an `auto` approver, typed conditions and a
/// future expiry the write is refused, and the refusal names the field that is missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bare_waived_write_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let refused = invoke(
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
    .expect_err("an unconditional waiver is no longer writable");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.errors.len(), 1, "{:?}", refused.errors);
    assert_eq!(refused.errors[0].field, "approver_kind");
    assert!(
        refused.errors[0].detail.contains("auto"),
        "the refusal names the earning approver: {}",
        refused.errors[0].detail
    );
}

/// D34 — even the earned shape cannot be written ungated: the policy change itself needs an
/// approved `approval_policies.set` gate naming the action it governs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_earned_waiver_without_an_approval_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            earned_waiver_input("ops.scale"),
            None,
        ),
    )
    .await
    .expect_err("the policy change is itself gated");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "approval_required");
    assert!(
        refused.detail.contains("approval_policies.set"),
        "the refusal names the gated change: {}",
        refused.detail
    );

    // The refused write recorded nothing: the strict default still answers.
    let got = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.get",
            json!({"action": "ops.scale"}),
            None,
        ),
    )
    .await
    .expect("read the default policy");
    assert_eq!(got.body["output"]["approval"], json!("required"));
}

/// D34 — a human approver cannot waive its own gate, even when the policy change itself is
/// properly approved: the waiver must be earned by the `auto` kind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_human_approver_waived_write_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let approval_id = approved(&pg, seeded.tenant, "approval_policies.set", "ops.scale").await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            json!({"action": "ops.scale", "approval": "waived", "self_approval": "forbidden"}),
            Some(approval_id),
        ),
    )
    .await
    .expect_err("a human policy cannot waive its gate");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.errors[0].field, "approver_kind");
}

/// D34 — the conditions are typed, never an empty document; an unparseable set is refused at the
/// field it names.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_earned_waiver_requires_typed_conditions() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let approval_id = approved(&pg, seeded.tenant, "approval_policies.set", "ops.scale").await;
    let mut input = earned_waiver_input("ops.scale");
    input["conditions"] = json!({});

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            input,
            Some(approval_id),
        ),
    )
    .await
    .expect_err("an empty condition set earns nothing");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.errors[0].field, "conditions");
}

/// D34 — an expired auto waiver fails closed: the gate's read answers `required`, so the
/// destructive call still demands its approval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_auto_waiver_does_not_waive() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");
    seed_earned_waiver(
        &pg,
        seeded.tenant,
        "ops.scale",
        chrono::Utc::now() - chrono::Duration::hours(1),
    )
    .await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(seeded.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect_err("an expired policy is not in force");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "approval_required");
}

/// D34 — an earned waiver with the gated change approved is written, and it admits the call it
/// names. This is the whole path: decision → policy row → gate's read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waiver_change_with_a_valid_approval_succeeds() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");
    let approval_id = approved(&pg, seeded.tenant, "approval_policies.set", "ops.scale").await;

    let written = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            earned_waiver_input("ops.scale"),
            Some(approval_id),
        ),
    )
    .await
    .expect("an approved policy change is written");
    assert_eq!(written.body["output"]["approval"], json!("waived"));
    assert_eq!(written.body["output"]["approver_kind"], json!("auto"));
    assert_eq!(
        written.body["output"]["conditions"],
        json!({"max_reverts": 0})
    );

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(seeded.user_id, "ops.scale", scale_input(), None),
    )
    .await
    .expect("the earned waiver admits the call with no approval");
    assert_eq!(outcome.body["output"]["dry_run"], json!(true));
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

    seed_earned_waiver(
        &pg,
        alice.tenant,
        "ops.scale",
        chrono::Utc::now() + chrono::Duration::days(1),
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
