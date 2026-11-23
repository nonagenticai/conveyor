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
    let key = format!("PC-{}", Uuid::now_v7().simple());
    programme_project_keyed(pg, running, seeded, &key).await
}

/// The same project under a chosen key: the committed findings ledger charges its open P0s to
/// project keys, so the P0 tests need a real one (`S06`, `S14`).
async fn programme_project_keyed(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    key: &str,
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
         VALUES ($1, $2, 1, 'authorised')",
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
                "key": key,
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
                    "tranche": 1,
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

/// Write programme-owned meta at the row's current version.
async fn set_programme_meta(running: &support::Running, project: &str, meta: Value) {
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
    let version = output(&body)["project"]["version"]
        .as_i64()
        .expect("a project version");
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({"id": project, "meta": meta}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
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
        "INSERT INTO governance_body
             (tenant_id, kind, key, name, charter, commissioning_authority, members, status)
         VALUES ($1, 'architecture_board', 'board', 'Conveyor Architecture Board',
                 'Approve the delivery contracts', 'P0 programme board',
                 '[{\"role\": \"chief architect\"}]'::jsonb, 'active')
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

async fn review(
    running: &support::Running,
    project: &str,
    contract: &str,
    delegation: &str,
) -> String {
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
    text(output(&body), "id")
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

/// The published documents the fixture's mandatory set requires: the `v2-built` class
/// (`project-brief`, `evidence`, `closure-review`), PMP's WBS and registers, and — because a
/// programme project is governed — TOGAF's artefact pages. The delivery contract/compliance
/// review are Phase-G rows; baseline and change control are typed rows.
const MANDATORY_DOCUMENTS: [&str; 7] = [
    "project-brief",
    "evidence",
    "closure-review",
    "project-wbs",
    "project-registers",
    "architecture-artifact",
    "statement-of-architecture-work",
];

/// Compose and publish one output; the standard's skeleton carries its required headings.
async fn publish_output(running: &support::Running, project: &str, category: &str) {
    let body = conveyor_domain::output_standard::template(category)
        .map(|template| template.skeleton)
        .unwrap_or_else(|| format!("# {category}\n\nWritten by the close gate test.\n"));
    let (status, body) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({
                "title": format!("{category} of {project}"),
                "project_id": project,
                "body": body,
                "category": category,
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
}

/// One change register row: `change-control`'s typed evidence. A change names the baseline it
/// changes and carries its impact block (0044's CHECK).
async fn change_register_item(pg: &support::Pg, seeded: &support::Seeded, project: &str) {
    let project_id: Uuid = project.parse().expect("the project uuid");
    let baseline: Uuid = sqlx::query_scalar(
        "SELECT id FROM project_baseline
          WHERE tenant_id = $1 AND project_id = $2 AND kind = 'scope'
          ORDER BY taken_at DESC, id DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the project's scope baseline");
    sqlx::query(
        "INSERT INTO project_register_item
            (tenant_id, project_id, kind, code, title, body, baseline_ref, meta)
         VALUES ($1, $2, 'change', 'CHG-1', 'An approved change', 'A change to the baseline.',
                 $3, '{\"impact\": {\"scope\": \"the accepted scope widens\"}}'::jsonb)",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .bind(baseline)
    .execute(&pg.super_pool)
    .await
    .expect("insert the change register row");
}

/// Every mandatory artefact the standard requires of the fixture project. `skip` leaves one
/// document unpublished for the refusal test. A standalone fixture publishes the programme-only
/// TOGAF pages too; an extra published document is not a defect.
async fn satisfy_mandatory_set(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
    skip: Option<&str>,
) {
    for category in MANDATORY_DOCUMENTS {
        if Some(category) == skip {
            continue;
        }
        publish_output(running, project, category).await;
    }
    change_register_item(pg, seeded, project).await;
}

/// An approved closure decision over an approved closure asset, written directly. A standalone
/// project has no `projects.close_request` path (it refuses non-programme projects), so the
/// fixture that needs a standalone close gate writes the two rows the gate reads.
async fn approve_closure_gate(pg: &support::Pg, seeded: &support::Seeded, project: &str) {
    let project_id: Uuid = project.parse().expect("the project uuid");
    let asset: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset
             (tenant_id, kind, key, title, body, status, project_id, owner_user_id)
         VALUES ($1, 'closure', $2, 'Closure', 'The closure narrative.', 'approved', $3, $4)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("closure-{}", Uuid::now_v7().simple()))
    .bind(project_id)
    .bind(seeded.user_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the approved closure asset");
    let gate: Uuid = sqlx::query_scalar(
        "INSERT INTO approval
             (tenant_id, project_id, gate, kind, status, requested_by, meta)
         VALUES ($1, $2, 'closure', 'pm', 'approved', $3,
                 jsonb_build_object('closure_asset_id', $4::text))
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .bind(seeded.user_id)
    .bind(asset.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the closure gate");
    sqlx::query(
        "INSERT INTO approval_decision
             (tenant_id, approval_id, approver_id, decision, decided_by_kind)
         VALUES ($1, $2, $3, 'approve', 'human')",
    )
    .bind(seeded.tenant_id)
    .bind(gate)
    .bind(support::reviewer_id(pg).await)
    .execute(&pg.super_pool)
    .await
    .expect("insert the closure decision");
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
    satisfy_mandatory_set(&pg, &running, &seeded, &project, None).await;

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

/// v1-close-1 — a project cannot close while one mandatory artefact is unpublished.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_a_missing_mandatory_artefact() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    satisfy_mandatory_set(&pg, &running, &seeded, &project, Some("project-wbs")).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"], "closure_mandatory_artefacts_missing",
        "{body}"
    );
    let detail = body["detail"].as_str().expect("a detail");
    assert!(
        detail.contains("project-wbs"),
        "the refusal names the missing artefact: {detail}"
    );
    assert_eq!(project_status(&running, &project).await, "active");
}

/// v1-close-1 — the committed ledger's open P0 for `S06` refuses the close even when every
/// document and typed row exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_refuses_an_open_p0_finding() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project_keyed(&pg, &running, &seeded, "S06").await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    satisfy_mandatory_set(&pg, &running, &seeded, &project, None).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_open_p0", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    assert!(
        detail.contains("v1-s06-2"),
        "the refusal names the open finding: {detail}"
    );
    assert_eq!(project_status(&running, &project).await, "active");
}

/// v1-close-1 — `S14`'s P0 is suppressed by its owner-accepted waiver (expires 2027-02-18 in
/// the committed ledger) while the waiver is in date; once it expires the finding refuses the
/// same close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unexpired_waiver_suppresses_an_open_p0_finding() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project_keyed(&pg, &running, &seeded, "S14").await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    satisfy_mandatory_set(&pg, &running, &seeded, &project, None).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    let reassessment = chrono::NaiveDate::from_ymd_opt(2027, 2, 18).expect("the waiver's date");
    if chrono::Utc::now().date_naive() < reassessment {
        assert_eq!(status, 200, "{body}");
        assert_eq!(output(&body)["project"]["status"], "closed");
    } else {
        // The ledger's only waiver has expired, so the same close is now refused by the finding
        // it used to suppress. The domain module's unit test covers both sides deterministically.
        assert_eq!(status, 409, "{body}");
        assert_eq!(body["type"], "closure_open_p0", "{body}");
    }
}

/// v1-close-1 — a standalone project with a closure gate owes its mandatory set: refused while
/// the PMP rows are missing, and green once they exist. A bare standalone project (no closure
/// gate) keeps the gate's first early return, covered by `closure_gate.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_standalone_project_with_a_closure_gate_is_held_to_its_mandatory_set() {
    let (pg, running, seeded) = fixture().await;
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
    charter(&running, &project).await;
    baseline(&running, &project).await;
    approve_closure_gate(&pg, &seeded, &project).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"], "closure_mandatory_artefacts_missing",
        "{body}"
    );
    assert_eq!(project_status(&running, &project).await, "active");

    satisfy_mandatory_set(&pg, &running, &seeded, &project, None).await;
    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed");
}

/// v1-close-1 — an archived measurement is history, not an artefact gap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archived_project_is_not_held_to_the_mandatory_set() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    set_programme_meta(&running, &project, json!({"measured_status": "archived"})).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed");
}

/// v1-close-1 — a project the retrospectives folded or closed by decision is exempt from the
/// mandatory set (the verdict is the programme's, written only by `projects.programme_meta`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viability_fold_is_not_held_to_the_mandatory_set() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    set_programme_meta(&running, &project, json!({"viability": "fold"})).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    review(&running, &project, &contract, &delegation).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed");
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
    sqlx::query("UPDATE programme_tranche SET state = 'planned' WHERE number = 1")
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

/// The Phase-G review reads back with its authority (#526 P1b).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviews_get_reads_the_review_with_its_delegation() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract = contract(&pg, &running, &seeded, &project, &delegation).await;
    let review_id = review(&running, &project, &contract, &delegation).await;

    let (status, body) = running
        .invoke(
            "architecture.reviews_get",
            TOKEN,
            json!({"id": review_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let read = output(&body);
    assert_eq!(text(read, "id"), review_id);
    assert_eq!(text(read, "project_id"), project);
    let evidence = read["evidence"]
        .as_object()
        .expect("the review carries its evidence");
    assert_eq!(
        evidence["delegation_ref"],
        json!(delegation),
        "the review reads back with the delegation it was recorded under"
    );

    let (status, body) = running
        .invoke(
            "architecture.reviews_get",
            TOKEN,
            json!({"id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found", "{body}");
}
