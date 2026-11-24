//! T9 / #526 — Phase G's close gate has no `meta.track` exemption.
//!
//! Finding v1-p81-2: a programme-track project closes only against a live board-approved
//! delivery contract and its Phase-G compliance review, exactly like every other project.
//!
//! `cargo test -p conveyor-actions --test togaf_close_gate`.

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

/// A programme project in an authorised tranche, active, on the `programme` track.
async fn programme_track_project(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
) -> String {
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Phase-G programme') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the programme");
    // T10: a tranche is authorised by its continue review, and the start gate reads the link.
    for number in [0, 1] {
        let tranche: Uuid = sqlx::query_scalar(
            "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
             VALUES ($1, $2, $3, 'authorised') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(programme)
        .bind(number)
        .fetch_one(&pg.super_pool)
        .await
        .expect("insert an authorised tranche");
        let review: Uuid = sqlx::query_scalar(
            "INSERT INTO programme_tranche_review (tenant_id, tranche_id, decision, evidence,
                                                   reviewed_by)
             VALUES ($1, $2, 'continue', '{\"acceptance\": \"fixture\"}'::jsonb, $3)
             RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(tranche)
        .bind(seeded.user_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("insert the tranche review");
        sqlx::query(
            "UPDATE programme_tranche SET authorisation_review_id = $2
              WHERE tenant_id = $1 AND id = $3",
        )
        .bind(seeded.tenant_id)
        .bind(review)
        .bind(tranche)
        .execute(&pg.super_pool)
        .await
        .expect("link the tranche review");
    }
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({
                "title": "Programme-track delivery",
                "key": format!("PG-{}", Uuid::now_v7().simple()),
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
                    "track": "programme",
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
            json!({ "id": id }),
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
            json!({ "approval_id": approval_id, "decision": "approve" }),
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
            json!({ "project_id": project, "charter": "The project's mandate and scope." }),
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

async fn close_request(running: &support::Running, project: &str, delegation: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "The project delivered its scope; the closure evidence is published.",
                "delegation_ref": delegation,
                "evidence": { "manifest": "acceptance/programme/T.toml" },
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
            json!({ "id": project }),
            None,
        )
        .await
}

async fn project_status(running: &support::Running, project: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({ "id": project }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    text(&output(&body)["project"], "status")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_programme_track_project_is_not_exempt_from_phase_g() {
    let (pg, running, seeded) = fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_track_project(&pg, &running, &seeded).await;
    charter(&running, &project).await;
    baseline(&running, &project).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(
        status, 409,
        "a programme-track project closed without Phase G: {body}"
    );
    assert_eq!(body["type"], "closure_contract_missing", "{body}");
    assert_eq!(
        project_status(&running, &project).await,
        "active",
        "the refused close must not close"
    );
}
