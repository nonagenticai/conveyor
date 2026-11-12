//! S19 Part C step 15 — the interop round trip.
//!
//! `eval_runs.export` writes the method's bundle as one gzip JSONL document: a run header, one
//! line per result, one per trace. `eval_cases.import` brings case *inputs* back in. What this
//! test pins is the contract between them: the bundle carries no answer key, and an import
//! creates cases only — no result and no attempt is ever claimed by a copy.

mod support;

use std::io::Read as _;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// An eval, one version, two cases with an answer key and one binary metric.
async fn instrument(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "roundtrip",
                "purpose": "the bundle goes out and the cases come back",
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
    let version_id = create_version(running, &eval_id).await;
    for n in 0..2 {
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
    (eval_id, version_id)
}

async fn create_version(running: &support::Running, eval_id: &str) -> String {
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
    body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned()
}

/// A completed run over the version's cases, with one scored slot per case.
async fn seed_run(pg: &support::Pg, tenant: TenantId, eval_id: &str, version_id: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: Uuid::parse_str(eval_id).expect("eval uuid"),
            version_id: Uuid::parse_str(version_id).expect("version uuid"),
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: 2,
            environment: &json!({}),
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the run");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("version uuid"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    for case in &cases {
        let grade = json!({ "pass": 1.0 });
        let explanation = json!({});
        let usage = json!({ "total_tokens": 4 });
        store::eval::insert_case_result_tx(
            &mut tx,
            tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run.id,
                case_id: *case,
                rep: 0,
                status: "ok",
                stop_reason: None,
                grade: &grade,
                explanation: &explanation,
                model: "stub-model",
                usage: &usage,
                latency_ms: 2,
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
    tx.commit().await.expect("commit");
    run.id
}

#[tokio::test]
async fn the_bundle_goes_out_without_the_answer_key_and_the_cases_come_back() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = instrument(&running).await;
    let run_id = seed_run(&pg, seeded.tenant, &eval_id, &version_id).await;

    // ── out: one gzip JSONL document ──
    let (status, body) = running
        .invoke(
            "eval_runs.export",
            TOKEN,
            json!({ "id": run_id.to_string() }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let document_id = Uuid::parse_str(body["output"]["document_id"].as_str().expect("document id"))
        .expect("document uuid");
    assert_eq!(body["output"]["results"], 2, "{body}");

    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let loaded = conveyor_actions::documents::actions::fetch_bytes(
        &mut tx,
        seeded.tenant,
        document_id,
        "eval_roundtrip",
    )
    .await
    .expect("fetch the exported bundle");
    tx.commit().await.expect("commit");
    assert_eq!(loaded.content_type, "application/gzip");

    let mut jsonl = String::new();
    GzDecoder::new(loaded.bytes.as_slice())
        .read_to_string(&mut jsonl)
        .expect("the bundle is gzip JSONL");
    let lines: Vec<Value> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is a JSON object"))
        .collect();
    assert_eq!(lines[0]["kind"], "run", "{lines:?}");
    assert_eq!(
        lines.iter().filter(|line| line["kind"] == "result").count(),
        2
    );
    // ⛔ The answer key never leaves: the input case carried `expected`, and no byte of the
    // bundle may.
    assert!(
        !jsonl.contains("\"expected\""),
        "the bundle must not carry an answer key: {jsonl}"
    );

    // ── in: the same case inputs into a second version, cases only ──
    let target = create_version(&running, &eval_id).await;
    let (status, body) = running
        .invoke(
            "eval_cases.import",
            TOKEN,
            json!({
                "version_id": target,
                "cases": [
                    {"input": "classify: case 0", "tags": ["triage"], "expected": {"label": "billing"}},
                    {"input": "classify: case 1", "tags": ["triage"], "expected": {"label": "billing"}},
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        body["output"]["imported"]
            .as_array()
            .expect("imported")
            .len(),
        2
    );

    // An import is cases only: no run, no result, no attempt exists for the target version.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let (runs,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM eval_run WHERE tenant_id = $1 AND version_id = $2")
            .bind(seeded.tenant.as_uuid())
            .bind(Uuid::parse_str(&target).expect("version uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("count runs");
    tx.commit().await.expect("commit");
    assert_eq!(runs, 0, "importing cases never claims a measurement");
}
