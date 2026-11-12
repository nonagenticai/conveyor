//! Tenant isolation and the seed/run semantics on S17's four tables (CONTRACT §2, §19).
//!
//! Four probes per table, the S03 shape, plus the contract rules the store must hold:
//! a suite is idempotent by `seed_sha256`, a re-seed with different hashes is a conflict,
//! answers are readable only through the grader projection, and a run's counters are derived.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_store::benchmark::{
    self, BenchmarkStoreError, CaseOutcome, NewRun, SeedDocument, SeedInstance,
};
use serde_json::json;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

const SEED_HASH: &str = "27421012ad8bbd712228c8a0be14a02bf82093d8777cf88a1c72b7ec2118ea0a";
const LOCK_HASH: &str = "ed5d1fabc5b835012a4969a7e4df7a1bbd947860dbb3aa99e6ab0ca89e8cb8f6";

fn instance(id: &str) -> SeedInstance {
    SeedInstance {
        instance_id: id.to_owned(),
        repo: "django/django".to_owned(),
        base_commit: "0".repeat(40),
        image: format!("registry.example.com:5000/swebench/{id}@sha256:abc"),
        problem_statement: "fix it".to_owned(),
        oracle_files: vec!["django/core.py".to_owned()],
        gold_patch: format!("gold-{id}"),
        test_patch: format!("test-{id}"),
        fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
        pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
        eval_script: format!("#!/bin/bash\necho {id}\n"),
        parser: "parse_log_django".to_owned(),
    }
}

fn doc(seed_sha256: &str) -> SeedDocument {
    SeedDocument {
        schema: 1,
        slug: "rls-v1".to_owned(),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "rev".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: LOCK_HASH.to_owned(),
        seed_sha256: seed_sha256.to_owned(),
        instances: vec![instance("repo__a-1"), instance("repo__b-2")],
    }
}

async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
        .bind(slug)
        .fetch_one(pool)
        .await
        .expect("tenant")
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("count {table}: {error}"))
}

fn instance_ids() -> Vec<String> {
    vec!["repo__a-1".to_owned(), "repo__b-2".to_owned()]
}

fn outcome<'a>(verdict: &'a str, detail: &'a serde_json::Value) -> CaseOutcome<'a> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_benchmark_tables_are_isolated_and_the_seed_is_immutable() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db.super_pool, "bench-a").await;
    let b = tenant(&db.super_pool, "bench-b").await;
    let ta = TenantId::from_uuid(a).expect("tenant");
    let tb = TenantId::from_uuid(b).expect("tenant");

    // ── seed: created, idempotent, and immutable identity ────────────────────────────────
    let mut tx = conveyor_store::begin_scoped(&db.runtime, ta)
        .await
        .expect("scoped tx");
    let (suite, created) = benchmark::seed_suite_tx(&mut tx, ta, &doc(SEED_HASH))
        .await
        .expect("seed");
    assert!(created, "the first seed must create the suite");
    assert_eq!(suite.instance_count, 2);

    let (_suite, created) = benchmark::seed_suite_tx(&mut tx, ta, &doc(SEED_HASH))
        .await
        .expect("re-seed");
    assert!(!created, "the same seed_sha256 must be a no-op");

    let refused = benchmark::seed_suite_tx(&mut tx, ta, &doc("ffff"))
        .await
        .expect_err("a different seed_sha256 must be refused");
    assert!(
        matches!(refused, BenchmarkStoreError::SeedConflict { .. }),
        "expected SeedConflict, got {refused:?}"
    );

    // ── answers are grader-only ──────────────────────────────────────────────────────────
    let grader = benchmark::grader_instance_tx(&mut tx, ta, suite.id, "repo__a-1")
        .await
        .expect("grader instance");
    assert!(grader.gold_patch.contains("gold-repo__a-1"));
    let producer = benchmark::producer_instances_tx(&mut tx, ta, suite.id, None)
        .await
        .expect("producer instances");
    assert_eq!(producer.len(), 2);
    assert_eq!(producer[0].oracle_files, vec!["django/core.py".to_owned()]);

    // ── one run: counters are derived from the cases ─────────────────────────────────────
    let ids = instance_ids();
    let run = benchmark::create_run_tx(
        &mut tx,
        ta,
        &NewRun {
            suite_id: suite.id,
            arm: "gold",
            subject_kind: "suite",
            capability_agent_id: None,
            process_id: None,
            subject_version: None,
            label: "control-gold",
            options: &json!({"concurrency": 2}),
            instance_ids: &ids,
        },
    )
    .await
    .expect("create run");
    assert_eq!(run.n, 2);
    assert_eq!(run.status, "queued");

    let detail = json!({"note": "banked"});
    benchmark::finalise_case_tx(
        &mut tx,
        ta,
        run.id,
        "repo__a-1",
        &outcome("resolved", &detail),
    )
    .await
    .expect("finalise resolved");
    benchmark::finalise_case_tx(
        &mut tx,
        ta,
        run.id,
        "repo__b-2",
        &outcome("unresolved", &detail),
    )
    .await
    .expect("finalise unresolved");
    let run = benchmark::refresh_run_counts_tx(&mut tx, ta, run.id)
        .await
        .expect("refresh counts");
    assert_eq!((run.n, run.n_graded, run.n_resolved), (2, 2, 1));
    assert_eq!(run.counts["resolved"], 1);

    // A re-delivered tick must not double-write a finalised case.
    let again = benchmark::finalise_case_tx(
        &mut tx,
        ta,
        run.id,
        "repo__a-1",
        &outcome("unresolved", &detail),
    )
    .await
    .expect_err("a finalised case is not pending");
    assert!(matches!(again, BenchmarkStoreError::NotFound { .. }));

    // A patch over the cap is refused before the CHECK can see it.
    let huge = "x".repeat(benchmark::MAX_PATCH_BYTES + 1);
    let over = benchmark::finalise_case_tx(
        &mut tx,
        ta,
        run.id,
        "repo__b-2",
        &CaseOutcome {
            produce_status: "ok",
            patch: Some(&huge),
            patch_sha256: None,
            verdict: "resolved",
            attempts: 1,
            seconds: None,
            sandbox_ref: None,
            log_document_id: None,
            detail: &detail,
        },
    )
    .await
    .expect_err("over-cap patch");
    assert!(matches!(over, BenchmarkStoreError::PatchTooLarge { .. }));

    // ── compare: the paired graded intersection ──────────────────────────────────────────
    let second = benchmark::create_run_tx(
        &mut tx,
        ta,
        &NewRun {
            suite_id: suite.id,
            arm: "baseline",
            subject_kind: "suite",
            capability_agent_id: None,
            process_id: None,
            subject_version: None,
            label: "baseline",
            options: &json!({}),
            instance_ids: &ids,
        },
    )
    .await
    .expect("create second run");
    benchmark::finalise_case_tx(
        &mut tx,
        ta,
        second.id,
        "repo__a-1",
        &outcome("unresolved", &detail),
    )
    .await
    .expect("second a");
    benchmark::finalise_case_tx(
        &mut tx,
        ta,
        second.id,
        "repo__b-2",
        &outcome("resolved", &detail),
    )
    .await
    .expect("second b");
    let pairs = benchmark::compare_cases_tx(&mut tx, ta, run.id, second.id)
        .await
        .expect("compare");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].a_verdict, "resolved");
    assert_eq!(pairs[0].b_verdict, "unresolved");
    let promoted = benchmark::promote_baseline_tx(&mut tx, ta, suite.id, second.id)
        .await
        .expect("promote");
    assert_eq!(promoted.baseline_run_id, Some(second.id));
    tx.commit().await.expect("commit");

    // ── isolation: B sees nothing of A, and a forged write is an RLS violation ───────────
    let mut txb = conveyor_store::begin_scoped(&db.runtime, tb)
        .await
        .expect("scoped B");
    assert!(
        benchmark::list_suites_tx(&mut txb, tb, None, 50)
            .await
            .expect("list B")
            .is_empty()
    );
    assert!(matches!(
        benchmark::get_suite_tx(&mut txb, tb, suite.id)
            .await
            .expect_err("B must not read A's suite"),
        BenchmarkStoreError::NotFound { .. }
    ));
    assert!(matches!(
        benchmark::get_run_tx(&mut txb, tb, run.id)
            .await
            .expect_err("B must not read A's run"),
        BenchmarkStoreError::NotFound { .. }
    ));
    let forged = sqlx::query(
        "INSERT INTO benchmark_suite
             (tenant_id, slug, dataset, dataset_revision, seed_sha256, image_lock_sha256,
              swebench_version, parser_version)
         VALUES ($1, 'forged', 'd', 'r', 's', 'i', '5.0.2', 'p')",
    )
    .bind(a)
    .execute(&mut *txb)
    .await;
    txb.rollback().await.expect("rollback");
    match forged {
        Ok(_) => panic!("the forged tenant_id was ACCEPTED"),
        Err(error) => assert_eq!(
            error
                .as_database_error()
                .and_then(|db| db.code())
                .as_deref(),
            Some("42501"),
            "a forged tenant_id must be an RLS violation, got {error}"
        ),
    }

    // ── the positive controls: FORCE RLS filters the owner; the superuser sees the rows ──
    for table in [
        "benchmark_suite",
        "benchmark_instance",
        "benchmark_run",
        "benchmark_case",
    ] {
        assert_eq!(
            count(&db.owner, table).await,
            0,
            "the owner must see no unscoped {table} rows (FORCE RLS)"
        );
    }
    assert_eq!(count(&db.super_pool, "benchmark_suite").await, 1);
    assert_eq!(count(&db.super_pool, "benchmark_instance").await, 2);
    assert_eq!(count(&db.super_pool, "benchmark_run").await, 2);
    assert_eq!(count(&db.super_pool, "benchmark_case").await, 4);
}
