//! #526 (v1-w1b-5) — a project gate's quorum and approver kind come from the tenant's policy
//! row, and `pm.route_gate` may raise that quorum but never lower it.
//!
//! The gate is opened through the real action (`pm.route_gate`), and every assertion reads the
//! row the action wrote through the superuser pool, so RLS cannot hide it. The decide assertions
//! use the one writer every decision path traverses (`approval_policy`'s read lives there).

#[path = "../../api/tests/architecture_support/mod.rs"]
mod arch;
mod support;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::{ApprovalStatus, DeciderKind, Decision, DecisionKind};
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A direct invocation as an admin caller.
fn request(actor: Uuid, action: &str, input: Value, actor_kind: &str) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: actor_kind.to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: if actor_kind == "user" {
            Authority::Role
        } else {
            Authority::Internal
        },
        request_id: Some(format!("rid-gate-policy-{action}")),
        trace_id: Some(format!("trace-gate-policy-{action}")),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// The architecture fixture's seeded project, live delivery contract and compliant Phase-G
/// review: what `pm.route_gate` on `design` requires before it will open the gate.
struct Fixture {
    arch: arch::Fixture,
    project: Uuid,
}

async fn fixture() -> Fixture {
    let arch = arch::fixture().await;
    let project = arch::raw_project(&arch.pg, arch.seeded.tenant_id, "Gate policy").await;
    let contract = arch::live_contract(&arch, &project.to_string(), "delivery_contract", "f").await;
    arch::created_as(
        &arch.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project.to_string(),
            "contract_id": contract,
            "level": "compliant",
            "delegation_ref": arch.delegation,
            "findings": [{ "finding": "the delivery is judged at Phase G" }],
            "evidence": { "ref": "phase G" },
        }),
        None,
    )
    .await;
    Fixture { arch, project }
}

/// `pm.route_gate` on `design`, as the seeded admin, with the recorded evidence waiver the
/// design gate's scenario uses.
async fn route_design(f: &Fixture, required_decisions: Option<u16>) -> Value {
    let mut input = json!({
        "job_binding_id": f.arch.binding,
        "project_id": f.project,
        "gate": "design",
        "kind": "design_gate",
        "evidence": [],
        "evidence_waiver": "the live delivery contract and its review are the design evidence",
    });
    if let Some(count) = required_decisions {
        input["required_decisions"] = json!(count);
    }
    let outcome = invoke(
        &f.arch.pg.runtime,
        registry(),
        f.arch.seeded.tenant,
        request(f.arch.seeded.user_id, "pm.route_gate", input, "user"),
    )
    .await
    .expect("the design gate opens");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    outcome.body
}

async fn approval_row(f: &Fixture, approval_id: Uuid) -> (i32, Value) {
    sqlx::query_as("SELECT required_decisions, meta FROM approval WHERE tenant_id = $1 AND id = $2")
        .bind(f.arch.seeded.tenant_id)
        .bind(approval_id)
        .fetch_one(&f.arch.pg.super_pool)
        .await
        .expect("read the opened gate")
}

/// SPEC_V2 §5.4 — no tenant row is the strict default: one human decision. The decide path
/// reads the same absent row and refuses a machine.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unconfigured_gate_defaults_to_human_and_one_decision() {
    let f = fixture().await;
    let body = route_design(&f, None).await;
    let approval_id: Uuid = body["output"]["approval_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {body}"));

    let (required_decisions, meta) = approval_row(&f, approval_id).await;
    assert_eq!(required_decisions, 1, "the default quorum is one decision");
    assert_eq!(meta["approver_kind"], json!("human"), "{meta}");
    assert_eq!(
        meta["gate_policy"],
        json!({"source": "default", "floor": 1}),
        "an absent row is recorded as the default floor: {meta}"
    );

    // The decide path's policy read is the same absent row: a machine cannot decide it.
    let reviewer = support::reviewer_id(&f.arch.pg).await;
    let refused = invoke(
        &f.arch.pg.runtime,
        registry(),
        f.arch.seeded.tenant,
        request(
            reviewer,
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
            "agent",
        ),
    )
    .await
    .expect_err("an unconfigured gate does not accept a machine decision");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "human_only_gate", "{}", refused.detail);

    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.arch.seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&f.arch.pg.super_pool)
    .await
    .expect("count the decision rows");
    assert_eq!(decisions, 0, "a refused decision writes no row");
}

/// The tenant's row is the floor: `pm.route_gate` sending one decision cannot lower a two-decision
/// policy, and the rule kind the row names is the decider the decide path accepts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tenant_gate_policy_raises_the_quorum_and_pm_route_gate_cannot_lower_it() {
    let f = fixture().await;
    let set = invoke(
        &f.arch.pg.runtime,
        registry(),
        f.arch.seeded.tenant,
        request(
            f.arch.seeded.user_id,
            "approval_policies.set",
            json!({
                "action": "design",
                "approval": "required",
                "self_approval": "forbidden",
                "approver_kind": "rule",
                "required_decisions": 2,
            }),
            "user",
        ),
    )
    .await
    .expect("a project gate is a policy name");
    assert_eq!(set.status, 200, "{}", set.body);

    // The caller asks for one; the tenant's floor is two.
    let body = route_design(&f, Some(1)).await;
    let approval_id: Uuid = body["output"]["approval_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {body}"));
    let (required_decisions, meta) = approval_row(&f, approval_id).await;
    assert_eq!(
        required_decisions, 2,
        "the caller's one cannot lower the tenant's two"
    );
    assert_eq!(meta["approver_kind"], json!("rule"), "{meta}");
    assert_eq!(
        meta["gate_policy"],
        json!({"source": "tenant", "floor": 2}),
        "the row that set the floor is recorded on the gate: {meta}"
    );

    // The tenant's rule names the decider; the one writer records it and keeps the gate open
    // until the second decision (the quorum is the row's, not the caller's).
    let reviewer = support::reviewer_id(&f.arch.pg).await;
    let mut tx = store::scope::begin_scoped(&f.arch.pg.runtime, f.arch.seeded.tenant)
        .await
        .expect("scope the rule decision");
    let decided = store::approval::record_decision_tx(
        &mut tx,
        f.arch.seeded.tenant,
        approval_id,
        &Decision {
            approver_id: reviewer,
            kind: DecisionKind::Approve,
            note: "the tenant's rule decides this gate".to_owned(),
            decided_by_kind: DeciderKind::Rule,
            via: None,
            item_decisions: None,
        },
        false,
    )
    .await
    .expect("the tenant's rule policy accepts a rule decision");
    assert_eq!(
        decided.status,
        ApprovalStatus::Pending,
        "one decision does not meet a two-decision quorum"
    );
    tx.commit().await.expect("commit the rule decision");

    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT decided_by_kind FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.arch.seeded.tenant_id)
    .bind(approval_id)
    .fetch_all(&f.arch.pg.super_pool)
    .await
    .expect("read the decision kind");
    assert_eq!(kinds, vec!["rule".to_owned()]);
}
