//! S20's headline proof: **a two-round climb keeps one round and reverts the other, with the
//! reason recorded** — and the report `eval_climbs.stop` writes names the winner.
//!
//! The baseline sits at 0.4 on both halves; round 1 lifts both to 0.8 (a keep); round 2 lifts
//! train to 1.0 while the held-out half moves to 0.85, inside the 0.2 band (a revert,
//! `held_out_flat`). That second shape is the whole reason the split exists: the change fitted
//! what it could see and did not transfer.
//!
//! The decisions are **recomputed from the result rows**, never read back from a stored verdict,
//! so this file is also the proof that a resumed session cannot inherit a number nobody can
//! reproduce.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const CASES: usize = 20;
const REPS: i32 = 4;
const DELTA: f64 = 0.2;

async fn empty_draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "climb loop",
                "purpose": "does a two-round climb keep one round and revert the other",
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
    let version_id = body["output"]["id"].as_str().expect("version").to_owned();
    (eval_id, version_id)
}

async fn ready_instrument(running: &support::Running) -> (String, String) {
    let (eval_id, version_id) = empty_draft(running).await;
    for n in 0..CASES {
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
        let approval_id = body["output"]["id"].as_str().expect("approval").to_owned();
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

/// Write one result per case at the score its split is given, then finalise the run.
async fn finish_run(
    pg: &support::Pg,
    tenant: TenantId,
    version_id: Uuid,
    run_id: Uuid,
    train: f64,
    test: f64,
) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let cases: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, split FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    let explanation = json!({});
    let usage = json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15});
    for (case_id, split) in &cases {
        let score = if split == "test" { test } else { train };
        let grade = json!({ "label_accuracy": score });
        for rep in 0..REPS {
            store::eval::insert_case_result_tx(
                &mut tx,
                tenant,
                &store::eval::NewCaseResult {
                    eval_run_id: run_id,
                    case_id: *case_id,
                    rep,
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
            .expect("insert a result");
        }
    }
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running)
        .await
        .expect("run to running");
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Complete)
        .await
        .expect("run to complete");
    tx.commit().await.expect("commit the finished run");
}

/// The run id a variant names.
async fn run_of(pg: &support::Pg, tenant: TenantId, eval_id: &str, variant: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_run WHERE tenant_id = $1 AND eval_id = $2 AND variant = $3",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(eval_id).expect("uuid"))
    .bind(variant)
    .fetch_one(&mut *tx)
    .await
    .expect("the variant's run");
    tx.commit().await.expect("commit");
    id
}

/// One round's recorded decision, as the run's `summary` carries it.
async fn decision_of(pg: &support::Pg, tenant: TenantId, run_id: Uuid) -> Value {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let summary: Value =
        sqlx::query_scalar("SELECT summary FROM eval_run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the summary");
    tx.commit().await.expect("commit");
    summary
}

#[tokio::test]
async fn a_two_round_climb_keeps_one_round_and_reverts_the_other_with_the_reason_recorded() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running).await;

    // ── the baseline: 0.4 on both halves ────────────────────────────────────────────────
    let baseline: Uuid = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let version = Uuid::parse_str(&version_id).expect("uuid");
        let environment = json!({"model": "stub"});
        let run = store::eval::insert_run_tx(
            &mut tx,
            seeded.tenant,
            &store::eval::NewRun {
                eval_id: Uuid::parse_str(&eval_id).expect("uuid"),
                version_id: version,
                variant: "baseline",
                round: 0,
                mode: "full",
                reps: 1,
                case_count: CASES as i32,
                environment: &environment,
                cost_ceiling_cents: None,
            },
        )
        .await
        .expect("insert the baseline");
        tx.commit().await.expect("commit");
        run.id
    };
    finish_run(
        &pg,
        seeded.tenant,
        Uuid::parse_str(&version_id).expect("uuid"),
        baseline,
        0.4,
        0.4,
    )
    .await;

    // ── start the climb and sign its plan ───────────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": { "target": "label_accuracy", "direction": "higher" },
                "delta": DELTA,
                "reps": REPS,
                "approve_each_round": false,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
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
    let plan = body["output"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["gate"] == "eval_climb_plan")
        .expect("the plan gate")
        .clone();
    let (status, body) = running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({ "approval_id": plan["id"], "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    // The plan gate exists and is now decided. (`approvals.list` does not project `meta`, so the
    // goal/won't-change/stop payload is read where it is written, in `eval_climb_gate.rs`.)
    assert_eq!(plan["gate"], "eval_climb_plan");
    assert_eq!(plan["kind"], "eval_climb");

    // ── round 1: both halves up → keep ──────────────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "round 1: name the billing field in the prompt",
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
    let round1 = run_of(&pg, seeded.tenant, &eval_id, "v1").await;
    finish_run(
        &pg,
        seeded.tenant,
        Uuid::parse_str(&version_id).unwrap(),
        round1,
        0.8,
        0.8,
    )
    .await;

    // ── round 2: train up, held-out flat → revert ───────────────────────────────────────
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "round 2: add two worked examples to the prompt",
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
    assert_eq!(
        body["output"]["variant"], "v2",
        "the variant numbering continues"
    );
    let round2 = run_of(&pg, seeded.tenant, &eval_id, "v2").await;
    finish_run(
        &pg,
        seeded.tenant,
        Uuid::parse_str(&version_id).unwrap(),
        round2,
        1.0,
        0.85,
    )
    .await;

    // ⭐ A round's completion is announced when the NEXT round settles it, not only when the
    // climb is stopped. A live consumer watching `eval_climb.changed` would otherwise see nothing
    // between round 1 and the report — the event existed but only `stop` ever raised it.
    let completed: Vec<Value> = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        // The outbox row carries `{id, status}`: the round's run, and its decision.
        let rows: Vec<Value> = sqlx::query_scalar(
            "SELECT payload FROM event
              WHERE tenant_id = $1 AND type = 'eval_climb.round_completed' ORDER BY seq",
        )
        .bind(seeded.tenant.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .expect("read the events");
        tx.commit().await.expect("commit");
        rows
    };
    assert_eq!(
        completed.len(),
        1,
        "creating round 2 settles round 1 and announces it exactly once: {completed:?}"
    );
    assert_eq!(
        completed[0]["id"],
        round1.to_string(),
        "the event names the round's run: {completed:?}"
    );
    assert_eq!(completed[0]["status"], "keep", "{completed:?}");

    // ── stop: settle both rounds and write the report ───────────────────────────────────
    let (status, body) = running
        .invoke(
            "eval_climbs.stop",
            TOKEN,
            json!({ "id": climb_id, "reason": "two rounds are enough to show the loop" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "stopped");
    assert_eq!(
        body["output"]["best_round"], 1,
        "round 1 is the winner: it is the only kept round{}",
        body
    );

    let report = body["output"]["narrative_md"]
        .as_str()
        .expect("the report is on the climb");
    assert!(
        report.contains("| 1 | v1 |"),
        "the status table lists round 1: {report}"
    );
    assert!(report.contains("| 2 | v2 |"), "and round 2: {report}");
    assert!(
        report.contains("one_off") || report.contains("held_out_flat"),
        "the revert's reason is named in the table: {report}"
    );
    assert!(
        report.contains("Winner: round 1"),
        "the report names the winner: {report}"
    );

    // The decisions are on the runs, not only in the prose.
    let first = decision_of(&pg, seeded.tenant, round1).await;
    let second = decision_of(&pg, seeded.tenant, round2).await;
    assert_eq!(first["decision"], "keep", "{first}");
    let first_test = first["test"].as_f64().expect("a number");
    assert!((first_test - 0.8).abs() < 1e-9, "{first}");
    assert_eq!(second["decision"], "revert", "{second}");
    assert_eq!(
        second["reason"], "held_out_flat",
        "train up, held out flat is the shape of fitting the visible set: {second}"
    );
    let second_test = second["test"].as_f64().expect("a number");
    assert!(
        (second_test - 0.85).abs() < 1e-9,
        "the recorded number is the held-out mean the rows produce: {second}"
    );

    // ── the reversal: the same rows give the same decisions on a fresh read ─────────────
    let (status, body) = running
        .invoke(
            "eval_climbs.get",
            TOKEN,
            json!({ "id": climb_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rounds = body["output"]["rounds"].as_array().expect("rounds");
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0]["decision"], "keep", "{}", body);
    assert_eq!(rounds[1]["decision"], "revert", "{}", body);
}
