//! S18 step 4 — the `conveyor` arm's door, over the real served API.
//!
//! Five properties, each a way the door could be wrong:
//!
//! 1. **A conveyor run must name a subject.** `arm='conveyor'` with `subject_kind='suite'` is a
//!    422 — the database's own CHECK agrees, and the API must say so first and by name.
//! 2. **A ready subject is measurable, and the run pins its version** (S18 §3: version pinning
//!    is the measurement, so the version is recorded at start, never joined at read time).
//! 3. **An unready binding is `409 arm_not_ready` before the first case** (S18 §5), not a run
//!    whose every case fails at the producer.
//! 4. **A process subject is the published `swebench-fix` or a `409 arm_not_ready`** — a missing
//!    id, a draft version and another published process all fail before the first case.
//! 5. **A baseline from another suite is refused**, and one from this suite enqueues the
//!    improvement job (S18 step 3's only entry point).
//!
//! `rbuild -- test -p conveyor-api --test benchmark_arm`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::benchmark::{self, SeedDocument, SeedInstance};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

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
        image_lock_sha256: "arm-lock".to_owned(),
        seed_sha256: format!("{slug}-seed"),
        instances: vec![instance(0), instance(1)],
    }
}

fn ids() -> Vec<String> {
    vec!["repo__i-0".to_owned(), "repo__i-1".to_owned()]
}

async fn agent(
    tx: &mut sqlx::PgConnection,
    tenant: TenantId,
    capability: Uuid,
    key: &str,
    status: &str,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, max_iterations, timeout_secs)
         VALUES ($1, $2, $3, 'tool_agent', $4, 'claude-sonnet-5-5', 8, 300) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(capability)
    .bind(key)
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .expect("a capability agent")
}

struct Fixture {
    suite: Uuid,
    other_suite: Uuid,
    active_agent: Uuid,
    retired_agent: Uuid,
    foreign_run: Uuid,
    /// The published `swebench-fix` version the process arm measures.
    swebench_fix: Uuid,
    /// A newer draft of the same key — never measurable.
    draft_fix: Uuid,
    /// A published process under another key — never the arm's producer.
    other_process: Uuid,
}

async fn fixture(pg: &support::Pg, tenant: TenantId) -> Fixture {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'arm', 'Arm', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("a capability");
    let active_agent = agent(&mut tx, tenant, capability, "arm-active", "active").await;
    let retired_agent = agent(&mut tx, tenant, capability, "arm-retired", "retired").await;

    // S18 P3a: the process arm resolves the published `swebench-fix` version and pins it; a
    // draft of the same key and a published process under another key are both refused.
    let swebench_fix: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version)
         VALUES ($1, 'swebench-fix', 'SWE-bench fix', 'published', 3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("the published swebench-fix process");
    let draft_fix: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version)
         VALUES ($1, 'swebench-fix', 'SWE-bench fix', 'draft', 4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("a draft swebench-fix process");
    let other_process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version)
         VALUES ($1, 'review-fix', 'Another process', 'published', 1) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("a published process under another key");

    let (suite, _) = benchmark::seed_suite_tx(&mut tx, tenant, &document("arm-v1"))
        .await
        .expect("the suite");
    let (other_suite, _) = benchmark::seed_suite_tx(&mut tx, tenant, &document("arm-v2"))
        .await
        .expect("the other suite");
    let foreign_run = benchmark::create_run_tx(
        &mut tx,
        tenant,
        &benchmark::NewRun {
            suite_id: other_suite.id,
            arm: "gold",
            subject_kind: "suite",
            capability_agent_id: None,
            process_id: None,
            subject_version: None,
            label: "control-gold",
            options: &json!({"concurrency": 1}),
            instance_ids: &ids(),
        },
    )
    .await
    .expect("a run in the other suite")
    .id;
    tx.commit().await.expect("commit the fixture");

    Fixture {
        suite: suite.id,
        other_suite: other_suite.id,
        active_agent,
        retired_agent,
        foreign_run,
        swebench_fix,
        draft_fix,
        other_process,
    }
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_conveyor_arm_refuses_what_it_cannot_measure() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("SANDBOX_NAMESPACE", "swe-bench");
    let running = support::Running::start(&pg).await;
    let f = fixture(&pg, seeded.tenant).await;

    // ── 1. a conveyor run must name a subject ───────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({"suite_id": f.suite, "arm": "conveyor", "label": "conveyor-oracle-files"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], json!("subject_kind"), "{body}");

    // ── 3. an unready binding is 409 before the first case ──────────────────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "capability_agent",
                "capability_agent_id": f.retired_agent,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(output(&body)["type"], json!("arm_not_ready"), "{body}");

    // ── 4. a process subject must name the published `swebench-fix` ─────────────────────
    // No id: the door cannot resolve a producer, so it refuses before the first case.
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "process",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(output(&body)["type"], json!("arm_not_ready"), "{body}");

    // A draft version of the right key is not the published producer.
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "process",
                "process_id": f.draft_fix,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(output(&body)["type"], json!("arm_not_ready"), "{body}");

    // A published process under another key is not this arm's producer.
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "process",
                "process_id": f.other_process,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(output(&body)["type"], json!("arm_not_ready"), "{body}");

    // ── 5a. a baseline from another suite is refused ────────────────────────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "capability_agent",
                "capability_agent_id": f.active_agent,
                "baseline_run_id": f.foreign_run,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["detail"],
        json!("the baseline run measured a different suite"),
        "{body}"
    );

    // ── a baseline names its model and starts; without one it is refused ────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "baseline",
                "label": "qwen-oneshot-oracle-files",
                "model": "claude-sonnet-5-5",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        output(&body)["options"]["model_ref"],
        json!("claude-sonnet-5-5"),
        "the baseline's model is pinned on the run: {body}"
    );

    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "baseline",
                "label": "qwen-oneshot-oracle-files",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], json!("model"), "{body}");
    let _ = f.other_suite;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ready_subject_is_measurable_and_the_run_pins_its_version() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("SANDBOX_NAMESPACE", "swe-bench");
    let running = support::Running::start(&pg).await;
    let f = fixture(&pg, seeded.tenant).await;

    // ── 2. an active executor is measurable, and the version is pinned ──────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "capability_agent",
                "capability_agent_id": f.active_agent,
                "concurrency": 1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run = output(&body);
    assert_eq!(run["subject_kind"], json!("capability_agent"), "{body}");
    assert_eq!(
        run["capability_agent_id"],
        json!(f.active_agent.to_string()),
        "{body}"
    );
    assert_eq!(
        run["subject_version"],
        json!(1),
        "the version the run measured is recorded at start: {body}"
    );
    assert_eq!(
        run["options"]["agent_snapshot"]["model_ref"],
        json!("claude-sonnet-5-5"),
        "the run freezes the configuration it measures: {body}"
    );
    assert_eq!(
        run["options"]["agent_snapshot"]["max_iterations"],
        json!(8),
        "the run freezes the loop caps: {body}"
    );

    // ── 2b. the process arm starts, and pins the published version it measures ──────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-swebench-fix",
                "subject_kind": "process",
                "process_id": f.swebench_fix,
                "concurrency": 1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let process_run = output(&body);
    assert_eq!(process_run["subject_kind"], json!("process"), "{body}");
    assert_eq!(
        process_run["process_id"],
        json!(f.swebench_fix.to_string()),
        "the run pins the process it measures: {body}"
    );
    assert_eq!(
        process_run["capability_agent_id"],
        json!(null),
        "a process subject names no capability agent: {body}"
    );
    assert_eq!(
        process_run["subject_version"],
        json!(3),
        "the process version the run measured is recorded at start: {body}"
    );

    // ── 5b. a baseline from this suite enqueues the improvement job ─────────────────────
    let (status, body) = running
        .invoke(
            "benchmark_runs.start",
            TOKEN,
            json!({
                "suite_id": f.suite,
                "arm": "conveyor",
                "label": "conveyor-oracle-files",
                "subject_kind": "capability_agent",
                "capability_agent_id": f.active_agent,
                "baseline_run_id": run["id"],
                "concurrency": 1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'benchmark.improvement.run'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the improvement jobs");
    assert_eq!(jobs, 1, "one candidate, one verdict job");
}
