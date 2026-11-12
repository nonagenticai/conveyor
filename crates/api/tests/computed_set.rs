//! S13 §5.8 — the computed set is CODE, not four n8n flows.
//!
//! `conveyor.start` became `projects.create` (S13 step 4); the move step is `cards.move`;
//! `err.card` runs on `run.failed`; `wip.pull` runs on board events. The four named flows, the
//! `POST /api/hooks/{hook}` seam, `CONVEYOR_HOOK_SECRET` and the `workflow.start` target of the
//! failed-run trigger are retired.
//!
//! Three properties, each one a way a silent bug would hide:
//!
//! * a failed run produces **exactly one** `err.card` job; the handler moves the card to the
//!   waiting lane (the old BLOCKED), writes **one** comment naming the run and starts **one live
//!   root** coach run. Re-delivering the job duplicates neither the comment nor the coach;
//! * `POST /api/hooks/conveyor.start` is a 404 like any other unknown path — not a 401/503 from a
//!   handler that still exists, and not a path in the served OpenAPI document;
//! * a `cards.move` into the ACTIVE lane writes ONE `card.moved` event on the board's topic — the
//!   row the relay publishes. The relay's bus topic (`card.moved`, the string the worker's
//!   consumer matches) is asserted in `apps/server/src/bus_relay.rs`, because the mapping lives in
//!   the server crate and this harness never starts Redis.
//!
//! `rbuild -- test -p conveyor-api --test computed_set`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::process::StepInput;
use conveyor_store::begin_scoped;
use conveyor_store::process::NewProcess;
use reqwest::Method;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
/// `support::config` serves `APP_KEY=conveyor`, and `support::seed` entitlement-matches it.
const APP_KEY: &str = "conveyor";
const START_CARD_TYPE: &str = "project_start_request_analysis";
const START_PROCESS_KEY: &str = "project_start_request_analysis.READY";
/// The coach `err.card` starts (S13 §5.8).
const COACH_PROCESS_KEY: &str = "cap.kanban_coach.handle_error";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// The fresh database, the seeded dev tenant and the served app.
async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

/// The start card type. `store::seed::catalogue` seeds only `feature`/`bug`; `projects.create`
/// needs the start card type S13 step 4 introduced.
async fn start_card_type(pg: &support::Pg, tenant: conveyor_domain::TenantId) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, $2, 'Project start request analysis')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = EXCLUDED.label
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(START_CARD_TYPE)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the start card type")
}

/// Create and publish one process with a single legal computed step, through the store (the way
/// the S13 seed publishes the 280-process catalogue; no action publishes a default process yet).
async fn published_process(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    key: &str,
    title: &str,
    card_type_id: Option<Uuid>,
    sop_id: Uuid,
) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let process = conveyor_store::process::create_draft_tx(
        &mut tx,
        tenant,
        &NewProcess {
            key: key.to_owned(),
            title: title.to_owned(),
            description: String::new(),
            card_type_id,
            list_kind: None,
            project_type_id: None,
            outputs: json!([]),
            capability_id: None,
            owner_user_id: None,
        },
    )
    .await
    .expect("create the process draft");
    let step: StepInput = serde_json::from_value(json!({
        "action": "cards.get",
        "procedure_id": sop_id,
        "input_template": {"id": "{{card.id}}"},
    }))
    .expect("a legal computed step");
    conveyor_store::process::insert_steps_tx(&mut tx, tenant, process.id, &[step])
        .await
        .expect("insert the step");
    conveyor_store::process::publish_tx(&mut tx, tenant, process.id, None)
        .await
        .expect("publish the process");
    tx.commit().await.expect("commit the process");
    process.id
}

/// The start process `projects.create` requires and the coach `err.card` starts, each with its
/// own published SOP (a step must name one).
async fn seed_start_and_coach(
    pg: &support::Pg,
    running: &support::Running,
    tenant: conveyor_domain::TenantId,
) {
    let card_type_id = start_card_type(pg, tenant).await;
    let start_sop =
        Uuid::parse_str(&support::publish_sop(running, START_CARD_TYPE).await).expect("start SOP");
    published_process(
        pg,
        tenant,
        START_PROCESS_KEY,
        "Project start request analysis READY",
        Some(card_type_id),
        start_sop,
    )
    .await;
    let coach_sop = Uuid::parse_str(&support::publish_sop(running, COACH_PROCESS_KEY).await)
        .expect("coach SOP");
    published_process(
        pg,
        tenant,
        COACH_PROCESS_KEY,
        "Kanban coach — handle error",
        None,
        coach_sop,
    )
    .await;
}

struct Subject {
    project_id: String,
    board_id: String,
    intake: String,
    active: String,
}

/// Create the `delivery` project whose board the computed handlers act on; the project type
/// supplies the `feature` matrix (`active → waiting` is legal, which `err.card` needs).
async fn subject(running: &support::Running) -> Subject {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({
                "title": format!("Computed set {}", Uuid::now_v7().simple()),
                "project_type_code": "delivery",
            }),
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
    let lists = output(&board)["lists"].as_array().expect("lists");
    let lane = |kind: &str| {
        lists
            .iter()
            .find(|entry| entry["list"]["kind"].as_str() == Some(kind))
            .map(|entry| text(&entry["list"], "id"))
            .unwrap_or_else(|| panic!("the board has no {kind} lane: {board}"))
    };
    Subject {
        project_id,
        board_id,
        intake: lane("intake"),
        active: lane("active"),
    }
}

/// Create one card; returns `(id, version)` — the version `cards.move` addresses.
async fn create_card(
    running: &support::Running,
    board: &str,
    lane: &str,
    name: &str,
) -> (String, i64) {
    let (status, body) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({"board_id": board, "list_id": lane, "name": name, "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let card = output(&body);
    (
        text(card, "id"),
        card["version"].as_i64().expect("the card version"),
    )
}

/// The card's lane kind.
async fn lane_kind(pg: &support::Pg, card: Uuid) -> String {
    sqlx::query_scalar(
        "SELECT l.kind FROM card c
           JOIN list l ON l.tenant_id = c.tenant_id AND l.id = c.list_id
          WHERE c.id = $1",
    )
    .bind(card)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the card's lane")
}

/// Every comment body on the card, oldest first.
async fn comment_bodies(pg: &support::Pg, tenant: Uuid, card: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT body FROM comment WHERE tenant_id = $1 AND card_id = $2 ORDER BY created_at, id",
    )
    .bind(tenant)
    .bind(card)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the card's comments")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_run_blocks_the_card_comments_once_and_starts_one_coach() {
    let (pg, seeded, running) = fixture().await;
    seed_start_and_coach(&pg, &running, seeded.tenant).await;
    let s = subject(&running).await;
    let (card, _version) = create_card(&running, &s.board_id, &s.active, "will fail").await;
    let card = Uuid::parse_str(&card).expect("card");

    // A REAL failing run: create → start → finish(failed). `runs.finish` is the emitter that
    // enqueues the err.card job in the same transaction as the failure.
    let (status, created) = running
        .invoke(
            "runs.create",
            TOKEN,
            json!({"card_id": card}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let run = Uuid::parse_str(&text(output(&created), "id")).expect("run id");
    let (status, started) = running
        .invoke("runs.start", TOKEN, json!({"id": run}), None, None, None)
        .await;
    assert_eq!(status, 200, "{started}");
    let (status, finished) = running
        .invoke(
            "runs.finish",
            TOKEN,
            json!({
                "id": run,
                "outcome": "failed",
                "error_class": "probe_failure",
                "error_message": "the computed-set probe failed",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{finished}");

    // (1) EXACTLY one err.card job, keyed by the run, naming the run/card/project.
    let jobs: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT kind, dedupe_key, payload FROM job WHERE tenant_id = $1 AND kind = 'err.card'",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the err.card jobs");
    assert_eq!(jobs.len(), 1, "exactly one err.card job: {jobs:?}");
    let (kind, dedupe, payload) = &jobs[0];
    assert_eq!(kind.as_str(), "err.card");
    assert_eq!(dedupe, &format!("err.card:{run}"));
    assert_eq!(payload["run_id"], json!(run));
    assert_eq!(payload["card_id"], json!(card));
    assert_eq!(payload["project_id"], json!(s.project_id));

    // A second enqueue while the job is live is refused by the dedupe key: one job per run.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    let again = conveyor_store::run::enqueue_run_failed_trigger_tx(&mut tx, seeded.tenant, run)
        .await
        .expect("re-enqueue");
    assert!(
        again.is_none(),
        "the live job's dedupe key refuses a second"
    );
    tx.commit().await.expect("commit");

    // (2) The handler the job arm runs: move → comment → coach, all through the action layer.
    let outcome = conveyor_actions::board::pull::err_card(&pg.runtime, APP_KEY, seeded.tenant, run)
        .await
        .expect("err.card");
    assert_eq!(outcome.card_id, Some(card));
    assert!(outcome.moved, "the card is moved to waiting: {outcome:?}");
    assert!(outcome.commented, "the failure comment exists: {outcome:?}");
    let coach = outcome.coach_run_id.expect("one coach root run");
    assert!(!outcome.coach_skipped);

    // The card sits in the board's waiting lane (the old flow's BLOCKED).
    assert_eq!(lane_kind(&pg, card).await, "waiting");

    // ONE comment, one sentence naming the failed run.
    let bodies = comment_bodies(&pg, seeded.tenant_id, card).await;
    assert_eq!(bodies.len(), 1, "exactly one failure comment: {bodies:?}");
    assert!(
        bodies[0].contains(&run.to_string()),
        "the comment names the failed run: {bodies:?}"
    );
    assert!(
        bodies[0].contains("BLOCKED/flow_error"),
        "the old flow's marker is kept: {bodies:?}"
    );

    // ONE live coach ROOT run on the card, pinned to the coach process.
    let runs: Vec<(Option<Uuid>, i32, String, String, String)> = sqlx::query_as(
        "SELECT r.parent_run_id, r.depth, r.status, p.key, r.id::text
           FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.card_id = $2 AND r.parent_run_id IS NULL",
    )
    .bind(seeded.tenant_id)
    .bind(card)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the card's root runs");
    let live: Vec<_> = runs
        .iter()
        .filter(|(_, _, status, _, _)| {
            matches!(status.as_str(), "queued" | "running" | "waiting_approval")
        })
        .collect();
    assert_eq!(live.len(), 1, "one live root run: {runs:?}");
    let (parent, depth, _status, key, id) = live[0];
    assert!(parent.is_none(), "the coach is a ROOT run");
    assert_eq!(*depth, 0);
    assert_eq!(key.as_str(), COACH_PROCESS_KEY);
    assert_eq!(*id, coach.to_string(), "the live run is the coach we saw");

    // (3) Re-delivery: the same job run again changes nothing.
    let replay = conveyor_actions::board::pull::err_card(&pg.runtime, APP_KEY, seeded.tenant, run)
        .await
        .expect("err.card replay");
    assert!(!replay.moved, "the card is already in waiting");
    assert_eq!(
        replay.coach_run_id,
        Some(coach),
        "the replay finds the coach it started"
    );
    assert!(replay.coach_skipped, "a live coach is never started twice");
    assert_eq!(
        comment_bodies(&pg, seeded.tenant_id, card).await.len(),
        1,
        "a replay writes no second comment"
    );
    let coach_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.card_id = $2 AND p.key = $3",
    )
    .bind(seeded.tenant_id)
    .bind(card)
    .bind(COACH_PROCESS_KEY)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the coach run count");
    assert_eq!(coach_runs, 1, "a replay starts no second coach");
    assert_eq!(lane_kind(&pg, card).await, "waiting");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_retired_hook_route_is_a_404_problem() {
    let (_pg, seeded, running) = fixture().await;

    // The body the retired handler accepted. The ROUTE is what is gone: a request that the old
    // seam would have authenticated now falls through to the unknown-path problem.
    let (status, body) = running
        .http(
            Method::POST,
            "/api/hooks/conveyor.start",
            None,
            &[],
            Some(json!({
                "tenant_id": seeded.tenant_id.to_string(),
                "project_id": Uuid::now_v7().to_string(),
            })),
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["status"], 404);
    assert_eq!(body["type"], "not_found");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("/api/hooks/conveyor.start"),
        "the problem names the path: {body}"
    );

    // The retired shared secret authenticates nothing: same 404, never a 401/503.
    let (status, _) = running
        .http(
            Method::POST,
            "/api/hooks/wip.pull",
            None,
            &[("x-conveyor-hook-secret", "anything")],
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 404);

    // And the operation is out of the served document, not merely unrouted.
    let spec = conveyor_api::openapi();
    assert!(
        spec["paths"].get("/api/hooks/{hook}").is_none(),
        "the retired hooks operation must not be documented"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_into_active_emits_one_card_moved_on_the_board_topic() {
    let (pg, seeded, running) = fixture().await;
    seed_start_and_coach(&pg, &running, seeded.tenant).await;
    let s = subject(&running).await;
    let (card, version) = create_card(&running, &s.board_id, &s.intake, "pull me").await;

    let (status, moved) = running
        .invoke(
            "cards.move",
            TOKEN,
            json!({"id": card, "list_id": s.active, "_version": version}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");

    // The outbox row the relay publishes: `card.moved` on `board:<id>`, naming the lane entered.
    // (The relay's BUS topic — `card.moved`, the string the worker's consumer matches — is
    // asserted in `apps/server/src/bus_relay.rs`; this harness has no Redis and the mapping lives
    // in the server crate.)
    let events: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT type, topic, payload FROM event
          WHERE tenant_id = $1 AND type = 'card.moved'",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the card.moved events");
    assert_eq!(events.len(), 1, "exactly one card.moved: {events:?}");
    let (event_type, topic, payload) = &events[0];
    assert_eq!(event_type.as_str(), "card.moved", "never `cards.moved`");
    assert_eq!(
        topic,
        &format!("board:{}", s.board_id),
        "the event lands on the board's topic"
    );
    assert_eq!(payload["card_id"], json!(card));
    assert_eq!(payload["from_list_id"], json!(s.intake));
    assert_eq!(payload["to_list_id"], json!(s.active));
}
