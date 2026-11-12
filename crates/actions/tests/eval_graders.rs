//! S19 Part B step 11 — the grader vocabulary at the action layer, and the human queue.
//!
//! The four kinds' **behaviour** is proved where it belongs: `apps/server/tests/eval_runner.rs`
//! runs `programmatic` against a real subject, and
//! `apps/server/tests/eval_runner_properties.rs::a_grader_that_cannot_decide_is_a_grader_failure_not_a_zero`
//! proves the judge guard refuses before a token is spent.
//!
//! What can only be proved through the action is the **vocabulary**: a `grader_kind` the database
//! would refuse, a rubric with no rubric text, and the one kind that parks a row for a person
//! instead of scoring it.

mod support;

use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn eval_with_version(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "grader vocabulary",
                "purpose": "which graders this instrument admits",
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

fn metric(
    version_id: &str,
    key: &str,
    grader_kind: &str,
    rubric: Option<&str>,
) -> serde_json::Value {
    let mut body = json!({
        "version_id": version_id,
        "key": key,
        "role": "quality",
        "kind": "binary",
        "better": "higher",
        "label": key,
        "grader_kind": grader_kind,
        "grader": {"kind": "exact"},
    });
    if let Some(rubric) = rubric {
        body["rubric"] = json!(rubric);
    }
    body
}

#[tokio::test]
async fn the_metric_vocabulary_refuses_what_the_database_would() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval, version_id) = eval_with_version(&running).await;

    // A kind outside the CHECK's five is refused at the action, with the field named.
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            metric(&version_id, "made_up", "vibes", None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert!(
        body["errors"][0]["field"] == "grader_kind",
        "the refusal names the field to fix: {body}"
    );

    // A `rubric` grader without rubric text is refused: the CHECK is the guarantee, and the
    // action's answer must be the same 422 rather than a 500 from the database.
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            metric(&version_id, "no_rubric", "rubric", None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"][0]["field"] == "rubric",
        "the refusal names the missing text: {body}"
    );

    // The four usable kinds are accepted, one by one, on their own key.
    for (key, kind, rubric) in [
        // Labels are capped at 14 characters — the column is a display label, not prose.
        ("prog_ok", "programmatic", None),
        ("rubric_ok", "rubric", Some("Score the answer 0..1.")),
        ("pairwise_ok", "pairwise", Some("Which answer is better?")),
        ("human_ok", "human", None),
    ] {
        let (status, body) = running
            .invoke(
                "eval_metrics.create",
                TOKEN,
                metric(&version_id, key, kind, rubric),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{key}: {body}");
        assert_eq!(body["output"]["grader_kind"], kind, "{body}");
    }
}

/// The human queue is `eval_results.list` filtered to `pending_label` — there is no second
/// surface, because a queue table would drift from the rows it queues.
#[tokio::test]
async fn a_human_metric_parks_its_result_and_the_queue_is_the_list() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = eval_with_version(&running).await;
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            metric(&version_id, "human_ok", "human", None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    // A case on the version, through the action, so the split column the view reads is real.
    let (status, body) = running
        .invoke(
            "eval_cases.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "input": "classify: refund",
                "tags": ["triage"],
                "expected": {"label": "billing"},
                "gold_provenance": "human_written",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let case_id = Uuid::parse_str(body["output"]["id"].as_str().expect("case id")).expect("uuid");

    // A run and a PARKED result. The driver writes these in production (`eval_runner.rs`); an
    // action cannot, because `eval_runs.start` refuses an unsigned instrument — which is the
    // point of a separate sign-off, not an obstacle to be routed around.
    let (run_id, result_id) = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let environment = json!({});
        let run = store::eval::insert_run_tx(
            &mut tx,
            seeded.tenant,
            &store::eval::NewRun {
                eval_id: Uuid::parse_str(&eval_id).expect("eval uuid"),
                version_id: Uuid::parse_str(&version_id).expect("version uuid"),
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
        .expect("insert the run")
        .id;
        let grade = json!({});
        let explanation = json!({});
        let usage = json!({});
        let result = store::eval::insert_case_result_tx(
            &mut tx,
            seeded.tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run,
                case_id,
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
        .expect("insert the parked result");
        tx.commit().await.expect("commit");
        (run, result)
    };

    // The queue: the parked row, and only the parked row.
    let (status, body) = running
        .invoke(
            "eval_results.list",
            TOKEN,
            json!({ "run_id": run_id, "status": "pending_label" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("a page");
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["id"], result_id.to_string(), "{body}");
    assert_eq!(items[0]["status"], "pending_label", "{body}");

    // The person scores it; the row leaves the queue and becomes `ok`.
    let (status, body) = running
        .invoke(
            "eval_results.label",
            TOKEN,
            json!({ "result_id": result_id, "human_score": 0.75, "note": "close enough" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["result"]["status"], "ok", "{body}");
    assert_eq!(
        body["output"]["result"]["human_score"], "0.75",
        "the person's score is kept as a decimal string, exact and not a float: {body}"
    );

    let (status, body) = running
        .invoke(
            "eval_results.list",
            TOKEN,
            json!({ "run_id": run_id, "status": "pending_label" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        body["output"]["items"]
            .as_array()
            .expect("a page")
            .is_empty(),
        "a labelled row is no longer in the queue: {body}"
    );
}
