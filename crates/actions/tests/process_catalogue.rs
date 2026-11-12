//! S13 — the shipped 280-process catalogue executes end to end.
//!
//! The store test proves the catalogue imports twice and publishes. This proves a process from
//! it **runs**: `bug_analysis.READY` (an agent step judged by laya, then a computed `cards.move`)
//! on a card in the intake lane reaches the active lane with both steps succeeded, the judge's
//! verdict recorded, and the run pinned to the process's bound executor (D29).
//!
//! `CONVEYOR_EXTERNAL_INFERENCE_URL` and `CONVEYOR_EXTERNAL_LAYA_URL` are process-global, so this
//! binary owns its own stubs and runs alone.
//!
//! `rbuild -- test -p conveyor-actions --test process_catalogue`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const CARD_TYPE: &str = "bug_analysis";
const PROCESS_KEY: &str = "bug_analysis.READY";

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

/// The shipped capability vocabulary the catalogue's agents hang off (`migrate --seed`'s import).
async fn seed_capabilities(pg: &support::Pg, seeded: &support::Seeded) {
    let export = conveyor_store::seed::capability_export().expect("the capability seed parses");
    for capability in export.capabilities {
        sqlx::query(
            "INSERT INTO capability (tenant_id, code, label, status)
             VALUES ($1, $2, $2, 'active')
             ON CONFLICT (tenant_id, code) DO NOTHING",
        )
        .bind(seeded.tenant_id)
        .bind(&capability.code)
        .execute(&pg.super_pool)
        .await
        .expect("seed a capability");
    }
}

/// The project types `maps.yaml` binds to, with the phases the shipped catalogue declares.
async fn seed_project_types(pg: &support::Pg, seeded: &support::Seeded) {
    let raw = include_str!("../../../seed/catalogue.json");
    let document: Value = serde_json::from_str(raw).expect("seed/catalogue.json parses");
    for entry in document["entries"].as_array().expect("entries") {
        if entry["kind"] != "project_type" {
            continue;
        }
        let Some(code) = entry["code"].as_str() else {
            continue;
        };
        let label = entry["label"].as_str().unwrap_or(code);
        let phases: Vec<String> = entry["meta"]["phases"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|phase| phase.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if phases.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
             VALUES ($1, 'project_type', $2, $3, $4, 0)
             ON CONFLICT (tenant_id, kind, code) DO NOTHING",
        )
        .bind(seeded.tenant_id)
        .bind(code)
        .bind(label)
        .bind(json!({ "phases": phases }))
        .execute(&pg.super_pool)
        .await
        .expect("seed a project type");
    }
}

/// A project type whose matrix lets a `bug_analysis` card move intake → active.
async fn seed_matrix_type(pg: &support::Pg, seeded: &support::Seeded) {
    let mut matrix = serde_json::Map::new();
    matrix.insert(
        CARD_TYPE.to_owned(),
        json!({"intake": ["active"], "active": ["done"], "done": ["archive"]}),
    );
    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
         VALUES ($1, 'project_type', 'bug_analysis_delivery', 'Bug analysis delivery', $2, 0)
         ON CONFLICT (tenant_id, kind, code) DO UPDATE SET meta = excluded.meta",
    )
    .bind(seeded.tenant_id)
    .bind(json!({ "matrix": matrix }))
    .execute(&pg.super_pool)
    .await
    .expect("seed the matrix project type");
}

/// Seal the tenant's `laya.token` so `laya.judge` is configured (a missing secret is an outage).
async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "laya.token",
        b"test-laya-token",
        "tenant",
    )
    .await
    .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

struct Stub {
    base: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(app: axum::Router) -> Stub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Stub { base, task }
}

/// The inference-router stub: one assistant message with no tool calls ends the loop.
async fn inference_stub() -> Stub {
    serve(axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(|| async {
            axum::Json(json!({
                "id": "chatcmpl-catalogue",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "the analysis is complete"},
                    "finish_reason": "stop"
                }],
                "usage": {"total_tokens": 12}
            }))
        }),
    ))
    .await
}

/// The laya stub: an accepted verdict above the SOP's 0.7 threshold.
async fn laya_stub() -> Stub {
    serve(axum::Router::new().fallback(axum::routing::post(|| async {
        axum::Json(json!({
            "answers": {"accept": {"score": 0.9, "reason": "meets the criteria"}}
        }))
    })))
    .await
}

/// A project of the matrix type, its board, and the board's intake lane.
async fn project_board_intake(running: &support::Running) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({
                "title": format!("Catalogue {}", Uuid::now_v7().simple()),
                "project_type_code": "bug_analysis_delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project = text(output(&created), "id");
    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board = text(&output(&view)["boards"][0], "id");
    let (status, board_view) = running
        .invoke("boards.get", TOKEN, json!({"id": board}), None, None, None)
        .await;
    assert_eq!(status, 200, "{board_view}");
    let intake = output(&board_view)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "intake")
        .unwrap_or_else(|| panic!("no intake lane in {board_view}"));
    (project, board, text(&intake["list"], "id"))
}

async fn create_card(running: &support::Running, board: &str, intake: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": intake,
                "name": format!("Catalogue card {}", Uuid::now_v7().simple()),
                "card_type_code": CARD_TYPE,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

async fn execute(pg: &support::Pg, seeded: &support::Seeded, run: &str, position: i32) {
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(run).expect("run id"),
        position,
        1,
    )
    .await
    .unwrap_or_else(|failure| panic!("step {position}: {}", failure.message));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_shipped_catalogue_runs_a_process_end_to_end() {
    let (pg, running, seeded) = fixture().await;
    // The vocabulary a process catalogue needs: card types, project types and the gate codes a
    // gate step names. `migrate --seed --catalogue` is the production command; this is its code
    // path. Without it `ask_fr`'s `human_approval` gate is "not a gate in this tenant".
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the catalogue vocabulary");
    seed_capabilities(&pg, &seeded).await;
    seed_project_types(&pg, &seeded).await;
    seed_matrix_type(&pg, &seeded).await;

    let counts = conveyor_store::process_seed::apply_all(&pg.runtime, seeded.tenant)
        .await
        .expect("apply the shipped catalogue");
    // 280 card-type processes (70 type files x four list stages) + the 53 named ones
    // (51 `cap.*`, `ask_fr`, `kata.open`). The manifest's 280 is the type catalogue alone.
    assert_eq!(counts.processes, 333, "{counts:?}");
    assert_eq!(counts.maps, 3, "{counts:?}");

    let inference = inference_stub().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &inference.base);
    let laya = laya_stub().await;
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya.base);
    seal_laya_token(&pg, &seeded).await;

    let (_project, board, intake) = project_board_intake(&running).await;
    let card = create_card(&running, &board, &intake).await;

    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": PROCESS_KEY}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run = text(output(&body), "id");

    execute(&pg, &seeded, &run, 1).await;
    execute(&pg, &seeded, &run, 2).await;

    let run_status: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run row");
    assert_eq!(run_status, "succeeded");

    let rows: Vec<(i32, String, Option<String>, Value)> = sqlx::query_as(
        "SELECT position, status, action, COALESCE(judge, 'null'::jsonb)
           FROM run_step
          WHERE tenant_id = $1 AND run_id = $2
          ORDER BY position, attempt",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the run's steps");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].1, "succeeded", "{rows:?}");
    assert_eq!(rows[0].2.as_deref(), Some("agent.run"));
    assert_eq!(rows[0].3["decision"], "accept", "{}", rows[0].3);
    assert!(
        rows[0].3["score"].as_f64().unwrap_or_default() >= 0.7,
        "{}",
        rows[0].3
    );
    assert_eq!(rows[1].1, "succeeded", "{rows:?}");
    assert_eq!(rows[1].2.as_deref(), Some("cards.move"));

    let list_now: Uuid =
        sqlx::query_scalar("SELECT list_id FROM card WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&card).expect("card id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the card row");
    let active: Uuid = sqlx::query_scalar(
        "SELECT id FROM list WHERE tenant_id = $1 AND board_id = $2 AND kind = 'active'",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&board).expect("board id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the active lane");
    assert_eq!(list_now, active, "the process moved the card to active");

    let agent: Option<Uuid> =
        sqlx::query_scalar("SELECT capability_agent_id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run's executor");
    assert!(
        agent.is_some(),
        "the run pinned the process's bound executor (D29)"
    );

    // ── D29: the catalogue a tenant receives is editable through the API ────────────────────
    //
    // Before 2026-11-10 this was impossible: `validate_publish` refused the stored action on
    // every agent step the seed writes, so publishing a fork of ANY catalogue process answered
    // `422 process.invalid_step`. The fork copies the steps verbatim, so this asserts the seeded
    // shape and the validator agree — the whole point of one step contract.
    let (status, forked) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": PROCESS_KEY, "title": "Bug analysis — ready (tenant copy)"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{forked}");
    let draft = text(output(&forked), "id");
    let (status, published) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": draft}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 200,
        "a tenant may publish its own version of a catalogue process: {published}"
    );
    assert_eq!(
        output(&published)["version"],
        json!(2),
        "the fork publishes as version 2: {published}"
    );

    let steps: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT position, kind, action FROM process_step
          WHERE tenant_id = $1 AND process_id = $2 ORDER BY position",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&draft).expect("draft id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the fork's steps");
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert_eq!(steps[0].1, "agent");
    assert_eq!(
        steps[0].2.as_deref(),
        Some("agent.run"),
        "the fork keeps the primitive its step runs"
    );
    assert_eq!(steps[1].1, "computed");
    assert_eq!(steps[1].2.as_deref(), Some("cards.move"));

    // ── the seeded GATE executes: ask_fr parks at human_approval and resumes on a decision ──
    //
    // The catalogue carried no gate step before 2026-11-10 (the seed could not express one), so
    // `ask_fr` silently lost the human wait its own SOP declares.
    let ask_card = create_card(&running, &board, &intake).await;
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": ask_card, "process_key": "ask_fr"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let ask_run = text(output(&body), "id");
    execute(&pg, &seeded, &ask_run, 1).await;
    execute(&pg, &seeded, &ask_run, 2).await;

    let (run_status,): (String,) =
        sqlx::query_as("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&ask_run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the ask run");
    assert_eq!(run_status, "waiting_approval", "the gate parks the run");
    let (approval, gate): (Uuid, String) = sqlx::query_as(
        "SELECT id, gate FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'process'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&ask_run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the seeded gate opened");
    assert_eq!(gate, "human_approval");

    // The worker would have consumed the park job; close it so the resume's slot is the live one.
    sqlx::query(
        "UPDATE job SET status = 'succeeded'
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{ask_run}:2:1"))
    .execute(&pg.super_pool)
    .await
    .expect("the park job is consumed");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    execute(&pg, &seeded, &ask_run, 2).await;

    let (step_status, step_kind): (String, String) = sqlx::query_as(
        "SELECT r.status, s.kind FROM run_step r
           JOIN process_step s ON s.id = r.process_step_id
          WHERE r.tenant_id = $1 AND r.run_id = $2 AND r.position = 2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&ask_run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the gate step");
    assert_eq!(step_kind, "gate");
    assert_eq!(step_status, "succeeded", "the approval IS the step's work");
    let (ask_status,): (String,) =
        sqlx::query_as("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&ask_run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the ask run");
    assert_eq!(ask_status, "succeeded");

    // ── the seeded CALL executes: the coach starts a real child kata run ────────────────────
    let coach_card = create_card(&running, &board, &intake).await;
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": coach_card, "process_key": "cap.kanban_coach.handle_error"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let coach_run = text(output(&body), "id");
    execute(&pg, &seeded, &coach_run, 1).await;
    execute(&pg, &seeded, &coach_run, 2).await;

    let child: Option<(Uuid, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, parent_run_id, step_id FROM run
          WHERE tenant_id = $1 AND parent_run_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&coach_run).expect("run id"))
    .fetch_optional(&pg.super_pool)
    .await
    .expect("the child run");
    let (child_id, parent, step) = child.expect("the seeded call step started a child run");
    assert_eq!(parent, Some(Uuid::parse_str(&coach_run).expect("run id")));
    let call_step: Uuid = sqlx::query_scalar(
        "SELECT r.process_step_id FROM run_step r
          WHERE r.tenant_id = $1 AND r.run_id = $2 AND r.position = 2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&coach_run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the call step");
    assert_eq!(
        step,
        Some(call_step),
        "the child names the parent's call step"
    );
    let child_process: Option<String> = sqlx::query_scalar(
        "SELECT p.key FROM run r JOIN process p ON p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(child_id)
    .fetch_optional(&pg.super_pool)
    .await
    .expect("the child's process");
    assert_eq!(
        child_process.as_deref(),
        Some("kata.open"),
        "the coach escalates to the kata capability's own process"
    );
}
