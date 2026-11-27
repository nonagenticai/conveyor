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
//! The coach PROCESS here is a stand-in with one computed step: the agent that calls
//! `coach.triage` is the engine's own business (and its tool loop is proved elsewhere), so these
//! tests call `coach.triage` and `coach.remedy` directly — they are the actions the process's
//! agent and computed steps call.

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
    let project = conveyor_store::project::insert_project_tx(
        &mut tx,
        tenant,
        &conveyor_store::project::NewProject::new(
            format!("C{}", Uuid::now_v7().simple()),
            "Coach fixture",
        ),
    )
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
async fn a_systemic_class_opens_one_kata() {
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
    assert!(
        output(&body)["kata_run_id"].is_string(),
        "the systemic class opened a kata: {body}"
    );

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
    assert_eq!(kata_runs, 1, "exactly one kata run");
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
    assert!(output(&body)["kata_run_id"].is_null(), "no kata: {body}");
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

// ── the remedy consumer (v1-p6-1) ─────────────────────────────────────────────────────────

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

async fn card_lane(pg: &support::Pg, seeded: &support::Seeded, card: &str) -> Uuid {
    sqlx::query_scalar("SELECT list_id FROM card WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(card).expect("card"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the card's lane")
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
