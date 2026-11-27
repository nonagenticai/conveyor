//! T4 — the kanban coach (SPEC_V2 §6.2 · §10.4; issue #75).
//!
//! Every test drives the real handlers and the real registry against a real Postgres, then
//! asserts on rows — never on a return value alone. The ten scenarios are the issue's:
//!
//! P1.1 one interruption, one coach card, one triage, one remedy · P1.2 the closed sets refuse ·
//! P1.3 a low-confidence triage routes to `ask` and a human gate · P2.1 a repeat signature
//! escalates · P2.2 an escalated signature is not retried · P3.1 a failed coach run never opens
//! a coach card · P3.2 no further `err.card` job exists · P4.1 a systemic class opens one kata ·
//! P4.2 a transient class opens none.
//!
//! Most tests publish a stand-in process and call `coach.triage` / `coach.remedy` directly. The
//! seeded `cap.kanban_coach.handle_error.v2` is exercised end to end by
//! `the_seeded_coach_process_yields_one_card_one_triage_and_one_remedy`: it is applied from the
//! shipped catalogue, started by `err.card`, its agent step is driven through the real step
//! executor against a stub inference router, and its computed step writes the remedy.
//!
//! `coach.remedy` is applied, not just recorded: `retry` starts the failed process again,
//! `repull` queues the board's `wip.pull`, `move` / `block` move the card through `cards.move`,
//! and `ask` starts `ask_fr` before the gate. A failed effect rolls the remedy back.

mod support;

use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_domain::ListKind;

const TOKEN: &str = "dev:alice";
const APP_KEY: &str = "conveyor";

/// The coach process `err.card` starts (`COACH_ERROR_PROCESS`).
const COACH_KEY: &str = "cap.kanban_coach.handle_error.v2";

/// A process in the coach FAMILY whose one step always fails — the "coach's own run fails" case.
const BROKEN_COACH_KEY: &str = "cap.kanban_coach.handle_error.broken";

/// The kata process a systemic class opens.
const KATA_KEY: &str = "kata.open";

/// The seeded human-question route an `ask` remedy starts.
const ASK_FR_KEY: &str = "ask_fr";

/// A process whose run stays LIVE: the injected failure a retry's live-root guard sees.
const HOLD_KEY: &str = "coach-test.hold";

/// The interrupted card's process: one step that addresses a card that does not exist, so the
/// step fails with a permanent `404` and the run ends `failed` with `process_step_failed`.
const FAILING_KEY: &str = "coach-test.failing";

const MISSING_CARD: &str = "00000000-0000-0000-0000-000000000000";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    // The full catalogue export carries the `coach` card type; the light `seed::catalogue`
    // above carries only the three types a project needs. Importing the one type here also
    // proves the seed file really carries it.
    let export = conveyor_store::seed::catalogue_export().expect("the catalogue export");
    let coach: Vec<_> = export
        .card_types
        .into_iter()
        .filter(|card_type| card_type.code == "coach")
        .collect();
    assert_eq!(
        coach.len(),
        1,
        "the catalogue seed carries the coach card type"
    );
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::catalogue::import_card_types_tx(&mut tx, seeded.tenant, &coach)
        .await
        .expect("import the coach card type");
    tx.commit().await.expect("commit the card type");
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

/// A project with a board and both lanes `err.card` needs: `active` (where a card is born) and
/// `waiting` (where a failed card is blocked).
async fn board_lanes(pg: &support::Pg, seeded: &support::Seeded) -> (String, String, String) {
    let tenant = seeded.tenant;
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    // A project type with a matrix, so the card can really enter `waiting` (err.card's move) and
    // leave it again (the `move` remedy). Without a type every cross-kind move is refused.
    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
         VALUES ($1, 'project_type', 'coach_fixture', 'Coach fixture', $2, 0)
         ON CONFLICT (tenant_id, kind, code) DO UPDATE SET meta = excluded.meta",
    )
    .bind(tenant.as_uuid())
    .bind(json!({
        "matrix": {"feature": {
            "active": ["active", "waiting"],
            "waiting": ["active", "waiting"]
        }}
    }))
    .execute(&mut *tx)
    .await
    .expect("the fixture project type");
    let mut project_input = conveyor_store::project::NewProject::new(
        format!("C{}", Uuid::now_v7().simple()),
        "Coach fixture",
    );
    project_input.project_type_code = Some("coach_fixture".to_owned());
    let project = conveyor_store::project::insert_project_tx(&mut tx, tenant, &project_input)
        .await
        .expect("the project");
    let board = conveyor_store::board::insert_board_tx(
        &mut tx,
        tenant,
        &conveyor_store::board::NewBoard::new(project.id, "Board"),
    )
    .await
    .expect("the board");
    let active = conveyor_store::list::insert_list_tx(
        &mut tx,
        tenant,
        &conveyor_store::list::NewList::new(board.id, "Active", Decimal::ZERO, ListKind::Active),
    )
    .await
    .expect("the active lane");
    let waiting = conveyor_store::list::insert_list_tx(
        &mut tx,
        tenant,
        &conveyor_store::list::NewList::new(
            board.id,
            "Waiting",
            Decimal::from(1),
            ListKind::Waiting,
        ),
    )
    .await
    .expect("the waiting lane");
    tx.commit().await.expect("commit the fixture board");
    (
        board.id.to_string(),
        active.id.to_string(),
        waiting.id.to_string(),
    )
}

async fn create_card(
    running: &support::Running,
    board: &str,
    lane: &str,
    card_type_code: &str,
    name: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": name,
                "card_type_code": card_type_code,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// A card that names its job binding — the binding a measurable kata opens under (§4.4).
async fn create_bound_card(
    running: &support::Running,
    board: &str,
    lane: &str,
    card_type_code: &str,
    name: &str,
    binding: Uuid,
) -> String {
    let (status, body) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": name,
                "card_type_code": card_type_code,
                "job_binding_id": binding,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// v1-p6-6: a coach card exists only as the record of an interruption. The public create path
/// (a person or an agent) is refused, so an orphan coach card nothing can triage is impossible;
/// the coach's own path creates its card inside `coach.interrupt`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_coach_card_cannot_be_created_through_the_public_path() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;

    let (status, body) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": active,
                "name": "orphan coach",
                "card_type_code": "coach",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "a public coach card is refused: {body}");
    assert_eq!(body["type"], json!("coach_card_internal"), "{body}");

    // The coach's own path still creates one: the interruption test's fixture proves it, and
    // this row asserts no orphan can be counted on the board.
    let cards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND ct.code = 'coach'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the coach card count");
    assert_eq!(cards, 0, "no orphan coach card exists");
}

async fn publish_sop(running: &support::Running, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("SOP {key}"),
                "acceptance_criteria": ["the step's output is complete"],
                "inspection": {"kind": "checker", "rule": "always"},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

/// Publish a one-step process. `template` is the step's own input document.
async fn publish_process(
    running: &support::Running,
    key: &str,
    action: &str,
    sop: &str,
    template: Value,
) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("Process {key}"),
                "steps": [{"action": action, "procedure_id": sop, "input_template": template}],
            }),
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

async fn start(running: &support::Running, card: &str, key: &str, inputs: Value) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": key, "inputs": inputs}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// The three processes every test needs: the coach stand-in, its always-failing family twin,
/// a kata stand-in, and the failing process for the interrupted card.
async fn publish_processes(running: &support::Running) {
    let coach_sop = publish_sop(running, "coach.test.coach").await;
    publish_process(
        running,
        COACH_KEY,
        "cards.get",
        &coach_sop,
        json!({"id": "{{card.id}}"}),
    )
    .await;
    let broken_sop = publish_sop(running, "coach.test.broken").await;
    publish_process(
        running,
        BROKEN_COACH_KEY,
        "cards.get",
        &broken_sop,
        json!({"id": MISSING_CARD}),
    )
    .await;
    let kata_sop = publish_sop(running, "coach.test.kata").await;
    publish_process(
        running,
        KATA_KEY,
        "cards.get",
        &kata_sop,
        json!({"id": "{{card.id}}"}),
    )
    .await;
    let ask_sop = publish_sop(running, "coach.test.ask_fr").await;
    publish_process(
        running,
        ASK_FR_KEY,
        "cards.get",
        &ask_sop,
        json!({"id": "{{card.id}}"}),
    )
    .await;
    let hold_sop = publish_sop(running, "coach.test.hold").await;
    publish_process(
        running,
        HOLD_KEY,
        "cards.get",
        &hold_sop,
        json!({"id": "{{card.id}}"}),
    )
    .await;
    let failing_sop = publish_sop(running, "coach.test.failing").await;
    publish_process(
        running,
        FAILING_KEY,
        "cards.get",
        &failing_sop,
        json!({"id": MISSING_CARD}),
    )
    .await;
}

/// Drive one step attempt, then assert the run reached the expected status: a permanent step
/// failure is recorded on the run and ends it `failed` (the handler returns `Ok` once the
/// failure is recorded — the worker's job is done either way).
async fn execute_step(pg: &support::Pg, seeded: &support::Seeded, run: &str, expect_failure: bool) {
    let result = conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        APP_KEY,
        Uuid::parse_str(run).expect("run id"),
        1,
        1,
    )
    .await;
    let status: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(run).expect("run id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run status");
    if expect_failure {
        assert_eq!(status, "failed", "the run ends failed: {result:?}");
    } else {
        assert!(result.is_ok(), "the step was expected to pass: {result:?}");
        assert_ne!(status, "failed", "the run did not fail");
    }
}

/// Start `key` on `card`, fail its first step, and return the failed run id.
async fn fail_run(
    pg: &support::Pg,
    seeded: &support::Seeded,
    running: &support::Running,
    card: &str,
    key: &str,
) -> String {
    let run = start(running, card, key, json!({})).await;
    execute_step(pg, seeded, &run, true).await;
    run
}

/// `err.card`, exactly as the worker's arm runs it.
async fn err_card(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
) -> conveyor_actions::board::pull::ErrCardOutcome {
    conveyor_actions::board::pull::err_card(
        &pg.runtime,
        APP_KEY,
        seeded.tenant,
        Uuid::parse_str(run).expect("run id"),
    )
    .await
    .expect("err.card")
}

async fn count_cards_of_type(
    pg: &support::Pg,
    seeded: &support::Seeded,
    board: &str,
    code: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.board_id = $2 AND ct.code = $3",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(board).expect("board"))
    .bind(code)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the card count")
}

async fn coach_card_id(pg: &support::Pg, seeded: &support::Seeded, board: &str) -> Uuid {
    sqlx::query_scalar(
        "SELECT c.id FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.board_id = $2 AND ct.code = 'coach'
          ORDER BY c.created_at DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(board).expect("board"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the coach card")
}

async fn interruption_row(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
) -> (Uuid, String, String, Option<String>, Option<Uuid>) {
    sqlx::query_as(
        "SELECT id, state, signature, triage_class, approval_id FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(card).expect("card"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the interruption")
}

async fn remedy_count(pg: &support::Pg, seeded: &support::Seeded, interruption: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM coach_remedy WHERE tenant_id = $1 AND interruption_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(interruption)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the remedy count")
}

async fn err_card_jobs_for(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'err.card' AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("err.card:{run}"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the err.card job count")
}

/// The one remedy's `detail`, including the effect it applied.
async fn remedy_detail(pg: &support::Pg, seeded: &support::Seeded, interruption: Uuid) -> Value {
    sqlx::query_scalar(
        "SELECT detail FROM coach_remedy WHERE tenant_id = $1 AND interruption_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(interruption)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the remedy detail")
}

/// How many ROOT runs of one process KEY the card has (the original + a retry).
async fn root_runs_for(pg: &support::Pg, seeded: &support::Seeded, card: &str, key: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.card_id = $2 AND p.key = $3
            AND r.parent_run_id IS NULL",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(card).expect("card"))
    .bind(key)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the root run count")
}

async fn card_lane(pg: &support::Pg, seeded: &support::Seeded, card: &str) -> Uuid {
    sqlx::query_scalar("SELECT list_id FROM card WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(card).expect("card"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the card's lane")
}

async fn escalate(running: &support::Running, interruption: Uuid, reason: &str) -> (u16, Value) {
    running
        .invoke(
            "coach.escalate",
            TOKEN,
            json!({"interruption_id": interruption, "reason": reason}),
            None,
            None,
            None,
        )
        .await
}

/// Drive one step attempt at `position` and require it not to fail the run.
async fn execute_at(pg: &support::Pg, seeded: &support::Seeded, run: &str, position: i32) {
    let result = conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        APP_KEY,
        Uuid::parse_str(run).expect("run id"),
        position,
        1,
    )
    .await;
    let (status, error_class, error): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, error_class, error_message FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run status");
    assert!(
        result.is_ok(),
        "step {position} was expected to pass: {result:?}"
    );
    assert_ne!(
        status, "failed",
        "step {position} failed the run: {error_class:?} {error:?}"
    );
}

async fn triage(
    running: &support::Running,
    interruption: Uuid,
    class: &str,
    confidence: f64,
) -> (u16, Value) {
    running
        .invoke(
            "coach.triage",
            TOKEN,
            json!({
                "interruption_id": interruption,
                "triage_class": class,
                "confidence": confidence,
                "narrative": "the failing step addresses a card that does not exist",
            }),
            None,
            None,
            None,
        )
        .await
}

async fn remedy(
    running: &support::Running,
    interruption: Uuid,
    class: Option<&str>,
) -> (u16, Value) {
    let mut input = json!({"interruption_id": interruption});
    if let Some(class) = class {
        input["remedy_class"] = json!(class);
    }
    running
        .invoke("coach.remedy", TOKEN, input, None, None, None)
        .await
}

// ── P1.1 — one interruption, one coach card, one triage, one remedy ────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interruption_yields_one_coach_card_one_triage_and_one_remedy() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "the interrupted card").await;

    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let outcome = err_card(&pg, &seeded, &run).await;
    assert!(!outcome.coach_skipped, "the coach is admitted: {outcome:?}");
    assert_eq!(
        count_cards_of_type(&pg, &seeded, &board, "coach").await,
        1,
        "exactly one coach card"
    );

    let (interruption, state, _signature, _class, _approval) =
        interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "open");

    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "retry");

    let (_, state, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "remedied");
    assert_eq!(class.as_deref(), Some("transient"));
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 1);
}

/// P1.1's other half: an interruption carries exactly one remedy, and a second `coach.remedy`
/// naming the same interruption is refused by name (`409 coach.already_remedied`) without a
/// second row. A sequential second call normally meets `coach.not_open` because the first
/// remedy closed the interruption, so the test re-opens it to reach the existing-remedy guard —
/// the state a concurrent writer leaves behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_duplicate_remedy_is_refused() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "duplicate remedy").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;

    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "retry");
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 1);

    // The first remedy marked the interruption `remedied`; re-open it so the second call meets
    // the existing-remedy guard instead of the earlier `coach.not_open` state check.
    sqlx::query("UPDATE coach_interruption SET state = 'open' WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(interruption)
        .execute(&pg.super_pool)
        .await
        .expect("re-open the interruption");

    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 409, "a duplicate remedy is refused: {body}");
    assert_eq!(
        body["type"], "coach.already_remedied",
        "the refusal is typed: {body}"
    );
    assert_eq!(
        remedy_count(&pg, &seeded, interruption).await,
        1,
        "the refused duplicate wrote no second remedy"
    );
}

// ── P1.2 — the closed sets refuse ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_out_of_set_triage_or_remedy_is_refused() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "closed sets").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;

    let (status, body) = triage(&running, interruption, "banana", 0.9).await;
    assert_eq!(status, 422, "an out-of-set triage is refused: {body}");
    let (_, _, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(class, None, "the refused triage wrote nothing");

    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, Some("explode")).await;
    assert_eq!(status, 422, "an out-of-set remedy is refused: {body}");
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 0);
}

// ── P1.3 — a low-confidence triage routes to ask ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_low_confidence_triage_routes_to_ask() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "low confidence").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.2).await;
    assert_eq!(status, 200, "{body}");

    // An explicit action below the floor is refused: a low-confidence triage never retries.
    let (status, body) = remedy(&running, interruption, Some("retry")).await;
    assert_eq!(status, 409, "a low-confidence retry is refused: {body}");
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 0);

    // The derived remedy is `ask`, and it opens the human gate.
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "ask");
    let (_, _, _, _, approval) = interruption_row(&pg, &seeded, &card).await;
    assert!(approval.is_some(), "the ask opens a human gate");
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND gate = 'coach.ask'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the ask gate");
    assert_eq!(gates, 1, "one human gate for the ask");

    // The ask is APPLIED: ask_fr is started on the interrupted card and the effect names it.
    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("ask"), "{detail}");
    assert!(
        detail["effect"]["run_id"].is_string(),
        "the ask_fr run is recorded: {detail}"
    );
    assert_eq!(
        root_runs_for(&pg, &seeded, &card, ASK_FR_KEY).await,
        1,
        "the ask hands the card to ask_fr"
    );
}

// ── P2.1 — a repeat signature escalates ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_repeat_signature_escalates_instead_of_a_second_card() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "repeat").await;

    let first = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &first).await;
    let (interruption, _, signature, _, _) = interruption_row(&pg, &seeded, &card).await;

    // The same process, the same step, the same error class: the signature repeats.
    let second = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let outcome = err_card(&pg, &seeded, &second).await;
    assert!(
        outcome.coach_skipped,
        "a repeat never starts a second coach: {outcome:?}"
    );
    assert_eq!(
        count_cards_of_type(&pg, &seeded, &board, "coach").await,
        1,
        "still exactly one coach card"
    );
    let (_, state, signature_after, _, approval) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(signature_after, signature, "the same signature");
    assert_eq!(state, "escalated");
    assert!(approval.is_some(), "the escalation names its approval");
    let id = interruption;
    assert_eq!(remedy_count(&pg, &seeded, id).await, 0);
}

// ── P2.2 — an escalated signature is not retried ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_escalated_signature_is_not_retried() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "escalated").await;

    let first = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &first).await;
    let second = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &second).await;
    let (interruption, state, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "escalated");

    // The escalated interruption carries no remedy and refuses one.
    let (status, body) = remedy(&running, interruption, Some("retry")).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 0);

    // A triage on it cannot be added either, and the card stays blocked where it is.
    let (_, state, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(
        state, "escalated",
        "an escalated signature stays blocked for its human"
    );
}

// ── P3.1 — a failed coach run never opens a coach card ────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_coach_run_never_opens_a_coach_card() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "coach fails").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let outcome = err_card(&pg, &seeded, &run).await;
    let coach = coach_card_id(&pg, &seeded, &board).await;
    // The coach run occupies the coach card's one live root slot; finish it first.
    let coach_run = outcome.coach_run_id.expect("the coach run");
    execute_step(&pg, &seeded, &coach_run.to_string(), false).await;

    // The coach card's own run fails (a process in the coach family, on the coach card).
    let failed = fail_run(&pg, &seeded, &running, &coach.to_string(), BROKEN_COACH_KEY).await;

    // The handler refuses (defence for an already-queued job) and records the failure on the
    // coach card itself.
    let outcome = err_card(&pg, &seeded, &failed).await;
    assert!(outcome.coach_skipped, "no coach about a coach: {outcome:?}");
    assert_eq!(outcome.coach_run_id, None, "no run was started");
    assert!(
        outcome.commented,
        "the failure is recorded on the coach card"
    );
    assert_eq!(
        count_cards_of_type(&pg, &seeded, &board, "coach").await,
        1,
        "still exactly one coach card"
    );
    let bodies: Vec<String> =
        sqlx::query_scalar("SELECT body FROM comment WHERE tenant_id = $1 AND card_id = $2")
            .bind(seeded.tenant_id)
            .bind(coach)
            .fetch_all(&pg.super_pool)
            .await
            .expect("the coach card's comments");
    assert_eq!(bodies.len(), 1, "one failure comment on the coach card");
    assert!(bodies[0].contains("BLOCKED/flow_error"), "{bodies:?}");
}

// ── P3.2 — no further err.card job exists ────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_coach_chain_settles_at_one_card() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "chain").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    assert_eq!(
        err_card_jobs_for(&pg, &seeded, &run).await,
        1,
        "the failure queues one job"
    );
    let outcome = err_card(&pg, &seeded, &run).await;
    let coach = coach_card_id(&pg, &seeded, &board).await;
    let coach_run = outcome.coach_run_id.expect("the coach run");
    execute_step(&pg, &seeded, &coach_run.to_string(), false).await;

    let failed = fail_run(&pg, &seeded, &running, &coach.to_string(), BROKEN_COACH_KEY).await;
    assert_eq!(
        err_card_jobs_for(&pg, &seeded, &failed).await,
        0,
        "a coach run's failure queues no err.card job at all"
    );
    assert_eq!(count_cards_of_type(&pg, &seeded, &board, "coach").await, 1);
}

// ── P4.1 — a systemic class opens one kata ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_systemic_class_without_a_pinned_instrument_escalates() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "systemic").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "conveyor_defect", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "block");
    // Without a pin the coach cannot name an instrument: §6.3 forbids opening an unmeasurable
    // kata, so the candidate escalates to a person and no kata row is created (v1-p6-2).
    assert!(
        output(&body)["kata_id"].is_null(),
        "no kata without a pin: {body}"
    );
    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert!(
        detail["effect"]["kata_escalation"].is_string(),
        "the candidate escalates: {detail}"
    );
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND gate = 'coach.kata'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the kata gates");
    assert_eq!(gates, 1, "one human gate names the missing instrument");
    let katas: i64 = sqlx::query_scalar("SELECT count(*) FROM kata WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the kata rows");
    assert_eq!(katas, 0, "an unmeasurable kata is not opened");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_systemic_class_opens_one_measured_kata() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    // The card names its binding and the policy names a real locked instrument, the held-out
    // split and the pre-registered prediction — the three things §6.3 says a kata measures.
    let spine = support::kata::spine(&pg, &seeded).await;
    let instrument = support::kata::instrument(&pg, &seeded, &[0.5], &[0.0], &[0.5], &[0.0]).await;
    let pin = json!({
        "systemic_classes": ["conveyor_defect"],
        "kata": {
            "instrument_kind": "eval",
            "instrument_id": instrument.version_id,
            "instrument_version": "eval@1",
            "split": "test",
            "split_sha256": "a".repeat(64),
            "prediction": {
                "statement": "the coach removes the systemic class",
                "threshold": 0.05,
            },
            "base_ref": {"kind": "fixture", "version": 1},
        },
    });
    sqlx::query(
        "INSERT INTO decision_policy (tenant_id, kind, threshold, fail_policy, action, meta)
         VALUES ($1, 'coach.window', 0.5, 'open', 'block', $2)",
    )
    .bind(seeded.tenant_id)
    .bind(&pin)
    .execute(&pg.super_pool)
    .await
    .expect("the coach policy");
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_bound_card(
        &running,
        &board,
        &active,
        "feature",
        "systemic",
        spine.binding_id,
    )
    .await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, signature, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "conveyor_defect", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "block");
    let kata_id = output(&body)["kata_id"]
        .as_str()
        .expect("the systemic class opened a kata")
        .to_owned();

    let (signature_row, instrument_version): (String, String) = sqlx::query_as(
        "SELECT signature, instrument_version FROM kata WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&kata_id).expect("kata id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the kata row");
    assert_eq!(
        signature_row, signature,
        "the kata names the repeat signature"
    );
    assert_eq!(
        instrument_version, "eval@1",
        "the pinned instrument is recorded"
    );

    // No documentation run: the systemic path opens the measured kata itself.
    let kata_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.card_id = $2 AND p.key = $3",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&card).expect("card"))
    .bind(KATA_KEY)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the kata runs");
    assert_eq!(kata_runs, 0, "the old process-run path is gone");

    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("block"), "{detail}");
    assert_eq!(
        detail["effect"]["kata_id"].as_str(),
        Some(kata_id.as_str()),
        "the effect records the kata: {detail}"
    );
}

// ── P4.2 — a transient class opens none ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transient_class_opens_no_kata() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "transient").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert!(output(&body)["kata_id"].is_null(), "no kata: {body}");
    let kata_run_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM kata WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the kata rows");
    assert_eq!(kata_run_rows, 0, "a transient class opens no kata");
    let kata_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1 AND r.card_id = $2 AND p.key = $3",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&card).expect("card"))
    .bind(KATA_KEY)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the kata runs");
    assert_eq!(kata_runs, 0);
}

// ── P6 — the remedy is applied, not only recorded ─────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_injected_failure_plus_coach_remedy_changes_the_run_state() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "retry applied").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");

    // The retry is a real second run of the failed process on the interrupted card — not only a
    // remedy row. Its id is the effect the remedy recorded.
    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("retry"), "{detail}");
    let effect_run = Uuid::parse_str(
        detail["effect"]["run_id"]
            .as_str()
            .unwrap_or_else(|| panic!("no effect run id in {detail}")),
    )
    .expect("the effect run id");
    assert_eq!(
        root_runs_for(&pg, &seeded, &card, FAILING_KEY).await,
        2,
        "the failure and its rerun"
    );
    let (run_card, run_status): (Option<Uuid>, String) =
        sqlx::query_as("SELECT card_id, status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(effect_run)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the retry run");
    assert_eq!(run_card, Some(Uuid::parse_str(&card).expect("card")));
    assert_eq!(run_status, "running", "the rerun is started, not queued");

    let (_, state, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "remedied");
    assert_eq!(class.as_deref(), Some("transient"));
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_remedy_returns_the_card_to_the_active_lane() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "move applied").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    assert_eq!(
        card_lane(&pg, &seeded, &card).await,
        Uuid::parse_str(&waiting).expect("waiting lane"),
        "err.card blocked the card first"
    );
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = remedy(&running, interruption, Some("move")).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        card_lane(&pg, &seeded, &card).await,
        Uuid::parse_str(&active).expect("active lane"),
        "the move remedy moved the card through cards.move"
    );
    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("move"), "{detail}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_repull_remedy_queues_the_boards_pull() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "repull applied").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "job_defect", 0.9).await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "remedy_class"), "repull");

    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("repull"), "{detail}");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'wip.pull' AND status = 'pending'
            AND payload->>'board_id' = $2",
    )
    .bind(seeded.tenant_id)
    .bind(&board)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the re-pull job");
    assert_eq!(jobs, 1, "the repull remedy queues the board's pull");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remedy_whose_effect_fails_is_not_marked_remedied() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "effect fails").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");

    // Inject the failure: a live root run on the card makes the retry's `uq_run_live` guard
    // refuse, which is exactly the production "something is already executing" answer.
    start(&running, &card, HOLD_KEY, json!({})).await;

    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 409, "the failed effect refuses the remedy: {body}");
    assert_eq!(
        remedy_count(&pg, &seeded, interruption).await,
        0,
        "no remedy is written for an effect that did not happen"
    );
    let (_, state, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "open", "the interruption is still open");
    assert_eq!(class.as_deref(), Some("transient"));
}

// ── T4 — the seeded process, end to end ───────────────────────────────────────────────────

/// One local stub server (the inference router and laya are process-global env keys).
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
    let base = format!("http://{}", listener.local_addr().expect("stub addr"));
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Stub { base, task }
}

/// The router's first call (no tool message yet) asks for `coach.triage` with the interruption
/// id the seeded step puts in its user message; the second answers with prose.
async fn coach_model_reply(axum::Json(body): axum::Json<Value>) -> axum::Json<Value> {
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let saw_tool = messages
        .iter()
        .any(|message| message["role"] == json!("tool"));
    let text = messages
        .iter()
        .filter_map(|message| message["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let interruption = text.split("INTERRUPTION=").nth(1).and_then(|rest| {
        let end = rest
            .find(|character: char| !(character.is_ascii_hexdigit() || character == '-'))
            .unwrap_or(rest.len());
        Uuid::parse_str(&rest[..end]).ok()
    });
    let message = match (saw_tool, interruption) {
        (false, Some(id)) => json!({
            "role": "assistant",
            "content": Value::Null,
            "tool_calls": [{
                "id": "call-triage",
                "type": "function",
                "function": {
                    "name": "coach.triage",
                    "arguments": json!({
                        "interruption_id": id,
                        "triage_class": "transient",
                        "confidence": 0.9,
                        "narrative": "the failing step addressed a card that does not exist",
                    }).to_string(),
                }
            }]
        }),
        _ => json!({"role": "assistant", "content": "triaged"}),
    };
    axum::Json(json!({
        "id": "chatcmpl-coach",
        "choices": [{"index": 0, "message": message, "finish_reason": "stop"}],
        "usage": {"total_tokens": 12}
    }))
}

async fn inference_stub() -> Stub {
    serve(axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(coach_model_reply),
    ))
    .await
}

/// The laya judge: an accepted verdict above the SOP's 0.7 threshold.
async fn laya_stub() -> Stub {
    serve(axum::Router::new().fallback(axum::routing::post(|| async {
        axum::Json(json!({
            "answers": {"accept": {"score": 0.9, "reason": "meets the criteria"}}
        }))
    })))
    .await
}

async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope the laya token");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_seeded_coach_process_yields_one_card_one_triage_and_one_remedy() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the catalogue vocabulary");
    conveyor_store::process_seed::apply_all(&pg.runtime, seeded.tenant)
        .await
        .expect("apply the shipped catalogue");
    let running = support::Running::start(&pg).await;
    let inference = inference_stub().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &inference.base);
    let laya = laya_stub().await;
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya.base);
    seal_laya_token(&pg, &seeded).await;

    // Only the interrupted card's process is a stand-in: the coach, kata and ask processes are
    // the shipped catalogue's own rows.
    let failing_sop = publish_sop(&running, "coach.test.failing").await;
    publish_process(
        &running,
        FAILING_KEY,
        "cards.get",
        &failing_sop,
        json!({"id": MISSING_CARD}),
    )
    .await;

    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "seeded coach").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;

    let outcome = err_card(&pg, &seeded, &failed).await;
    assert!(!outcome.coach_skipped, "the coach is admitted: {outcome:?}");
    assert_eq!(
        count_cards_of_type(&pg, &seeded, &board, "coach").await,
        1,
        "exactly one coach card"
    );
    let coach_run = outcome.coach_run_id.expect("the seeded coach run");
    let (interruption, _, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(class, None, "the seeded process has not triaged yet");

    // Step 1 is the seeded agent: the real executor, the real prompt, the real tool loop.
    execute_at(&pg, &seeded, &coach_run.to_string(), 1).await;
    let (_, _, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(
        class.as_deref(),
        Some("transient"),
        "the seeded agent step recorded the one triage"
    );

    // Step 2 is the computed `coach.remedy`.
    execute_at(&pg, &seeded, &coach_run.to_string(), 2).await;
    let (_, state, _, class, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "remedied");
    assert_eq!(class.as_deref(), Some("transient"));
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 1);
    let detail = remedy_detail(&pg, &seeded, interruption).await;
    assert_eq!(detail["effect"]["kind"], json!("retry"), "{detail}");
    assert_eq!(
        root_runs_for(&pg, &seeded, &card, FAILING_KEY).await,
        2,
        "the computed step applied the retry"
    );
    let run_status: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(coach_run)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the seeded coach run");
    assert_eq!(run_status, "succeeded", "both seeded steps ran");
}

// ── T4 — the agent principal's refusal, and the explicit escalation ───────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_principal_cannot_remedy_but_a_person_can() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "agent principal").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");

    // The licence-bearer session's own mapping: actor_kind `agent`, an MCP channel authority.
    // `coach.remedy` is not in that channel's compiled allowlist, so the existing boundary
    // refuses it — no second authorization path.
    let request = conveyor_actions::InvokeRequest {
        credential_alias: None,
        action: "coach.remedy".to_owned(),
        input: json!({"interruption_id": interruption}),
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: Uuid::nil(),
        actor_kind: "agent".to_owned(),
        role: conveyor_auth::Role::KataExecutor,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Channel(conveyor_auth::ChannelAuthority::new(
            "mcp",
            None,
            Default::default(),
        )),
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: APP_KEY.to_owned(),
        usage: None,
    };
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        request,
    )
    .await
    .expect_err("an agent channel is not allowed to remedy");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "action-forbidden");
    assert_eq!(
        remedy_count(&pg, &seeded, interruption).await,
        0,
        "the refusal wrote nothing"
    );

    // A person (the HTTP dev principal) succeeds through the same action.
    let (status, body) = remedy(&running, interruption, None).await;
    assert_eq!(status, 201, "a person may remedy: {body}");
    assert_eq!(remedy_count(&pg, &seeded, interruption).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_escalates_an_open_interruption_and_a_closed_one_refuses() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;

    let card = create_card(&running, &board, &active, "feature", "escalate open").await;
    let failed = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    err_card(&pg, &seeded, &failed).await;
    let (interruption, state, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "open");

    let (status, body) = escalate(&running, interruption, "the flow is replayable").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "state"), "escalated");
    let approval_id = output(&body)["approval_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no approval id in {body}"))
        .to_owned();

    let (_, state, _, _, approval) = interruption_row(&pg, &seeded, &card).await;
    assert_eq!(state, "escalated");
    assert_eq!(approval.map(|id| id.to_string()), Some(approval_id.clone()));
    let gate: String =
        sqlx::query_scalar("SELECT gate FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(Uuid::parse_str(&approval_id).expect("approval id"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the escalation gate");
    assert_eq!(gate, "coach.escalation");
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = 'coach.escalated' AND aggregate_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(interruption)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the escalation event");
    assert_eq!(events, 1, "one coach.escalated event");

    // A closed interruption refuses: it is history, and no gate is raised for it.
    let closed_card = create_card(&running, &board, &active, "feature", "escalate closed").await;
    let closed_failed = fail_run(&pg, &seeded, &running, &closed_card, FAILING_KEY).await;
    err_card(&pg, &seeded, &closed_failed).await;
    let (closed, _, _, _, _) = interruption_row(&pg, &seeded, &closed_card).await;
    sqlx::query(
        "UPDATE coach_interruption SET state = 'closed', closed_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(closed)
    .execute(&pg.super_pool)
    .await
    .expect("close the interruption");

    let (status, body) = escalate(&running, closed, "too late").await;
    assert_eq!(status, 409, "a closed interruption refuses: {body}");
    assert_eq!(body["type"], json!("coach.not_open"));
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval
          WHERE tenant_id = $1 AND gate = 'coach.escalation' AND card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&closed_card).expect("card"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the closed card's gates");
    assert_eq!(gates, 0, "the refusal raised no gate");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_nested_coach_run_is_refused() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "nesting").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let outcome = err_card(&pg, &seeded, &run).await;
    let coach = coach_card_id(&pg, &seeded, &board).await;
    let coach_run = outcome.coach_run_id.expect("the coach run");
    execute_step(&pg, &seeded, &coach_run.to_string(), false).await;

    // A coach card's own failure is the nested path: `err.card` skips it, and the action itself
    // must refuse it too (a queued job cannot slip past the queue-time guard).
    let nested = fail_run(&pg, &seeded, &running, &coach.to_string(), BROKEN_COACH_KEY).await;
    let skipped = err_card(&pg, &seeded, &nested).await;
    assert!(skipped.coach_skipped, "no coach about a coach: {skipped:?}");

    let (status, body) = running
        .invoke(
            "coach.interrupt",
            TOKEN,
            json!({"run_id": nested}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "a nested coach run is refused: {body}");
    assert_eq!(body["type"], json!("coach.about_a_coach"));
    assert_eq!(
        count_cards_of_type(&pg, &seeded, &board, "coach").await,
        1,
        "still exactly one coach card"
    );
}

/// v1-p6-5: a §6.2 flow input opens an interruption with no run. The producer carries its own
/// repeat key, and the row records which input opened it — so a WIP breach no longer waits for a
/// downstream crash to be noticed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flow_input_opens_an_interruption_without_a_run() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "over wip").await;

    let (status, body) = running
        .invoke(
            "coach.interrupt",
            TOKEN,
            json!({
                "card_id": card,
                "trigger_kind": "wip_breach",
                "signature": "wip_breach|active|limit:1",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let interruption =
        Uuid::parse_str(&text(output(&body), "interruption_id")).expect("an interruption id");

    let (failed_run_id, trigger_kind, signature): (Option<Uuid>, String, String) = sqlx::query_as(
        "SELECT failed_run_id, trigger_kind, signature FROM coach_interruption
              WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(interruption)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the interruption row");
    assert_eq!(failed_run_id, None, "a flow input carries no failed run");
    assert_eq!(trigger_kind, "wip_breach");
    assert_eq!(signature, "wip_breach|active|limit:1");

    // The coach card and its run exist: a flow interruption is triaged like any other.
    assert_eq!(count_cards_of_type(&pg, &seeded, &board, "coach").await, 1);
    assert!(
        !text(output(&body), "coach_run_id").is_empty(),
        "the coach run starts for a flow input too: {body}"
    );
}

/// v1-p54-1: the coach reads the card's Definition-of-Done override reason and surfaces it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_coach_surfaces_the_definition_of_done_override_reason() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "dod override").await;

    // The card's card_done gate carries an explicit waiver: the approval the move reads.
    let (status, gate) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "card_id": card,
                "gate": "card_done",
                "kind": "card_dod",
                "require_evidence": true,
                "evidence_waiver_reason": "the acceptance was verified by inspection",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{gate}");
    let gate_id = text(output(&gate), "id");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({
                "approval_id": gate_id,
                "decision": "approve",
                "note": "verified by inspection",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");

    let (status, body) = running
        .invoke(
            "coach.interrupt",
            TOKEN,
            json!({
                "card_id": card,
                "trigger_kind": "wip_breach",
                "signature": "wip_breach|active|limit:1",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let interruption = text(output(&body), "interruption_id");
    let (status, triaged) = running
        .invoke(
            "coach.triage",
            TOKEN,
            json!({
                "interruption_id": interruption,
                "triage_class": "project_decision",
                "confidence": 0.9,
                "narrative": "the card's DoD was waived explicitly",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{triaged}");
    assert_eq!(
        output(&triaged)["override_reason"],
        json!("the acceptance was verified by inspection"),
        "{triaged}"
    );
}

/// An interruption names exactly one cause, and a flow cause carries its own repeat key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flow_input_must_name_its_cause_and_signature() {
    let (pg, running, seeded) = fixture().await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "no cause").await;

    let (status, body) = running
        .invoke(
            "coach.interrupt",
            TOKEN,
            json!({ "card_id": card }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["type"], json!("coach.no_cause"));

    let (status, body) = running
        .invoke(
            "coach.interrupt",
            TOKEN,
            json!({ "card_id": card, "trigger_kind": "wip_breach" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["type"], json!("coach.no_signature"));
}

/// v1-p6-5: a card moved into a blocked lane is a §6.2 board event, and the move opens exactly
/// one interruption — inside the move's own transaction.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_into_a_blocked_lane_opens_one_board_event_interruption() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "block me").await;

    let (status, body) = running
        .invoke(
            "cards.move",
            TOKEN,
            json!({ "id": card, "list_id": waiting, "_version": 1 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let rows: Vec<(String, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT trigger_kind, failed_run_id, signature FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&card).expect("card"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the interruptions");
    assert_eq!(rows.len(), 1, "one move, one interruption: {rows:?}");
    assert_eq!(rows[0].0, "board_event");
    assert_eq!(rows[0].1, None, "a flow input carries no failed run");
    assert_eq!(rows[0].2, format!("board_event|card.moved|to:{waiting}"));
    assert_eq!(count_cards_of_type(&pg, &seeded, &board, "coach").await, 1);
}

/// v1-p6-5: a move that takes its lane past `wip_limit` is a breach, and a breach pre-empts the
/// blocked-lane event, so one move still yields one interruption.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_past_the_wip_limit_opens_one_breach_interruption() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, waiting) = board_lanes(&pg, &seeded).await;
    let waiting_id = Uuid::parse_str(&waiting).expect("waiting");
    sqlx::query("UPDATE list SET wip_limit = 1 WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(waiting_id)
        .execute(&pg.super_pool)
        .await
        .expect("set the wip limit");

    let first = create_card(&running, &board, &active, "feature", "first").await;
    let second = create_card(&running, &board, &active, "feature", "second").await;
    for card in [&first, &second] {
        let (status, body) = running
            .invoke(
                "cards.move",
                TOKEN,
                json!({ "id": card, "list_id": waiting, "_version": 1 }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{body}");
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT trigger_kind, signature FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&second).expect("second"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the interruptions");
    assert_eq!(
        rows,
        vec![(
            "wip_breach".to_owned(),
            format!("wip_breach|list:{waiting}|limit:1")
        )],
        "a breach pre-empts the board event: one move, one interruption"
    );
}

/// v1-p6-5: `internal_coach` is the engine's marker for a move made while an existing
/// interruption is handled — never a caller's switch. A role-token move that names it is
/// refused at the invocation boundary and writes nothing; the same move without it is the
/// ordinary §6.2 board event. The served schema does not advertise the member.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_public_move_cannot_hide_itself_from_the_coach() {
    let schema = serde_json::to_string(
        &conveyor_actions::registry::registry()
            .get("cards.move")
            .expect("cards.move is registered")
            .input_schema(),
    )
    .expect("serialise the input schema");
    assert!(
        !schema.contains("internal_coach"),
        "the engine's marker is not part of the public input contract: {schema}"
    );

    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "not hidden").await;
    let card_id = Uuid::parse_str(&card).expect("card");

    let (status, body) = running
        .invoke(
            "cards.move",
            TOKEN,
            json!({
                "id": card,
                "list_id": waiting,
                "_version": 1,
                "internal_coach": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], json!("engine_only_input"), "{body}");

    let (lane, version): (Uuid, i64) =
        sqlx::query_as("SELECT list_id, version FROM card WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(card_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the refused card");
    assert_eq!(lane, Uuid::parse_str(&active).expect("active"));
    assert_eq!(version, 1, "the refused move wrote nothing");
    let hidden: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(card_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the interruptions");
    assert_eq!(hidden, 0, "a refused move opens no interruption");

    // The same move, without the engine's marker, is the ordinary flow event.
    let (status, body) = running
        .invoke(
            "cards.move",
            TOKEN,
            json!({ "id": card, "list_id": waiting, "_version": 1 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT trigger_kind, signature FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(card_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the interruptions");
    assert_eq!(rows.len(), 1, "one move, one interruption: {rows:?}");
    assert_eq!(rows[0].0, "board_event");
    assert_eq!(rows[0].1, format!("board_event|card.moved|to:{waiting}"));
    assert_eq!(count_cards_of_type(&pg, &seeded, &board, "coach").await, 1);
}

fn error_code(body: &Value) -> &str {
    body["errors"][0]["code"]
        .as_str()
        .or_else(|| body["type"].as_str())
        .unwrap_or_else(|| panic!("no error code in {body}"))
}

/// The next dispatch of one card: `process_runs.start`, without the 201 assertion.
async fn try_start(running: &support::Running, card: &str, key: &str) -> (u16, Value) {
    running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": key, "inputs": {}}),
            None,
            None,
            None,
        )
        .await
}

async fn card_meta_field(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
    field: &str,
) -> Option<String> {
    sqlx::query_scalar("SELECT meta->>$3 FROM card WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(card).expect("card"))
        .bind(field)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the card's meta field")
}


/// A blocked remedy stops the card's next dispatch, and the block is on the card.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_block_remedy_stops_the_next_dispatch() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "the card to block").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let _ = err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    let (status, body) = triage(&running, interruption, "transient", 0.9).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = remedy(&running, interruption, Some("block")).await;
    assert_eq!(status, 201, "{body}");

    let (status, body) = try_start(&running, &card, FAILING_KEY).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(error_code(&body), "coach_blocked", "{body}");
    assert_eq!(
        card_meta_field(&pg, &seeded, &card, "coach_blocked")
            .await
            .as_deref(),
        Some("true"),
        "the block is recorded on the card"
    );
}

/// A retry remedy lets the next dispatch create the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_remedy_lets_the_next_dispatch_run() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, _waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "the card to retry").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let _ = err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    triage(&running, interruption, "transient", 0.9).await;
    let (status, body) = remedy(&running, interruption, Some("retry")).await;
    assert_eq!(status, 201, "{body}");

    let (status, body) = try_start(&running, &card, FAILING_KEY).await;
    assert_eq!(
        status, 201,
        "the retry remedy lets the dispatch run: {body}"
    );
    assert_eq!(
        card_meta_field(&pg, &seeded, &card, "coach_remedy_effect")
            .await
            .as_deref(),
        Some("retry"),
        "the card records the applied remedy"
    );
}

/// A move remedy places the card in the lane its detail names.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_remedy_places_the_card() {
    let (pg, running, seeded) = fixture().await;
    publish_processes(&running).await;
    let (board, active, waiting) = board_lanes(&pg, &seeded).await;
    let card = create_card(&running, &board, &active, "feature", "the card to move").await;
    let run = fail_run(&pg, &seeded, &running, &card, FAILING_KEY).await;
    let _ = err_card(&pg, &seeded, &run).await;
    let (interruption, _, _, _, _) = interruption_row(&pg, &seeded, &card).await;
    triage(&running, interruption, "transient", 0.9).await;
    let (status, body) = running
        .invoke(
            "coach.remedy",
            TOKEN,
            json!({
                "interruption_id": interruption,
                "remedy_class": "move",
                "detail": { "list_id": waiting },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    let (status, body) = try_start(&running, &card, FAILING_KEY).await;
    assert_eq!(status, 201, "the move is applied before the run: {body}");
    assert_eq!(
        card_lane(&pg, &seeded, &card).await.to_string(),
        waiting,
        "the card is in the lane the remedy named"
    );
}
