//! S20 Part A — the climb's gates, over the wire: no round before the plan is signed, none
//! without a baseline, none that predicts another metric, none that cites a held-out transcript,
//! and — when the plan says so — none that runs before its own round gate is decided.
//!
//! These are the four reversal proofs Part A owes the slice, plus the two guards that make the
//! loop honest: a delta inside the measured noise floor is refused **with the number**, and the
//! run a round creates carries its hypothesis before anything executes.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

// ── fixtures ────────────────────────────────────────────────────────────────────────────

/// An eval with a version and nothing else.
async fn empty_draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "climb contracts",
                "purpose": "does a round wait for its plan, its baseline and its own gate",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let version_id = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    (eval_id, version_id)
}

/// A **ready** instrument: `cases` cases, one programmatic metric, both sign-offs, a drawn split.
async fn ready_instrument(running: &support::Running, cases: usize) -> (String, String) {
    let (eval_id, version_id) = empty_draft(running).await;
    for n in 0..cases {
        let (status, body) = running
            .invoke(
                "eval_cases.create",
                TOKEN,
                json!({
                    "version_id": version_id,
                    "input": format!("classify: case {n}"),
                    "tags": ["triage"],
                    "expected": { "label": "billing" },
                    "gold_provenance": "human_written",
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "key": "label_accuracy",
                "role": "quality",
                "kind": "binary",
                "label": "label acc",
                "grader_kind": "programmatic",
                "grader": { "kind": "exact", "field": "label" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    for gate in ["eval_inputs", "eval_grading"] {
        let (status, body) = running
            .invoke(
                "eval_signoffs.request",
                TOKEN,
                json!({ "eval_id": eval_id, "gate": gate, "ask": "review the instrument" }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{body}");
        let approval_id = body["output"]["id"]
            .as_str()
            .expect("approval id")
            .to_owned();
        let (status, body) = running
            .invoke(
                "eval_signoffs.decide",
                TOKEN,
                json!({ "approval_id": approval_id, "decision": "approve" }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{body}");
    }
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    (eval_id, version_id)
}

/// Start a climb and return its id.
async fn start_climb(running: &support::Running, eval_id: &str, delta: f64) -> (u16, Value) {
    running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": { "target": "label_accuracy", "direction": "higher" },
                "stopping": "plateau",
                "plateau_k": 3,
                "delta": delta,
                "reps": 4,
                "approve_each_round": false,
            }),
            None,
            None,
            None,
        )
        .await
}

/// Approve the climb's plan through the ordinary approval surface (S06's action, not a shortcut).
async fn approve_plan(running: &support::Running, eval_id: &str) -> String {
    let approval_id = plan_gate_id(running, eval_id).await;
    let (status, body) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "approved");
    approval_id
}

/// The pending plan gate's id, read from the ordinary approvals list.
///
/// Each test runs against a fresh database, so the tenant holds exactly one pending gate — the
/// plan — and no `meta` projection is needed to find it.
async fn plan_gate_id(running: &support::Running, eval_id: &str) -> String {
    let (status, body) = running
        .invoke(
            "approvals.list",
            TOKEN,
            json!({ "status": "pending", "limit": 50 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    let gate = items
        .iter()
        .find(|item| item["gate"] == "eval_climb_plan" && item["kind"] == "eval_climb")
        .unwrap_or_else(|| panic!("no pending plan gate for eval {eval_id} in {body}"));
    gate["id"].as_str().expect("approval id").to_owned()
}

/// A terminal `baseline` run with one result per case, every one at 1.0.
///
/// A climb measures round 1 against the baseline, so a baseline that never ran is a climb with
/// no reference — the action refuses rather than inventing one.
async fn seed_baseline(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: &str,
    version_id: &str,
) -> Uuid {
    let eval_id = Uuid::parse_str(eval_id).expect("eval uuid");
    let version_id = Uuid::parse_str(version_id).expect("version uuid");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    let environment = json!({"model": "stub"});
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id,
            version_id,
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: cases.len() as i32,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the baseline run");
    let grade = json!({"label_accuracy": 1.0});
    let explanation = json!({});
    let usage = json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15});
    for case_id in &cases {
        store::eval::insert_case_result_tx(
            &mut tx,
            tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run.id,
                case_id: *case_id,
                rep: 0,
                status: "ok",
                stop_reason: None,
                grade: &grade,
                explanation: &explanation,
                model: "stub-model",
                usage: &usage,
                latency_ms: 3,
                tool_calls: 0,
                action_invocation_id: None,
                subject_run_id: None,
                judge_model: None,
                judge_usage: None,
                human_score: None,
            },
        )
        .await
        .expect("insert a baseline result");
    }
    store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Running)
        .await
        .expect("baseline to running");
    store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Complete)
        .await
        .expect("baseline to complete");
    tx.commit().await.expect("commit the baseline");
    run.id
}

/// The `eval.case.run` jobs a run owns.
async fn case_jobs(pg: &support::Pg, tenant: TenantId, run_id: &str) -> Vec<String> {
    let prefix = format!("{run_id}:%");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT dedupe_key FROM job
          WHERE tenant_id = $1 AND kind = 'eval.case.run' AND dedupe_key LIKE $2
          ORDER BY dedupe_key",
    )
    .bind(tenant.as_uuid())
    .bind(prefix)
    .fetch_all(&mut *tx)
    .await
    .expect("read the run's jobs");
    tx.commit().await.expect("commit");
    rows
}

// ── the gates ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_refuses_an_unready_instrument_naming_every_gate() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _version) = empty_draft(&running).await;

    let (status, body) = start_climb(&running, &eval_id, 0.3).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "eval_not_ready", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    for gate in ["eval_inputs", "eval_grading", "metrics", "split_missing"] {
        assert!(
            detail.contains(gate),
            "the refusal names every missing gate, and {gate} is absent: {detail}"
        );
    }
}

#[tokio::test]
async fn start_refuses_a_delta_inside_the_noise_floor_with_the_number() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _version) = ready_instrument(&running, 20).await;

    let (status, body) = start_climb(&running, &eval_id, 0.05).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "delta_below_noise", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    assert!(
        detail.contains("0.05") && detail.contains('0'),
        "the refusal carries both numbers: {detail}"
    );
    // 10 held-out cases at 4 reps: the half-width is 1/sqrt(40) ≈ 0.158.
    assert!(
        detail.contains("0.158") || detail.contains("0.15"),
        "the floor is stated, not implied: {detail}"
    );
}

#[tokio::test]
async fn no_round_before_the_plan_is_signed_and_none_without_a_baseline() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 20).await;

    let (status, body) = start_climb(&running, &eval_id, 0.2).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    assert_eq!(body["output"]["status"], "planned");

    // (a) the plan is pending: the round is refused, and it says why.
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "tighten the classifier prompt",
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "plan_pending", "{body}");

    // (b) the plan is approved, but nothing has been measured: no baseline, no round.
    approve_plan(&running, &eval_id).await;
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "tighten the classifier prompt",
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "baseline_missing", "{body}");

    // (c) with a baseline the round is created, and its run carries the hypothesis.
    seed_baseline(&pg, seeded.tenant, &eval_id, &version_id).await;
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "tighten the classifier prompt",
                "change_patch": "--- a/prompt.md\n+++ b/prompt.md\n",
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["round"], 1);
    assert_eq!(body["output"]["variant"], "v1");
    let run_id = body["output"]["run_id"]
        .as_str()
        .expect("run id")
        .to_owned();

    // One job per (case, rep): 20 cases × 4 reps.
    let jobs = case_jobs(&pg, seeded.tenant, &run_id).await;
    assert_eq!(jobs.len(), 80, "20 cases × 4 reps");

    // The hypothesis is on the run BEFORE anything executes.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let (round, summary): (i32, Value) =
        sqlx::query_as("SELECT round, summary FROM eval_run WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant.as_uuid())
            .bind(Uuid::parse_str(&run_id).expect("uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("read the round's run");
    tx.commit().await.expect("commit");
    assert_eq!(round, 1);
    assert_eq!(summary["prediction"]["metric"], "label_accuracy");
    assert_eq!(
        summary["prediction"]["falsifier"],
        "held-out does not move by 0.2"
    );

    // A second round while the first is still running is refused.
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "another change",
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "round_in_flight", "{body}");
}

#[tokio::test]
async fn a_round_that_predicts_another_metric_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 20).await;
    let (status, body) = start_climb(&running, &eval_id, 0.2).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    approve_plan(&running, &eval_id).await;
    seed_baseline(&pg, seeded.tenant, &eval_id, &version_id).await;

    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "speed the classifier up",
                "prediction": {
                    "metric": "latency_ms", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "latency does not move"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "one_hypothesis", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    assert!(
        detail.contains("label_accuracy"),
        "the refusal names the climb's own target: {detail}"
    );
}

#[tokio::test]
async fn a_round_citing_a_held_out_transcript_is_void() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 20).await;
    let (status, body) = start_climb(&running, &eval_id, 0.2).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    approve_plan(&running, &eval_id).await;
    let baseline = seed_baseline(&pg, seeded.tenant, &eval_id, &version_id).await;

    // A trace on a `test`-split case, written by the baseline run.
    let test_case: Uuid = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM eval_case
              WHERE tenant_id = $1 AND version_id = $2 AND split = 'test'
              ORDER BY ordinal LIMIT 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(Uuid::parse_str(&version_id).expect("uuid"))
        .fetch_one(&mut *tx)
        .await
        .expect("a held-out case");
        let turns = json!([
            {"role": "user", "content": "classify: case"},
            {"role": "assistant", "content": "{\"label\": \"billing\"}"}
        ]);
        sqlx::query(
            "INSERT INTO eval_trace (tenant_id, eval_run_id, case_id, rep, turns, sha256, bytes)
             VALUES ($1, $2, $3, 0, $4, $5, 10)",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(baseline)
        .bind(id)
        .bind(&turns)
        .bind("deadbeef")
        .execute(&mut *tx)
        .await
        .expect("seed a held-out trace");
        tx.commit().await.expect("commit");
        id
    };
    let trace_id: Uuid = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM eval_trace WHERE tenant_id = $1 AND case_id = $2 LIMIT 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(test_case)
        .fetch_one(&mut *tx)
        .await
        .expect("the seeded trace");
        tx.commit().await.expect("commit");
        id
    };

    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "a change proposed from a held-out transcript",
                "analysis_trace_ids": [trace_id],
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "test_transcript_in_round", "{body}");
}

#[tokio::test]
async fn approve_each_round_refuses_execution_until_the_round_is_decided() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 20).await;

    let (status, body) = running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": { "target": "label_accuracy", "direction": "higher" },
                "delta": 0.2,
                "reps": 4,
                "approve_each_round": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    approve_plan(&running, &eval_id).await;
    seed_baseline(&pg, seeded.tenant, &eval_id, &version_id).await;

    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "tighten the classifier prompt",
                "prediction": {
                    "metric": "label_accuracy", "direction": "higher",
                    "interval": [0.2, 0.4], "falsifier": "held-out does not move by 0.2"
                },
            }),
            None,
            None,
            None,
        )
        .await;
    // The round is recorded and the call succeeds; EXECUTION is what is gated. (A refusal
    // returned as an error would roll the run and its gate back with the transaction.)
    assert_eq!(status, 201, "{body}");
    assert!(
        body["output"]["awaiting_approval"].is_string(),
        "the round names the sign-off it is waiting on: {body}"
    );
    assert_eq!(
        body["output"]["queued_jobs"], 0,
        "an undecided round enqueues nothing: {body}"
    );

    // The run exists, its change is recorded, and NOTHING has been enqueued.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_run WHERE tenant_id = $1 AND eval_id = $2 AND variant = 'v1'",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(Uuid::parse_str(&eval_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("the round's run exists before its gate is decided");
    tx.commit().await.expect("commit");
    assert!(
        case_jobs(&pg, seeded.tenant, &run_id.to_string())
            .await
            .is_empty(),
        "an undecided round must enqueue nothing"
    );

    // Decide the round's gate, then resume the round.
    let round_gate: Uuid = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM approval
              WHERE tenant_id = $1 AND eval_id = $2 AND gate = 'eval_round' AND status = 'pending'
                AND meta->>'round' = '1'",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(Uuid::parse_str(&eval_id).expect("uuid"))
        .fetch_one(&mut *tx)
        .await
        .expect("the round gate");
        tx.commit().await.expect("commit");
        id
    };
    let (status, body) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({ "approval_id": round_gate, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({ "id": climb_id, "round": 1 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["round"], 1);
    assert_eq!(
        case_jobs(&pg, seeded.tenant, &run_id.to_string())
            .await
            .len(),
        80,
        "the resumed round enqueues one job per (case, rep)"
    );
}

/// **The split belongs to S19's version and a climb cannot move it.**
///
/// The held-out number is only worth reporting because the climb never chose what it is measured
/// on. This proof is deliberately physical rather than a schema claim: it records the version's
/// split hash and every case's assignment, runs a round through the action, and re-reads both.
/// A climb that could redraw the split would be able to fit the test set and still show a delta.
#[tokio::test]
async fn a_climb_cannot_redraw_the_split() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running, 20).await;

    let before = split_state(&pg, seeded.tenant, &version_id).await;
    assert!(before.0.is_some(), "the fixture drew a split");

    let (status, body) = start_climb(&running, &eval_id, 0.2).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb").to_owned();
    approve_plan(&running, &eval_id).await;
    seed_baseline(&pg, seeded.tenant, &eval_id, &version_id).await;
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "round 1: tighten the label prompt",
                "change_patch": "--- a/prompt\n+++ b/prompt\n@@ one change",
                "prediction": {
                    "metric": "label_accuracy",
                    "direction": "higher",
                    "interval": [0.2, 0.5],
                    "falsifier": "held-out accuracy does not move by the band",
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    let after = split_state(&pg, seeded.tenant, &version_id).await;
    assert_eq!(
        after, before,
        "the split hash and every case's assignment must be byte-identical after a round"
    );

    // And the only action that could redraw it refuses, because S19 locked it.
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "split_locked", "{body}");
}

/// `(split hash, [(ordinal, split)])` — the whole of what "the split" means for a version.
async fn split_state(
    pg: &support::Pg,
    tenant: TenantId,
    version_id: &str,
) -> (Option<String>, Vec<(i32, String)>) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let hash: Option<String> = sqlx::query_scalar(
        "SELECT split_sha256 FROM eval_version WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("read the version");
    let assignments: Vec<(i32, String)> = sqlx::query_as(
        "SELECT ordinal, split FROM eval_case WHERE tenant_id = $1 AND version_id = $2 \
         ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("uuid"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    tx.commit().await.expect("commit");
    (hash, assignments)
}
