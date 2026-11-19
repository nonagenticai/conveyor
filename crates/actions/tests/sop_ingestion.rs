//! T16 — SOP / process-document ingestion, the publication gate, the PDDL run and the source gate.
//!
//! Every scenario runs against real Postgres 18 through the public HTTP action surface; the model
//! and the `strata_ingest` handoff are the repo's stub seam. Nothing here is a mock of the engine:
//! the actions, the transaction, the gates and the executor are the production code.

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::Mutex;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

/// The inference and strata destinations are process-global; every test that binds one holds this
/// lock for the whole scenario so a concurrent test cannot overwrite its stub.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn field<'a>(body: &'a Value, name: &str) -> &'a str {
    output(body)[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name} is absent from {body}"))
}

/// The inference stub: answers `POST /api/chat/completions` with the draft this test sets.
async fn stub_inference() -> (String, Arc<Mutex<String>>) {
    let draft: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let state = draft.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move || {
            let state = state.clone();
            async move {
                let content = state.lock().await.clone();
                axum::Json(json!({
                    "id": "chatcmpl-t16",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": content},
                        "finish_reason": "stop"
                    }],
                    "usage": {"total_tokens": 9}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the inference stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, draft)
}

/// One step of a model draft.
fn step(action: &str, text: &str, pre: &[&str], add: &[&str], delete: &[&str]) -> Value {
    json!({
        "action": action,
        "action_text": text,
        "preconditions": pre,
        "effects": {"add": add, "delete": delete},
    })
}

/// The whole draft document, as the model's `content` string.
fn draft(steps: Vec<Value>, requested_tools: &[&str]) -> String {
    json!({
        "domain": "(define (domain receiving) (:requirements :strips :typing))",
        "init": [],
        "steps": steps,
        "capability": {"code": "receiving", "label": "Receiving", "description": ""},
        "agent": {"key": "receiving_agent", "model_ref": ""},
        "job": {"job_key": "receiving_job", "title": "Receiving job", "family": "operations"},
        "requested_tools": requested_tools,
    })
    .to_string()
}

/// A two-step draft whose second step needs the first step's effect.
fn receiving_draft() -> String {
    draft(
        vec![
            step(
                "procedures.list",
                "verify_stock",
                &[],
                &["stock_verified"],
                &[],
            ),
            step(
                "processes.list",
                "put_away",
                &["stock_verified"],
                &["stored"],
                &["dock_free"],
            ),
        ],
        &[],
    )
}

fn sop_input(process_key: &str, source_key: &str) -> Value {
    let source = std::fs::read_to_string("tests/fixtures/t16/sop.txt").expect("the SOP fixture");
    json!({
        "title": "Synthetic receiving SOP",
        "process_key": process_key,
        "source_key": source_key,
        "source": {"kind": "text", "media_type": "text/plain", "text": source},
    })
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The asset's `(kind, status, meta)`.
async fn asset_row(pg: &support::Pg, id: &str) -> (String, String, Value) {
    sqlx::query_as::<_, (String, String, Value)>(
        "SELECT kind, status, meta FROM governed_asset WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(id).expect("asset id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the asset row")
}

/// A process row's `(status, source_asset_id, knowledge_pack_ref, approval_id)`.
async fn process_row(
    pg: &support::Pg,
    id: &str,
) -> (String, Option<uuid::Uuid>, String, Option<uuid::Uuid>) {
    sqlx::query_as(
        "SELECT status, source_asset_id, knowledge_pack_ref, approval_id FROM process WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(id).expect("process id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the process row")
}

/// One step of the stored draft: `(kind, action_text, pddl_preconditions, pddl_effects)`.
#[allow(clippy::type_complexity)]
async fn step_rows(
    pg: &support::Pg,
    process_id: &str,
) -> Vec<(String, Option<String>, Value, Value)> {
    sqlx::query_as(
        "SELECT kind, action_text, pddl_preconditions, pddl_effects FROM process_step \
          WHERE process_id = $1 ORDER BY position",
    )
    .bind(uuid::Uuid::parse_str(process_id).expect("process id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the step rows")
}

async fn review_source(running: &support::Running, asset_id: &str) -> (u16, Value) {
    running
        .invoke(
            "procedures.review_source",
            REVIEWER,
            json!({"asset_id": asset_id, "decision": "approve", "note": "reviewed by a person"}),
            None,
            None,
            None,
        )
        .await
}

/// Request and decide the publication gate the way a tenant does: alice asks, the reviewer decides.
async fn publish_gate(running: &support::Running, process_id: &str) -> uuid::Uuid {
    let (status, requested) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "gate": "processes.publish",
                "kind": "publication",
                "target": process_id,
                "description": "Release the derived procedure",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{requested}");
    let approval_id = field(&requested, "id").to_owned();
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    uuid::Uuid::parse_str(&approval_id).expect("approval id")
}

/// Publish the derived SOP judge (the executable's steps are judged against it).
async fn publish_judge(running: &support::Running, procedure_id: &str) {
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": procedure_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// Publish a derived process through its gate (source reviewed, judge published, human decided).
async fn publish_derived(running: &support::Running, process_id: &str, procedure_id: &str) {
    let approval = publish_gate(running, process_id).await;
    let (status, body) = running
        .invoke_approved(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            approval,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    publish_judge(running, procedure_id).await;
}

// ── P1 — ingestion ───────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pdf_sop_becomes_a_pddl_procedure_draft() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    // The real fixture path: a PDF carried as base64, exactly what a browser upload sends.
    let pdf = std::fs::read("tests/fixtures/t16/sop.txt").expect("the SOP fixture");
    draft_content.lock().await.clone_from(&receiving_draft());

    let (status, body) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            json!({
                "title": "Synthetic receiving SOP",
                "process_key": "receiving",
                "source_key": "receiving-sop-pdf",
                "source": {
                    "kind": "pdf",
                    "media_type": "application/pdf",
                    "filename": "receiving.pdf",
                    "content_base64": b64(&pdf),
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    let procedure_id = field(&body, "procedure_id").to_owned();
    let asset_id = field(&body, "source_asset_id").to_owned();
    assert_eq!(output(&body)["steps"], 2, "{body}");
    assert_eq!(output(&body)["converter_version"], "prose@1");

    // The source is untrusted and unreviewed.
    let (kind, status_of_source, meta) = asset_row(&pg, &asset_id).await;
    assert_eq!(kind, "sop_source");
    assert_eq!(status_of_source, "draft");
    assert_eq!(meta["converter_version"], "prose@1");
    assert_eq!(meta["media_type"], "application/pdf");
    assert!(
        meta["sha256"].as_str().is_some_and(|hash| hash.len() == 64),
        "{meta}"
    );

    // The draft is a draft: neither row is published.
    assert_eq!(process_row(&pg, &process_id).await.0, "draft");
    let procedure_status: String = sqlx::query_scalar("SELECT status FROM procedure WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&procedure_id).expect("procedure id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the procedure row");
    assert_eq!(procedure_status, "draft");

    // The steps are PDDL: action text plus ground-literal preconditions and effects.
    let steps = step_rows(&pg, &process_id).await;
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].1.as_deref(), Some("verify_stock"));
    assert_eq!(steps[0].2, json!([]));
    assert_eq!(steps[0].3, json!({"add": ["stock_verified"], "delete": []}));
    assert_eq!(steps[1].1.as_deref(), Some("put_away"));
    assert_eq!(steps[1].2, json!(["stock_verified"]));
    assert_eq!(
        steps[1].3,
        json!({"add": ["stored"], "delete": ["dock_free"]})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_owl_file_becomes_a_knowledge_pack() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let stub = support::Stub::start().await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);
    let (status, body) = running
        .invoke(
            "secrets.create",
            TOKEN,
            json!({"name": "strata_ingest.token", "value": "test-token-not-a-credential"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    let ontology = std::fs::read_to_string("tests/fixtures/t16/ontology.ttl").expect("fixture");
    let store_id = "0199c3c6-0000-7000-8000-000000000001";
    let (status, body) = running
        .invoke(
            "procedures.ingest_rdf",
            TOKEN,
            json!({
                "title": "Synthetic receiving ontology",
                "source_key": "receiving-ontology",
                "source": {"kind": "rdf", "media_type": "text/turtle", "text": ontology},
                "strata_store_id": store_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let asset_id = field(&body, "source_asset_id").to_owned();
    let document_id = field(&body, "document_id").to_owned();
    let pack = field(&body, "knowledge_pack_ref").to_owned();
    assert!(pack.starts_with(store_id), "{body}");
    assert!(!pack.is_empty());

    // The document is stored and names the strata job it became.
    let external: Option<String> =
        sqlx::query_scalar("SELECT external_ref FROM document WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&document_id).expect("document id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the document row");
    assert_eq!(external.as_deref(), Some(pack.as_str()));

    // The source is a draft governed asset carrying the pack, and NOTHING local represents the
    // ontology: no PKO table exists at all (owner decision 2026-11-19).
    let (kind, status_of_source, meta) = asset_row(&pg, &asset_id).await;
    assert_eq!(kind, "sop_source");
    assert_eq!(status_of_source, "draft");
    assert_eq!(meta["knowledge_pack_ref"], pack);
    let pko_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables \
          WHERE table_schema = 'public' AND table_name LIKE 'pko\\_%'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("the pko-table count");
    assert_eq!(
        pko_tables, 0,
        "no PKO table may exist (#413 owner decision)"
    );
    // The external_ref above is the proof the stub was reached; its counter covers only the
    // catch-all routes, not `/api/ingest/jobs`.
    let _ = &stub;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bpmn_diagram_becomes_a_procedure_draft() {
    let (pg, _seeded, running) = fixture().await;
    let bpmn = std::fs::read("tests/fixtures/t16/process.bpmn").expect("the BPMN fixture");
    let (status, body) = running
        .invoke(
            "procedures.ingest_process",
            TOKEN,
            json!({
                "title": "Synthetic receiving process",
                "process_key": "receiving-process",
                "source_key": "receiving-bpmn",
                "source": {
                    "kind": "bpmn",
                    "media_type": "application/xml",
                    "filename": "process.bpmn",
                    "content_base64": b64(&bpmn),
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    assert_eq!(output(&body)["steps"], 3, "{body}");

    // Activities become steps, the gateway becomes a gate.
    let steps = step_rows(&pg, &process_id).await;
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].1.as_deref(), Some("task_verify"));
    assert_eq!(steps[1].0, "computed");
    assert_eq!(steps[1].1.as_deref(), Some("gateway_stock"));
    assert_eq!(steps[2].1.as_deref(), Some("task_putaway"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_sop_yields_capability_and_job_drafts() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content.lock().await.clone_from(&receiving_draft());

    let (status, industry) = running
        .invoke(
            "industries.create",
            TOKEN,
            json!({"key": "logistics", "name": "Logistics"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{industry}");
    let industry_id = field(&industry, "id").to_owned();
    let (status, department) = running
        .invoke(
            "departments.create",
            TOKEN,
            json!({"name": "Receiving", "industry_id": industry_id, "ordinal": 1, "is_primary": true}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{department}");
    let department_id = field(&department, "id").to_owned();

    let mut input = sop_input("receiving-design", "receiving-sop-design");
    input["industry_id"] = json!(industry_id);
    input["department_id"] = json!(department_id);
    let (status, body) = running
        .invoke("procedures.ingest_sop", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 201, "{body}");
    let capability_id = field(&body, "capability_id").to_owned();
    let agent_id = field(&body, "agent_id").to_owned();
    let job_id = field(&body, "job_id").to_owned();

    // All three are drafts; the agent may never hold a tool the SOP asked for.
    let capability_status: String =
        sqlx::query_scalar("SELECT status FROM capability WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&capability_id).expect("capability id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the capability row");
    assert_eq!(capability_status, "draft");
    let (agent_status, allowed): (String, Vec<String>) =
        sqlx::query_as("SELECT status, allowed_actions FROM capability_agent WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&agent_id).expect("agent id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the agent row");
    assert_eq!(agent_status, "draft");
    assert!(allowed.is_empty(), "{allowed:?}");
    let job_status: String = sqlx::query_scalar("SELECT status FROM job_catalogue WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&job_id).expect("job id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the job row");
    assert_eq!(job_status, "draft");
}

// ── P2 — the human publication gate ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_drafting_job_cannot_publish_its_own_procedure() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content.lock().await.clone_from(&receiving_draft());
    let (status, body) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-gate", "receiving-sop-gate"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    let asset_id = field(&body, "source_asset_id").to_owned();
    let (status, body) = review_source(&running, &asset_id).await;
    assert_eq!(status, 200, "{body}");

    // The drafting caller (alice) attempts publication with no gate: refused, still a draft.
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "approval_required",
        "{body}"
    );
    assert_eq!(process_row(&pg, &process_id).await.0, "draft");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_human_approval_publishes_the_procedure() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content.lock().await.clone_from(&receiving_draft());
    let (status, body) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-publish", "receiving-sop-publish"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    let procedure_id = field(&body, "procedure_id").to_owned();
    let asset_id = field(&body, "source_asset_id").to_owned();
    assert_eq!(review_source(&running, &asset_id).await.0, 200);
    publish_judge(&running, &procedure_id).await;

    let approval = publish_gate(&running, &process_id).await;
    let (status, body) = running
        .invoke_approved(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            approval,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (process_status, _source, _pack, recorded_approval) = process_row(&pg, &process_id).await;
    assert_eq!(process_status, "published");
    assert_eq!(
        recorded_approval,
        Some(approval),
        "the approval is recorded"
    );
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE type = 'process.published' AND aggregate_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&process_id).expect("process id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the event count");
    assert_eq!(events, 1, "exactly one publication event");
}

// ── P4 — the PDDL run ───────────────────────────────────────────────────────────────────

/// A published hand-authored process with two PDDL steps, a card and its board.
///
/// `first_adds` is the first step's effect; `second_requires` is the second step's precondition.
async fn published_pddl_process(
    running: &support::Running,
    key: &str,
    first_adds: &[&str],
    second_requires: &str,
) -> (String, String) {
    let sop = support::publish_sop(running, &format!("{key}-sop")).await;
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("Process {key}"),
                "card_type_code": "feature",
                "steps": [
                    {
                        "action": "procedures.list",
                        "procedure_id": sop,
                        "input_template": {},
                        "action_text": "verify_stock",
                        "pddl_preconditions": [],
                        "pddl_effects": {"add": first_adds, "delete": []},
                    },
                    {
                        "action": "processes.list",
                        "procedure_id": sop,
                        "input_template": {},
                        "action_text": "put_away",
                        "pddl_preconditions": [second_requires],
                        "pddl_effects": {"add": ["stored"], "delete": []},
                    },
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "id").to_owned();
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, project) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("PDDL project {key}"), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = field(&project, "id").to_owned();
    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = output(&view)["boards"][0]["id"]
        .as_str()
        .expect("the project's board")
        .to_owned();
    let (status, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let active = board_active_lane(&board).expect("the board's active lane");
    let (status, card) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board_id,
                "list_id": active,
                "name": "A card to run the procedure",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = field(&card, "id").to_owned();
    (process_id, card_id)
}

/// Find the board's `active` list id from a `boards.get` answer.
fn board_active_lane(board: &Value) -> Option<String> {
    let lists = board
        .get("output")
        .unwrap_or(board)
        .get("lists")?
        .as_array()?;
    for entry in lists {
        let list = entry.get("list").unwrap_or(entry);
        if list.get("kind").and_then(Value::as_str) == Some("active") {
            return list.get("id").and_then(Value::as_str).map(str::to_owned);
        }
    }
    None
}

async fn start_run(running: &support::Running, card_id: &str, key: &str) -> String {
    let (status, _headers, body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card_id, "process_key": key, "inputs": {}}),
            None,
            Some(&format!("t16-{key}-start")),
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    field(&body, "id").to_owned()
}

/// Drive the run's steps exactly as the worker does: one `process.step` execution per position.
async fn drive_steps(pg: &support::Pg, seeded: &support::Seeded, run_id: &str, positions: &[i32]) {
    for position in positions {
        conveyor_actions::process::step::execute_step(
            &pg.runtime,
            seeded.tenant,
            "conveyor",
            uuid::Uuid::parse_str(run_id).expect("run id"),
            *position,
            1,
        )
        .await
        .expect("the step executes");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_job_runs_the_pddl_steps() {
    let (pg, seeded, running) = fixture().await;
    let (_process_id, card_id) =
        published_pddl_process(&running, "pddl-run", &["stock_verified"], "stock_verified").await;
    let run_id = start_run(&running, &card_id, "pddl-run").await;
    drive_steps(&pg, &seeded, &run_id, &[1, 2]).await;

    // Every step is recorded with the fact state either side of it, in position order.
    let rows: Vec<(i32, String, Option<Value>, Option<Value>)> = sqlx::query_as(
        "SELECT position, status, pddl_state_before, pddl_state_after FROM run_step \
          WHERE run_id = $1 ORDER BY position",
    )
    .bind(uuid::Uuid::parse_str(&run_id).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the run-step rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1, "succeeded");
    assert_eq!(rows[0].2, Some(json!([])));
    assert_eq!(rows[0].3, Some(json!(["stock_verified"])));
    assert_eq!(rows[1].1, "succeeded");
    assert_eq!(rows[1].2, Some(json!(["stock_verified"])));
    assert_eq!(rows[1].3, Some(json!(["stock_verified", "stored"])));

    let run_status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&run_id).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row");
    assert_eq!(run_status, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_false_precondition_stops_the_run() {
    let (pg, seeded, running) = fixture().await;
    // The first step establishes `dock_cleared`, the second needs `stock_verified`: the run must
    // stop at position 2 and never record position 3 (there is none) nor apply the effect.
    let (_process_id, card_id) =
        published_pddl_process(&running, "pddl-stop", &["dock_cleared"], "stock_verified").await;
    let run_id = start_run(&running, &card_id, "pddl-stop").await;
    drive_steps(&pg, &seeded, &run_id, &[1]).await;
    // Position 2 fails deterministically; `execute_step` returns the failure to its caller (the
    // job), so the test asserts the recorded row rather than the returned error.
    let _ = conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        uuid::Uuid::parse_str(&run_id).expect("run id"),
        2,
        1,
    )
    .await;

    let rows: Vec<(i32, String, Option<String>, Option<Value>, Option<Value>)> = sqlx::query_as(
        "SELECT position, status, error, pddl_state_before, pddl_state_after FROM run_step \
          WHERE run_id = $1 ORDER BY position",
    )
    .bind(uuid::Uuid::parse_str(&run_id).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the run-step rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].1, "succeeded");
    assert_eq!(rows[0].4, Some(json!(["dock_cleared"])));
    assert_eq!(rows[1].1, "failed");
    assert!(
        rows[1]
            .2
            .as_deref()
            .unwrap_or_default()
            .contains("pddl_precondition_false"),
        "{rows:?}"
    );
    // The failed step changed no state: its effect (`stored`) never holds and the trace is NULL.
    assert_eq!(rows[1].3, None);
    assert_eq!(rows[1].4, None);

    let run_status: String = sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&run_id).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row");
    assert_eq!(run_status, "failed");
}

// ── P5 — the source gate ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreviewed_source_cannot_bind() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content.lock().await.clone_from(&receiving_draft());
    let (status, body) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-unsourced", "receiving-sop-unsourced"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    let asset_id = field(&body, "source_asset_id").to_owned();

    // Publish is refused naming the unreviewed source — before any approval is considered.
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "source.unapproved",
        "{body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains(&asset_id),
        "{body}"
    );

    // A binding cannot name the draft version at all.
    let (status, body) = running
        .invoke(
            "job_bindings.create",
            TOKEN,
            json!({
                "job_id": "0199c3c6-0000-7000-8000-000000000002",
                "release_id": "0199c3c6-0000-7000-8000-000000000003",
                "executor_kind": "procedure",
                "process_id": process_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "process.not_published",
        "{body}"
    );
    assert_eq!(process_row(&pg, &process_id).await.0, "draft");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_sop_cannot_grant_a_tool() {
    let _env = ENV_LOCK.lock().await;
    let (pg, _seeded, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content.lock().await.clone_from(&draft(
        vec![step(
            "procedures.list",
            "verify_stock",
            &[],
            &["stock_verified"],
            &[],
        )],
        &["shell.exec"],
    ));
    let (status, body) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-tool", "receiving-sop-tool"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_id = field(&body, "process_id").to_owned();
    let agent_id = field(&body, "agent_id").to_owned();
    assert_eq!(
        output(&body)["requested_tools"],
        Value::Null,
        "the tool request is never echoed as a grant"
    );

    // The agent holds nothing, and validation refuses the draft by name.
    let allowed: Vec<String> =
        sqlx::query_scalar("SELECT allowed_actions FROM capability_agent WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&agent_id).expect("agent id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the agent row");
    assert!(allowed.is_empty(), "{allowed:?}");
    let (status, body) = running
        .invoke(
            "procedures.validate",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "sop.cannot_grant_tool",
        "{body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("shell.exec"),
        "{body}"
    );
    let allowed_after: Vec<String> =
        sqlx::query_scalar("SELECT allowed_actions FROM capability_agent WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&agent_id).expect("agent id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the agent row");
    assert!(allowed_after.is_empty(), "the ceiling is unchanged");
}
