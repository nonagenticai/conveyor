//! S11 step 7 — `process_runs.start` over real HTTP.
//!
//! Every call is a real `POST /api/actions/<name>/invocations` against a real Postgres and the served
//! router. The properties asserted here are the ones a silent bug would hide:
//!
//! * a published version — never a draft, and by key always the PUBLISHED one — starts the run;
//! * the card-type guard is a `409 process.card_type_mismatch`, and a process with no card type
//!   applies to any card;
//! * the run is `running` with the caller's `inputs` and the version on its row, every step is
//!   materialised `queued` at attempt 1, and exactly ONE `process.step` job exists (position 1) —
//!   with no `run.execute` job, because this action is the driver (S11 §5.5);
//! * `Idempotency-Key` replays the stored run, while a second start on a live card is a `409`.
//!
//! The S11 doc names `crates/api/tests/process_*.rs`; the six `processes.*` tests live in
//! `crates/actions/tests/process.rs` and this file follows them (both harnesses are the same
//! `support` module over real HTTP).

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
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

fn slug(body: &Value) -> &str {
    body["type"]
        .as_str()
        .unwrap_or_else(|| panic!("no problem `type` in {body}"))
}

/// A project with its template board, and that board's `active` lane.
async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Start test {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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
    (board_id, text(&lane["list"], "id"))
}

async fn create_card(
    running: &support::Running,
    board: &str,
    lane: &str,
    card_type: &str,
) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Card {}", Uuid::now_v7().simple()),
                "card_type_code": card_type,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

/// Two publish-legal steps; the executor is S11 step 8, so only legality matters here.
fn two_steps() -> Value {
    json!([
        {"action": "cards.claim",
         "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"}},
        {"action": "cards.comment",
         "input_template": {"id": "{{card.id}}", "body": "run {{run.inputs.ticket}}"}}
    ])
}

async fn create_process(
    running: &support::Running,
    key: &str,
    sop: &str,
    card_type: Option<&str>,
    steps: Value,
) -> (u16, String, Value) {
    // S13: every step names the published SOP it is judged against.
    let mut steps = steps;
    if let Some(items) = steps.as_array_mut() {
        for step in items {
            step["procedure_id"] = json!(sop);
        }
    }
    let mut input = json!({"key": key, "title": format!("Process {key}"), "steps": steps});
    if let Some(code) = card_type {
        input["card_type_code"] = json!(code);
    }
    let (status, body) = running
        .invoke("processes.create", TOKEN, input, None, None, None)
        .await;
    let id = if status == 201 {
        text(output(&body), "id")
    } else {
        String::new()
    };
    (status, id, body)
}

async fn publish(running: &support::Running, id: &str) {
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
}

async fn start(
    running: &support::Running,
    input: Value,
    idempotency: Option<&str>,
) -> (u16, std::collections::HashMap<String, String>, Value) {
    running
        .invoke_full("process_runs.start", TOKEN, input, None, idempotency, None)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_materialises_the_run_and_enqueues_the_first_step() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;
    let (_, process, _) =
        create_process(&running, "dev-start", &sop, Some("feature"), two_steps()).await;
    publish(&running, &process).await;
    let published = process.clone();

    let (status, headers, body) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-start", "inputs": {"ticket": "T-1"}}),
        None,
    )
    .await;
    assert_eq!(status, 201, "{body}");
    // The ride through `queued` bumps the row to version 2 before the response is read.
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    let run = output(&body);
    assert_eq!(text(run, "status"), "running");
    assert_eq!(run["attempt"].as_i64(), Some(1));
    assert_eq!(text(run, "card_id"), card);
    let run_id = text(run, "id");

    // The run row carries the version and the caller's inputs.
    let (process_id, inputs, status): (Option<Uuid>, Value, String) = sqlx::query_as(
        "SELECT process_id, inputs, status FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded_tenant(&pg).await)
    .bind(Uuid::parse_str(&run_id).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run row");
    assert_eq!(
        process_id,
        Some(Uuid::parse_str(&published).expect("process id"))
    );
    assert_eq!(inputs, json!({"ticket": "T-1"}));
    assert_eq!(status, "running");

    // One `queued` run_step per step, attempt 1, in position order.
    let steps: Vec<(i32, String, String, i32)> = sqlx::query_as(
        "SELECT position, action, status, attempt FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(seeded_tenant(&pg).await)
    .bind(Uuid::parse_str(&run_id).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("run_step rows");
    assert_eq!(
        steps,
        vec![
            (1, "cards.claim".to_owned(), "queued".to_owned(), 1),
            (2, "cards.comment".to_owned(), "queued".to_owned(), 1),
        ]
    );

    // Exactly one process.step job — position 1, attempt 1 — and NO run.execute driver.
    let jobs: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT kind, dedupe_key, payload FROM job WHERE tenant_id = $1 AND kind = 'process.step'",
    )
    .bind(seeded_tenant(&pg).await)
    .fetch_all(&pg.super_pool)
    .await
    .expect("process.step jobs");
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert_eq!(jobs[0].0, "process.step");
    assert_eq!(jobs[0].1, format!("{run_id}:1:1"));
    assert_eq!(jobs[0].2["position"].as_i64(), Some(1));
    assert_eq!(jobs[0].2["attempt"].as_i64(), Some(1));
    assert_eq!(jobs[0].2["run_id"], json!(run_id));
    let drivers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'run.execute'",
    )
    .bind(seeded_tenant(&pg).await)
    .fetch_one(&pg.super_pool)
    .await
    .expect("run.execute count");
    assert_eq!(
        drivers, 0,
        "the start action is the driver; run.execute must not run too"
    );

    // Both run events are on the run's own topic.
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT type FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq",
    )
    .bind(seeded_tenant(&pg).await)
    .bind(format!("run:{run_id}"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("run events");
    assert_eq!(events, ["run.created", "run_step.requested", "run.started"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idempotency_key_replays_the_same_run() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;
    let (_, process, _) = create_process(&running, "dev-idem", &sop, None, two_steps()).await;
    publish(&running, &process).await;

    let input = json!({"card_id": card, "process_key": "dev-idem", "inputs": {"ticket": "T-1"}});
    let (status, _, first) = start(&running, input.clone(), Some("idem-start")).await;
    assert_eq!(status, 201, "{first}");
    let (status, _, second) = start(&running, input, Some("idem-start")).await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(output(&first)["id"], output(&second)["id"]);

    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded_tenant(&pg).await)
        .fetch_one(&pg.super_pool)
        .await
        .expect("run count");
    assert_eq!(runs, 1, "a replayed key must not create a second run");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'process.step'",
    )
    .bind(seeded_tenant(&pg).await)
    .fetch_one(&pg.super_pool)
    .await
    .expect("job count");
    assert_eq!(jobs, 1);

    // A different input under the same key is a conflict, not a second run.
    let (status, _, refused) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-idem", "inputs": {"ticket": "T-2"}}),
        Some("idem-start"),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(slug(&refused), "idempotency-conflict");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_start_for_the_same_card_is_refused() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;
    let (_, process, _) = create_process(&running, "dev-live", &sop, None, two_steps()).await;
    publish(&running, &process).await;

    let input = json!({"card_id": card, "process_key": "dev-live"});
    let (status, _, first) = start(&running, input.clone(), None).await;
    assert_eq!(status, 201, "{first}");
    let (status, _, second) = start(&running, input, None).await;
    assert_eq!(status, 409, "{second}");
    assert_eq!(slug(&second), "conflict");

    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded_tenant(&pg).await)
        .fetch_one(&pg.super_pool)
        .await
        .expect("run count");
    assert_eq!(runs, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_card_type_mismatch_is_refused_and_a_typeless_process_is_not() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let bug = create_card(&running, &board, &lane, "bug").await;

    // Named a type the card does not have: refused, and nothing written.
    let (_, typed, _) =
        create_process(&running, "dev-typed", &sop, Some("feature"), two_steps()).await;
    publish(&running, &typed).await;
    let (status, _, refused) = start(
        &running,
        json!({"card_id": bug, "process_key": "dev-typed"}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(slug(&refused), "process.card_type_mismatch");
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seeded_tenant(&pg).await)
        .fetch_one(&pg.super_pool)
        .await
        .expect("run count");
    assert_eq!(runs, 0, "a refused start writes nothing");

    // No card type on the process: it applies to any card.
    let (_, untyped, _) = create_process(&running, "dev-untyped", &sop, None, two_steps()).await;
    publish(&running, &untyped).await;
    let (status, _, body) = start(
        &running,
        json!({"card_id": bug, "process_key": "dev-untyped"}),
        None,
    )
    .await;
    assert_eq!(status, 201, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_published_version_can_start() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;

    let (_, draft, _) = create_process(&running, "dev-versioned", &sop, None, two_steps()).await;
    let (status, _, refused) = start(
        &running,
        json!({"card_id": card, "process_id": draft}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(slug(&refused), "process.not_published");

    // By key with only a draft under it: the same refusal, not a 404.
    let (status, _, refused) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-versioned"}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(slug(&refused), "process.not_published");

    // No such key at all is a 404.
    let (status, _, missing) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-absent"}),
        None,
    )
    .await;
    assert_eq!(status, 404, "{missing}");
    assert_eq!(slug(&missing), "not_found");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM run WHERE tenant_id = $1")
            .bind(seeded_tenant(&pg).await)
            .fetch_one(&pg.super_pool)
            .await
            .expect("run count"),
        0
    );

    // Publish v1, then fork a draft v2 — by key still runs v1's steps.
    publish(&running, &draft).await;
    let (status, fork, body) = create_process(
        &running,
        "dev-versioned",
        &sop,
        None,
        json!([{"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "v2"}}]),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_ne!(fork, draft);

    let (status, _, started) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-versioned"}),
        None,
    )
    .await;
    assert_eq!(status, 201, "{started}");
    let run_id = Uuid::parse_str(&text(output(&started), "id")).expect("run id");
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM run_step WHERE tenant_id = $1 AND run_id = $2 ORDER BY position",
    )
    .bind(seeded_tenant(&pg).await)
    .bind(run_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("run steps");
    assert_eq!(
        actions,
        ["cards.claim", "cards.comment"],
        "the PUBLISHED v1 steps, not the draft v2's single step"
    );
    let version: i64 = sqlx::query_scalar(
        "SELECT p.version FROM process p JOIN run r ON r.process_id = p.id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(seeded_tenant(&pg).await)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run's process");
    assert_eq!(version, 1, "the run is bound to the PUBLISHED v1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_cannot_start() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-start-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;
    let (_, process, _) = create_process(&running, "dev-scope", &sop, None, two_steps()).await;
    publish(&running, &process).await;

    let carol = support::seed(&pg, "dev-a", "carol", "Carol").await;
    {
        let mut tx = conveyor_store::begin_scoped(&pg.owner, carol.tenant)
            .await
            .expect("scope");
        sqlx::query("UPDATE membership SET role = 'viewer' WHERE user_id = $1")
            .bind(carol.user_id)
            .execute(&mut *tx)
            .await
            .expect("demote carol");
        tx.commit().await.expect("commit");
    }

    let (status, body) = running
        .invoke(
            "process_runs.start",
            "dev:carol",
            json!({"card_id": card, "process_key": "dev-scope"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(slug(&body), "action-forbidden");
}

/// D29: a process whose `agent` step binds a `process_agent` starts even when its capability has
/// more than one active executor. The "exactly one active executor" rule is about a capability
/// run with no bound agent; it must not refuse a process whose steps say who runs them. The same
/// capability with a computed-only process keeps the ambiguity refusal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_process_agent_starts_where_the_capability_is_ambiguous() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-bound-agent-sop").await;

    // One capability, two active process agents — the catalogue's shape (one agent per process,
    // many processes under one capability).
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.bound', 'Bound', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let mut agents = Vec::new();
    for key in ["bound-a", "bound-b"] {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, process_key, status, model_ref,
                  allowed_actions, max_iterations, timeout_secs)
             VALUES ($1, $2, $3, 'process_agent', $3, 'active', 'test-model',
                     ARRAY['cards.claim'], 1, 30)
             RETURNING id",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(capability_id)
        .bind(key)
        .fetch_one(&mut *tx)
        .await
        .expect("agent");
        agents.push(id);
    }
    tx.commit().await.expect("commit");

    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;
    let (status, process, body) = create_process(
        &running,
        "dev-bound-agent",
        &sop,
        None,
        json!([{
            "kind": "agent",
            "actor": "process_agent",
            "process_agent_id": agents[0],
            "input_template": {"messages": [{"role": "user", "content": "go"}]},
        }]),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    bind_capability(&pg, seeded.tenant, capability_id, &process).await;
    publish(&running, &process).await;

    let (status, _headers, body) = start(
        &running,
        json!({"card_id": card, "process_key": "dev-bound-agent", "inputs": {}}),
        None,
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let run_id = Uuid::parse_str(&text(output(&body), "id")).expect("run id");
    let (recorded,): (Option<Uuid>,) =
        sqlx::query_as("SELECT capability_agent_id FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant.as_uuid())
            .bind(run_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run row");
    assert_eq!(
        recorded,
        Some(agents[0]),
        "the step's bound agent is the run's executor"
    );

    // The same capability on a computed-only process keeps the ambiguity refusal.
    let card2 = create_card(&running, &board, &lane, "feature").await;
    let (status, process2, body) = create_process(
        &running,
        "dev-unbound-agent",
        &sop,
        None,
        json!([{
            "action": "cards.claim",
            "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"},
        }]),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    bind_capability(&pg, seeded.tenant, capability_id, &process2).await;
    publish(&running, &process2).await;

    let (status, _headers, body) = start(
        &running,
        json!({"card_id": card2, "process_key": "dev-unbound-agent", "inputs": {}}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "capability_not_ready");
    assert_eq!(body["errors"][0]["code"].as_str(), Some("agent_ambiguous"));
}

/// `approvals.list` carries the decision history — who decided, what kind of actor, and the
/// channel — not just a count. The cockpit cannot render attribution the projection omits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approval_list_projects_who_decided_and_through_which_channel() {
    let (_pg, running, _seeded) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane, "feature").await;

    let (status, body) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({"card_id": card, "gate": "signoff", "kind": "process"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let approval_id = text(output(&body), "id");

    let (status, body) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": approval_id, "decision": "approve", "note": "looks good"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "approvals.list",
            TOKEN,
            json!({"card_id": card}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rows = output(&body)["items"].as_array().expect("items");
    let gate = rows
        .iter()
        .find(|row| row["id"] == approval_id.as_str())
        .unwrap_or_else(|| panic!("the gate is not in the list: {body}"));
    let decisions = gate["decisions"].as_array().expect("decisions");
    assert_eq!(decisions.len(), 1, "{gate}");
    assert_eq!(decisions[0]["decision"], "approve");
    assert_eq!(decisions[0]["decided_by_kind"], "human");
    assert_eq!(decisions[0]["approver"], "Alice");
    assert_eq!(decisions[0]["via"]["channel"], "web");
}

/// Bind a capability to a process. `processes.create` takes no capability; the card type or this
/// column names it (the same route the S13 catalogue seed uses).
async fn bind_capability(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    capability_id: Uuid,
    process: &str,
) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope");
    sqlx::query("UPDATE process SET capability_id = $1 WHERE id = $2")
        .bind(capability_id)
        .bind(Uuid::parse_str(process).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("bind the capability");
    tx.commit().await.expect("commit");
}

/// The fixture tenant's id, read once per assertion block.
async fn seeded_tenant(pg: &support::Pg) -> Uuid {
    sqlx::query_scalar("SELECT id FROM tenant WHERE slug = 'dev-a'")
        .fetch_one(&pg.super_pool)
        .await
        .expect("the fixture tenant")
}
