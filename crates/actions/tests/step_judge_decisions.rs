//! v1-p101-1 — a step judge's verdict is ONE decision row, not a second store.
//!
//! A readable accept/reject from any judge (deterministic checker, `laya.judge`, or a settled
//! eval grade) is written as a `decision` row in the same transaction that writes
//! `run_step.judge`, with the same audit shape the ladder uses: kind, rung, answer, pinned SOP
//! version, input hash, latency and — for laya — the recorded `laya.judge` invocation. The row
//! is linked to the exact `(run, run_step)` attempt it judged, so a step gate is visible to
//! `ladder.list` and reproducible after the fact.
//!
//! The fixture is the step judge's own (`judge.rs`): the executor is driven directly against a
//! real Postgres, and every assertion reads the rows it wrote. A parked attempt (an eval still
//! grading, a laya outage) decides nothing and writes no decision; the re-entry that settles it
//! writes exactly one.

mod support;

use conveyor_domain::ListKind;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use support::LayaStub;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// The base URL the external invoker reads for the `laya` service.
const LAYA_URL_ENV: &str = "CONVEYOR_EXTERNAL_LAYA_URL";

/// `CONVEYOR_EXTERNAL_LAYA_URL` is process-wide and each laya test needs its own stub, so the
/// laya tests hold this lock for their whole body.
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
            "Step decision fixture",
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
                "name": format!("Decision card {}", Uuid::now_v7().simple()),
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

/// A published SOP whose inspection is the judge under test.
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

async fn start(running: &support::Running, card: &str, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card, "process_key": key, "inputs": {}}),
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

/// The step's `judge` document (`None` when the attempt has none yet).
async fn judge_of(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> Option<Value> {
    sqlx::query_scalar(
        "SELECT judge FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .bind(attempt)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run_step judge")
}

/// One `decision` row a step judge wrote, joined to the step it is linked to.
#[derive(Debug, sqlx::FromRow)]
struct StepDecisionRow {
    position: i32,
    run_id: Uuid,
    run_step_id: Uuid,
    rung: String,
    answer: String,
    judge: String,
    policy_version: Option<i64>,
    input_hash: Option<String>,
    latency_ms: Option<i32>,
    laya_invocation_id: Option<Uuid>,
}

async fn step_decisions(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
) -> Vec<StepDecisionRow> {
    sqlx::query_as(
        "SELECT s.position, d.run_id, d.run_step_id, d.rung, d.answer,
                d.meta->>'judge' AS judge, d.policy_version, d.input_hash, d.latency_ms,
                d.laya_invocation_id
           FROM decision d
           JOIN run_step s ON s.id = d.run_step_id
          WHERE d.tenant_id = $1 AND d.run_id = $2 AND d.kind = 'process.step.judge'
          ORDER BY s.position",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("the step decisions")
}

/// The `run_step` id of one attempt.
async fn run_step_id(
    pg: &support::Pg,
    seeded: &support::Seeded,
    run: &str,
    position: i32,
    attempt: i32,
) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM run_step
          WHERE tenant_id = $1 AND run_id = $2 AND position = $3 AND attempt = $4",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(run).expect("run id"))
    .bind(position)
    .bind(attempt)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run_step id")
}

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

/// Settle a pending grade `accepted`, as the grader does.
async fn accept_grade(pg: &support::Pg, seeded: &support::Seeded, grade_id: &str) {
    let grade_id = Uuid::parse_str(grade_id).expect("grade id");
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    let written = conveyor_store::eval::set_terminal_tx(
        &mut tx,
        seeded.tenant,
        grade_id,
        &conveyor_store::eval::TerminalGrade {
            status: "accepted",
            grade: &json!({"quality": 1.0}),
            detail: &json!({}),
            score: Some(Decimal::from(1)),
            reason: Some("the output meets the criteria"),
            error: None,
        },
    )
    .await
    .expect("the terminal write");
    assert!(written.is_some(), "the grade was pending");
    tx.commit().await.expect("commit the accepted grade");
}

/// The published process's single-step shape: `actions.get` on the card, judged by `sop`.
fn one_step(sop: &str) -> Value {
    json!([{
        "action": "cards.get",
        "procedure_id": sop,
        "input_template": {"id": "{{card.id}}"},
    }])
}

// ── every judge writes one linked decision ──────────────────────────────────────────────

/// A checker, a laya judge and an eval judge each accept one step; each verdict is exactly one
/// `decision` row, linked to the attempt it judged, carrying the audit fields v1-p101-1 names.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_checker_laya_and_eval_verdict_writes_one_decision_row_linked_to_its_step() {
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
    let checker = publish_sop(
        &running,
        "decision.checker",
        json!({"kind": "checker", "rule": "non_empty"}),
    )
    .await;
    let laya = publish_sop(
        &running,
        "decision.laya",
        json!({"kind": "laya", "threshold": 0.5}),
    )
    .await;
    let eval = eval_with_version(&running).await;
    let evaluated = publish_sop(
        &running,
        "decision.eval",
        json!({"kind": "eval", "eval_id": eval, "threshold": 0.5, "max_rejects": 2}),
    )
    .await;
    let process = create_process(
        &running,
        "decision-judges",
        json!([
            {"action": "cards.get", "procedure_id": checker,
             "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "procedure_id": laya,
             "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.get", "procedure_id": evaluated,
             "input_template": {"id": "{{card.id}}"}},
        ]),
    )
    .await;
    publish(&running, &process).await;
    let run = start(&running, &card, "decision-judges").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    execute(&pg, &seeded, &run, 2, 1).await;
    // Steps 1 and 2 each wrote their verdict. The eval's first entry on step 3 creates the
    // one-shot grade and parks; the settled grade is the verdict on re-entry, and only then is
    // a decision owed — so step 3 adds no row yet.
    let before = step_decisions(&pg, &seeded, &run).await.len();
    execute(&pg, &seeded, &run, 3, 1).await;
    assert_eq!(
        step_decisions(&pg, &seeded, &run).await.len(),
        before,
        "a pending grade decides nothing, so no decision row exists for step 3 yet"
    );
    let park = judge_of(&pg, &seeded, &run, 3, 1)
        .await
        .expect("the eval park");
    let grade_id = park["grade_id"].as_str().expect("the park names its grade");
    accept_grade(&pg, &seeded, grade_id).await;
    execute(&pg, &seeded, &run, 3, 1).await;

    let decisions = step_decisions(&pg, &seeded, &run).await;
    assert_eq!(
        decisions.len(),
        3,
        "one decision per judged step, never a second store: {decisions:?}"
    );
    let run_uuid = Uuid::parse_str(&run).expect("run id");
    for (index, decision) in decisions.iter().enumerate() {
        let position = i32::try_from(index + 1).expect("position");
        assert_eq!(decision.position, position);
        assert_eq!(decision.run_id, run_uuid, "the row names its run");
        assert_eq!(
            decision.run_step_id,
            run_step_id(&pg, &seeded, &run, position, 1).await,
            "the row names the exact attempt it judged"
        );
        assert_eq!(decision.answer, "accept");
        assert!(
            decision.policy_version.is_some_and(|version| version > 0),
            "the SOP version is pinned: {decision:?}"
        );
        let hash = decision.input_hash.as_deref().expect("an input hash");
        assert_eq!(hash.len(), 64, "a sha256 hex: {hash}");
        assert!(
            hash.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "a sha256 hex: {hash}"
        );
        assert!(
            decision.latency_ms.is_some_and(|latency| latency >= 0),
            "the row carries the judge call's latency: {decision:?}"
        );
        // The verdict is still the step's own record too, and the two agree.
        let judge = judge_of(&pg, &seeded, &run, position, 1)
            .await
            .expect("the step verdict");
        assert_eq!(judge["decision"], "accept", "position {position}");
    }
    assert_eq!(
        decisions
            .iter()
            .map(|decision| decision.judge.as_str())
            .collect::<Vec<_>>(),
        ["checker", "laya", "eval"]
    );
    assert_eq!(
        decisions
            .iter()
            .map(|decision| decision.rung.as_str())
            .collect::<Vec<_>>(),
        ["rule", "laya", "rule"],
        "laya is its own rung; a checker or a settled eval applies a rule"
    );
    assert!(
        decisions[1].laya_invocation_id.is_some(),
        "the laya verdict pins the recorded laya.judge invocation"
    );
    assert_eq!(decisions[0].laya_invocation_id, None);
    assert_eq!(decisions[2].laya_invocation_id, None);
}

// ── one rejudge, one decision ───────────────────────────────────────────────────────────

/// A laya outage parks the attempt and decides nothing; the re-entry that settles it writes
/// exactly one decision — not one per park.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejudged_step_writes_one_decision_not_two() {
    let _env = LAYA_ENV.lock().await;
    let (pg, running, seeded) = fixture().await;
    // Configured service, no `laya.token`: the call fails closed with a 503 — an outage.
    let outage = support::Stub::start().await;
    support::set_env(LAYA_URL_ENV, &outage.base);

    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "decision.rejudge",
        json!({"kind": "laya", "threshold": 0.5}),
    )
    .await;
    let process = create_process(&running, "decision-rejudge", one_step(&sop)).await;
    publish(&running, &process).await;
    let run = start(&running, &card, "decision-rejudge").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    let park = judge_of(&pg, &seeded, &run, 1, 1)
        .await
        .expect("the outage park");
    assert_eq!(park["decision"], "unavailable");
    assert_eq!(
        step_decisions(&pg, &seeded, &run).await.len(),
        0,
        "an outage is not a verdict and writes no decision"
    );

    // The judge comes back and the same attempt is re-judged from its stored output.
    let stub = LayaStub::start(json!({
        "answers": {"accept": {"score": 0.9, "reason": "meets the criteria"}}
    }))
    .await;
    support::set_env(LAYA_URL_ENV, &stub.base);
    seal_laya_token(&pg, &seeded).await;
    execute(&pg, &seeded, &run, 1, 1).await;

    let decisions = step_decisions(&pg, &seeded, &run).await;
    assert_eq!(
        decisions.len(),
        1,
        "a rejudged attempt writes one decision, not one per re-entry: {decisions:?}"
    );
    assert_eq!(decisions[0].answer, "accept");
    assert_eq!(decisions[0].judge, "laya");
    assert_eq!(decisions[0].rung, "laya");
    assert!(decisions[0].laya_invocation_id.is_some());
    assert_eq!(
        judge_of(&pg, &seeded, &run, 1, 1)
            .await
            .expect("the settled verdict")["decision"],
        "accept"
    );
}

// ── the ladder's own list sees the step gate ───────────────────────────────────────────

/// `ladder.list` returns the step judge's decisions: the per-step verdict is visible to the same
/// reporting that reads every other ladder decision, with no second store to read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ladder_list_returns_the_step_judge_decisions() {
    let (pg, running, seeded) = fixture().await;
    let (board, lane) = board_and_lane(&pg, &seeded).await;
    let card = create_card(&running, &board, &lane).await;
    let sop = publish_sop(
        &running,
        "decision.list",
        json!({"kind": "checker", "rule": "non_empty"}),
    )
    .await;
    let process = create_process(&running, "decision-list", one_step(&sop)).await;
    publish(&running, &process).await;
    let run = start(&running, &card, "decision-list").await;

    execute(&pg, &seeded, &run, 1, 1).await;
    assert_eq!(step_decisions(&pg, &seeded, &run).await.len(), 1);

    let (status, body) = running
        .invoke(
            "ladder.list",
            TOKEN,
            json!({"kind": "process.step.judge"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"]
        .as_array()
        .unwrap_or_else(|| panic!("a page of decisions, got {body}"));
    assert_eq!(
        items.len(),
        1,
        "the step gate is one ladder decision, not a second store: {body}"
    );
    assert_eq!(items[0]["kind"], "process.step.judge");
    assert_eq!(items[0]["rung"], "rule");
    assert_eq!(items[0]["answer"], "accept");
    assert!(
        items[0]["input_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64),
        "the listed row carries the input hash: {body}"
    );
}
