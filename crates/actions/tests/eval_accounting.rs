//! S19 P1 — **one ledger-derived accounting model.**
//!
//! `eval_runs.report` prices the run's `inference_call` rows (subject, judge and billed failed
//! attempts); `eval_climbs.get` prices the same calls from the result rows it reads. Both run the
//! same `price_usage` arithmetic, so on a run whose ledger and rows describe the same calls the
//! two surfaces must agree **to the cent** — a cent of drift means a second accounting model
//! exists, which is exactly what W12 forbids.
//!
//! The fixture seeds both sides by hand: the result rows through the store (the driver's own
//! proof is `apps/server/tests/eval_runner.rs`) and the ledger through
//! `store::inference::insert_call_tx` with `caller_kind='eval'` and the run's id.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_domain::inference::Usage;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use conveyor_store::inference::NewInferenceCall;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const CASES: usize = 8;
/// Above `1/sqrt(4 held-out cases)`; the climb refuses a band inside the noise.
const DELTA: f64 = 0.6;
/// Every subject call in the agreement fixture: 1000 prompt + 500 completion tokens.
const PROMPT_TOKENS: i32 = 1000;
const COMPLETION_TOKENS: i32 = 500;

fn usage() -> Usage {
    Usage {
        prompt_tokens: PROMPT_TOKENS,
        completion_tokens: COMPLETION_TOKENS,
        total_tokens: PROMPT_TOKENS + COMPLETION_TOKENS,
        ..Usage::default()
    }
}

/// An eval, a version, eight cases, the `pass` metric, both sign-offs and a drawn split.
async fn ready_instrument(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "accounting fixture",
                "purpose": "the report and the climb price the same calls",
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
                "key": "pass",
                "role": "quality",
                "kind": "binary",
                "better": "higher",
                "label": "pass",
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
                support::REVIEWER,
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

/// Approve the climb's plan gate; no round exists before it.
async fn approve_plan(running: &support::Running) {
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
    let approval_id = body["output"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["gate"] == "eval_climb_plan" && item["kind"] == "eval_climb")
        .unwrap_or_else(|| panic!("no pending plan gate in {body}"))["id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    let (status, body) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// Insert a run in the caller's tenant and return its id.
async fn run_row(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: &str,
    version_id: &str,
    variant: &str,
    round: i32,
) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: Uuid::parse_str(eval_id).expect("eval uuid"),
            version_id: Uuid::parse_str(version_id).expect("version uuid"),
            variant,
            round,
            mode: "full",
            reps: 1,
            case_count: CASES as i32,
            environment: &json!({}),
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit");
    run.id
}

/// One scorable result per case, each carrying the usage the runner copied off the response,
/// then drive the run to `complete`.
async fn fill_run(pg: &support::Pg, tenant: TenantId, version_id: &str, run_id: Uuid) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("version uuid"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    let row_usage = json!({
        "prompt_tokens": PROMPT_TOKENS,
        "completion_tokens": COMPLETION_TOKENS,
        "total_tokens": PROMPT_TOKENS + COMPLETION_TOKENS,
    });
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
                grade: &json!({ "pass": 1.0 }),
                explanation: &json!({}),
                model: "stub-model",
                usage: &row_usage,
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

/// The ledger row the runner writes for one subject call of `run_id`.
async fn ledger_call(pg: &support::Pg, tenant: TenantId, run_id: Uuid) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    store::inference::insert_call_tx(
        &mut tx,
        tenant,
        &NewInferenceCall {
            action_invocation_id: None,
            trace_id: None,
            correlation_id: None,
            caller_kind: "eval",
            run_id: None,
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: Some(run_id),
            prompt_ref: Some("eval:completions.create"),
            prompt_version: None,
            prompt_sha256: None,
            model: "stub-model",
            served_model: None,
            usage: Some(usage()),
            cache_status: "miss",
            usage_unit: None,
            usage_units: None,
            latency_ms: 3,
            outcome: "ok",
        },
    )
    .await
    .expect("insert a ledger row");
    tx.commit().await.expect("commit");
}

async fn set_prices(pg: &support::Pg, tenant: TenantId, version_id: &str) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("UPDATE eval_version SET prices = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(Uuid::parse_str(version_id).expect("version uuid"))
        .bind(json!({ "input": 3.0, "output": 15.0 }))
        .execute(&mut *tx)
        .await
        .expect("write the prices");
    tx.commit().await.expect("commit");
}

async fn run_of(pg: &support::Pg, tenant: TenantId, eval_id: &str, variant: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_run WHERE tenant_id = $1 AND eval_id = $2 AND variant = $3",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(eval_id).expect("eval uuid"))
    .bind(variant)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or_else(|error| panic!("no {variant} run: {error}"));
    tx.commit().await.expect("commit");
    id
}

fn close(actual: &Value) -> f64 {
    actual.as_f64().expect("a number")
}

#[tokio::test]
async fn the_report_and_the_climb_agree_to_the_cent_on_the_same_model() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = ready_instrument(&running).await;
    set_prices(&pg, seeded.tenant, &version_id).await;

    // ── the climb: quality goal, plan signed, a baseline to measure against ──
    let (status, body) = running
        .invoke(
            "eval_climbs.start",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "goal": { "target": "pass", "direction": "higher" },
                "stopping": "plateau",
                "plateau_k": 5,
                "delta": DELTA,
                "reps": 1,
                "approve_each_round": false,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let climb_id = body["output"]["id"].as_str().expect("climb id").to_owned();
    approve_plan(&running).await;

    let baseline = run_row(&pg, seeded.tenant, &eval_id, &version_id, "baseline", 0).await;
    fill_run(&pg, seeded.tenant, &version_id, baseline).await;

    // ── round 1 creates the `v1` run; fill it and write its ledger ──
    let (status, body) = running
        .invoke(
            "eval_climbs.round",
            TOKEN,
            json!({
                "id": climb_id,
                "change_md": "round 1: the same instrument, priced by both surfaces",
                "prediction": {
                    "metric": "pass",
                    "direction": "higher",
                    "interval": [DELTA, 0.9],
                    "falsifier": "the measured mean stays inside the noise band",
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let v1 = run_of(&pg, seeded.tenant, &eval_id, "v1").await;
    fill_run(&pg, seeded.tenant, &version_id, v1).await;
    for _ in 0..CASES {
        ledger_call(&pg, seeded.tenant, v1).await;
    }

    // ── the climb's spend, from the result rows ──
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
    let spend = close(&body["output"]["spend_cents"]);
    assert_eq!(body["output"]["spend_unit"], "cents", "{body}");

    // ── the report's cost, from the ledger ──
    let (status, body) = running
        .invoke(
            "eval_runs.report",
            TOKEN,
            json!({ "run_id": v1 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let cost = &body["output"]["cost"];
    assert_eq!(cost["unit"], "cents", "{cost}");
    let total = close(&cost["total"]);

    // Eight calls at (1000·3 + 500·15) / 10 000 = 1.05 cents each.
    assert!(
        (total - 8.4).abs() < 1e-9,
        "the ledger prices eight subject calls: {cost}"
    );
    assert!(
        (spend - total).abs() < 1e-9,
        "the report and the climb must agree to the cent: spend {spend} vs report {total}"
    );
    assert_eq!(cost["unpriced"], 0, "{cost}");
}
