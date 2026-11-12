//! S20 step 6 — **stall triage**: a loop that has stopped moving must categorise before it spends
//! another round on content.
//!
//! Two predicates, both on the settled flat streak (`domain::eval::climb`):
//!
//! - after **two** consecutive rounds whose held-out delta does not clear the band, the next round
//!   must carry `buckets` — `422 buckets_required`;
//! - after **three**, a further content round is refused outright — the failure is the grader, the
//!   harness, the artifact's structure or variance, and a fourth content change cannot tell which.
//!
//! The climb declares `plateau_k = 5` on purpose: with the default 3 the *stopping predicate*
//! would fire first and the test would prove the wrong refusal. Both are real, and conflating them
//! is how "we plateaued" gets reported for a loop that was merely never allowed to categorise.

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
                "name": "stall triage",
                "purpose": "a wall is categorised, not ground",
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

/// A ready instrument: cases, one metric, both sign-offs, a drawn split.
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

/// Start a climb whose plateau fires at 5 — later than the triage wall the test is about.
async fn start_climb(running: &support::Running, eval_id: &str) -> (u16, Value) {
    running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": { "target": "label_accuracy", "direction": "higher" },
                "stopping": "plateau",
                "plateau_k": 5,
                "delta": DELTA,
                "reps": REPS,
                "approve_each_round": false,
            }),
            None,
            None,
            None,
        )
        .await
}

async fn plan_gate_id(running: &support::Running) -> String {
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
        .unwrap_or_else(|| panic!("no pending plan gate in {body}"));
    gate["id"].as_str().expect("approval id").to_owned()
}

async fn approve_plan(running: &support::Running) -> String {
    let approval_id = plan_gate_id(running).await;
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
    approval_id
}

/// A terminal run with one result per case, every one at `score` on both halves.
async fn fill_run(pg: &support::Pg, tenant: TenantId, version_id: &str, run_id: Uuid, score: f64) {
    let version_id = Uuid::parse_str(version_id).expect("version uuid");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    let grade = json!({ "label_accuracy": score });
    let explanation = json!({});
    let usage = json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15});
    for case_id in &cases {
        store::eval::insert_case_result_tx(
            &mut tx,
            tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run_id,
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
        .expect("insert a result");
    }
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running)
        .await
        .expect("to running");
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Complete)
        .await
        .expect("to complete");
    tx.commit().await.expect("commit");
}

/// The `baseline` run's id.
async fn baseline_run(pg: &support::Pg, tenant: TenantId, eval_id: &str, version_id: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: Uuid::parse_str(eval_id).expect("uuid"),
            version_id: Uuid::parse_str(version_id).expect("uuid"),
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: REPS,
            case_count: CASES as i32,
            environment: &json!({}),
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the baseline");
    tx.commit().await.expect("commit");
    run.id
}

async fn run_of(pg: &support::Pg, tenant: TenantId, eval_id: &str, variant: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_run WHERE tenant_id = $1 AND eval_id = $2 AND variant = $3",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(eval_id).expect("uuid"))
    .bind(variant)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or_else(|error| panic!("no {variant} run: {error}"));
    tx.commit().await.expect("commit");
    id
}

/// One content round with its pre-registered prediction.
async fn content_round(running: &support::Running, climb_id: &str, n: usize) -> (u16, Value) {
    running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": format!("round {n}: tighten the label prompt"),
                "change_patch": format!("--- a/prompt\n+++ b/prompt\n@@ round {n}"),
                "prediction": {
                    "metric": "label_accuracy",
                    "direction": "higher",
                    "interval": [DELTA, 0.5],
                    "falsifier": "held-out accuracy does not move by the band",
                },
            }),
            None,
            None,
            None,
        )
        .await
}

#[tokio::test]
async fn a_fourth_content_round_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running).await;
    let (status, body) = start_climb(&running, &eval_id).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    approve_plan(&running).await;
    let baseline = baseline_run(&pg, seeded.tenant, &eval_id, &version_id).await;
    fill_run(&pg, seeded.tenant, &version_id, baseline, 1.0).await;

    // ── two flat rounds: the loop has not moved by the band ──
    for n in 1..=2 {
        let (status, body) = content_round(&running, &climb_id, n).await;
        assert_eq!(status, 201, "round {n}: {body}");
        let run = run_of(&pg, seeded.tenant, &eval_id, &format!("v{n}")).await;
        fill_run(&pg, seeded.tenant, &version_id, run, 1.0).await;
    }

    // ── round 3 without a categorisation: refused, and it names what to carry ──
    let (status, body) = content_round(&running, &climb_id, 3).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "buckets_required", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("buckets"),
        "the refusal must name the payload to carry: {body}"
    );

    // ── round 3 AS a categorisation: accepted ──
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "buckets": [
                    {"bucket": "grader_disagreement", "count": 2,
                     "line": "the judge marks two paraphrases wrong"},
                    {"bucket": "variance", "count": 2,
                     "line": "both cases flip between reps"}
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run = run_of(&pg, seeded.tenant, &eval_id, "v3").await;
    fill_run(&pg, seeded.tenant, &version_id, run, 1.0).await;

    // ── a FOURTH content round against the same wall: refused ──
    let (status, body) = content_round(&running, &climb_id, 4).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"], "content_round_refused",
        "three flat rounds mean the failure is not content: {body}"
    );
}
