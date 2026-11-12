//! S13's judge phase — the ladder the `process.step` executor runs after a step succeeds
//! (SPEC D28, `crates/actions/src/process/step.rs::judge_and_record`).
//!
//! Every test drives the executor directly (`process::step::execute_step`) against a real
//! Postgres and the real registry, then asserts on the rows the ladder wrote — `run_step`,
//! `run`, `job` and `event` — never on a return value. The six properties:
//!
//! 1. **accept** — a readable verdict ends the attempt `succeeded` with
//!    `run_step.judge.decision = 'accept'`;
//! 2. **reject → rerun** — a refused output records `judge.decision = 'reject'`, the attempt
//!    fails with `process.judge_rejected`, and the SAME position gets a fresh attempt whose
//!    persisted input carries `_judge_reason` for the rerun to read;
//! 3. **budget exhausted** — `max_rejects` rejects block the run (the ordinary `fail_run` path:
//!    one `run.failed` event and the card's coach started exactly once), and no further attempt
//!    is appended;
//! 4. **outage** — a judge that cannot decide (a `laya.judge` call answering `503` because the
//!    tenant has no sealed `laya.token`) is NEVER a rejection: the row stays `running` with
//!    `judge.decision = 'unavailable'` and a `process.step` job is re-enqueued for the SAME
//!    `(position, attempt)` under the park's own dedupe slot;
//! 5. **an answer that is not a verdict** — a laya answer naming `error`/`abstain`/`unknown`, or
//!    a failed call's placeholder score, is the judge declining to decide: it parks exactly like
//!    an outage and spends no reject;
//! 6. **a failed eval grade** — a grade that ends `failed` parks the attempt on the grade it
//!    already owns: no second `eval_grade` row and no second `eval.grade` job across parks.
//!
//! The fixture writes the project/board/lane through the store rather than through
//! `projects.create`: that action requires the tenant's seeded start process, which is a
//! different slice's dependency. The SOP, the process and the run go through their real HTTP
//! actions, so the SOP pin, publish validation and `run_step` materialisation are the
//! production paths.

mod support;

use std::sync::atomic::Ordering;

use conveyor_domain::ListKind;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// The base URL the external invoker reads for the `laya` service (`invoke::service_url`).
const LAYA_URL_ENV: &str = "CONVEYOR_EXTERNAL_LAYA_URL";

/// `CONVEYOR_EXTERNAL_LAYA_URL` is process-wide and the laya tests need different stubs, so
/// they take this lock for their whole body rather than racing each other's environment.
static LAYA_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

/// A project, its board and that board's `active` lane, written through the store.
///
/// `projects.create` also requires the tenant's seeded start process (`process_not_seeded`
/// otherwise), which is not what these tests are about; the board and its lanes are the only
/// thing a process run needs from it.
async fn board_and_lane(pg: &support::Pg, seeded: &support::Seeded) -> (String, String) {
    let tenant = seeded.tenant;
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    let project = conveyor_store::project::insert_project_tx(
        &mut tx,
        tenant,
        &conveyor_store::project::NewProject::new(
            format!("J{}", Uuid::now_v7().simple()),
            "Judge fixture",
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
    let lane = conveyor_store::list::insert_list_tx(
        &mut tx,
        tenant,
        &conveyor_store::list::NewList::new(board.id, "Active", Decimal::ZERO, ListKind::Active),
    )
    .await
    .expect("the active lane");
    tx.commit().await.expect("commit the fixture board");
    (board.id.to_string(), lane.id.to_string())
}

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Judge card {}", Uuid::now_v7().simple()),
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

/// A published SOP whose inspection is the judge under test — the only way a step names a judge.
async fn publish_sop(running: &support::Running, key: &str, inspection: Value) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("SOP {key}"),
                "acceptance_criteria": ["the step's output is complete"],
                "inspection": inspection,
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

/// A board field for the step that produces an empty (`204`) output.
async fn create_custom_field(running: &support::Running, board: &str) -> String {
    let (status, body) = running
        .invoke(
            "custom_fields.create",
            TOKEN,
            json!({
                "board_id": board,
                "name": format!("field-{}", Uuid::now_v7().simple()),
                "kind": "text",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_process(running: &support::Running, key: &str, steps: Value) -> String {
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({"key": key, "title": format!("Process {key}"), "steps": steps}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
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

/// Run one step attempt exactly as the worker's `process.step` arm does.
async fn execute(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) {
    conveyor_actions::process::step::execute_step(
        &pg.runtime,
        seeded.tenant,
        "conveyor",
        Uuid::parse_str(run).expect("run id"),
        position,
        attempt,
    )
    .await
    .unwrap_or_else(|failure| panic!("step {position}/{attempt} failed: {}", failure.message));
}

/// One attempt's trace row: `(status, input, output, error, judge)`.
async fn step_row(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> (String, Value, Value, Option<String>, Option<Value>) {
    sqlx::query_as(
        "SELECT status, input, output, error, judge FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .bind(attempt)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run_step row")
}

/// Every attempt's verdict at one position, in attempt order (`''` when none was written).
async fn judge_decisions(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT coalesce(judge->>'decision', '') FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 ORDER BY attempt",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the judge decisions")
}

async fn attempt_count(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the attempt count")
}

/// The live (`pending`/`running`) `process.step` jobs for one attempt: `(dedupe_key, payload)`.
async fn live_step_jobs(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> Vec<(String, Value)> {
    sqlx::query_as(
        "SELECT dedupe_key, payload FROM job
          WHERE tenant_id = $1 AND kind = 'process.step'
            AND payload->>'run_id' = $2
            AND payload->>'position' = $3
            AND payload->>'attempt' = $4
            AND status IN ('pending', 'running')
          ORDER BY dedupe_key",
    )
    .bind(seeded.tenant_id)
    .bind(run)
    .bind(position.to_string())
    .bind(attempt.to_string())
    .fetch_all(&pg.super_pool)
    .await
    .expect("the live step jobs")
}

async fn events(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT type FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq")
        .bind(seeded.tenant_id)
        .bind(format!("run:{run}"))
        .fetch_all(&pg.super_pool)
        .await
        .expect("run events")
}

async fn event_count(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    event_type: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND topic = $2 AND type = $3",
    )
    .bind(seeded.tenant_id)
    .bind(format!("run:{run}"))
    .bind(event_type)
    .fetch_one(&pg.super_pool)
    .await
    .expect("event count")
}

async fn run_status(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(run).expect("run id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the run row")
}

async fn invocations(pg: &support::Pg, seeded: &support::Seeded, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation WHERE tenant_id = $1 AND action = $2",
    )
    .bind(seeded.tenant_id)
    .bind(action)
    .fetch_one(&pg.super_pool)
    .await
    .expect("invocation count")
}

/// The card's coach: `fail_run` enqueues the `err.card` handler for the failed run (S13 §5.8).
async fn coach_jobs(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'err.card' AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("err.card:{run}"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the coach job")
}

/// Seal the tenant's `laya.token`, so `laya.judge` reaches its service instead of failing
/// closed with the `503 action-service-unconfigured` the outage test relies on.
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

/// A laya stub: every call is answered with one fixed document and counted.
struct LayaStub {
    base: String,
    hits: support::StubHits,
    task: tokio::task::JoinHandle<()>,
}

impl LayaStub {
    /// A stub answering every laya call with `body`, counting the calls it served.
    async fn start(body: Value) -> LayaStub {
        let hits = support::StubHits::default();
        let app = axum::Router::new()
            .fallback(axum::routing::post(laya_answer))
            .with_state(LayaAnswer {
                body,
                hits: hits.clone(),
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the laya stub");
        let addr = listener.local_addr().expect("the laya stub's address");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        LayaStub {
            base: format!("http://{addr}"),
            hits,
            task,
        }
    }
}

impl Drop for LayaStub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The stub's state: the document it answers and the count of answers served.
#[derive(Clone)]
struct LayaAnswer {
    body: Value,
    hits: support::StubHits,
}

async fn laya_answer(
    axum::extract::State(answer): axum::extract::State<LayaAnswer>,
) -> axum::Json<Value> {
    answer.hits.0.fetch_add(1, Ordering::SeqCst);
    axum::Json(answer.body)
}

/// An eval with a current version — the instrument an `inspection.kind = 'eval'` SOP needs.
async fn eval_with_version(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": format!("judge eval {}", Uuid::now_v7().simple()),
                "purpose": "the SOP judge grades one step output",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({"eval_id": eval_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    eval_id
}

/// The grade the step's park named, if it named one.
fn parked_grade(judge: &Value) -> String {
    judge["grade_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the park names its grade in {judge}"))
        .to_owned()
}

/// How many `eval_grade` rows this eval owns — one per graded output, never one per park.
async fn grade_rows(pg: &support::Pg, seeded: &support::Seeded, eval_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM eval_grade WHERE tenant_id = $1 AND eval_id = $2")
        .bind(seeded.tenant_id)
        .bind(Uuid::parse_str(eval_id).expect("eval id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the eval_grade count")
}

/// How many `eval.grade` jobs were ever enqueued (any status) — the grading model calls.
async fn grade_jobs(pg: &support::Pg, seeded: &support::Seeded) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'eval.grade'")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the eval.grade job count")
}

/// End a grade `failed` through the store's own terminal write, exactly as the grader does when
/// it cannot decide.
async fn fail_grade(pg: &support::Pg, seeded: &support::Seeded, grade_id: &str) {
    let grade_id = Uuid::parse_str(grade_id).expect("grade id");
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    let written = conveyor_store::eval::set_terminal_tx(
        &mut tx,
        seeded.tenant,
        grade_id,
        &conveyor_store::eval::TerminalGrade {
            status: "failed",
            grade: &json!({}),
            detail: &json!({}),
            score: None,
            reason: None,
            error: Some("the grader could not decide"),
        },
    )
    .await
    .expect("the terminal write");
    assert!(written.is_some(), "the grade was pending");
    tx.commit().await.expect("commit the failed grade");
}

/// Every live park job of one attempt, in dedupe-key order.
async fn live_park_jobs(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> Vec<String> {
    live_step_jobs(pg, seeded, run, position, attempt)
        .await
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.contains(":grade:") || key.contains(":wait:"))
        .collect()
}

// ── 1. accept ────────────────────────────────────────────────────────────────────────────

/// A checker-inspected step with a readable, non-empty output is accepted, and the verdict is
/// recorded on the attempt's own row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_checker_accepts_a_non_empty_step_output() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "judge.accept",
        json!({"kind": "checker", "rule": "non_empty"}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-accept",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-accept", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, input, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(
        input,
        json!({"id": card}),
        "the resolved input is persisted"
    );
    assert!(
        out.pointer("/card/id").is_some(),
        "the action's output is recorded: {out}"
    );
    assert_eq!(error, None);
    let judge = judge.expect("the step's SOP judged the output");
    assert_eq!(judge["decision"], "accept");
    assert_eq!(judge["score"], json!(1.0));
    assert!(
        judge["output_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty()),
        "every verdict carries the output hash: {judge}"
    );
    assert_eq!(judge["raw"], json!({"rule": "non_empty", "accepted": true}));

    // The accept is a normal success: the last step closes the run.
    let (run_state, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(run_state, "succeeded");
    assert_eq!(error_class, None);
    assert!(
        events(&pg, &seeded, &run)
            .await
            .iter()
            .any(|event| event == "run.succeeded")
    );
    assert_eq!(
        invocations(&pg, &seeded, "cards.get").await,
        1,
        "the judge never re-invokes the action"
    );
}

// ── 2. reject → rerun with the reason ───────────────────────────────────────────────────

/// A refused output fails the attempt, keeps its verdict, and reruns the SAME position with the
/// judge's reason on the new attempt's input — the only channel that tells the rerun what was
/// wrong with the last output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_checker_reject_reruns_the_position_with_the_reason_on_its_input() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let field = create_custom_field(&running, &board).await;
    // A `204` action: the step succeeds but its output is `null`, which `non_empty` refuses.
    let sop = publish_sop(
        &running,
        "judge.reject",
        json!({"kind": "checker", "rule": "non_empty", "max_rejects": 3}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-reject",
        json!([{"action": "custom_fields.clear_value", "procedure_id": sop,
                "input_template": {"card_id": "{{card.id}}", "custom_field_id": field}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-reject", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    // Attempt 1: refused, and the refusal is a first-class failed record.
    let (status, _, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed");
    assert_eq!(out, Value::Null, "a 204 records a null output");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.starts_with("process.judge_rejected:")),
        "the record names the judge refusal, got {error:?}"
    );
    let judge = judge.expect("attempt 1's verdict");
    assert_eq!(judge["decision"], "reject");
    assert_eq!(judge["reason"], "checker `non_empty`");

    // The rerun and its job committed with the record: same position, next attempt.
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["reject", ""],
        "attempt 1 is refused; the appended attempt has no verdict yet"
    );
    let (queued, _, _, _, queued_judge) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(queued, "queued", "the rerun is appended, not overwritten");
    assert!(
        queued_judge.is_none(),
        "the appended attempt has not been judged yet"
    );
    assert_eq!(
        live_step_jobs(&pg, &seeded, &run, 1, 2).await,
        [(
            format!("{run}:1:2"),
            json!({"run_id": run, "position": 1, "attempt": 2}),
        )],
        "the rerun's job committed with the record"
    );
    assert_eq!(
        run_status(&pg, &seeded, &run).await.0,
        "running",
        "a budget of 3 is not spent by one reject"
    );

    execute(&pg, &seeded, &run, 1, 2).await;

    // Attempt 2: the reason rides the persisted input, and the action really ran again.
    let (next, input, _, error, next_judge) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(next, "failed", "{error:?}");
    assert_eq!(
        input["card_id"],
        json!(card),
        "the resolved template survives"
    );
    assert_eq!(
        input["custom_field_id"],
        json!(field),
        "the resolved template survives"
    );
    assert_eq!(
        input["_judge_reason"],
        json!("checker `non_empty`"),
        "the rerun is told what the last output was refused for"
    );
    assert_eq!(
        next_judge.expect("attempt 2's verdict")["decision"],
        "reject"
    );
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["reject", "reject", ""],
        "attempt 1 keeps its own verdict; attempt 2's refusal queues attempt 3"
    );
    assert_eq!(
        invocations(&pg, &seeded, "custom_fields.clear_value").await,
        2,
        "the rerun executed the step again"
    );
    assert_eq!(
        attempt_count(&pg, &seeded, &run, 1).await,
        3,
        "a budget of 3 lets the second refusal queue one more attempt"
    );
    assert_eq!(
        run_status(&pg, &seeded, &run).await.0,
        "running",
        "two rejects have not spent a budget of 3"
    );
}

// ── 3. the reject budget blocks the run ─────────────────────────────────────────────────

/// With `max_rejects = 2`, the second reject is the last: no third attempt is queued, the run
/// fails through the ordinary `fail_run` path (one `run.failed` event, the coach started once).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reject_budget_blocks_the_run_and_starts_the_coach_once() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let field = create_custom_field(&running, &board).await;
    let sop = publish_sop(
        &running,
        "judge.block",
        json!({"kind": "checker", "rule": "non_empty", "max_rejects": 2}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-block",
        json!([{"action": "custom_fields.clear_value", "procedure_id": sop,
                "input_template": {"card_id": "{{card.id}}", "custom_field_id": field}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-block", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    execute(&pg, &seeded, &run, 1, 2).await;

    // Exactly two attempts, both refused; the budget queued no third.
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["reject", "reject"]
    );
    assert_eq!(
        attempt_count(&pg, &seeded, &run, 1).await,
        2,
        "an exhausted budget appends no third attempt"
    );
    assert!(
        live_step_jobs(&pg, &seeded, &run, 1, 3).await.is_empty(),
        "an exhausted budget queues no third job"
    );

    // The block is the ordinary run failure, fired exactly once.
    let (run_state, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(run_state, "failed");
    assert_eq!(error_class.as_deref(), Some("process_step_failed"));
    assert_eq!(
        event_count(&pg, &seeded, &run, "run.failed").await,
        1,
        "one failure transition, not one per rejected attempt"
    );
    assert_eq!(
        coach_jobs(&pg, &seeded, &run).await,
        1,
        "the card's coach is started exactly once"
    );
    assert!(
        !events(&pg, &seeded, &run)
            .await
            .iter()
            .any(|event| event == "run.succeeded"),
        "a blocked run never succeeds"
    );
}

// ── 4. a judge outage waits ─────────────────────────────────────────────────────────────

/// A `laya.judge` call that answers `503` (the tenant has no sealed `laya.token`) is an outage,
/// never a rejection: the output is stored, the row stays `running` with
/// `judge.decision = 'unavailable'`, and a `process.step` job comes back to the SAME attempt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_laya_outage_parks_the_attempt_without_rejecting_it() {
    // The laya tests share one process-wide base URL; hold the lock for this test's lifetime.
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    // The service is configured (the stub's base URL) and the tenant has no `laya.token`, so
    // the external invoker fails closed with `503 action-service-unconfigured` — the outage
    // contract, not a decision.
    let stub = support::Stub::start().await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "judge.laya",
        json!({"kind": "laya", "threshold": 0.5}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-laya",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-laya", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        status, "running",
        "an outage neither decides nor drops the attempt"
    );
    assert_eq!(error, None, "nothing was decided, so nothing failed");
    assert!(
        out.pointer("/card/id").is_some(),
        "the action's output is stored while the judge is down: {out}"
    );
    let judge = judge.expect("a park document");
    assert_eq!(judge["decision"], "unavailable");
    assert!(
        judge["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("action-service-unconfigured")),
        "the park names the 503, got {judge}"
    );
    assert_eq!(judge["parks"], 1);
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["unavailable"],
        "an outage is never counted as a reject"
    );
    assert_eq!(
        attempt_count(&pg, &seeded, &run, 1).await,
        1,
        "an outage never reruns the position"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");

    // The park job is for the SAME `(position, attempt)` under its own dedupe slot. The plain
    // `{run}:1:1` job is still pending (this harness calls the executor directly, so nothing
    // consumed it), which is why the park slot is filtered for rather than assumed alone.
    let parked: Vec<(String, Value)> = live_step_jobs(&pg, &seeded, &run, 1, 1)
        .await
        .into_iter()
        .filter(|(key, _)| key.contains(":wait:"))
        .collect();
    assert_eq!(
        parked,
        [(
            format!("{run}:1:1:wait:1"),
            json!({"run_id": run, "position": 1, "attempt": 1})
        )],
        "the wait job carries the same attempt"
    );

    // A re-delivered park re-reads the STORED output: the action is never called a second time.
    execute(&pg, &seeded, &run, 1, 1).await;

    assert_eq!(
        invocations(&pg, &seeded, "cards.get").await,
        1,
        "the parked attempt is judged, never re-invoked (D12)"
    );
    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    assert_eq!(
        judge.expect("the second park")["parks"],
        2,
        "each wait is its own park"
    );
    let keys: Vec<String> = live_step_jobs(&pg, &seeded, &run, 1, 1)
        .await
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.contains(":wait:"))
        .collect();
    assert_eq!(
        keys,
        [format!("{run}:1:1:wait:1"), format!("{run}:1:1:wait:2")],
        "each wait gets its own dedupe slot so the next one cannot collapse"
    );
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["unavailable"],
        "still no reject after a second outage"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
}

/// Past the bound, an outage fails CLOSED: the step records `judge_unavailable`, the run fails,
/// and no further park job is enqueued. The output is never accepted and no reject is spent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_judge_that_never_decides_fails_closed_at_the_bound() {
    // The laya tests share one process-wide base URL; hold the lock for this test's lifetime.
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let stub = support::Stub::start().await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "judge.bound",
        json!({"kind": "laya", "threshold": 0.5}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-bound",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-bound", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _, _, _, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "the first outage parks");
    assert_eq!(judge.expect("the first park")["parks"], 1);

    // Drive the attempt to the bound without 63 real calls: the count is the whole state.
    sqlx::query(
        "UPDATE run_step SET judge = jsonb_set(judge, '{parks}', to_jsonb($3::int))
          WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).expect("run id"))
    .bind(i32::try_from(conveyor_actions::process::step::JUDGE_MAX_PARKS).expect("the cap"))
    .execute(&pg.super_pool)
    .await
    .expect("set the park count");

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, error, _judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed", "the bound fails the step CLOSED");
    let error = error.expect("the bound is recorded");
    assert!(
        error.contains("judge_unavailable"),
        "the failure names the judge, got {error}"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "failed");
    let parks = live_step_jobs(&pg, &seeded, &run, 1, 1)
        .await
        .into_iter()
        .filter(|(key, _)| key.contains(":wait:"))
        .count();
    assert_eq!(parks, 1, "the bound enqueues no further park job");
}

// ── 5. an answer with no verdict in it waits ────────────────────────────────────────────

/// A laya answer that names `error` (the same shape as `abstain`/`unknown`) is the judge
/// declining to decide, not a refusal: the attempt parks, the reject budget is untouched and the
/// run keeps running. `max_rejects = 1` makes a mis-read `false` a run failure on attempt 1, so
/// the assertions cannot pass by accident.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_laya_answer_that_is_not_a_verdict_parks_without_spending_the_budget() {
    // The laya tests share one process-wide base URL; hold the lock for this test's lifetime.
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let stub = LayaStub::start(json!({
        "answers": {"accept": {
            "choice": "error",
            "reason": "the judge declined to decide",
        }}
    }))
    .await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    // Without this the call fails closed with `503 action-service-unconfigured` before it
    // reaches the stub, and the test would be proving the outage path a second time.
    seal_laya_token(&pg, &seeded).await;

    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "judge.laya.error",
        json!({"kind": "laya", "threshold": 0.5, "max_rejects": 1}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-laya-error",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-laya-error", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    assert_eq!(
        stub.hits.get(),
        1,
        "the call reached laya; the answer was read and refused as a verdict"
    );
    let (status, _, out, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        status, "running",
        "an unreadable answer neither decides nor drops the attempt"
    );
    assert_eq!(error, None, "nothing was decided, so nothing failed");
    assert!(
        out.pointer("/card/id").is_some(),
        "the action's output is stored while the judge declines: {out}"
    );
    let judge = judge.expect("a park document");
    assert_eq!(
        judge["decision"], "unavailable",
        "`error` is not a rejection: {judge}"
    );
    assert!(
        judge["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("no readable verdict")),
        "the park says the answer carried no verdict, got {judge}"
    );
    assert_eq!(judge["parks"], 1);
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["unavailable"],
        "an unreadable answer is never counted as a reject"
    );
    assert_eq!(
        attempt_count(&pg, &seeded, &run, 1).await,
        1,
        "an unreadable answer never reruns the position"
    );
    assert_eq!(
        run_status(&pg, &seeded, &run).await.0,
        "running",
        "a budget of 1 is not spent by an outage"
    );
    assert_eq!(
        live_park_jobs(&pg, &seeded, &run, 1, 1).await,
        [format!("{run}:1:1:wait:1")],
        "a wait job comes back to the same attempt"
    );

    // The next park asks laya again and still spends no reject.
    execute(&pg, &seeded, &run, 1, 1).await;

    assert_eq!(stub.hits.get(), 2, "the judge is asked again");
    assert_eq!(
        invocations(&pg, &seeded, "cards.get").await,
        1,
        "the parked attempt is judged, never re-invoked (D12)"
    );
    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    assert_eq!(judge.expect("the second park")["parks"], 2);
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["unavailable"]
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
}

/// The positive control for the test above: the same stub and tenant, with a readable score,
/// accept the output. Without it a parser fed the wrong document (the invocation envelope rather
/// than its `output`) would park every laya call and the `error` test would still pass — for the
/// wrong reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_readable_laya_score_accepts_the_step_output() {
    // The laya tests share one process-wide base URL; hold the lock for this test's lifetime.
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    let stub = LayaStub::start(json!({
        "answers": {"accept": {"score": 0.9, "reason": "meets the criteria"}}
    }))
    .await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    seal_laya_token(&pg, &seeded).await;

    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "judge.laya.accept",
        json!({"kind": "laya", "threshold": 0.5}),
    )
    .await;
    let process = create_process(
        &running,
        "judge-laya-accept",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-laya-accept", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    assert_eq!(stub.hits.get(), 1, "the judge answered exactly once");
    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    let judge = judge.expect("the laya verdict");
    assert_eq!(judge["decision"], "accept");
    assert_eq!(judge["score"], json!(0.9));
    assert_eq!(judge["reason"], "meets the criteria");
    // S13 §5.5: the verdict names what it judged against and the hash of what it judged, so the
    // accept is reproducible after the fact instead of an opinion with no subject.
    assert!(
        judge["criteria"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "the verdict carries the SOP's criteria names: {judge}"
    );
    assert!(
        judge["output_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty()),
        "the verdict carries the output hash: {judge}"
    );
    let (run_state, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(run_state, "succeeded");
    assert_eq!(error_class, None);
}

// ── 6. a failed grade parks on the grade it already owns ────────────────────────────────

/// An eval grade that ends `failed` is an outage, not a verdict — and the park must keep the
/// `grade_id`, or every re-entry would create another `eval_grade` row and another `eval.grade`
/// job: one grading model call per park, forever. Two park attempts, one grade, one job.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_eval_grade_never_creates_a_second_grade_or_job() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let eval = eval_with_version(&running).await;
    let sop = publish_sop(
        &running,
        "judge.eval.failed",
        json!({
            "kind": "eval",
            "eval_id": eval,
            "threshold": 0.5,
            "max_rejects": 2,
        }),
    )
    .await;
    let process = create_process(
        &running,
        "judge-eval-failed",
        json!([{"action": "cards.get", "procedure_id": sop,
                "input_template": {"id": "{{card.id}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "judge-eval-failed", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    // The first attempt grades the output once and parks on that grade.
    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    let judge = judge.expect("the eval park");
    assert_eq!(judge["decision"], "pending");
    assert_eq!(judge["parks"], 1);
    let grade_id = parked_grade(&judge);
    assert_eq!(
        grade_rows(&pg, &seeded, &eval).await,
        1,
        "one grade for one output"
    );
    assert_eq!(
        grade_jobs(&pg, &seeded).await,
        1,
        "one eval.grade job for one grade"
    );

    // The grader cannot decide.
    fail_grade(&pg, &seeded, &grade_id).await;

    // Park attempt 1: the failed grade is an outage on the grade already owned.
    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    assert_eq!(error, None, "nothing was decided, so nothing failed");
    let judge = judge.expect("the failed-grade park");
    assert_eq!(
        judge["decision"], "unavailable",
        "a grader failure is not a rejection: {judge}"
    );
    assert_eq!(
        judge["grade_id"],
        json!(grade_id),
        "the park keeps the grade it already owns"
    );
    assert!(
        judge["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("the grader could not decide")),
        "the park names the grader's own failure, got {judge}"
    );
    assert_eq!(judge["parks"], 2);
    assert_eq!(
        grade_rows(&pg, &seeded, &eval).await,
        1,
        "a parked attempt never creates a second eval_grade row"
    );
    assert_eq!(
        grade_jobs(&pg, &seeded).await,
        1,
        "a parked attempt never enqueues a second eval.grade job"
    );
    assert_eq!(
        judge_decisions(&pg, &seeded, &run, 1).await,
        ["unavailable"],
        "a failed grade is never counted as a reject"
    );
    assert_eq!(attempt_count(&pg, &seeded, &run, 1).await, 1);
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");

    // Park attempt 2: still the same grade, still the same job.
    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, error, judge) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "running", "{error:?}");
    let judge = judge.expect("the second failed-grade park");
    assert_eq!(judge["parks"], 3);
    assert_eq!(judge["grade_id"], json!(grade_id));
    assert_eq!(
        grade_rows(&pg, &seeded, &eval).await,
        1,
        "two park attempts, one grade"
    );
    assert_eq!(
        grade_jobs(&pg, &seeded).await,
        1,
        "two park attempts, one grading model call"
    );
    assert_eq!(
        invocations(&pg, &seeded, "cards.get").await,
        1,
        "the parked attempt is judged, never re-invoked (D12)"
    );
    // The grade's slot is emitted once; every later park is a wait with its own, still
    // monotonic, suffix.
    assert_eq!(
        live_park_jobs(&pg, &seeded, &run, 1, 1).await,
        [
            format!("{run}:1:1:grade:1"),
            format!("{run}:1:1:wait:2"),
            format!("{run}:1:1:wait:3"),
        ],
        "the `:grade:<n>` slot is written once and the wait suffixes keep rising"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
}
