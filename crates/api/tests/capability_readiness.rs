//! S15 step 12 — readiness over the real served API (the slice's named gate binary).
//!
//! Two properties, each a way the projection could lie:
//!
//! * `capabilities.get` reports `{ready, blocking[]}` computed from the same facts the pre-run
//!   guard uses, and `process_runs.start` refuses with the SAME `FieldError` array and **zero**
//!   `run_step` rows;
//! * an unresolvable `prompt_ref` (the tenant's catalogue has no such active pin) is a readiness
//!   blocker, not a write error and not a later model call.
//!
//! `rbuild -- test -p conveyor-api --test capability_readiness`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // The older catalogue seed carries the project types and card types the board fixture needs
    // (S15's `seed_catalogue` is the capability-aware importer, not a superset of these).
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn codes(rows: &Value) -> Vec<String> {
    rows.as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Cap {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = text(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    (board_id, text(&lane["list"], "id"))
}

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

async fn published_process(running: &support::Running) -> String {
    // S13: every step names a published SOP or `processes.publish` refuses it.
    let sop = support::publish_sop(running, &format!("cap-{}-sop", Uuid::now_v7().simple())).await;
    let steps = json!([
        {"action": "cards.claim", "procedure_id": sop,
         "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"}},
        {"action": "cards.comment", "procedure_id": sop,
         "input_template": {"id": "{{card.id}}", "body": "done"}}
    ]);
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": format!("cap-{}", Uuid::now_v7().simple()), "title": "Capability", "card_type_code": "feature", "steps": steps}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

/// A capability row plus one active executor, bound to the process.
async fn bind(
    pg: &support::Pg,
    seeded: &support::Seeded,
    process: &str,
    allowed_actions: &[&str],
    prompt_ref: Option<&str>,
) -> (Uuid, Uuid) {
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, $2, 'Design', 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("design-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed capability");
    let actions: Vec<String> = allowed_actions
        .iter()
        .map(|action| (*action).to_owned())
        .collect();
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, allowed_actions)
         VALUES ($1, $2, $3, 'tool_agent', 'active', 'local-llm', $4, $5) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(capability)
    .bind(format!("agent-{}", Uuid::now_v7().simple()))
    .bind(prompt_ref)
    .bind(actions)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed agent");
    sqlx::query("UPDATE process SET capability_id = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(process).expect("process id"))
        .bind(capability)
        .execute(&pg.super_pool)
        .await
        .expect("bind the process");
    (capability, agent)
}

async fn counts(pg: &support::Pg, seeded: &support::Seeded) -> (i64, i64) {
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count runs");
    let steps: i64 = sqlx::query_scalar("SELECT count(*) FROM run_step WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count run steps");
    (runs, steps)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blocked_capability_refuses_the_same_projection_before_any_run_row() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running).await;
    let (capability, _) = bind(
        &pg,
        &seeded,
        &process,
        &["cards.claim", "does.not_exist"],
        None,
    )
    .await;

    let (status, get_body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": capability}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{get_body}");
    let document = output(&get_body);
    assert_eq!(document["ready"].as_bool(), Some(false));
    let blocked = codes(&document["blocking"]);
    assert_eq!(blocked, vec!["action_not_registered".to_owned()]);

    let (status, _, start_body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{start_body}");
    assert_eq!(
        start_body["type"].as_str(),
        Some("capability_not_ready"),
        "{start_body}"
    );
    assert_eq!(
        codes(&start_body["errors"]),
        blocked,
        "the GET and the 409 must not drift"
    );
    assert_eq!(counts(&pg, &seeded).await, (0, 0), "a refusal wrote rows");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unresolvable_prompt_is_a_readiness_blocker_not_a_write_error() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running).await;
    let (capability, _) = bind(&pg, &seeded, &process, &["cards.claim"], Some("missing@1")).await;

    let (status, get_body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": capability}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{get_body}");
    assert_eq!(
        codes(&output(&get_body)["blocking"]),
        vec!["prompt_unresolved".to_owned()]
    );

    let (status, _, start_body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{start_body}");
    assert_eq!(
        codes(&start_body["errors"]),
        vec!["prompt_unresolved".to_owned()]
    );
    assert_eq!(counts(&pg, &seeded).await, (0, 0), "a refusal wrote rows");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_capability_starts() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running).await;
    let (capability, agent) = bind(&pg, &seeded, &process, &["cards.claim"], Some("agent@1")).await;

    let (status, get_body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": capability}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{get_body}");
    assert_eq!(
        output(&get_body)["ready"].as_bool(),
        Some(true),
        "{get_body}"
    );
    assert!(codes(&output(&get_body)["blocking"]).is_empty());

    let (status, _, start_body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{start_body}");
    let (recorded_capability, recorded_agent): (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT capability_id, capability_agent_id FROM run WHERE tenant_id = $1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run row");
    assert_eq!(recorded_capability, Some(capability));
    assert_eq!(recorded_agent, Some(agent));
}

/// The live case row j names: a `required` pack pinned to a version the real strata does not have
/// is `pack_not_published`, before any run row. Ignored by default because it needs the compose
/// `strata` profile on mini3; run with `-- --ignored` while that profile is up.
#[tokio::test]
#[ignore = "needs the real strata_rag (compose profile `strata`) at http://mini3.local:8092"]
async fn a_required_pack_with_a_missing_version_is_not_published() {
    support::set_env("CONVEYOR_STRATA_RAG_URL", "http://mini3.local:8092");
    let (pg, running, seeded) = fixture().await;
    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        tenant,
        "strata_rag.token",
        b"conveyor-strata-dev",
        "tenant",
    )
    .await
    .expect("seal the dev strata token");
    tx.commit().await.expect("commit");

    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = published_process(&running).await;
    let (capability, _) = bind(&pg, &seeded, &process, &["cards.claim"], Some("agent@1")).await;

    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": capability,
                "pack_key": "capability-design",
                "pack_version": 999,
                "required": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, get_body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": capability}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{get_body}");
    assert_eq!(
        codes(&output(&get_body)["blocking"]),
        vec!["pack_not_published".to_owned()],
        "{get_body}"
    );

    let (status, _, start_body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{start_body}");
    assert_eq!(
        codes(&start_body["errors"]),
        vec!["pack_not_published".to_owned()]
    );
    assert_eq!(counts(&pg, &seeded).await, (0, 0), "a refusal wrote rows");
}

/// D29: a process whose steps bind their executor starts even when its capability has many
/// active agents; the unbound path keeps the unique-active rule and still refuses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_process_starts_while_the_unbound_capability_stays_ambiguous() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let first_card = create_card(&running, &board, &lane).await;
    let second_card = create_card(&running, &board, &lane).await;
    let bound_process = published_process(&running).await;
    let unbound_process = published_process(&running).await;

    // One capability, one bound process_agent and two more active agents.
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, $2, 'Design', 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("design-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed capability");
    let bound_agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, process_key, status, model_ref, allowed_actions)
         VALUES ($1, $2, $3, 'process_agent', $4, 'active', 'local-llm', '{}') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(capability)
    .bind(format!("agent-{}", Uuid::now_v7().simple()))
    .bind(format!("process-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the bound agent");
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, status, model_ref, allowed_actions)
             VALUES ($1, $2, $3, 'tool_agent', 'active', 'local-llm', '{}')",
        )
        .bind(seeded.tenant_id)
        .bind(capability)
        .bind(format!("agent-{}", Uuid::now_v7().simple()))
        .execute(&pg.super_pool)
        .await
        .expect("seed another active agent");
    }
    for process in [&bound_process, &unbound_process] {
        sqlx::query("UPDATE process SET capability_id = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(process).expect("process id"))
            .bind(capability)
            .execute(&pg.super_pool)
            .await
            .expect("bind the process");
    }
    // Only the bound process's steps name the executor (D29's process_agent_id).
    sqlx::query(
        "UPDATE process_step SET process_agent_id = $3
          WHERE tenant_id = $1 AND process_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&bound_process).expect("process id"))
    .bind(bound_agent)
    .execute(&pg.super_pool)
    .await
    .expect("bind the step");

    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": first_card, "process_id": bound_process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    let (status, _, start_body) = running
        .invoke_full(
            "process_runs.start",
            TOKEN,
            json!({"card_id": second_card, "process_id": unbound_process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{start_body}");
    assert_eq!(
        codes(&start_body["errors"]),
        vec!["agent_ambiguous".to_owned()],
        "the unbound capability keeps the unique-active rule"
    );
}
