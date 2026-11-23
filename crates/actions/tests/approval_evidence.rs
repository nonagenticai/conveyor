//! `v1-u5a-approvals-1` (SPEC_V2 §5.4) — `approvals.request` cannot open an evidence-less gate
//! that is supposed to carry evidence.
//!
//! `require_evidence` is a caller's flag and defaults to false, so a destructive gate could be
//! opened with nothing for its approver to read. The rule is now the gate's, not the caller's:
//!
//! * a gate whose name has an `approval_policy` row is a destructive gate, and requires evidence
//!   even when the caller says `require_evidence: false` — the flag can add the requirement,
//!   never remove it;
//! * the one override is an explicit, non-empty `evidence_waiver_reason`, which is stored on the
//!   approval (`meta.evidence_waiver`) so the audit shows why;
//! * an empty or whitespace-only reason is not a waiver, and a non-destructive gate still honours
//!   a caller who turns the requirement on.

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (holds `approvals:write`).
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
        request_id: Some("rid-approval-evidence".to_owned()),
        trace_id: Some("trace-approval-evidence".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// A dry-run scale: the target is computed without any cluster call.
fn scale_input() -> Value {
    json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3, "dry_run": true})
}

/// The gate for a destructive action, in the shape the console's destructive flow uses: the
/// action and its input, so the request computes the target the call will compare against.
fn destructive_request(gate: &str, extra: Value) -> Value {
    let mut input = json!({
        "gate": gate,
        "kind": "action",
        "action": gate,
        "input": scale_input(),
        "evidence": [],
    });
    if let (Some(fields), Some(extra)) = (input.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            fields.insert(key.clone(), value.clone());
        }
    }
    input
}

/// A stored policy row is what marks a destructive action: `approval_policies.set` refuses
/// every other effect. This writes that row through the real action.
async fn require_policy(pg: &support::Pg, seeded: &support::Seeded, action: &str) {
    invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approval_policies.set",
            json!({"action": action, "approval": "required", "self_approval": "forbidden"}),
            None,
        ),
    )
    .await
    .unwrap_or_else(|error| panic!("approval_policies.set refused: {}", error.detail));
}

/// A project row for the non-destructive parent, by raw SQL: these tests are about the evidence
/// rule, not `projects.create`.
async fn project(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status)
         VALUES ($1, $2, 'Evidence gate', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("EVID-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project")
}

async fn approval_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count approvals")
}

async fn gate_meta(pg: &support::Pg, tenant_id: Uuid, id: Uuid) -> Value {
    sqlx::query_scalar("SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the gate")
}

/// The finding: `require_evidence` is omitted (and defaults false), so before the fix the gate
/// opened with nothing to read. The policy row now makes evidence the gate's own requirement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destructive_gate_refuses_an_evidence_less_request() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    require_policy(&pg, &seeded, "ops.scale").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            destructive_request("ops.scale", json!({})),
            None,
        ),
    )
    .await
    .expect_err("a destructive gate with no evidence must be refused");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "gate_evidence_missing", "{}", error.detail);
    assert_eq!(
        approval_count(&pg, seeded.tenant_id).await,
        0,
        "the refused request opened an approval row"
    );
}

/// The §5.4 override: an explicit reason opens the gate, and the row records it. The reason is
/// trimmed before storage, so whitespace around it is not part of the audit text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recorded_waiver_opens_a_destructive_gate() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    require_policy(&pg, &seeded, "ops.scale").await;

    let reason = "the release artefact does not exist yet";
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            destructive_request(
                "ops.scale",
                json!({"evidence_waiver_reason": format!("  {reason}\n")}),
            ),
            None,
        ),
    )
    .await
    .expect("a recorded waiver opens an evidence-less destructive gate");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    let id: Uuid = outcome.body["output"]["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {}", outcome.body));

    let meta = gate_meta(&pg, seeded.tenant_id, id).await;
    assert_eq!(
        meta["evidence_waiver"],
        json!(reason),
        "the override is a row the governance audit reads: {meta}"
    );
}

/// The caller can add the requirement: a non-destructive gate with no policy row opens empty by
/// default, and is refused empty when `require_evidence` is true — a whitespace-only waiver does
/// not stand in for evidence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_non_destructive_gate_honours_a_caller_added_requirement() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let project = project(&pg, seeded.tenant_id).await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "project_id": project,
                "gate": "deploy",
                "kind": "gate",
                "evidence": [],
                "require_evidence": true,
                "evidence_waiver_reason": "   ",
            }),
            None,
        ),
    )
    .await
    .expect_err("a whitespace-only reason is not a waiver");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "gate_evidence_missing", "{}", error.detail);
    assert_eq!(approval_count(&pg, seeded.tenant_id).await, 0);

    // No policy row and no flag: the same gate opens with no evidence, so the refusal above was
    // the caller's own requirement and nothing else.
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            json!({
                "project_id": project,
                "gate": "deploy",
                "kind": "gate",
                "evidence": [],
            }),
            None,
        ),
    )
    .await
    .expect("a non-destructive gate with no requirement opens");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
}

/// `false` cannot remove a destructive gate's requirement, and an empty reason is not a waiver.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn require_evidence_false_cannot_bypass_a_destructive_gate() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    require_policy(&pg, &seeded, "ops.scale").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_as(
            seeded.user_id,
            "approvals.request",
            destructive_request(
                "ops.scale",
                json!({"require_evidence": false, "evidence_waiver_reason": ""}),
            ),
            None,
        ),
    )
    .await
    .expect_err("false cannot remove a requirement the gate carries");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "gate_evidence_missing", "{}", error.detail);
    assert_eq!(
        approval_count(&pg, seeded.tenant_id).await,
        0,
        "the refused request opened an approval row"
    );
}
