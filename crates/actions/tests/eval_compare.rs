//! S20 step 5 — **`eval_climbs.compare`**: the paired delta, reproduced by hand.
//!
//! Two runs over one version, four cases each, filled with numbers chosen so every field of the
//! answer has an arithmetic derivation a reader can redo on paper:
//!
//! | ordinal | a | b | b − a |
//! |---|---|---|---|
//! | 0 | 1 | 1 | 0 |
//! | 1 | 1 | 0 | −1 |
//! | 2 | 0 | 1 | 1 |
//! | 3 | 0 | 1 | 1 |
//!
//! At a 0.5 resolution threshold that is `both = 1`, `a_only = 1`, `b_only = 2`,
//! `discordant = 3`. The exact two-sided McNemar on `(1, 2)` is `2·(C(3,0)+C(3,1))/2³ = 1.0`;
//! the mean paired difference is `0.25`; the sample SD is `√(2.75/3) = 0.957427…`, so the
//! 1.96-SE interval is `0.25 ∓ 0.938278…`.
//!
//! The second test is the refusal the owner ruling depends on: **a regrade is compared against
//! another regrade, never against the run it replaced** — two versions are two scales.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const CASES: usize = 4;

/// An eval with one version, four cases and one programmatic metric.
async fn draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "compare contract",
                "purpose": "the paired delta reproduces by hand",
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
    (eval_id, version_id)
}

/// A finished run whose cases score exactly `scores[ordinal]`.
async fn run_with_scores(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: &str,
    version_id: &str,
    variant: &str,
    scores: &[f64],
) -> Uuid {
    let eval_uuid = Uuid::parse_str(eval_id).expect("uuid");
    let version_uuid = Uuid::parse_str(version_id).expect("uuid");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(version_uuid)
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    assert_eq!(cases.len(), scores.len(), "one score per case");
    let environment = json!({});
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: eval_uuid,
            version_id: version_uuid,
            variant,
            round: 0,
            mode: "full",
            reps: 1,
            case_count: cases.len() as i32,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the run");
    let explanation = json!({});
    let usage = json!({"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2});
    for (case_id, score) in cases.iter().zip(scores) {
        let grade = json!({ "label_accuracy": score });
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
                latency_ms: 5,
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
    store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Running)
        .await
        .expect("to running");
    store::eval::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Complete)
        .await
        .expect("to complete");
    tx.commit().await.expect("commit");
    run.id
}

#[tokio::test]
async fn a_paired_delta_recomputes_by_hand() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = draft(&running).await;
    let a = run_with_scores(
        &pg,
        seeded.tenant,
        &eval_id,
        &version_id,
        "baseline",
        &[1.0, 1.0, 0.0, 0.0],
    )
    .await;
    let b = run_with_scores(
        &pg,
        seeded.tenant,
        &eval_id,
        &version_id,
        "v1",
        &[1.0, 0.0, 1.0, 1.0],
    )
    .await;

    let (status, body) = running
        .invoke(
            "eval_climbs.compare",
            TOKEN,
            json!({ "a_run_id": a, "b_run_id": b, "pass_threshold": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let out = &body["output"];
    assert_eq!(out["comparable"], true, "{body}");
    assert_eq!(out["metric"], "label_accuracy", "{body}");
    assert_eq!(out["both"], 1, "{body}");
    assert_eq!(out["neither"], 0, "{body}");
    assert_eq!(out["a_only"], 1, "{body}");
    assert_eq!(out["b_only"], 2, "{body}");
    assert_eq!(out["discordant"], 3, "{body}");
    assert_eq!(out["missing_in_one"], 0, "{body}");
    assert!(
        (out["p_value"].as_f64().expect("p") - 1.0).abs() < 1e-9,
        "exact two-sided McNemar on (1, 2) is 2·(1+3)/8 = 1.0: {body}"
    );
    assert!(
        (out["delta"].as_f64().expect("delta") - 0.25).abs() < 1e-9,
        "mean of [0, −1, 1, 1] is 0.25: {body}"
    );
    // Sample SD √(2.75/3) = 0.957427…; SE = SD/√4; 1.96·SE = 0.938278…
    assert!(
        (out["ci_low"].as_f64().expect("low") - (-0.688278)).abs() < 1e-5,
        "0.25 − 0.938278: {body}"
    );
    assert!(
        (out["ci_high"].as_f64().expect("high") - 1.188278).abs() < 1e-5,
        "0.25 + 0.938278: {body}"
    );
    assert_eq!(
        out["significant"], false,
        "p = 1.0 clears nothing, and three discordant cases cannot: {body}"
    );
}

#[tokio::test]
async fn compare_refuses_two_versions_or_graders_or_splits() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = draft(&running).await;
    let a = run_with_scores(
        &pg,
        seeded.tenant,
        &eval_id,
        &version_id,
        "baseline",
        &[1.0, 1.0, 0.0, 0.0],
    )
    .await;

    // A second version is a second instrument: same cases, different digest, and therefore a
    // different scale. This is the arm that makes "re-grade every variant, then compare" the only
    // legal way to compare across a grader fix.
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
    let second = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    let c = run_with_scores(
        &pg,
        seeded.tenant,
        &eval_id,
        &second,
        "v2",
        &[0.0, 0.0, 1.0, 1.0],
    )
    .await;

    let (status, body) = running
        .invoke(
            "eval_climbs.compare",
            TOKEN,
            json!({ "a_run_id": a, "b_run_id": c }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "not_comparable", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("version"),
        "the refusal names WHICH identity differs: {body}"
    );
}
