//! S21 step 3 (#15) — the work-package compile over real facts.
//!
//! One run, one card, one capability with a bound pack, two memory rows and a tenant recipe that
//! names a memory selector and a knowledge query. The compile is the executor's own
//! (`compile_for_run`): it resolves the recipe and facts in a transaction, commits, retrieves
//! through the one path `knowledge.query` uses, and only then renders. The assertions are about
//! what the model would receive — pack identity, retrieved content, memory rows and the card —
//! and about the hash staying the identity of the same recipe + facts.

mod support;

use axum::Json;
use axum::routing::get;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const LEAF: &str = "0195f48b-d000-7000-8000-0000000000aa";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn board_lane_card(running: &support::Running) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Prompt knowledge {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");
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
    let board_id = text(&output(&view)["boards"][0], "id");
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
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    let lane_id = text(&lane["list"], "id");
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({"board_id": board_id, "list_id": lane_id,
                   "name": "Explain the header", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    (project_id, board_id, text(output(&created), "id"))
}

async fn seed_capability(pg: &support::Pg, seeded: &support::Seeded) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.prompt_knowledge', 'Prompt knowledge', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert capability");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, process_key, status, model_ref, prompt_ref,
              params, allowed_actions, max_iterations, timeout_secs)
         VALUES ($1, $2, 'agent', 'process_agent', 'prompt-knowledge', 'active',
                 'resolved-model', 'agent@1', '{\"role\": \"researcher\"}'::jsonb,
                 ARRAY['workflows.list'], 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert executor");
    sqlx::query(
        "INSERT INTO capability_knowledge
             (tenant_id, capability_id, pack_key, pack_version, required)
         VALUES ($1, $2, 'devpack', 1, true)",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .execute(&mut *tx)
    .await
    .expect("bind the pack");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

async fn create_recipe(running: &support::Running) {
    let (status, body) = running
        .invoke(
            "prompt_recipes.create",
            TOKEN,
            json!({
                "key": "knowledge_test",
                "title": "Knowledge test",
                "sections": [
                    {"position": 1, "source_kind": "system", "required": true,
                     "template": "You answer from the pinned knowledge."},
                    {"position": 2, "source_kind": "knowledge",
                     "selector": {"query": "the header", "top_k": 4}},
                    {"position": 3, "source_kind": "memory", "selector": {"limit": 8}},
                    {"position": 4, "source_kind": "project_card"}
                ]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = running
        .invoke(
            "prompt_recipes.activate",
            TOKEN,
            json!({"key": "knowledge_test", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// The one `strata_rag` path the compiler uses: pack leaves, then the query.
async fn strata_stub() -> String {
    let app = axum::Router::new()
        .route(
            "/api/platform/packs/{key}/{version}/stores",
            get(|| async { Json(json!({"stores": [{"kind": "leaf", "id": LEAF}]})) }),
        )
        .route(
            "/api/query",
            axum::routing::post(|| async {
                Json(json!({
                    "results": [
                        {"id": "hit-1", "text": "the pinned knowledge"},
                        {"id": "hit-2", "text": "the second passage"}
                    ]
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind strata stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_compile_carries_the_pack_content_the_memory_rows_and_the_card() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    let (project, _board, card) = board_lane_card(&running).await;
    let (capability, agent) = seed_capability(&pg, &seeded).await;

    let (status, body) = running
        .invoke(
            "secrets.create",
            TOKEN,
            json!({"name": "strata_rag.token", "value": "test-token-not-a-credential"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    support::set_env("CONVEYOR_STRATA_RAG_URL", &strata_stub().await);
    create_recipe(&running).await;

    // Two hot rows on the card; the recipe selects them as `prompt.memory`.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    for (kind, body) in [
        ("fact", "the header regressed before"),
        ("decision", "we chose a CSS fix"),
    ] {
        sqlx::query(
            "INSERT INTO work_package_memory
                 (tenant_id, project_id, card_id, capability_id, kind, source, body, meta)
             VALUES ($1, $2, $3, $4, $5, 'human', $6, '{}')",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(Uuid::parse_str(&project).expect("project"))
        .bind(Uuid::parse_str(&card).expect("card"))
        .bind(capability)
        .bind(kind)
        .bind(body)
        .execute(&mut *tx)
        .await
        .expect("insert memory");
    }
    let (run_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO run
             (tenant_id, card_id, project_id, status, capability_id, capability_agent_id, inputs)
         VALUES ($1, $2, $3, 'running', $4, $5, '{}')
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(Uuid::parse_str(&card).expect("card"))
    .bind(Uuid::parse_str(&project).expect("project"))
    .bind(capability)
    .bind(agent)
    .fetch_one(&mut *tx)
    .await
    .expect("insert run");
    tx.commit().await.expect("commit");

    let compiled = conveyor_actions::prompt::compile_for_run(
        &pg.runtime,
        seeded.tenant,
        run_id,
        None,
        "knowledge_test@1",
    )
    .await
    .expect("compile");
    assert!(
        compiled
            .text
            .contains("## Knowledge\npacks: devpack@1\n- hit-1: the pinned knowledge\n- hit-2: the second passage"),
        "retrieved content and pack identity: {}",
        compiled.text
    );
    assert!(
        compiled.text.contains("## Memory\n- [fact/human#"),
        "memory rows render: {}",
        compiled.text
    );
    assert!(
        compiled.text.contains("the header regressed before"),
        "{}",
        compiled.text
    );
    assert!(
        compiled.text.contains("we chose a CSS fix"),
        "{}",
        compiled.text
    );
    assert!(
        compiled.text.contains("card: Explain the header"),
        "{}",
        compiled.text
    );

    // The same recipe and the same facts compile byte-identically; the hash is the identity.
    let again = conveyor_actions::prompt::compile_for_run(
        &pg.runtime,
        seeded.tenant,
        run_id,
        None,
        "knowledge_test@1",
    )
    .await
    .expect("second compile");
    assert_eq!(compiled.text, again.text);
    assert_eq!(compiled.sha256, again.sha256);

    // The memory the run already recorded is no longer a hidden side channel: it is in the text.
    let names: Vec<&str> = compiled.text.lines().collect();
    assert!(names.iter().any(|line| line.contains("[decision/human#")));
}
