//! S19 Part B step 12 — the served-model assertion and the human label.
//!
//! The **assertion** is proved where it can fire: the driver calls the subject and compares the
//! model the response names against `eval.model_ref`, and
//! `apps/server/tests/eval_runner.rs::the_served_model_is_the_one_the_run_named` shows a
//! substituted model failing the attempt as `served_model_mismatch`, permanently and without a
//! row. What is here is the function that comparison rests on, and the label contract that makes
//! a human number reconcilable with a judge's.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::served_model_matches;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// The contract, exactly as `served_model_matches` documents it: the configured id, or that id
/// followed by a separator and a build suffix. Nothing else.
#[test]
fn a_served_model_matches_its_id_and_a_build_suffix_and_nothing_else() {
    assert!(served_model_matches(
        "claude-sonnet-5-5",
        "claude-sonnet-5-5"
    ));
    assert!(
        served_model_matches("claude-sonnet-5-5", "claude-sonnet-5-5-20260115"),
        "a router that pins a dated build is the same model, not a substitution"
    );
    assert!(
        served_model_matches("claude-opus-5-5", "claude-opus-5-5@latest"),
        "`@` is the other separator the contract admits"
    );
    assert!(
        !served_model_matches("claude-sonnet-5-5", "claude-opus-5-5"),
        "a different model is a substitution and must fail the attempt"
    );
    assert!(
        !served_model_matches("claude-sonnet-5-5", "gpt-5"),
        "and a different family certainly is"
    );
    // ⚠️ A PROVIDER-PREFIXED response does not match, and that is the documented contract: the
    // prefix makes the served id a different string, and guessing that `vendor/id` means `id`
    // is exactly the silent substitution trap 7 forbids. Recorded here so nobody "fixes" the
    // test into asserting the opposite.
    assert!(
        !served_model_matches("claude-opus-5-5", "anthropic/claude-opus-5-5"),
        "a provider prefix is NOT stripped — the assertion is deliberately literal"
    );
    // An empty side never matches: an unnamed model cannot be the one that was asked for.
    assert!(!served_model_matches("", "claude-sonnet-5-5"));
    assert!(!served_model_matches("claude-sonnet-5-5", ""));
}

/// A parked row, seeded the way the driver writes one.
async fn parked(pg: &support::Pg, tenant: TenantId) -> (Uuid, Uuid) {
    let inputs_schema = json!({"type": "object"});
    let grader = json!({"kind": "human"});
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let eval = store::eval::insert_eval_tx(
        &mut tx,
        tenant,
        &store::eval::NewEval {
            name: "label contract",
            purpose: "a person scores a parked row",
            subject_kind: "completions",
            entry_action: "completions.create",
            model_ref: Some("eval-model"),
            prompt_ref: None,
            inputs_schema: &inputs_schema,
            trace_source: "synthesized",
            created_by: None,
        },
    )
    .await
    .expect("insert eval");
    let version = store::eval::create_version_tx(&mut tx, tenant, eval.id, None)
        .await
        .expect("insert version");
    store::eval::insert_metric_tx(
        &mut tx,
        tenant,
        &store::eval::NewEvalMetric {
            version_id: version.id,
            key: "human_quality",
            role: "quality",
            kind: "binary",
            better: "higher",
            label: "human quality",
            unit: None,
            grader_kind: "human",
            grader: &grader,
            rubric: None,
            ordinal: 0,
        },
    )
    .await
    .expect("insert metric");
    let input = json!({"messages": [{"role": "user", "content": "classify: refund"}]}).to_string();
    let expected = json!("billing");
    let tags = vec!["triage".to_owned()];
    let attachments = json!([]);
    let meta = json!({});
    let case = store::eval::insert_case_tx(
        &mut tx,
        tenant,
        &store::eval::NewEvalCase {
            version_id: version.id,
            input: input.as_str(),
            tags: &tags,
            expected: Some(&expected),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("insert case");
    let environment = json!({});
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: eval.id,
            version_id: version.id,
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: 1,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert run");
    let grade = json!({});
    let explanation = json!({});
    let usage = json!({});
    let result = store::eval::insert_case_result_tx(
        &mut tx,
        tenant,
        &store::eval::NewCaseResult {
            eval_run_id: run.id,
            case_id: case.id,
            rep: 0,
            status: "pending_label",
            stop_reason: None,
            grade: &grade,
            explanation: &explanation,
            model: "eval-model",
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
    .expect("insert result");
    tx.commit().await.expect("commit");
    (run.id, result)
}

#[tokio::test]
async fn a_label_is_a_number_in_range_a_once_only_release() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_run_id, result_id) = parked(&pg, seeded.tenant).await;

    // Outside `0..=1` is refused: a human score that is not a probability would silently skew
    // every agreement figure the calibration rests on.
    let (status, body) = running
        .invoke(
            "eval_results.label",
            TOKEN,
            json!({ "result_id": result_id, "human_score": 1.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["field"], "human_score", "{body}");

    let (status, body) = running
        .invoke(
            "eval_results.label",
            TOKEN,
            json!({ "result_id": result_id, "human_score": 1.0, "note": "clearly billing" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["result"]["status"], "ok", "{body}");
    assert_eq!(
        body["output"]["result"]["human_score"], "1",
        "the decimal is exact and unadorned: {body}"
    );

    // A second label on an `ok` row would overwrite the number the report already used.
    let (status, body) = running
        .invoke(
            "eval_results.label",
            TOKEN,
            json!({ "result_id": result_id, "human_score": 0.0 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "not_pending_label", "{body}");
}
