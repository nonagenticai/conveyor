//! S20 step 7 — **the cost search**: a `cost_usd` climb under a quality guardrail.
//!
//! The gate the slice names is `no_round_without_registered_gates`, and the registration is the
//! **hold**: a cost goal with no quality floor is refused at `start`, the floor is carried on the
//! plan sign-off, and no round exists until that plan is approved.
//!
//! The rest of the file proves the search actually searches — three mechanisms that existed in
//! `domain::eval::climb` and were never wired into the action:
//!
//! - **cost is measured.** A cost round has no `cost_usd` grade key; while spend and holds were
//!   read only from rows carrying the *target's* grade, every cost round saw cost 0 and reverted
//!   `cost_not_cleared`, and the guardrail passed vacuously.
//! - **the staircase moves.** A kept round advances `goal.lever`; a reverted one does not. The
//!   lever was persisted on creation but never advanced, so a cost walk asked for
//!   `caching_health` forever.
//! - **the guardrail holds.** A cheaper round that drops quality below the floor is reverted
//!   `hold_broken`, however much money it saved.

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
/// The quality floor registered before round 1.
const HOLD_MIN: f64 = 0.8;

async fn empty_draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "cost walk",
                "purpose": "cheaper, never worse",
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

/// Start a cost climb, with or without the quality hold the gate is about.
async fn start_cost_climb(running: &support::Running, eval_id: &str, hold: bool) -> (u16, Value) {
    let mut goal = json!({ "target": "cost_usd", "direction": "lower" });
    if hold {
        goal["hold"] = json!([{ "metric": "label_accuracy", "min": HOLD_MIN }]);
    }
    running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": goal,
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
    items
        .iter()
        .find(|item| item["gate"] == "eval_climb_plan" && item["kind"] == "eval_climb")
        .unwrap_or_else(|| panic!("no pending plan gate in {body}"))["id"]
        .as_str()
        .expect("approval id")
        .to_owned()
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

/// The plan's `meta.goal.hold` — the gates the climb registered before round 1.
async fn registered_hold(pg: &support::Pg, tenant: TenantId, approval_id: &str) -> Value {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let meta: Value =
        sqlx::query_scalar("SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(Uuid::parse_str(approval_id).expect("uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("read the plan gate");
    tx.commit().await.expect("commit");
    meta["goal"]["hold"].clone()
}

/// A terminal run: every case at `score`, every case costing `tokens` tokens.
async fn fill_run(
    pg: &support::Pg,
    tenant: TenantId,
    version_id: &str,
    run_id: Uuid,
    score: f64,
    tokens: f64,
) {
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
    let usage = json!({"total_tokens": tokens});
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

async fn run_summary(pg: &support::Pg, tenant: TenantId, run_id: Uuid) -> Value {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
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

/// One cost round at `lever`.
async fn cost_round(
    running: &support::Running,
    climb_id: &str,
    lever: &str,
    n: usize,
) -> (u16, Value) {
    running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": format!("round {n} at {lever}"),
                "lever": lever,
                "prediction": {
                    "metric": "cost_usd",
                    "direction": "lower",
                    "interval": [DELTA, 0.5],
                    "falsifier": "cost per case does not fall by the band",
                },
            }),
            None,
            None,
            None,
        )
        .await
}

#[tokio::test]
async fn no_round_without_registered_gates() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running).await;

    // ── the guardrail IS the registration: a cost goal without it is refused at `start` ──
    let (status, body) = start_cost_climb(&running, &eval_id, false).await;
    assert_eq!(status, 422, "{body}");
    // The schema-level refusal carries its reason per field, not at the top level.
    assert!(
        body.to_string().contains("hold"),
        "the refusal names the missing guardrail: {body}"
    );

    let (status, body) = start_cost_climb(&running, &eval_id, true).await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();

    // ── and no round exists before the plan carrying it is signed ──
    let (status, body) = cost_round(&running, &climb_id, "caching_health", 1).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "plan_pending", "{body}");

    let approval_id = approve_plan(&running).await;
    let hold = registered_hold(&pg, seeded.tenant, &approval_id).await;
    assert_eq!(
        hold[0]["metric"], "label_accuracy",
        "the plan the person signs carries the floor the search must protect: {hold}"
    );
    assert_eq!(hold[0]["min"], HOLD_MIN, "{hold}");

    let baseline = baseline_run(&pg, seeded.tenant, &eval_id, &version_id).await;
    fill_run(&pg, seeded.tenant, &version_id, baseline, 1.0, 100.0).await;

    // ── skipping a lever is refused: the order exists because a cache fix costs nothing ──
    let (status, body) = cost_round(&running, &climb_id, "model_effort", 1).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "lever_order", "{body}");

    // ── round 1 at the first lever, half the tokens: a KEEP ──
    let (status, body) = cost_round(&running, &climb_id, "caching_health", 1).await;
    assert_eq!(status, 201, "{body}");
    let v1 = run_of(&pg, seeded.tenant, &eval_id, "v1").await;
    fill_run(&pg, seeded.tenant, &version_id, v1, 1.0, 50.0).await;

    // ── round 2 is what settles round 1, and the kept round moves the staircase ──
    let (status, body) = cost_round(&running, &climb_id, "prompt_audit", 2).await;
    assert_eq!(status, 201, "{body}");
    let summary = run_summary(&pg, seeded.tenant, v1).await;
    assert_eq!(
        summary["decision"], "keep",
        "cost fell 100 → 50 tokens and the hold held: {summary}"
    );
    assert_eq!(summary["cost_movement"], Value::Null, "{summary}");
    let v2 = run_of(&pg, seeded.tenant, &eval_id, "v2").await;
    // Ten tokens — a 5× saving — but quality drops below the registered floor.
    fill_run(&pg, seeded.tenant, &version_id, v2, 0.5, 10.0).await;

    // ── round 3 must name the SAME lever (round 2 will revert, so nothing advances) ──
    let (status, body) = cost_round(&running, &climb_id, "caching_health", 3).await;
    assert_eq!(
        status, 422,
        "the staircase is at prompt_audit; walking back to the top is skipping: {body}"
    );
    let (status, body) = cost_round(&running, &climb_id, "prompt_audit", 3).await;
    assert_eq!(status, 201, "{body}");
    let summary = run_summary(&pg, seeded.tenant, v2).await;
    assert_eq!(
        summary["decision"], "revert",
        "the guardrail is checked before any gain can be claimed: {summary}"
    );
    assert_eq!(
        summary["reason"], "hold_broken",
        "quality 0.5 is below the 0.8 floor the plan registered: {summary}"
    );
}
