//! U4 (#524) — the programme's one close gate.
//!
//! A programme project cannot close on a status edit. The closure is a request (a draft closure
//! asset and a closure gate), a distinct delegate's decision, and then the close gate: an
//! authorised tranche, published declared outputs, an approved closure asset the author did not
//! approve, an approved charter and scope baseline, and — for a delivery project — a live
//! board-approved delivery contract with its Phase-G compliance review.
//!
//! `rbuild -- test -p conveyor-actions --test programme_close`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

/// A programme project in an authorised tranche, active, delivery track, with no output gate.
async fn programme_project(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
) -> String {
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Close programme') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the programme");
    sqlx::query(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 0, 'authorised')",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .execute(&pg.super_pool)
    .await
    .expect("insert the authorised tranche");
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({
                "title": "Programme close",
                "key": format!("PC-{}", Uuid::now_v7().simple()),
                "programme_id": programme,
                "project_type_code": "delivery",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let id = text(output(&created), "id");
    let version = output(&created)["version"].as_i64().expect("a version");
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({
                "id": id,
                "meta": {
                    "track": "v2",
                    "tranche": 0,
                    "expected_outputs": [],
                    "closure_outputs": [],
                },
            }),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "projects.activate",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

async fn approve(running: &support::Running, approval_id: &str) {
    let (status, body) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn charter(running: &support::Running, project: &str) {
    let (status, body) = running
        .invoke(
            "projects.charter",
            TOKEN,
            json!({"project_id": project, "charter": "The project's mandate and scope."}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    approve(running, &text(output(&body), "approval_id")).await;
}

async fn baseline(running: &support::Running, project: &str) {
    let (status, body) = running
        .invoke(
            "projects.baseline",
            TOKEN,
            json!({
                "project_id": project,
                "baseline_kind": "scope",
                "wbs": [{
                    "kind": "work_package",
                    "title": "Deliver the project",
                    "business_value": 1,
                    "time_criticality": 1,
                    "risk_reduction": 1,
                    "job_size": 1,
                }],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    approve(running, &text(output(&body), "approval_id")).await;
}

async fn board(pg: &support::Pg, tenant: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO governance_body (tenant_id, kind, key, name)
         VALUES ($1, 'architecture_board', 'board', 'Conveyor Architecture Board')
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board")
}

async fn contract(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
    delegation: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "architecture.contracts_request",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "delivery_contract",
                "phase_code": "f",
                "title": "Delivery contract",
                "body": {"compliance": "the acceptance manifest at the published SHA"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let contract_id = text(output(&body), "id");
    let version = output(&body)["version"]
        .as_i64()
        .expect("a contract version");

    let board = board(pg, seeded.tenant_id).await;
    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "approved",
                "evidence": {"delegation_ref": delegation},
                "subject_kind": "contract",
                "subject_id": contract_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = text(output(&body), "approval_id");

    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            REVIEWER,
            json!({
                "id": contract_id,
                "approval_id": approval_id,
                "delegation_ref": delegation,
            }),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    contract_id
}

async fn review(running: &support::Running, project: &str, contract: &str, delegation: &str) {
    let (status, body) = running
        .invoke(
            "architecture.reviews_record",
            REVIEWER,
            json!({
                "project_id": project,
                "contract_id": contract,
                "level": "compliant",
                "delegation_ref": delegation,
                "findings": [],
                "evidence": {
                    "manifest": "acceptance/programme/T.toml",
                    "delegation_ref": delegation,
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

async fn close_request(running: &support::Running, project: &str, delegation: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "The project delivered its scope; the closure evidence is published.",
                "delegation_ref": delegation,
                "evidence": {"manifest": "acceptance/programme/T.toml"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "approval_id")
}

async fn close(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
) -> (u16, Value) {
    running
        .invoke_gated(
            pg,
            seeded.tenant,
            "projects.close",
            TOKEN,
            json!({"id": project}),
            None,
        )
        .await
}

async fn project_status(running: &support::Running, project: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    text(&output(&body)["project"], "status")
}

/// The conformant path: request, distinct decision, then the one gate passes and the project
/// closes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_programme_project_closes_through_the_one_gate() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;

    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;

    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["project"]["status"],
        "closed",
        "the conformant close closes: {body}"
    );
}

/// A close without a closure request is refused before anything is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_without_a_closure_gate() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_gate_missing", "{body}");
    assert_eq!(
        project_status(&running, &project).await,
        "active",
        "the refused close must not close"
    );
}

/// A pending closure decision is not a close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_a_pending_closure() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    close_request(&running, &project, &delegation).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_gate_pending", "{body}");
}

/// The scope baseline is an MSP prerequisite, not an optional record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_without_a_baseline() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_baseline_missing", "{body}");
}

/// A delivery project without a live board-approved contract does not close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_without_a_contract() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_contract_missing", "{body}");
}

/// A project in a tranche the programme never authorised does not close, even with every record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_an_unauthorised_tranche() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;
    sqlx::query("UPDATE programme_tranche SET state = 'planned' WHERE number = 0")
        .execute(&pg.super_pool)
        .await
        .expect("withdraw the authorisation");

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_tranche_not_authorised", "{body}");
}

/// The closure request refuses a delegation that is not the published P0 decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_request_refuses_an_unknown_delegation() {
    let (pg, running, seeded) = fixture().await;
    let project = programme_project(&pg, &running, &seeded).await;
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "A closure.",
                "delegation_ref": Uuid::now_v7().to_string(),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "delegation_ref_invalid", "{body}");
}

/// A standalone project has no tranche, charter or delivery contract to close against; it
/// closes through the older output gate, not this one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_request_refuses_a_standalone_project() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Standalone", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project = text(output(&created), "id");
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "A closure.",
                "delegation_ref": delegation,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "not_a_programme_project", "{body}");
}

/// A delegated act without the reference is refused at the boundary, before the handler.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_request_requires_a_delegation_ref() {
    let (pg, running, seeded) = fixture().await;
    let project = programme_project(&pg, &running, &seeded).await;
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({"id": project, "closure": "A closure."}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
}

/// A decision output under another project is not the standing delegation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_request_refuses_a_decision_from_another_project() {
    let (pg, running, seeded) = fixture().await;
    let project = programme_project(&pg, &running, &seeded).await;
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Not P0"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let other = text(output(&created), "id");
    let decision_body = conveyor_domain::output_standard::template("decision")
        .map(|template| template.skeleton)
        .unwrap_or_else(|| {
            "# Decision record\n\n## Context\n\nA context.\n\n## Decision\n\nA decision.\n\n## Consequences\n\nThe consequences.\n\n## Evidence\n\nThe evidence.\n".to_owned()
        });
    let (status, body) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({
                "title": "Another decision",
                "project_id": other,
                "body": decision_body,
                "category": "decision",
                "publishable": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let document = text(output(&body), "id");
    let version = output(&body)["version"].as_i64().expect("a version");
    let (status, body) = running
        .invoke(
            "documents.publish",
            TOKEN,
            json!({"id": document, "publish": true}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "A closure.",
                "delegation_ref": document,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "delegation_wrong_project", "{body}");
}

/// The contract's requester cannot record the compliance review of their own contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviews_record_refuses_the_contract_requester() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;

    let (status, body) = running
        .invoke(
            "architecture.reviews_record",
            TOKEN,
            json!({
                "project_id": project,
                "contract_id": contract,
                "level": "compliant",
                "findings": [],
                "evidence": {"delegation_ref": delegation},
                "delegation_ref": delegation,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "requester_is_reviewer", "{body}");
}
