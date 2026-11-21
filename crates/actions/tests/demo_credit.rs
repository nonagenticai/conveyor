//! #526 (5a) — the SAFe demo credit and the retro lesson rows through the action layer.
//!
//! A credit needs a demo cadence, an accepted deliverable and its evidence (`credit_demo_tx`);
//! a lesson needs a statement (`propose_lesson_tx`). The refusal cases hold the action layer to
//! those writers, and the round trip holds the reads to the rows they wrote.
//!
//! GitHub preflight: `cargo test -p conveyor-actions --test demo_credit`.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

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

async fn create_cadence(running: &support::Running, programme: &str, kind: &str) -> String {
    let (status, body) = running
        .invoke(
            "cadences.create",
            TOKEN,
            json!({"programme_id": programme, "kind": kind, "period": "PI-2027.1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_project(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Demo credit", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_deliverable(running: &support::Running, project: &str) -> (String, i64) {
    let (status, body) = running
        .invoke(
            "wbs.create",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "deliverable",
                "title": "The increment",
                "business_value": 3,
                "time_criticality": 2,
                "risk_reduction": 1,
                "job_size": 2,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let out = output(&body);
    (text(out, "id"), out["version"].as_i64().expect("a version"))
}

async fn update_deliverable(
    running: &support::Running,
    version: i64,
    input: Value,
) -> (u16, Value) {
    let etag = format!("\"{version}\"");
    running
        .invoke("wbs.update", TOKEN, input, Some(&etag), None, None)
        .await
}

async fn accept(running: &support::Running, deliverable: &str, version: i64, evidence: Value) {
    let (status, body) = update_deliverable(
        running,
        version,
        json!({"id": deliverable, "status": "accepted", "evidence": evidence}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
}

async fn credit(running: &support::Running, cadence: &str, deliverable: &str) -> (u16, Value) {
    running
        .invoke(
            "cadences.credit_demo",
            TOKEN,
            json!({"cadence_id": cadence, "deliverable_id": deliverable}),
            None,
            None,
            None,
        )
        .await
}

fn refused(status: u16, body: &Value, field: &str) {
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    let fields: Vec<&str> = body["errors"]
        .as_array()
        .expect("errors[] is populated")
        .iter()
        .filter_map(|error| error["field"].as_str())
        .collect();
    assert!(fields.contains(&field), "{fields:?} in {body}");
}

/// A credit needs a `demo` cadence; a sprint cadence is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_credit_needs_a_demo_cadence() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Demo credit kinds").await;
    let cadence = create_cadence(&running, &programme, "sprint").await;
    let project = create_project(&running).await;
    let (deliverable, version) = create_deliverable(&running, &project).await;
    accept(&running, &deliverable, version, json!(["doc:increment"])).await;

    let (status, body) = credit(&running, &cadence, &deliverable).await;
    refused(status, &body, "cadence_id");
}

/// A credit needs the deliverable accepted; an open one is refused even with evidence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_credit_needs_an_accepted_deliverable() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Demo credit acceptance").await;
    let cadence = create_cadence(&running, &programme, "demo").await;
    let project = create_project(&running).await;
    let (deliverable, version) = create_deliverable(&running, &project).await;
    let (status, body) = update_deliverable(
        &running,
        version,
        json!({"id": deliverable, "evidence": ["doc:increment"]}),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = credit(&running, &cadence, &deliverable).await;
    refused(status, &body, "deliverable_id");
}

/// A credit needs the deliverable's evidence, not only its acceptance.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_credit_needs_evidence() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Demo credit evidence").await;
    let cadence = create_cadence(&running, &programme, "demo").await;
    let project = create_project(&running).await;
    let (deliverable, version) = create_deliverable(&running, &project).await;
    let (status, body) = update_deliverable(
        &running,
        version,
        json!({"id": deliverable, "status": "accepted"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = credit(&running, &cadence, &deliverable).await;
    refused(status, &body, "deliverable_id");
}

/// A lesson needs a statement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lesson_needs_a_statement() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Lessons").await;
    let (status, body) = running
        .invoke(
            "lessons.propose",
            TOKEN,
            json!({
                "programme_id": programme,
                "finding_key": "F-1",
                "statement": "   ",
                "evidence": [],
            }),
            None,
            None,
            None,
        )
        .await;
    refused(status, &body, "statement");
}

/// The credit is recorded once and listed with the deliverable's evidence; the lesson joins it
/// in the programme's memory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_credit_and_a_lesson_round_trip() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Demo memory").await;
    let cadence = create_cadence(&running, &programme, "demo").await;
    let project = create_project(&running).await;
    let (deliverable, version) = create_deliverable(&running, &project).await;
    let evidence = json!(["doc:increment", "test:acceptance"]);
    accept(&running, &deliverable, version, evidence.clone()).await;

    let (status, body) = credit(&running, &cadence, &deliverable).await;
    assert_eq!(status, 201, "{body}");
    let credited = output(&body);
    assert_eq!(credited["cadence_id"], json!(cadence), "{body}");
    assert_eq!(credited["deliverable_id"], json!(deliverable), "{body}");
    assert_eq!(credited["evidence"], evidence, "{body}");
    assert_eq!(credited["version"], 1, "{body}");

    // The second call returns the same row, not a second credit.
    let (status, again) = credit(&running, &cadence, &deliverable).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(output(&again)["id"], credited["id"], "{again}");

    let (status, body) = running
        .invoke(
            "cadences.credit_list",
            TOKEN,
            json!({"cadence_id": cadence}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let credits = output(&body)["items"].as_array().expect("a page");
    assert_eq!(credits.len(), 1, "{body}");
    assert_eq!(credits[0]["id"], credited["id"], "{body}");

    let (status, body) = running
        .invoke(
            "lessons.propose",
            TOKEN,
            json!({
                "programme_id": programme,
                "cadence_id": cadence,
                "finding_key": "F-1",
                "statement": "Accept the increment with its evidence.",
                "evidence": ["retro:note-1"],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(output(&body)["independent_count"], 1, "{body}");

    let (status, body) = running
        .invoke(
            "lessons.list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let lessons = output(&body)["items"].as_array().expect("a page");
    assert_eq!(lessons.len(), 1, "{body}");
    assert_eq!(lessons[0]["finding_key"], "F-1", "{body}");
    assert_eq!(
        lessons[0]["statement"], "Accept the increment with its evidence.",
        "{body}"
    );
}
