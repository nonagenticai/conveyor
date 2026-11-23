//! v1-p44-2 (T0) — a job binding goes live only through an approved gate (SPEC_V2 §4.4).
//!
//! `cargo test -p conveyor-actions --test job_binding_gate`.
//!
//! The finding: `job_bindings.create` defaulted `status` to `active` and wrote no decision, so
//! one `organisation:write` call made a release executable with no approver in the audit. Now
//! the default create writes a **draft** and opens a `job_binding` gate; only an approved
//! decision flips it to `active`, a direct `status: "active"` is refused
//! `409 job_binding_approval_required`, and a non-approved decision leaves it draft.
//!
//! Everything runs through the public action surface (`conveyor_actions::invoke`'s HTTP door),
//! with the fixture rows the organisation spine needs seeded through the store.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewIndustry, NewJob, PlacementIn, industry_create_tx, job_create_tx, job_release_publish_tx,
};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

struct Fixture {
    pg: support::Pg,
    running: support::Running,
    seeded: support::Seeded,
    job: Uuid,
    release: Uuid,
    capability: Uuid,
    agent: Uuid,
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn field<'a>(body: &'a Value, name: &str) -> &'a str {
    output(body)[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name} is absent from {body}"))
}

/// A job (internal_agent executor), its published release, and one active capability agent.
async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (job, release, capability, agent) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let industry = industry_create_tx(
            &mut tx,
            seeded.tenant,
            &NewIndustry {
                key: "software".to_owned(),
                name: "Software".to_owned(),
                meta: json!({}),
            },
        )
        .await
        .expect("industry");
        let department = conveyor_store::actions::department_create(
            &mut tx,
            seeded.tenant,
            &conveyor_store::actions::NewDepartment {
                name: "delivery",
                parent_id: None,
                industry_id: Some(industry.id),
                ordinal: Some(1),
                is_primary: true,
            },
        )
        .await
        .expect("department");
        let job = job_create_tx(
            &mut tx,
            seeded.tenant,
            &NewJob {
                job_key: "binding_gate".to_owned(),
                industry_id: industry.id,
                family: "engineering".to_owned(),
                title: "Binding gate job".to_owned(),
                status: "active".to_owned(),
                definition: json!({
                    "executor_kind": "internal_agent",
                    "prompt_recipe": "agent@1",
                    "model_ref": "router/medium"
                }),
                meta: json!({}),
                departments: vec![PlacementIn {
                    department_id: department.id,
                    ordinal: 1,
                    is_primary: true,
                }],
            },
        )
        .await
        .expect("job");
        let release = job_release_publish_tx(&mut tx, seeded.tenant, job.id)
            .await
            .expect("release");
        let (capability,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability (tenant_id, code, label, status)
             VALUES ($1, 'cap.binding_gate', 'Binding gate', 'active') RETURNING id",
        )
        .bind(seeded.tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("capability");
        let allowed: Vec<String> = Vec::new();
        let (agent,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, status, model_ref, prompt_ref,
                  allowed_actions, max_iterations, timeout_secs)
             VALUES ($1, $2, 'binding_gate', 'tool_agent', 'active', 'router/medium',
                     'agent@1', $3, 3, 60)
             RETURNING id",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(capability)
        .bind(&allowed)
        .fetch_one(&mut *tx)
        .await
        .expect("agent");
        tx.commit().await.expect("commit");
        (job.id, release.id, capability, agent)
    };
    Fixture {
        pg,
        running,
        seeded,
        job,
        release,
        capability,
        agent,
    }
}

fn create_input(fixture: &Fixture, status: Option<&str>) -> Value {
    let mut input = json!({
        "job_id": fixture.job,
        "release_id": fixture.release,
        "executor_kind": "internal_agent",
        "capability_agent_id": fixture.agent,
        "capability_id": fixture.capability,
    });
    if let Some(status) = status {
        input["status"] = json!(status);
    }
    input
}

/// The tenant's one pending binding-activation gate. Each test runs against a fresh database,
/// so the gate list needs no binding filter — `approvals.list` does not project `meta`.
async fn pending_gate(running: &support::Running) -> Value {
    let (status, body) = running
        .invoke(
            "approvals.list",
            TOKEN,
            json!({"status": "pending", "limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    let gates: Vec<&Value> = items
        .iter()
        .filter(|item| item["gate"] == "job_bindings.create" && item["kind"] == "job_binding")
        .collect();
    assert_eq!(gates.len(), 1, "expected exactly one binding gate: {body}");
    gates[0].clone()
}

async fn count(pg: &support::Pg, tenant: TenantId, table: &str) -> i64 {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1");
    let count = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .expect("count");
    tx.commit().await.expect("commit");
    count
}

#[tokio::test]
async fn the_default_create_is_draft_and_opens_the_activation_gate() {
    let fixture = fixture().await;
    let (status, body) = fixture
        .running
        .invoke(
            "job_bindings.create",
            TOKEN,
            create_input(&fixture, None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let binding_id = field(&body, "id").to_owned();
    assert_eq!(
        output(&body)["status"],
        "draft",
        "a binding is never born live: {body}"
    );

    let gate = pending_gate(&fixture.running).await;
    assert_eq!(gate["status"], "pending", "{gate}");
    assert_eq!(
        gate["requested_by"],
        fixture.seeded.user_id.to_string(),
        "the gate names the person who created the binding: {gate}"
    );
    let description = gate["description"].as_str().unwrap_or_default();
    assert!(
        description.contains(&binding_id),
        "the approver is told which binding: {gate}"
    );

    // The gate's meta is what the approved decision flips; it must name this binding.
    let bound: Option<String> = sqlx::query_scalar(
        "SELECT meta->>'job_binding_id' FROM approval WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.seeded.tenant_id)
    .bind(Uuid::parse_str(gate["id"].as_str().expect("gate id")).expect("uuid"))
    .fetch_one(&fixture.pg.super_pool)
    .await
    .expect("gate row");
    assert_eq!(bound.as_deref(), Some(binding_id.as_str()));
}

#[tokio::test]
async fn an_explicit_draft_create_is_inert_and_opens_no_gate() {
    let fixture = fixture().await;
    let (status, body) = fixture
        .running
        .invoke(
            "job_bindings.create",
            TOKEN,
            create_input(&fixture, Some("draft")),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "draft");
    assert_eq!(
        count(&fixture.pg, fixture.seeded.tenant, "approval").await,
        0,
        "a draft that is not being activated needs no decision"
    );
}

#[tokio::test]
async fn a_direct_active_create_is_refused_and_writes_nothing() {
    let fixture = fixture().await;
    let (status, body) = fixture
        .running
        .invoke(
            "job_bindings.create",
            TOKEN,
            create_input(&fixture, Some("active")),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "job_binding_approval_required"
    );
    assert_eq!(
        count(&fixture.pg, fixture.seeded.tenant, "job_binding").await,
        0,
        "the refused request wrote no binding"
    );
    assert_eq!(
        count(&fixture.pg, fixture.seeded.tenant, "approval").await,
        0,
        "the refused request wrote no gate"
    );
}

#[tokio::test]
async fn an_approved_decision_activates_the_binding() {
    let fixture = fixture().await;
    let (status, body) = fixture
        .running
        .invoke(
            "job_bindings.create",
            TOKEN,
            create_input(&fixture, None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let binding_id = field(&body, "id").to_owned();
    let gate = pending_gate(&fixture.running).await;

    // The requester cannot decide their own activation (§5.4, unconditional on this lane).
    let (status, refused) = fixture
        .running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("requester"),
        "{refused}"
    );

    let (status, decided) = fixture
        .running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(output(&decided)["status"], "approved", "{decided}");

    let (status, read) = fixture
        .running
        .invoke(
            "job_bindings.get",
            TOKEN,
            json!({"id": binding_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["status"], "active", "{read}");
    assert_eq!(
        output(&read)["version"],
        2,
        "activation moves the version, so a stale If-Match cannot pass: {read}"
    );
}

#[tokio::test]
async fn a_rejected_decision_leaves_the_binding_draft() {
    let fixture = fixture().await;
    let (status, body) = fixture
        .running
        .invoke(
            "job_bindings.create",
            TOKEN,
            create_input(&fixture, None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let binding_id = field(&body, "id").to_owned();
    let gate = pending_gate(&fixture.running).await;

    let (status, decided) = fixture
        .running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate["id"], "decision": "reject"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(output(&decided)["status"], "rejected", "{decided}");

    let (status, read) = fixture
        .running
        .invoke(
            "job_bindings.get",
            TOKEN,
            json!({"id": binding_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        output(&read)["status"],
        "draft",
        "a rejected gate activates nothing: {read}"
    );

    // The rejected gate is terminal: a later approve cannot flip the binding behind it.
    let (status, refused) = fixture
        .running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
}
