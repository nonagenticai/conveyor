//! #508 — the write path refuses its bypasses.
//!
//! Five invariants, each through the served action surface against real Postgres:
//!
//! * a project that belongs to a programme is born `draft`, and status moves only through the
//!   transition actions;
//! * programme-owned `meta` keys cannot be written by a general edit;
//! * a contract records its requester and cannot be approved by that actor;
//! * a tranche authorisation is linked to an existing `continue` review, never manufactured;
//! * a board decision is a row with its subject, and the board's active state derives from it;
//! * a chartered project can be baselined without a PM job binding.
//!
//! `rbuild -- test -p conveyor-actions --test write_path` (preflight gives the verdict on this
//! estate; no local gate compiles).

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

fn error_code(body: &Value) -> &str {
    body["errors"][0]["code"]
        .as_str()
        .or_else(|| body["type"].as_str())
        .unwrap_or_else(|| panic!("no error code in {body}"))
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

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            TOKEN,
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_project(
    running: &support::Running,
    title: &str,
    programme: Option<&str>,
) -> String {
    let mut input = json!({"title": title, "project_type_code": "delivery"});
    if let Some(programme) = programme {
        input["programme_id"] = json!(programme);
    }
    let (status, body) = running
        .invoke("projects.create", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
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

async fn create_tranche(running: &support::Running, programme: &str, number: i64) -> String {
    let (status, body) = running
        .invoke(
            "tranches.create",
            TOKEN,
            json!({"programme_id": programme, "number": number}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn review_tranche(
    running: &support::Running,
    tranche: &str,
    decision: &str,
    delegation: &str,
) -> (u16, Value) {
    let input = json!({
        "id": tranche,
        "decision": decision,
        "delegation_ref": delegation,
        "evidence": {
            // v1-tr2-2: the review names the snapshot it judged.
            "snapshot_sha": "0123456789abcdef0123456789abcdef01234567",
            "baselines": ["B0"],
            "benefit_status": {"B1": "tracking"},
            "risk_register": ["risk-1"]
        }
    });
    let gate = support::review_gate(running, TOKEN, &input).await;
    running
        .invoke_approved("tranches.review", TOKEN, input, None, gate)
        .await
}

async fn latest_review(running: &support::Running, programme: &str) -> String {
    let (status, body) = running
        .invoke(
            "tranches.review_list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("review items");
    text(&items[0], "id")
}

async fn tranche_state(running: &support::Running, programme: &str, number: i64) -> Value {
    let (status, body) = running
        .invoke(
            "tranches.list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["items"]
        .as_array()
        .expect("tranche items")
        .iter()
        .find(|row| row["number"] == json!(number))
        .unwrap_or_else(|| panic!("tranche {number} in {body}"))
        .clone()
}

async fn request_contract(running: &support::Running, project: &str) -> Value {
    let (status, body) = running
        .invoke(
            "architecture.contracts_request",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "delivery_contract",
                "phase_code": "f",
                "title": "Delivery contract",
                "body": {"acceptance_criteria": ["the gate text, executable"]}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body).clone()
}

async fn create_board(pg: &support::Pg, tenant: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO governance_body
             (tenant_id, kind, key, name, charter, commissioning_authority, members)
         VALUES ($1, 'architecture_board', 'board', 'Conveyor Architecture Board',
                 'Approve the delivery contracts', 'P0 programme board',
                 '[{\"role\": \"chief architect\"}]'::jsonb)
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the board")
}

async fn set_meta(pg: &support::Pg, tenant: Uuid, project: &str, key: &str, value: Value) {
    sqlx::query("UPDATE project SET meta = meta || jsonb_build_object($3, $4) WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(Uuid::parse_str(project).expect("project id"))
        .bind(key)
        .bind(value)
        .execute(&pg.super_pool)
        .await
        .expect("set project meta");
}

async fn set_status(pg: &support::Pg, tenant: Uuid, project: &str, status: &str) {
    sqlx::query("UPDATE project SET status = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(Uuid::parse_str(project).expect("project id"))
        .bind(status)
        .execute(&pg.super_pool)
        .await
        .expect("set project status");
}

// ── A. project status and programme-owned metadata ────────────────────────────────────────

#[tokio::test]
async fn project_born_draft_in_a_programme() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Draft programme").await;
    let project = create_project(&running, "Programme child", Some(&programme)).await;
    assert_eq!(project_status(&running, &project).await, "draft");

    let standalone = create_project(&running, "Standalone", None).await;
    assert_eq!(project_status(&running, &standalone).await, "active");
}

#[tokio::test]
async fn update_refuses_status() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Status edit", None).await;
    let (status, body) = running
        .invoke(
            "projects.update",
            TOKEN,
            json!({"id": project, "status": "closed"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_code(&body), "status_not_editable", "{body}");
}

#[tokio::test]
async fn update_refuses_programme_meta() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Meta edit", None).await;
    let (status, body) = running
        .invoke(
            "projects.update",
            TOKEN,
            json!({"id": project, "meta": {"closure_outputs": []}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_code(&body), "meta_key_protected", "{body}");
}

#[tokio::test]
async fn activate_requires_authorised_tranche() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Tranche-gated programme").await;
    let project = create_project(&running, "Gated project", Some(&programme)).await;
    set_meta(&pg, seeded.tenant_id, &project, "tranche", json!(1)).await;

    // No tranche row at all: a programme project fails closed, never starts ungoverned.
    let (status, body) = running
        .invoke(
            "projects.activate",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "tranche_not_authorised", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("declares no tranche"),
        "{body}"
    );

    create_tranche(&running, &programme, 1).await;
    let (status, body) = running
        .invoke(
            "projects.activate",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "tranche_not_authorised", "{body}");
    assert_eq!(project_status(&running, &project).await, "draft");

    // A continue review authorises the next tranche in the same write; then activation passes.
    let t0 = create_tranche(&running, &programme, 0).await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let (status, _body) = review_tranche(&running, &t0, "continue", &delegation).await;
    assert_eq!(status, 200);
    assert_eq!(
        tranche_state(&running, &programme, 1).await["state"],
        "authorised"
    );
    let (status, body) = running
        .invoke(
            "projects.activate",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(project_status(&running, &project).await, "active");
}

#[tokio::test]
async fn pause_and_archive_are_gated() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Pausable", None).await;
    let (status, body) = running
        .invoke(
            "projects.pause",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(project_status(&running, &project).await, "paused");

    set_status(&pg, seeded.tenant_id, &project, "closed").await;
    let (status, body) = running
        .invoke(
            "projects.archive",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(project_status(&running, &project).await, "archived");
}

#[tokio::test]
async fn programme_meta_is_the_programme_write_path() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Classed", None).await;
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
        .expect("version");
    let etag = format!("\"{version}\"");

    // The console's path: the owner sets the class-derived keys through the programme action.
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({
                "id": project,
                "meta": {
                    "expected_outputs": ["evidence"],
                    "closure_outputs": ["closure-review"],
                    "output_class": "P0",
                    "tranche": 3,
                    "plan": "docs/plans/x.md"
                }
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["meta"]["output_class"], "P0");
    assert_eq!(output(&body)["project"]["meta"]["tranche"], 3);

    // A key the programme does not own is refused: that is `projects.update`'s business.
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({"id": project, "meta": {"description": "not programme-owned"}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_code(&body), "meta_key_not_programme", "{body}");
}

// ── B. contract requester identity ───────────────────────────────────────────────────────

#[tokio::test]
async fn contract_records_requester() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Requester", None).await;
    let contract = request_contract(&running, &project).await;
    assert_eq!(
        contract["requested_by"],
        seeded.user_id.to_string(),
        "{contract}"
    );
    assert_eq!(contract["requester_unknown"], false, "{contract}");

    let (status, body) = running
        .invoke(
            "architecture.contracts_get",
            TOKEN,
            json!({"id": contract["id"]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["requested_by"], seeded.user_id.to_string());
    let _ = pg;
}

#[tokio::test]
async fn contract_cannot_self_approve() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Self approval", None).await;
    let contract = request_contract(&running, &project).await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let version = contract["version"].as_i64().expect("version").to_string();
    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            TOKEN,
            json!({"id": contract["id"], "delegation_ref": delegation}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "requester_is_approver", "{body}");
}

#[tokio::test]
async fn legacy_contract_requester_flagged() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Legacy contract", None).await;
    let contract = request_contract(&running, &project).await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let id = Uuid::parse_str(&text(&contract, "id")).expect("contract id");
    sqlx::query(
        "UPDATE architecture_contract SET requested_by = NULL WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(id)
    .execute(&pg.super_pool)
    .await
    .expect("clear the requester");

    let version = contract["version"].as_i64().expect("version").to_string();
    // The missing requester cannot be excluded, so no approval without a board decision.
    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            REVIEWER,
            json!({"id": contract["id"], "delegation_ref": delegation}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "board_approval_required", "{body}");

    // The board decision is the independent authority; the row still reads its flag. The board
    // must be active before it can decide the contract.
    let board = create_board(&pg, seeded.tenant_id).await;
    sqlx::query("UPDATE governance_body SET status = 'active' WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(board)
        .execute(&pg.super_pool)
        .await
        .expect("activate the board");
    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "approved",
                "evidence": {"legacy": "no recorded requester", "delegation_ref": delegation},
                "subject_kind": "contract",
                "subject_id": contract["id"]
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
                "id": contract["id"],
                "approval_id": approval_id,
                "delegation_ref": delegation
            }),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["requester_unknown"], true, "{body}");
    assert_eq!(output(&body)["approval_id"], approval_id, "{body}");
}

// ── C. tranche authorisation repair ──────────────────────────────────────────────────────

#[tokio::test]
async fn tranche_authorise_links_review() {
    let (pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Repair programme").await;
    let t0 = create_tranche(&running, &programme, 0).await;
    let t1 = create_tranche(&running, &programme, 1).await;
    // The real review authorises tranche 1 in the same write. The repair path exists for the
    // historical review whose authorisation never landed, so the fixture withdraws it after the
    // fact: the review row stays, the next tranche reads planned, and the repair re-links it.
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let (status, _body) = review_tranche(&running, &t0, "continue", &delegation).await;
    assert_eq!(status, 200);
    let review = latest_review(&running, &programme).await;
    sqlx::query(
        "UPDATE programme_tranche
            SET state = 'planned', authorised_at = NULL, authorisation_review_id = NULL
          WHERE id = $1::uuid",
    )
    .bind(&t1)
    .execute(&pg.super_pool)
    .await
    .expect("withdraw the authorisation");
    assert_eq!(
        tranche_state(&running, &programme, 1).await["state"],
        "planned"
    );

    let (status, body) = running
        .invoke(
            "tranches.authorise",
            TOKEN,
            json!({"review_id": review}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["state"], "authorised");
    assert_eq!(output(&body)["authorisation_review_id"], review);
    let t1 = tranche_state(&running, &programme, 1).await;
    assert_eq!(t1["state"], "authorised");
    assert_eq!(t1["authorisation_review_id"], review);
}

#[tokio::test]
async fn tranche_authorise_refuses_non_continue_and_unknown() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Refusal programme").await;
    let t0 = create_tranche(&running, &programme, 0).await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let (status, _body) = review_tranche(&running, &t0, "pause", &delegation).await;
    assert_eq!(status, 200);
    let review = latest_review(&running, &programme).await;
    create_tranche(&running, &programme, 1).await;

    let (status, body) = running
        .invoke(
            "tranches.authorise",
            TOKEN,
            json!({"review_id": review}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "tranche_review_not_continue", "{body}");
    assert_eq!(
        tranche_state(&running, &programme, 1).await["state"],
        "planned"
    );

    let (status, body) = running
        .invoke(
            "tranches.authorise",
            TOKEN,
            json!({"review_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test]
async fn tranche_authorise_is_idempotent() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Idempotent programme").await;
    let t0 = create_tranche(&running, &programme, 0).await;
    let _t1 = create_tranche(&running, &programme, 1).await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let (status, _body) = review_tranche(&running, &t0, "continue", &delegation).await;
    assert_eq!(status, 200);
    let review = latest_review(&running, &programme).await;

    let (status, first) = running
        .invoke(
            "tranches.authorise",
            TOKEN,
            json!({"review_id": review}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{first}");
    let (status, second) = running
        .invoke(
            "tranches.authorise",
            TOKEN,
            json!({"review_id": review}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(
        output(&first)["version"],
        output(&second)["version"],
        "{second}"
    );
    assert_eq!(
        output(&second)["authorisation_review_id"],
        review,
        "{second}"
    );
}

// ── D. board decision record ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn board_decision_is_a_row_and_activates_the_board() {
    let (pg, running, seeded) = fixture().await;
    let board = create_board(&pg, seeded.tenant_id).await;
    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "approved",
                "evidence": {"decision": "convene the board"},
                "subject_kind": "board"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = text(output(&body), "approval_id");
    assert!(!approval_id.is_empty());

    // The decision is a durable row: the actor, the outcome, the subject and a timestamp.
    let (decided_by, decision, subject, stamped): (Option<Uuid>, String, Option<String>, bool) =
        sqlx::query_as(
            "SELECT requested_by, status, meta->>'subject_kind', created_at IS NOT NULL
               FROM approval WHERE tenant_id = $1 AND id = $2",
        )
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(&approval_id).expect("approval id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the decision row");
    assert_eq!(decided_by, Some(seeded.user_id));
    assert_eq!(decision, "approved");
    assert_eq!(subject.as_deref(), Some("board"));
    assert!(stamped, "the decision carries its timestamp");

    let (status, body) = running
        .invoke(
            "architecture.board_get",
            TOKEN,
            json!({"id": board}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "active", "{body}");
}

#[tokio::test]
async fn rejected_board_decision_leaves_it_draft() {
    let (pg, running, seeded) = fixture().await;
    let board = create_board(&pg, seeded.tenant_id).await;
    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "rejected",
                "evidence": {"decision": "not yet"},
                "subject_kind": "board"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = running
        .invoke(
            "architecture.board_get",
            TOKEN,
            json!({"id": board}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "draft", "{body}");
}

#[tokio::test]
async fn contract_approval_links_the_board_decision() {
    let (pg, running, seeded) = fixture().await;
    let board = create_board(&pg, seeded.tenant_id).await;
    sqlx::query("UPDATE governance_body SET status = 'active' WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(board)
        .execute(&pg.super_pool)
        .await
        .expect("activate the board");
    let project = create_project(&running, "Board-approved contract", None).await;
    let contract = request_contract(&running, &project).await;
    let contract_id = text(&contract, "id");
    let delegation = support::delegation_ref(&running, TOKEN).await;

    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "approved",
                "evidence": {"contract": contract_id, "delegation_ref": delegation},
                "subject_kind": "contract",
                "subject_id": contract_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = text(output(&body), "approval_id");

    let version = contract["version"].as_i64().expect("version").to_string();
    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            REVIEWER,
            json!({
                "id": contract_id,
                "approval_id": approval_id,
                "delegation_ref": delegation
            }),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["approval_id"], approval_id, "{body}");
}

// ── E. standalone baseline ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn baseline_without_pm_binding() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Baselined", None).await;
    let project_id = Uuid::parse_str(&project).expect("project id");
    sqlx::query(
        "INSERT INTO approval
            (tenant_id, project_id, gate, kind, status, required_decisions, requested_by)
         VALUES ($1, $2, 'charter', 'charter', 'approved', 1, $3)",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .bind(seeded.user_id)
    .execute(&pg.super_pool)
    .await
    .expect("an approved charter gate");

    let (status, body) = running
        .invoke(
            "projects.baseline",
            TOKEN,
            json!({
                "project_id": project,
                "wbs": [
                    {"kind": "phase", "title": "Delivery"},
                    {"kind": "deliverable", "title": "The contract gate", "parent_index": 0}
                ]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let baseline_id = text(output(&body), "baseline_id");
    assert_eq!(
        output(&body)["wbs_ids"].as_array().map(Vec::len),
        Some(2),
        "{body}"
    );

    let baselines: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_baseline WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count baselines");
    assert_eq!(baselines, 1, "{baseline_id}");
    let nodes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wbs_node WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count wbs nodes");
    assert_eq!(nodes, 2);
}

#[tokio::test]
async fn baseline_requires_charter() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Unchartered", None).await;
    let (status, body) = running
        .invoke(
            "projects.baseline",
            TOKEN,
            json!({
                "project_id": project,
                "wbs": [{"kind": "phase", "title": "Delivery"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "charter_not_approved", "{body}");
}
