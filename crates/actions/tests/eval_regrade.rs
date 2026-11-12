//! S20 B4 — **`eval_runs.regrade`**: a grader fix re-grades stored transcripts into a NEW run.
//!
//! The three properties this file proves over the wire:
//!
//! - **A regrade is a new run, never an edit.** It takes the next free `v<N>` (the variant
//!   numbering continues across climbs and regrades), writes `summary.regraded_from`, and leaves
//!   the source run's numbers untouched — so `eval_climbs.compare`'s refusal to mix versions
//!   still means something.
//! - **Slots are matched by `eval_case.ordinal`.** `eval_versions.create` clones cases with new
//!   ids and the same ordinals, so the id is not the identity across versions. An ordinal whose
//!   input changed is counted `not_regradable`, never graded against the wrong case.
//! - **The gates are the ones the owner ruled.** `evals:approve` (a regrade creates a run and can
//!   spend a judge call), only a finished source run, and only onto a *different* version.
//!
//! What the job does with a slot — grade the stored output, never re-invoke the subject — is
//! proved in `apps/server/tests/eval_runner.rs`, where the driver lives.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
/// Three cases: enough for an ordinal to be missing from the middle, small enough to read.
const CASES: usize = 3;

/// An eval with one version, `CASES` cases and one programmatic metric.
async fn draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "regrade contracts",
                "purpose": "a grader fix re-grades stored transcripts",
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

/// The source instrument: a finished baseline run whose every slot has a transcript that captured
/// its output and a result row.
async fn source(
    pg: &support::Pg,
    running: &support::Running,
    tenant: TenantId,
) -> (String, String, Uuid) {
    let (eval_id, version_id) = draft(running).await;
    let eval_uuid = Uuid::parse_str(&eval_id).expect("uuid");
    let version_uuid = Uuid::parse_str(&version_id).expect("uuid");
    let run_id = finished_run(pg, tenant, eval_uuid, version_uuid, CASES).await;
    (eval_id, version_id, run_id)
}

/// A run over the version's first `cases` cases, filled and finalised.
async fn finished_run(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: Uuid,
    version_id: Uuid,
    cases: usize,
) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let case_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal \
         LIMIT $3",
    )
    .bind(tenant.as_uuid())
    .bind(version_id)
    .bind(i64::try_from(cases).expect("small"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    let environment = json!({});
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
            case_count: case_ids.len() as i32,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the source run");
    let explanation = json!({});
    let usage = json!({"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6});
    for case_id in &case_ids {
        let turns = json!([
            {"role": "user", "content": "classify"},
            {"role": "assistant", "content": "billing", "output": {"label": "billing"}},
        ]);
        store::eval::insert_trace_tx(
            &mut tx,
            tenant,
            &store::eval::NewTrace {
                eval_run_id: run.id,
                case_id: *case_id,
                rep: 0,
                turns: &turns,
            },
        )
        .await
        .expect("insert the transcript");
        let grade = json!({ "label_accuracy": 1.0 });
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
                latency_ms: 9,
                tool_calls: 0,
                action_invocation_id: None,
                subject_run_id: None,
                judge_model: None,
                judge_usage: None,
                human_score: None,
            },
        )
        .await
        .expect("insert the result");
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

/// Create the next version of an eval and return its id.
async fn next_version(running: &support::Running, eval_id: &str) -> String {
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

/// The `eval.regrade` job rows of one run: `(dedupe_key, payload)`.
async fn regrade_jobs(pg: &support::Pg, tenant: TenantId, run_id: &str) -> Vec<(String, Value)> {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT dedupe_key, payload FROM job
          WHERE tenant_id = $1 AND kind = 'eval.regrade' AND dedupe_key LIKE $2
          ORDER BY dedupe_key",
    )
    .bind(tenant.as_uuid())
    .bind(format!("{run_id}:%"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the jobs");
    tx.commit().await.expect("commit");
    rows
}

#[tokio::test]
async fn a_regrade_creates_a_new_variant_and_one_job_per_stored_slot() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, source_version, source_run) = source(&pg, &running, seeded.tenant).await;
    let target_version = next_version(&running, &eval_id).await;

    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            TOKEN,
            json!({ "source_run_id": source_run, "change_md": "the label grader was case-sensitive" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = body["output"]["run"]["id"]
        .as_str()
        .expect("run id")
        .to_owned();
    assert_eq!(body["output"]["variant"], "v1", "{body}");
    assert_eq!(body["output"]["regradable_slots"], CASES, "{body}");
    assert_eq!(body["output"]["queued_jobs"], CASES, "{body}");
    assert_eq!(body["output"]["run"]["status"], "queued", "{body}");
    assert_eq!(
        body["output"]["run"]["version_id"], target_version,
        "a regrade measures the eval's CURRENT version: {body}"
    );
    assert_ne!(
        body["output"]["run"]["version_id"], source_version,
        "and that version must be a new one"
    );

    // One job per stored slot, carrying both ids: the target case to grade, the source case to
    // read the transcript from.
    let jobs = regrade_jobs(&pg, seeded.tenant, &run_id).await;
    assert_eq!(jobs.len(), CASES, "{jobs:?}");
    for (dedupe, payload) in &jobs {
        assert!(dedupe.starts_with(&run_id), "{dedupe}");
        assert_eq!(payload["run_id"].as_str(), Some(run_id.as_str()));
        assert_eq!(
            payload["source_run_id"].as_str(),
            Some(source_run.to_string().as_str()),
            "the job must name the run whose transcript it reads: {payload}"
        );
        assert!(payload["source_case_id"].is_string(), "{payload}");
    }

    // The new run is a run like any other: its variant is taken, so a second regrade resumes it.
    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            TOKEN,
            json!({ "source_run_id": source_run }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["run"]["id"], run_id, "{body}");
    assert_eq!(
        body["output"]["queued_jobs"], 0,
        "a repeat regrade is a resume, not a second variant: {body}"
    );
}

#[tokio::test]
async fn a_regrade_under_the_same_version_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval_id, _version_id, source_run) = source(&pg, &running, seeded.tenant).await;

    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            TOKEN,
            json!({ "source_run_id": source_run }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "version_unchanged", "{body}");
}

#[tokio::test]
async fn an_ordinal_whose_input_changed_is_skipped_and_counted() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _source_version, source_run) = source(&pg, &running, seeded.tenant).await;
    let target_version = next_version(&running, &eval_id).await;

    // Ordinal 1 is edited in the new version: the stored transcript answered a different question,
    // so grading it would be a number about a case nobody measured.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 AND ordinal = 1",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(Uuid::parse_str(&target_version).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("the case at ordinal 1");
    tx.commit().await.expect("commit");
    let (status, body) = running
        .invoke(
            "eval_cases.update",
            TOKEN,
            json!({
                "id": case_id,
                "input": "classify: a question the stored transcript never answered",
                "tags": ["triage"],
                "attachments": [],
                "source": "handwritten",
                "gold_provenance": "human_written",
                "meta": {},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            TOKEN,
            json!({ "source_run_id": source_run }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["regradable_slots"], CASES - 1, "{body}");
    let skipped = body["output"]["not_regradable"]
        .as_array()
        .expect("the skip list");
    assert_eq!(skipped.len(), 1, "{body}");
    assert_eq!(skipped[0]["ordinal"], 1, "{body}");
    assert_eq!(skipped[0]["reason"], "input_changed", "{body}");
}

#[tokio::test]
async fn an_unfinished_source_run_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = draft(&running).await;
    let source_run = {
        let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
        let environment = json!({});
        let run = store::eval::insert_run_tx(
            &mut tx,
            seeded.tenant,
            &store::eval::NewRun {
                eval_id: Uuid::parse_str(&eval_id).expect("uuid"),
                version_id: Uuid::parse_str(&version_id).expect("uuid"),
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
        .expect("insert the run");
        tx.commit().await.expect("commit");
        run.id
    };
    let _target = next_version(&running, &eval_id).await;

    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            TOKEN,
            json!({ "source_run_id": source_run }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "source_run_open", "{body}");
}

/// `evals:approve` is granted to a person and withheld from the compiled machine default: a
/// regrade creates a run and can spend a judge call, which is the budget decision.
#[tokio::test]
async fn the_machine_role_cannot_regrade() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _version_id, source_run) = source(&pg, &running, seeded.tenant).await;
    let _target = next_version(&running, &eval_id).await;

    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ('dev:robot', 'robot@dev.invalid', 'Robot', 'service') RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the machine user");
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'kata_executor')",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("membership");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "eval_runs.regrade",
            "dev:robot",
            json!({ "source_run_id": source_run }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "action-forbidden", "{body}");
}
