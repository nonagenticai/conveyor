//! S11 step 8 — the `process.step` executor.
//!
//! Each test starts a real run over HTTP (`process_runs.start`), then calls
//! `process::step::execute_step` directly against the same database and registry the worker
//! uses. That keeps the job's dispatch out of the test while exercising everything the executor
//! does: claim, template resolution, the real `invoke` engine, the record, the events and the
//! chain.
//!
//! The properties a silent bug would hide are the ones asserted:
//!
//! * the resolved input is persisted at claim, and a replay uses the STORED input — never a fresh
//!   resolution against changed state;
//! * a terminal replay is a no-op (no second invocation, no second event);
//! * the record and the next position's job commit together;
//! * `on_result` is honoured (`continue` skips and chains, `retry` appends until the cap then fails,
//!   `fail` closes the run and skips the rest);
//! * a gated step parks on an `approval` before it is claimed, and only its resolution can make
//!   it runnable (S11 step 9).

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
            json!({"title": format!("Step test {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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

async fn create_process(running: &support::Running, key: &str, sop: &str, steps: Value) -> String {
    // S13: every step names the published SOP it is judged against.
    let mut steps = steps;
    if let Some(items) = steps.as_array_mut() {
        for step in items {
            step["procedure_id"] = json!(sop);
        }
    }
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

async fn step_row(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> (String, String, Value, Value, Option<String>) {
    sqlx::query_as(
        "SELECT status, action, input, output, error FROM run_step
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

async fn events(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT type FROM event WHERE tenant_id = $1 AND topic = $2 ORDER BY seq")
        .bind(seeded.tenant_id)
        .bind(format!("run:{run}"))
        .fetch_all(&pg.super_pool)
        .await
        .expect("run events")
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

/// A gate code in the tenant's catalogue, with the meta the executor reads.
async fn seed_gate(pg: &support::Pg, tenant: conveyor_domain::TenantId, code: &str, meta: Value) {
    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta)
         VALUES ($1, 'gate', $2, 'Gate', $3)
         ON CONFLICT (tenant_id, kind, code) DO UPDATE SET meta = EXCLUDED.meta",
    )
    .bind(tenant.as_uuid())
    .bind(code)
    .bind(meta)
    .execute(&pg.super_pool)
    .await
    .expect("seed the gate code");
}

/// The newest process gate on a run: `(id, status, required_decisions, gate)`.
async fn approval_for(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
) -> Option<(Uuid, String, i32, String)> {
    sqlx::query_as(
        "SELECT id, status, required_decisions, gate FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'process'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_optional(&pg.super_pool)
    .await
    .expect("the process gate")
}

async fn gates_for(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'process'",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("gate count")
}

async fn decide(running: &support::Running, approval: Uuid, decision: &str) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": approval, "decision": decision}),
            None,
            None,
            None,
        )
        .await
}

/// Live (`pending`/`running`) jobs for one step attempt.
async fn live_jobs(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'process.step'
            AND payload->>'run_id' = $2
            AND payload->>'position' = $3
            AND payload->>'attempt' = $4
            AND status IN ('pending', 'running')",
    )
    .bind(seeded.tenant_id)
    .bind(run)
    .bind(position.to_string())
    .bind(attempt.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("live job count")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_claims_runs_and_records_its_resolved_input_and_output() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-one-step",
        &sop,
        json!([{"action": "cards.comment",
                "input_template": {"id": "{{card.id}}", "body": "run {{run.inputs.ticket}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-one-step", json!({"ticket": "T-1"})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, action, input, out, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(action, "cards.comment");
    assert_eq!(
        input,
        json!({"id": card, "body": "run T-1"}),
        "the resolved input is persisted"
    );
    assert!(
        out.get("id").is_some(),
        "the action's output is recorded: {out}"
    );
    assert_eq!(error, None);

    assert_eq!(invocations(&pg, &seeded, "cards.comment").await, 1);
    let key: Option<String> = sqlx::query_scalar(
        "SELECT idempotency_key FROM action_invocation
          WHERE tenant_id = $1 AND action = 'cards.comment' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the invocation row");
    assert!(
        key.as_deref()
            .is_some_and(|key| Uuid::parse_str(key).is_ok()),
        "the invocation carries the step's uuid Idempotency-Key, got {key:?}"
    );

    assert_eq!(
        events(&pg, &seeded, &run).await,
        [
            "run.created",
            "run_step.requested",
            "run.started",
            "run_step.changed",
            "run_step.changed",
            "run.succeeded"
        ]
    );
    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT payload->>'status' FROM event
          WHERE tenant_id = $1 AND topic = $2 AND type = 'run_step.changed' ORDER BY seq",
    )
    .bind(seeded.tenant_id)
    .bind(format!("run:{run}"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("step event statuses");
    assert_eq!(statuses, ["running", "succeeded"]);
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terminal_replay_is_a_noop() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-replay",
        &sop,
        json!([{"action": "cards.comment",
                "input_template": {"id": "{{card.id}}", "body": "once"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-replay", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let before = events(&pg, &seeded, &run).await;
    // The record and its chain already committed; a reclaimed job must not repeat either.
    execute(&pg, &seeded, &run, 1, 1).await;

    assert_eq!(invocations(&pg, &seeded, "cards.comment").await, 1);
    assert_eq!(events(&pg, &seeded, &run).await, before);
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replayed_running_step_reinvokes_with_the_stored_input() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-stored",
        &sop,
        json!([{"action": "cards.comment",
                "input_template": {"id": "{{card.id}}", "body": "resolved now {{run.inputs.ticket}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-stored", json!({"ticket": "T-1"})).await;

    // A previous attempt claimed the step and died before invoking. Its input is what the action
    // must receive now, even though a fresh resolution would say "resolved now T-1".
    {
        let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        conveyor_store::process::claim_run_step_tx(
            &mut tx,
            seeded.tenant,
            Uuid::parse_str(&run).expect("run id"),
            1,
            1,
            &json!({"id": card, "body": "stored body"}),
        )
        .await
        .expect("claim")
        .expect("the step was queued");
        tx.commit().await.expect("commit the claim");
    }

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, input, out, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(
        input["body"],
        json!("stored body"),
        "the STORED input was used"
    );
    assert_eq!(
        out["body"],
        json!("stored body"),
        "the action received the stored input"
    );
    assert_eq!(invocations(&pg, &seeded, "cards.comment").await, 1);
    // No second `running` event: the replay did not transition the claim again.
    assert_eq!(
        events(&pg, &seeded, &run).await,
        [
            "run.created",
            "run_step.requested",
            "run.started",
            "run_step.changed",
            "run.succeeded"
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_chains_the_next_position_and_the_last_step_succeeds_the_run() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-chain",
        &sop,
        json!([
            {"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "one"}},
            {"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "two"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-chain", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    // The record and the next position's job committed together.
    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded");
    let (next_status, _, _, _, _) = step_row(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(next_status, "queued");
    let payload: Value = sqlx::query_scalar(
        "SELECT payload FROM job WHERE tenant_id = $1 AND kind = 'process.step'
            AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{run}:2:1"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the next position's job");
    assert_eq!(payload["position"].as_i64(), Some(2));
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");

    execute(&pg, &seeded, &run, 2, 1).await;

    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(status, "succeeded");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
    let position_three: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'process.step'
            AND dedupe_key LIKE $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{run}:3:%"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("position three jobs");
    assert_eq!(
        position_three, 0,
        "the last step must not enqueue a position three"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unresolved_template_is_a_recorded_failure_not_a_crash() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-template",
        &sop,
        json!([{"action": "cards.comment",
                "input_template": {"id": "{{card.id}}", "body": "hello {{run.inputs.nope}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-template", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("process.template_unresolved")),
        "the record names the resolver failure, got {error:?}"
    );
    assert_eq!(invocations(&pg, &seeded, "cards.comment").await, 0);
    let (status, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(status, "failed");
    assert_eq!(error_class.as_deref(), Some("process_step_failed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continue_skips_the_step_and_chains() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let missing = Uuid::now_v7().to_string();
    let process = create_process(
        &running,
        "dev-continue",
        &sop,
        json!([
            {"action": "cards.get", "on_result": "continue",
             "input_template": {"id": "{{run.inputs.missing}}"}},
            {"action": "cards.comment", "input_template": {"id": "{{card.id}}", "body": "after"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-continue", json!({"missing": missing})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "skipped", "continue records the failure as skipped");
    assert!(error.is_some(), "the failure text is kept");
    let (next_status, _, _, _, _) = step_row(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(next_status, "queued", "continue chains");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");

    execute(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_appends_a_new_attempt_and_stops_at_the_cap() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let missing = Uuid::now_v7().to_string();
    let process = create_process(
        &running,
        "dev-retry",
        &sop,
        json!([{"action": "cards.get", "on_result": "retry", "max_attempts": 2,
                "input_template": {"id": "{{run.inputs.missing}}"}}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-retry", json!({"missing": missing})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed");
    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(status, "queued", "the retry appends, never overwrites");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
    // The retry's job is a NEW dedupe key — never the first attempt's job row.
    let retry_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'process.step'
            AND dedupe_key = $2 AND status = 'pending'",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{run}:1:2"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the retry job");
    assert_eq!(retry_jobs, 1);

    execute(&pg, &seeded, &run, 1, 2).await;

    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(status, "failed");
    let attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run_step WHERE tenant_id = $1 AND run_id = $2 AND position = 1",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&run).expect("run id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("attempt count");
    assert_eq!(attempts, 2, "the cap is 2; no third attempt row");
    let (status, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(status, "failed");
    assert_eq!(error_class.as_deref(), Some("process_step_failed"));
}

/// A gated step parks on its approval: nothing is claimed, nothing is invoked, and a duplicate
/// delivery while the gate is pending cannot slip past it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gated_step_parks_on_an_approval_without_invoking_anything() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(
        &pg,
        seeded.tenant,
        "human_approval",
        json!({"dual_control": true}),
    )
    .await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(
        status, "queued",
        "a gated step is never claimed before its approval"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "waiting_approval");
    let (_, gate_status, quorum, gate) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the gate was opened");
    assert_eq!(gate_status, "pending");
    assert_eq!(quorum, 2, "dual_control maps to a two-person quorum");
    assert_eq!(gate, "human_approval");
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);

    // A duplicate delivery of the same job while the gate is pending is a no-op: the run is
    // `waiting_approval`, so the executor returns before the gate branch.
    execute(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(gates_for(&pg, &seeded, &run).await, 1, "one gate, not two");
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);
}

/// Approval resumes the step once, under the same `(position, attempt)` idempotency key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_gate_records_the_step_once_without_invoking_an_action() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(&pg, seeded.tenant, "human_approval", json!({})).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate-approve",
        &sop,
        json!([{"gate_kind": "human_approval",
                "input_template": {"id": "{{card.id}}"},
                "on_result": "continue"}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate-approve", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    // The direct `execute_step` call stands in for the worker; in production the job that opened
    // the gate is consumed. Close it here so the resume's own slot is the one left live.
    sqlx::query(
        "UPDATE job SET status = 'succeeded'
          WHERE tenant_id = $1 AND kind = 'process.step' AND dedupe_key = $2",
    )
    .bind(seeded.tenant_id)
    .bind(format!("{run}:1:1"))
    .execute(&pg.super_pool)
    .await
    .expect("the park job is consumed");
    let (approval, _, quorum, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the gate was opened");
    assert_eq!(quorum, 1, "no dual_control means a single approver");

    let (status, body) = decide(&running, approval, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
    assert_eq!(
        live_jobs(&pg, &seeded, &run, 1, 1).await,
        1,
        "approval enqueues the same step attempt"
    );
    let resume_key: String = sqlx::query_scalar(
        "SELECT dedupe_key FROM job
          WHERE tenant_id = $1 AND kind = 'process.step'
            AND payload->>'run_id' = $2 AND payload->>'position' = '1'
            AND payload->>'attempt' = '1' AND status IN ('pending', 'running')",
    )
    .bind(seeded.tenant_id)
    .bind(&run)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the resume job");
    assert!(
        resume_key.ends_with(":gate"),
        "the resume has its own dedupe slot, got {resume_key}"
    );

    execute(&pg, &seeded, &run, 1, 1).await;
    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
    assert_eq!(
        invocations(&pg, &seeded, "cards.get").await,
        0,
        "a gate step names no action; the approval is its work"
    );

    // A replay of the resume job is terminal: no second record, no invocation.
    execute(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);
}

/// A rejection applies `on_result = continue`: the step is skipped, the run chains on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_gate_with_continue_skips_the_step_and_chains() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(&pg, seeded.tenant, "human_approval", json!({})).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate-continue",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"}, "on_result": "continue"},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate-continue", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let (approval, _, _, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the gate was opened");
    let (status, body) = decide(&running, approval, "reject").await;
    assert_eq!(status, 200, "{body}");

    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "skipped");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("process.gate_rejected")),
        "the rejection is kept on the row, got {error:?}"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
    assert_eq!(live_jobs(&pg, &seeded, &run, 2, 1).await, 1);
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);

    execute(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 1);
}

/// A rejection applies `on_result = fail`: the step fails, the rest is skipped, the run closes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_gate_with_fail_closes_the_run_and_skips_the_rest() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(&pg, seeded.tenant, "human_approval", json!({})).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate-fail",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate-fail", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let (approval, _, _, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the gate was opened");
    let (status, body) = decide(&running, approval, "reject").await;
    assert_eq!(status, 200, "{body}");

    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "failed");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("process.gate_rejected")),
        "got {error:?}"
    );
    let (next, _, _, _, _) = step_row(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(next, "skipped");
    let (run_state, error_class) = run_status(&pg, &seeded, &run).await;
    assert_eq!(run_state, "failed");
    assert_eq!(error_class.as_deref(), Some("process_step_failed"));
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);
}

/// A rejection under `on_result = retry` appends the next attempt and re-gates it; the cap stops
/// the loop and fails the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_gate_with_retry_reopens_the_gate_for_the_next_attempt() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(&pg, seeded.tenant, "human_approval", json!({})).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate-retry",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"},
             "on_result": "retry", "max_attempts": 2},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate-retry", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let (first, _, _, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the first gate");
    let (status, body) = decide(&running, first, "reject").await;
    assert_eq!(status, 200, "{body}");

    let (attempt_two, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(attempt_two, "queued", "retry appends a new attempt");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
    assert_eq!(live_jobs(&pg, &seeded, &run, 1, 2).await, 1);

    // The next attempt re-enters the gate (a rejection is not an approval for the retry).
    execute(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(gates_for(&pg, &seeded, &run).await, 2);
    let (second, gate_status, _, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the second gate");
    assert_eq!(gate_status, "pending");

    let (status, body) = decide(&running, second, "reject").await;
    assert_eq!(status, 200, "{body}");
    let (status, _, _, _, _) = step_row(&pg, &seeded, &run, 1, 2).await;
    assert_eq!(status, "failed", "the attempt cap is reached");
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "failed");
    assert_eq!(gates_for(&pg, &seeded, &run).await, 2, "no third gate");
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 0);
}

/// Expiry is a rejection for `on_result` purposes; the worker path calls the same resolver.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_gate_applies_the_steps_on_error() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    seed_gate(
        &pg,
        seeded.tenant,
        "human_approval",
        json!({"expires_at": "2000-01-01T00:00:00Z"}),
    )
    .await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-gate-expire",
        &sop,
        json!([
            {"gate_kind": "human_approval",
             "input_template": {"id": "{{card.id}}"}, "on_result": "continue"},
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-gate-expire", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let (approval, _, _, _) = approval_for(&pg, &seeded, &run)
        .await
        .expect("the gate was opened");

    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::approval::expire_due_tx(&mut tx, seeded.tenant)
        .await
        .expect("expire the due gate");
    conveyor_actions::process::step::resolve_gate(&mut tx, seeded.tenant, approval)
        .await
        .expect("resolve the expired gate");
    tx.commit().await.expect("commit");

    let (status, _, _, _, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "skipped");
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.contains("process.gate_expired")),
        "got {error:?}"
    );
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "running");
    assert_eq!(live_jobs(&pg, &seeded, &run, 2, 1).await, 1);

    execute(&pg, &seeded, &run, 2, 1).await;
    assert_eq!(run_status(&pg, &seeded, &run).await.0, "succeeded");
    assert_eq!(invocations(&pg, &seeded, "cards.get").await, 1);
}

/// The publish-time authority check (S11 step 8, decision 3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_member_may_not_publish_an_action_they_cannot_invoke() {
    let (pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-step-sop").await;
    let carol = support::seed(&pg, "dev-a", "carol", "Carol").await;
    {
        let mut tx = conveyor_store::begin_scoped(&pg.owner, carol.tenant)
            .await
            .expect("scope");
        sqlx::query("UPDATE membership SET role = 'member' WHERE user_id = $1")
            .bind(carol.user_id)
            .execute(&mut *tx)
            .await
            .expect("demote carol");
        tx.commit().await.expect("commit");
    }

    // `cards.move` is admin-only; a member holds `processes:write`, so only the publish check
    // stands between them and a process that performs admin card writes.
    let (status, body) = running
        .invoke(
            "processes.create",
            "dev:carol",
            json!({"key": "dev-forbidden", "title": "Forbidden",
                   "steps": [{"action": "cards.move", "procedure_id": sop,
                              "input_template": {"id": "{{card.id}}", "list_id": "{{run.inputs.done}}"}}]}),
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
            "dev:carol",
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(slug(&body), "process.action_forbidden");

    // A member-scoped action publishes under the same member: the check is about scopes, not rank.
    let (status, body) = running
        .invoke(
            "processes.create",
            "dev:carol",
            json!({"key": "dev-member-ok", "title": "Member ok",
                   "steps": [{"action": "runs.list", "procedure_id": sop, "input_template": {}}]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let allowed = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "processes.publish",
            "dev:carol",
            json!({"process_id": allowed}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    // And an owner publishes the same admin-only document: the publisher is the authority.
    publish(&running, &id).await;
}

/// A computed move addresses its destination lane by KIND (`{{board.lists.active}}`), because a
/// catalogue is content and cannot know a lane uuid; `cards.move` also needs the row's
/// `_version`. This proves both resolve at execution, and the card actually lands in `active`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_computed_move_resolves_its_lane_by_kind() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "move-by-kind-sop").await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let process = create_process(
        &running,
        "dev-move-by-kind",
        &sop,
        json!([{"action": "cards.move",
        "input_template": {
            "id": "{{card.id}}",
            "list_id": "{{board.lists.active}}",
            "_version": "{{card.version}}"
        }}]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "dev-move-by-kind", json!({})).await;

    execute(&pg, &seeded, &run, 1, 1).await;

    let (status, _action, _input, _out, error) = step_row(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(status, "succeeded", "{error:?}");

    let card_id = Uuid::parse_str(&card).expect("card id");
    let (kind,): (String,) = sqlx::query_as(
        "SELECT l.kind FROM card c
           JOIN list l ON l.tenant_id = c.tenant_id AND l.id = c.list_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(card_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the moved card's lane");
    assert_eq!(kind, "active", "the move resolved board.lists.active");
}

/// S13 §5.5: a SOP's `eval` inspection pins one eval. A pin that names a missing eval used to
/// pass publish and only fail after the step had parked on it `JUDGE_MAX_PARKS` times; publish
/// refuses it now, in the same validator that refuses every other unrunnable reference.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publishing_a_process_whose_sop_pins_a_missing_eval_is_refused() {
    let (_pg, running, _seeded) = fixture().await;

    // A SOP that pins an eval nobody created.
    let missing = Uuid::now_v7();
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": "eval-pin-missing",
                "title": "eval-pin-missing",
                "definition_of_ready": {},
                "definition_of_done": {},
                "acceptance_criteria": ["the output is graded"],
                "inspection": {"kind": "eval", "eval_id": missing, "threshold": 0.8},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let sop = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": sop}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let process = create_process(
        &running,
        "eval-pin-process",
        &sop,
        json!([
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "process.invalid_step", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains(&missing.to_string())),
        "the refusal names the eval: {body}"
    );

    // The eval now exists: the same process publishes.
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": format!("pin-{}", Uuid::now_v7().simple()),
                "purpose": "prove the pin",
                "subject_kind": "completions",
                "entry_action": "completions.create",
                "model_ref": "test-model",
                "trace_source": "synthesized",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({
                "key": "eval-pin-present",
                "title": "eval-pin-present",
                "definition_of_ready": {},
                "definition_of_done": {},
                "acceptance_criteria": ["the output is graded"],
                "inspection": {"kind": "eval", "eval_id": eval, "threshold": 0.8},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let sop = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": sop}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let process = create_process(
        &running,
        "eval-pin-present-process",
        &sop,
        json!([
            {"action": "cards.get", "input_template": {"id": "{{card.id}}"}}
        ]),
    )
    .await;
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": process}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "a live eval pin publishes: {body}");
}
