//! T3 (#74) P4.1–P4.3 — a PM gate resolves its typed evidence before it opens, and an approved
//! gate leaves the trace edge from that evidence to the project.
//!
//! The fixture is the real one: the production catalogue seed, the PM job rows exactly as the
//! harness seeds them (`job_catalogue` → `job_release` → `job_binding`), a project, and the
//! governed assets created through `requirements.*`. Routing and deciding are the served actions;
//! the assertions about what was written read the database on the **superuser** pool, so a row a
//! leak into another tenant writes is visible rather than hidden by RLS (CONTRACT §2).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> Uuid {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .parse()
        .unwrap_or_else(|_| panic!("{field} is not a uuid in {value}"))
}

/// The tenant, its PM job rows and one project — what every scenario routes a gate against.
struct Fixture {
    pg: support::Pg,
    running: support::Running,
    seeded: support::Seeded,
    project: Uuid,
    binding: Uuid,
}

async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the production catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'PM project') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("PM-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project");
    // T9 (#81): a design gate resolves only against a live architecture contract, so the
    // fixture holds one — these tests are about the gate's EVIDENCE, not the contract rule.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    let contract = conveyor_store::architecture::request_contract_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::architecture::NewContract {
            project_id: project,
            kind: "delivery_contract".to_owned(),
            phase_code: "f".to_owned(),
            title: "Delivery contract".to_owned(),
            body: json!({}),
            requested_by: seeded.user_id,
            supersedes_id: None,
        },
    )
    .await
    .expect("request the governing contract");
    conveyor_store::architecture::approve_contract_tx(
        &mut tx,
        seeded.tenant,
        contract.id,
        contract.version,
        seeded.user_id,
        None,
    )
    .await
    .expect("approve the governing contract");
    // T9 (#81): the design gate reads Phase G, which is the live contract AND its compliance
    // review. These tests are about the gate's evidence, so the review is fixture state.
    conveyor_store::architecture::record_review_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::architecture::NewReview {
            project_id: project,
            contract_id: contract.id,
            level: "compliant".to_owned(),
            findings: json!([]),
            evidence: json!({ "ref": "fixture design review" }),
            outcome: None,
            reviewed_by: seeded.user_id,
        },
    )
    .await
    .expect("record the Phase-G compliance review");
    tx.commit().await.expect("commit the contract");
    let binding = seed_pm_job(&pg, seeded.tenant_id).await;
    let running = support::Running::start(&pg).await;
    Fixture {
        pg,
        running,
        seeded,
        project,
        binding,
    }
}

/// The PM job rows, as the harness seeds them: an active `pm` position in an industry, its
/// published release, and one active `procedure` binding pinning that release.
async fn seed_pm_job(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         ON CONFLICT (tenant_id, key) DO UPDATE SET name = excluded.name
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the industry");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title)
         VALUES ($1, 'pm_delivery', 'PM delivery') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the process the binding executes");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue
             (tenant_id, job_key, industry_id, family, title, status, definition)
         VALUES ($1, 'pm', $2, 'delivery', 'Project manager', 'active', '{}'::jsonb)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM job");
    let release_hash = "a".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM release");
    sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active')
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM binding")
}

/// Create one governed asset through `requirements.create` and return its id.
async fn create_asset(running: &support::Running, title: &str) -> Uuid {
    let (status, body) = running
        .invoke(
            "requirements.create",
            "dev:alice",
            json!({ "title": title }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "requirements.create: {body}");
    id_of(output(&body), "id")
}

/// One accepted `requirements.transition`.
async fn transition(running: &support::Running, id: Uuid, to: &str) {
    let (status, body) = running
        .invoke(
            "requirements.transition",
            "dev:alice",
            json!({ "id": id, "to": to }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "requirements.transition to {to}: {body}");
    assert_eq!(output(&body)["status"], to, "{body}");
}

/// draft → in_review → approved: the legal path to a live asset.
async fn approve_asset(running: &support::Running, id: Uuid) {
    transition(running, id, "in_review").await;
    transition(running, id, "approved").await;
}

/// `pm.route_gate` as the PM (alice). The requester is the caller, so alice cannot decide it.
///
/// SPEC_V2 §5.4: a gate with no evidence is refused; the only override is a recorded
/// `evidence_waiver` reason, which none of these scenarios uses.
async fn route_gate(fixture: &Fixture, gate: &str, evidence: Value) -> (u16, Value) {
    fixture
        .running
        .invoke(
            "pm.route_gate",
            "dev:alice",
            json!({
                "job_binding_id": fixture.binding,
                "project_id": fixture.project,
                "gate": gate,
                "kind": "gate",
                "evidence": evidence,
            }),
            None,
            None,
            None,
        )
        .await
}

/// `approvals.decide` as `token`.
async fn decide(
    running: &support::Running,
    token: &str,
    approval_id: Uuid,
    decision: &str,
) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            token,
            json!({ "approval_id": approval_id, "decision": decision }),
            None,
            None,
            None,
        )
        .await
}

/// Every approval row in the tenant, read past RLS.
async fn approval_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count approvals")
}

/// P4.1 — a live approved asset opens the gate, and the approval leaves the trace edge from the
/// asset to the project.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_contract_resolves_the_gate() {
    let fixture = fixture().await;
    let asset = create_asset(&fixture.running, "Live design requirement").await;
    approve_asset(&fixture.running, asset).await;

    let (status, body) = route_gate(
        &fixture,
        "design",
        json!([{ "kind": "governed_asset", "id": asset }]),
    )
    .await;
    assert_eq!(status, 201, "a live approved asset opens the gate: {body}");
    let approval_id = id_of(output(&body), "approval_id");

    // The gate exists, is pending, and carries the evidence it resolved.
    let (project_id, gate_status, meta): (Option<Uuid>, String, Value) = sqlx::query_as(
        "SELECT project_id, status, meta FROM approval WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&fixture.pg.super_pool)
    .await
    .expect("read the routed gate");
    assert_eq!(
        project_id,
        Some(fixture.project),
        "the gate belongs to the project"
    );
    assert_eq!(gate_status, "pending");
    assert_eq!(
        meta["evidence"],
        json!([{ "kind": "governed_asset", "id": asset }]),
        "the resolved evidence is stored on the gate"
    );

    // The requester cannot decide their own gate; a distinct human approves it.
    let (status, body) = decide(&fixture.running, "dev:reviewer", approval_id, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved");

    // The trace edge is read back through the action, filtered at the end the gate wrote from.
    // The row itself is asserted on BOTH ends: P4.1 is the asset → project edge, not merely an
    // edge. The action's end-type vocabulary is the store's `END_TYPES`
    // (crates/store/src/trace_link.rs), which admits `governed_asset`, `card`, `project` and a
    // published `document`; this read filters on `from_*` because that is the end the gate
    // wrote from.
    let (status, body) = fixture
        .running
        .invoke(
            "trace_links.list",
            "dev:alice",
            json!({
                "from_type": "governed_asset",
                "from_id": asset,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(
        items.len(),
        1,
        "an approved gate leaves exactly one edge from its evidence: {body}"
    );
    let edge = &items[0];
    assert_eq!(edge["from_type"], "governed_asset");
    assert_eq!(edge["from_id"], asset.to_string());
    assert_eq!(edge["to_type"], "project");
    assert_eq!(edge["to_id"], fixture.project.to_string());
    assert_eq!(edge["kind"], "traces");

    fixture.running.shutdown().await;
}

/// P4.2 — an asset that is not live (draft, or superseded) is refused with a code that names its
/// state, and no gate is opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_contract_refuses_the_gate() {
    let fixture = fixture().await;

    // A draft asset: written but never reviewed.
    let draft = create_asset(&fixture.running, "Draft design requirement").await;
    let (status, body) = route_gate(
        &fixture,
        "design",
        json!([{ "kind": "governed_asset", "id": draft }]),
    )
    .await;
    assert_eq!(status, 422, "a draft asset must be refused: {body}");
    assert_eq!(body["type"], "gate_evidence_not_live", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("draft"),
        "the refusal names the state: {body}"
    );

    // A superseded asset: live once, and no longer.
    let old = create_asset(&fixture.running, "Superseded design requirement").await;
    approve_asset(&fixture.running, old).await;
    let successor = create_asset(&fixture.running, "Successor requirement").await;
    let (status, body) = fixture
        .running
        .invoke(
            "requirements.transition",
            "dev:alice",
            json!({ "id": old, "to": "superseded", "superseded_by": successor }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "supersede the asset: {body}");
    assert_eq!(output(&body)["status"], "superseded");

    let (status, body) = route_gate(
        &fixture,
        "design",
        json!([{ "kind": "governed_asset", "id": old }]),
    )
    .await;
    assert_eq!(status, 422, "a superseded asset must be refused: {body}");
    assert_eq!(body["type"], "gate_evidence_not_live", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("superseded"),
        "the refusal names the state: {body}"
    );

    assert_eq!(
        approval_count(&fixture.pg, fixture.seeded.tenant_id).await,
        0,
        "a refused evidence reference opened a gate"
    );

    fixture.running.shutdown().await;
}

/// P4.3 — a gate with no evidence and no recorded waiver is refused before any row is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_without_evidence_is_refused() {
    let fixture = fixture().await;

    let (status, body) = route_gate(&fixture, "design", json!([])).await;
    assert_eq!(status, 422, "an evidence-less gate must be refused: {body}");
    assert_eq!(body["type"], "gate_evidence_missing", "{body}");

    assert_eq!(
        approval_count(&fixture.pg, fixture.seeded.tenant_id).await,
        0,
        "a refused evidence reference opened a gate"
    );

    fixture.running.shutdown().await;
}
