//! S18 step 3 — the improvement job performs the same calls a person does.
//!
//! Three properties, each a way the loop could become a second runtime:
//!
//! 1. **The candidate is observed through `benchmark_runs.get`.** The job records a verdict on a
//!    terminal run and leaves an `action_invocation` row for the read and the comparison.
//! 2. **The verdict is the action's.** A candidate that resolves six paired cases the baseline
//!    missed is `keep` at the exact McNemar p, with the baseline id recorded on the candidate.
//! 3. **A candidate may be started through `benchmark_runs.start`.** A payload that names a
//!    subject instead of a run id starts the run through the public action and schedules its own
//!    next look — no private store write and no second state machine.
//!
//! `rbuild -- test -p conveyor-server --test benchmark_improvement`.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::benchmark_improvement::run;
use conveyor_store::begin_scoped;
use conveyor_store::benchmark::{self, CaseOutcome, NewRun, SeedDocument, SeedInstance};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

/// One paired pattern: the candidate resolves every case the baseline misses.
const N: usize = 6;
const APP_KEY: &str = "conveyor";
const LABEL: &str = "conveyor-oracle-files";

fn instance(index: usize) -> SeedInstance {
    let id = format!("repo__i-{index}");
    SeedInstance {
        instance_id: id.clone(),
        repo: "django/django".to_owned(),
        base_commit: "0".repeat(40),
        image: format!("registry.example.com:5000/swebench/{id}@sha256:abc"),
        problem_statement: "fix it".to_owned(),
        oracle_files: vec!["django/core.py".to_owned()],
        gold_patch: format!("gold-{index}"),
        test_patch: format!("test-{index}"),
        fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
        pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
        eval_script: format!("#!/bin/bash\necho {index}\n"),
        parser: "parse_log_django".to_owned(),
    }
}

fn document(slug: &str) -> SeedDocument {
    SeedDocument {
        schema: 1,
        slug: slug.to_owned(),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "rev".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: "improvement-lock".to_owned(),
        seed_sha256: format!("{slug}-seed"),
        instances: (0..N).map(instance).collect(),
    }
}

fn instance_ids() -> Vec<String> {
    (0..N).map(|index| format!("repo__i-{index}")).collect()
}

fn outcome<'a>(verdict: &'a str, detail: &'a Value) -> CaseOutcome<'a> {
    CaseOutcome {
        produce_status: "ok",
        patch: Some("diff --git a/x b/x\n"),
        patch_sha256: None,
        verdict,
        attempts: 1,
        seconds: None,
        sandbox_ref: None,
        log_document_id: None,
        detail,
    }
}

struct Fixture {
    tenant: TenantId,
    suite: Uuid,
    agent: Uuid,
    baseline: Uuid,
}

/// One finished run. `resolved` lists the instance indexes the grader resolved.
async fn finished_run(
    tx: &mut PgConnection,
    tenant: TenantId,
    suite: Uuid,
    agent: Uuid,
    resolved: &[usize],
) -> Uuid {
    let run = benchmark::create_run_tx(
        &mut *tx,
        tenant,
        &NewRun {
            suite_id: suite,
            arm: "conveyor",
            subject_kind: "capability_agent",
            capability_agent_id: Some(agent),
            process_id: None,
            subject_version: Some(1),
            label: LABEL,
            options: &json!({"concurrency": 2}),
            instance_ids: &instance_ids(),
        },
    )
    .await
    .expect("create run");
    let detail = json!({"improvement": true});
    for index in 0..N {
        let verdict = if resolved.contains(&index) {
            "resolved"
        } else {
            "unresolved"
        };
        benchmark::finalise_case_tx(
            &mut *tx,
            tenant,
            run.id,
            &format!("repo__i-{index}"),
            &outcome(verdict, &detail),
        )
        .await
        .expect("finalise the case");
    }
    benchmark::refresh_run_counts_tx(&mut *tx, tenant, run.id)
        .await
        .expect("refresh the counts");
    benchmark::finish_run_tx(&mut *tx, tenant, run.id, "succeeded", None, Some(true))
        .await
        .expect("finish the run");
    run.id
}

async fn fixture(db: &common::TestDb) -> Fixture {
    let slug = format!("improvement-{}", Uuid::now_v7());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");

    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('improvement actor') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("actor");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant_id)
        .bind(actor)
        .execute(&db.super_pool)
        .await
        .expect("the tenant's actor");

    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("a scoped transaction");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'improvement', 'Improvement', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("a capability");
    // SPEC_V2 §7.7 (#697): an active executor must name a resolvable prompt.
    let (improvement_prompt,): (Uuid,) = sqlx::query_as(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'improvement_prompt', 1, 'active', 'Improvement prompt') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the improvement prompt");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 1, 'system', true, 'Improve the benchmark arm.')",
    )
    .bind(tenant_id)
    .bind(improvement_prompt)
    .execute(&mut *tx)
    .await
    .expect("the improvement prompt section");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'improvement-agent', 'tool_agent', 'active', 'test-model',
                 'improvement_prompt@1', 8, 300)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(capability)
    .fetch_one(&mut *tx)
    .await
    .expect("a capability agent");
    let (suite, _) = benchmark::seed_suite_tx(
        &mut tx,
        tenant,
        &document(&format!("improvement-{tenant_id}")),
    )
    .await
    .expect("the suite");
    let baseline = finished_run(&mut tx, tenant, suite.id, agent, &[]).await;
    tx.commit().await.expect("commit the fixture");

    Fixture {
        tenant,
        suite: suite.id,
        agent,
        baseline,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_loop_reads_and_compares_through_the_public_actions() {
    let db = common::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.runtime, f.tenant)
        .await
        .expect("a scoped transaction");
    let candidate = finished_run(
        &mut tx,
        f.tenant,
        f.suite,
        f.agent,
        &(0..N).collect::<Vec<_>>(),
    )
    .await;
    tx.commit().await.expect("commit the candidate");

    run(
        &db.runtime,
        f.tenant,
        APP_KEY,
        &json!({"run_id": candidate, "baseline_run_id": f.baseline}),
    )
    .await
    .expect("the improvement job");

    let mut tx = begin_scoped(&db.runtime, f.tenant)
        .await
        .expect("a scoped transaction");
    let after = benchmark::get_run_tx(&mut tx, f.tenant, candidate)
        .await
        .expect("the candidate");
    assert_eq!(
        after.improvement_verdict.as_deref(),
        Some("keep"),
        "six discordant resolutions are below alpha: {after:?}"
    );
    assert_eq!(after.improvement_of_run_id, Some(f.baseline));
    assert!(
        after
            .improvement_note
            .as_deref()
            .is_some_and(|note| note.contains("p = 0.0312")),
        "the exact McNemar p is recorded: {:?}",
        after.improvement_note
    );

    let reads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation
          WHERE tenant_id = $1 AND action = 'benchmark_runs.get'",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("the read invocation");
    let compares: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation
          WHERE tenant_id = $1 AND action = 'benchmark_runs.compare'",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("the compare invocation");
    assert!(reads >= 1, "the loop observed through benchmark_runs.get");
    assert!(
        compares >= 1,
        "the loop judged through benchmark_runs.compare"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_candidate_can_be_started_through_the_public_action() {
    // The sandbox check is fail-closed at the door, so the start path needs it configured.
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        std::env::set_var("SANDBOX_NAMESPACE", "swe-bench");
    });

    let db = common::TestDb::fresh().await;
    let f = fixture(&db).await;

    run(
        &db.runtime,
        f.tenant,
        APP_KEY,
        &json!({
            "suite_id": f.suite,
            "arm": "conveyor",
            "subject_kind": "capability_agent",
            "capability_agent_id": f.agent,
            "label": "candidate-started-by-the-loop",
            "baseline_run_id": f.baseline,
        }),
    )
    .await
    .expect("the improvement job starts the candidate");

    let candidate: Uuid = sqlx::query_scalar(
        "SELECT id FROM benchmark_run
          WHERE tenant_id = $1 AND label = 'candidate-started-by-the-loop'",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("the started run");
    let status: String =
        sqlx::query_scalar("SELECT status FROM benchmark_run WHERE tenant_id = $1 AND id = $2")
            .bind(f.tenant.as_uuid())
            .bind(candidate)
            .fetch_one(&db.super_pool)
            .await
            .expect("the started run's status");
    assert_eq!(status, "queued", "the action created the candidate");

    let starts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation
          WHERE tenant_id = $1 AND action = 'benchmark_runs.start'",
    )
    .bind(f.tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("the start invocation");
    assert!(starts >= 1, "the loop started it through the action");

    let ticks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'benchmark.improvement.run'
            AND payload->>'run_id' = $2",
    )
    .bind(f.tenant.as_uuid())
    .bind(candidate.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("the loop's next look");
    assert_eq!(ticks, 1, "a queued candidate gets exactly one next look");
}
