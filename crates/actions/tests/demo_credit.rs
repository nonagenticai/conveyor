//! #526 (5a) — the SAFe demo credit and the retro lesson rows through the action layer.
//!
//! A credit needs a demo cadence, an accepted deliverable and its evidence (`credit_demo_tx`);
//! a lesson needs a statement (`propose_lesson_tx`). The refusal cases hold the action layer to
//! those writers, and the round trip holds the reads to the rows they wrote.
//!
//! GitHub preflight: `cargo test -p conveyor-actions --test demo_credit`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

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

/// An accepted deliverable cannot be written without the evidence that met its acceptance. The
/// action layer refuses the update; the store-level `demo_gate.rs` still proves the credit refuses
/// an evidence-less row written outside the action.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_accepted_deliverable_must_carry_its_evidence() {
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
    refused(status, &body, "evidence");

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
            "programme_lessons.propose",
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
            "programme_lessons.propose",
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
    // T8's wall reaches the programme register: one reference is not evidence.
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict", "{body}");

    let (status, body) = running
        .invoke(
            "programme_lessons.propose",
            TOKEN,
            json!({
                "programme_id": programme,
                "cadence_id": cadence,
                "finding_key": "F-1",
                "statement": "Accept the increment with its evidence.",
                "evidence": ["retro:note-1", "retro:note-2"],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(output(&body)["independent_count"], 2, "{body}");

    let (status, body) = running
        .invoke(
            "programme_lessons.list",
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

async fn retro_meta(pg: &support::Pg, tenant: Uuid, project: &str) -> Value {
    let project_id = Uuid::parse_str(project).expect("the project id");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Retro board', 0) RETURNING id",
    )
    .bind(tenant)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Intake', 0, 'intake')",
    )
    .bind(tenant)
    .bind(board)
    .execute(&pg.super_pool)
    .await
    .expect("insert the intake lane");
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'retro', 'Retro')
         ON CONFLICT DO NOTHING",
    )
    .bind(tenant)
    .execute(&pg.super_pool)
    .await
    .expect("insert the card type");
    json!({"board_id": board.to_string(), "card_type_code": "retro"})
}

async fn create_retro_cadence(running: &support::Running, programme: &str, meta: Value) -> String {
    let (status, body) = running
        .invoke(
            "cadences.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "kind": "retro",
                "period": "PI 1 retro",
                "meta": meta,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn cadence_state(running: &support::Running, programme: &str, cadence: &str) -> String {
    let (status, body) = running
        .invoke(
            "cadences.list",
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
        .expect("a page")
        .iter()
        .find(|row| row["id"] == json!(cadence))
        .map(|row| text(row, "state"))
        .expect("the cadence row")
}

async fn cadence_version(running: &support::Running, programme: &str, cadence: &str) -> String {
    let (status, body) = running
        .invoke(
            "cadences.list",
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
        .expect("a page")
        .iter()
        .find(|row| row["id"] == json!(cadence))
        .map(|row| row["version"].as_i64().expect("version").to_string())
        .expect("the cadence row")
}

/// A retro cannot close over a finding with no lesson; the refusal writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retro_close_without_a_lesson_is_refused_and_stays_open() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Retro refusal").await;
    let project = create_project(&running).await;
    let meta = retro_meta(&pg, seeded.tenant_id, &project).await;
    let cadence = create_retro_cadence(&running, &programme, meta).await;

    let (status, body) = running
        .invoke(
            "retros.record_findings",
            TOKEN,
            json!({
                "cadence_id": cadence,
                "findings": [{"key": "F-1", "statement": "the deploy took four tries"}],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "retros.close",
            TOKEN,
            json!({"cadence_id": cadence}),
            None,
            None,
            None,
        )
        .await;
    refused(status, &body, "cadence_id");
    assert_ne!(
        cadence_state(&running, &programme, &cadence).await,
        "closed",
        "a refused close leaves the cadence open"
    );
}

/// The retro records its findings and closes only once every one has a lesson.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retro_closes_once_each_finding_has_a_lesson() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Retro close").await;
    let project = create_project(&running).await;
    let meta = retro_meta(&pg, seeded.tenant_id, &project).await;
    let cadence = create_retro_cadence(&running, &programme, meta).await;

    let (status, body) = running
        .invoke(
            "retros.record_findings",
            TOKEN,
            json!({
                "cadence_id": cadence,
                "findings": [
                    {"key": "F-1", "statement": "the deploy took four tries"},
                    {"key": "F-2", "statement": "the lease proof was missing"},
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    for key in ["F-1", "F-2"] {
        let (status, body) = running
            .invoke(
                "programme_lessons.propose",
                TOKEN,
                json!({
                    "programme_id": programme,
                    "cadence_id": cadence,
                    "finding_key": key,
                    "statement": format!("lesson for {key}"),
                    "evidence": ["retro:note-1", "retro:note-2"],
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "retros.close",
            TOKEN,
            json!({"cadence_id": cadence}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["state"], json!("closed"), "{body}");
    assert_eq!(
        cadence_state(&running, &programme, &cadence).await,
        "closed",
        "the cadence closes with the retro"
    );
}

/// The close writes each finding's lesson from the finding itself, and a systemic finding carries
/// the typed `kata_candidate` marker the kata opener consumes (v1-w2c-2, T5's contract).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retro_close_writes_the_lesson_and_a_systemic_candidate() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Retro writes").await;
    let project = create_project(&running).await;
    let meta = retro_meta(&pg, seeded.tenant_id, &project).await;
    let cadence = create_retro_cadence(&running, &programme, meta).await;

    let findings = json!([
        {
            "key": "F-1",
            "statement": "the deploy took four tries",
            "evidence": ["retro:note-1", "retro:note-2"]
        },
        {
            "key": "F-2",
            "statement": "the lease proof was missing",
            "evidence": ["retro:note-3", "retro:note-4"],
            "systemic": true,
            "kata": {
                "signature": "lease-proof",
                "title": "Pin the lease proof",
                "rationale": "the same gap recurred",
                "target_ref": {"kind": "procedure", "id": "lease"}
            }
        }
    ]);
    let (status, body) = running
        .invoke(
            "retros.record_findings",
            TOKEN,
            json!({"cadence_id": cadence, "findings": findings}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "retros.close",
            TOKEN,
            json!({"cadence_id": cadence}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        cadence_state(&running, &programme, &cadence).await,
        "closed"
    );

    let lessons: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT source, state, meta->'kata_candidate'->>'signature'
           FROM lesson
          WHERE tenant_id = $1 AND cadence_id = $2::uuid
          ORDER BY finding_key",
    )
    .bind(seeded.tenant_id)
    .bind(&cadence)
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the lessons");
    assert_eq!(lessons.len(), 2, "one lesson per finding: {lessons:?}");
    assert!(
        lessons
            .iter()
            .all(|(source, state, _)| source == "retro" && state == "proposed"),
        "both lessons are retro proposals: {lessons:?}"
    );
    let signatures: Vec<&str> = lessons
        .iter()
        .filter_map(|(_, _, signature)| signature.as_deref())
        .collect();
    assert_eq!(
        signatures,
        vec!["lease-proof"],
        "only the systemic finding carries a kata marker: {lessons:?}"
    );
}

/// A retro cannot be closed through a generic cadence edit; the close gate is the only path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cadences_update_cannot_close_a_retro() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Retro bypass").await;
    let project = create_project(&running).await;
    let meta = retro_meta(&pg, seeded.tenant_id, &project).await;
    let cadence = create_retro_cadence(&running, &programme, meta).await;
    let version = cadence_version(&running, &programme, &cadence).await;

    let (status, body) = running
        .invoke(
            "cadences.update",
            TOKEN,
            json!({"id": cadence, "state": "closed"}),
            Some(&version),
            None,
            None,
        )
        .await;
    refused(status, &body, "state");
    assert_ne!(
        cadence_state(&running, &programme, &cadence).await,
        "closed",
        "the refused edit leaves the retro open"
    );
}
