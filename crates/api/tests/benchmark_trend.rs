//! S18 build-order step 2 — `benchmark_runs.trend` over the real served API.
//!
//! Five properties, each a way the trend could lie:
//!
//! 1. **The delta is exact.** A promoted baseline and a candidate with a fixed paired pattern
//!    answer the exact McNemar p-value, not an approximation and not a re-derivation.
//! 2. **A non-comparable run is displayed, never averaged.** A different arm and a different
//!    oracle each produce a point with `comparable:false` and a reason. *(The reversal assertion:
//!    an implementation that plots everything fails here.)*
//! 3. **A control that missed is flagged and never compared** — `control_ok:false` on the point,
//!    and a refusal instead of a number.
//! 4. **No promoted baseline is an honest empty**, not an error: `baseline_run_id:null` and every
//!    `vs_baseline:null`.
//! 5. **Another suite never appears** — the trend is scoped to the suite it names.
//!
//! `rbuild -- test -p conveyor-api --test benchmark_trend`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::benchmark::{self, CaseOutcome, NewRun, SeedDocument, SeedInstance};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
/// The graded instances every run in this test measures.
const N: usize = 13;

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
        image_lock_sha256: "trend-lock".to_owned(),
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

/// The subject every `conveyor` run pins — the schema refuses `arm='conveyor'` on a `suite`
/// subject (`benchmark_run_arm_subject`), which is the point: a conveyor run measures an executor.
async fn subject(tx: &mut PgConnection, tenant: TenantId) -> Uuid {
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'trend', 'Trend', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("a capability");
    sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, max_iterations, timeout_secs)
         VALUES ($1, $2, 'trend-agent', 'tool_agent', 'active', 'claude-sonnet-5-5', 8, 300)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(capability)
    .fetch_one(&mut *tx)
    .await
    .expect("a capability agent")
}

/// One run to seed: what it measures, as whom, and which instances it resolves.
#[derive(Clone, Copy)]
struct Spec<'a> {
    arm: &'a str,
    label: &'a str,
    agent: Option<Uuid>,
    resolved: &'a [usize],
    control_ok: Option<bool>,
}

/// One finished, graded run. `spec.resolved` lists the instance indexes the grader resolved.
async fn run(tx: &mut PgConnection, tenant: TenantId, suite_id: Uuid, spec: &Spec<'_>) -> Uuid {
    let run = benchmark::create_run_tx(
        &mut *tx,
        tenant,
        &NewRun {
            suite_id,
            arm: spec.arm,
            subject_kind: if spec.agent.is_some() {
                "capability_agent"
            } else {
                "suite"
            },
            capability_agent_id: spec.agent,
            process_id: None,
            // S18 §3: a run pins the version it measured. These fixtures measure a subject id
            // only, so the pin is absent rather than guessed.
            subject_version: spec.agent.map(|_| 1),
            label: spec.label,
            options: &json!({"concurrency": 2}),
            instance_ids: &instance_ids(),
        },
    )
    .await
    .expect("create run");
    let detail = json!({"trend": true});
    for index in 0..N {
        let verdict = if spec.resolved.contains(&index) {
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
    benchmark::finish_run_tx(&mut *tx, tenant, run.id, "succeeded", None, spec.control_ok)
        .await
        .expect("finish the run");
    run.id
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn point(trend: &Value, run_id: Uuid) -> &Value {
    let wanted = run_id.to_string();
    trend["points"]
        .as_array()
        .expect("points")
        .iter()
        .find(|row| row["run_id"] == wanted.as_str())
        .unwrap_or_else(|| panic!("no point for {run_id} in {trend}"))
}

/// The four suites the properties need, seeded once.
struct Fixtures {
    suite: Uuid,
    baseline: Uuid,
    candidate: Uuid,
    other_arm: Uuid,
    other_oracle: Uuid,
    missed_control: Uuid,
}

async fn fixtures(pg: &support::Pg, tenant: TenantId) -> Fixtures {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let agent = subject(&mut tx, tenant).await;
    let (suite, _) = benchmark::seed_suite_tx(&mut tx, tenant, &document("trend-v1"))
        .await
        .expect("the trend suite");

    // The baseline resolves the first five; the candidate resolves all thirteen. Paired:
    // both = 5, candidate-only = 8, baseline-only = 0 → the exact two-sided McNemar p is
    // 2 · C(8,0)/2^8 = 0.0078125.
    let baseline = run(
        &mut tx,
        tenant,
        suite.id,
        &Spec {
            arm: "conveyor",
            label: "conveyor-oracle-files",
            agent: Some(agent),
            resolved: &[0, 1, 2, 3, 4],
            control_ok: None,
        },
    )
    .await;
    let candidate = run(
        &mut tx,
        tenant,
        suite.id,
        &Spec {
            arm: "conveyor",
            label: "conveyor-oracle-files",
            agent: Some(agent),
            resolved: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            control_ok: None,
        },
    )
    .await;
    let other_arm = run(
        &mut tx,
        tenant,
        suite.id,
        &Spec {
            arm: "gold",
            label: "control-gold",
            agent: None,
            resolved: &[0, 1, 2],
            control_ok: None,
        },
    )
    .await;
    let other_oracle = run(
        &mut tx,
        tenant,
        suite.id,
        &Spec {
            arm: "conveyor",
            label: "conveyor-oracle-files+failing-test",
            agent: Some(agent),
            resolved: &[0, 1, 2],
            control_ok: None,
        },
    )
    .await;
    let missed_control = run(
        &mut tx,
        tenant,
        suite.id,
        &Spec {
            arm: "conveyor",
            label: "conveyor-oracle-files",
            agent: Some(agent),
            resolved: &[0, 1, 2],
            control_ok: Some(false),
        },
    )
    .await;

    benchmark::promote_baseline_tx(&mut tx, tenant, suite.id, baseline)
        .await
        .expect("promote the baseline");
    tx.commit().await.expect("commit the trend fixture");

    Fixtures {
        suite: suite.id,
        baseline,
        candidate,
        other_arm,
        other_oracle,
        missed_control,
    }
}

async fn trend(running: &support::Running, suite: Uuid) -> Value {
    let (status, body) = running
        .invoke(
            "benchmark_runs.trend",
            TOKEN,
            json!({ "suite_id": suite, "limit": 20 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_trend_reads_each_point_against_the_promoted_baseline() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let f = fixtures(&pg, seeded.tenant).await;
    let view = trend(&running, f.suite).await;

    assert_eq!(
        view["baseline_run_id"],
        json!(f.baseline.to_string()),
        "the promoted baseline is read from the suite, never inferred: {view}"
    );
    assert_eq!(
        view["points"].as_array().expect("points").len(),
        5,
        "every succeeded run of the suite is a point: {view}"
    );

    // ── 1. the delta is exact ───────────────────────────────────────────────────────────
    let candidate = point(&view, f.candidate);
    let delta = &candidate["vs_baseline"];
    assert_eq!(delta["comparable"], json!(true), "{candidate}");
    assert_eq!(delta["a_only"], json!(8), "{candidate}");
    assert_eq!(delta["b_only"], json!(0), "{candidate}");
    assert_eq!(delta["discordant"], json!(8), "{candidate}");
    let p = delta["p_value"].as_f64().expect("a p-value");
    assert!(
        (p - 0.007_812_5).abs() < 1e-12,
        "the exact two-sided McNemar p for 8-0 is 0.0078125, got {p}"
    );
    assert_eq!(
        candidate["direction"],
        json!("candidate_better"),
        "{candidate}"
    );
    assert_eq!(candidate["rate"], json!(1.0), "{candidate}");

    // The baseline is its own point, and it is not compared with itself.
    let baseline = point(&view, f.baseline);
    assert!(
        baseline.get("vs_baseline").is_none(),
        "the baseline is not compared with itself: {baseline}"
    );

    // ── 2. a non-comparable run is displayed, never averaged ────────────────────────────
    let other_arm = point(&view, f.other_arm);
    assert_eq!(other_arm["arm"], json!("gold"), "{other_arm}");
    assert_eq!(other_arm["vs_baseline"]["comparable"], json!(false));
    assert!(
        other_arm["vs_baseline"]["p_value"].is_null(),
        "a refused comparison carries no p-value: {other_arm}"
    );
    assert!(
        other_arm["vs_baseline"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("control")),
        "the reason names what it is: {other_arm}"
    );
    assert!(
        other_arm.get("direction").is_none(),
        "a refused comparison has no direction: {other_arm}"
    );

    let other_oracle = point(&view, f.other_oracle);
    assert_eq!(other_oracle["arm"], json!("conveyor"), "{other_oracle}");
    assert_eq!(
        other_oracle["vs_baseline"]["comparable"],
        json!(false),
        "a different oracle is not comparable even on the same arm: {other_oracle}"
    );

    // ── 3. a control that missed is flagged and never compared ──────────────────────────
    let missed = point(&view, f.missed_control);
    assert_eq!(missed["control_ok"], json!(false), "{missed}");
    assert_eq!(
        missed["vs_baseline"]["comparable"],
        json!(false),
        "{missed}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_baseline_is_an_honest_empty_and_another_suite_never_appears() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let mut tx = begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let agent = subject(&mut tx, seeded.tenant).await;
    let (bare, _) = benchmark::seed_suite_tx(&mut tx, seeded.tenant, &document("trend-bare"))
        .await
        .expect("the bare suite");
    let (other, _) = benchmark::seed_suite_tx(&mut tx, seeded.tenant, &document("trend-other"))
        .await
        .expect("the other suite");
    let spec = Spec {
        arm: "conveyor",
        label: "conveyor-oracle-files",
        agent: Some(agent),
        resolved: &[0, 1],
        control_ok: None,
    };
    let lonely = run(&mut tx, seeded.tenant, bare.id, &spec).await;
    // Same shape, different suite: it must never leak into the bare suite's trend.
    let foreign = run(
        &mut tx,
        seeded.tenant,
        other.id,
        &Spec {
            resolved: &[0, 1, 2],
            ..spec
        },
    )
    .await;
    tx.commit().await.expect("commit the fixtures");

    let view = trend(&running, bare.id).await;

    // ── 4. no promoted baseline is an honest empty ──────────────────────────────────────
    assert!(
        view.get("baseline_run_id").is_none(),
        "no baseline is absent, not a made-up id: {view}"
    );
    let points = view["points"].as_array().expect("points");
    assert_eq!(points.len(), 1, "only the bare suite's own run: {view}");
    assert_eq!(points[0]["run_id"], json!(lonely.to_string()), "{view}");
    assert!(
        points[0].get("vs_baseline").is_none(),
        "nothing to compare against is absent, never a fabricated delta: {view}"
    );
    // ── 5. another suite never appears ──────────────────────────────────────────────────
    assert!(
        points
            .iter()
            .all(|row| row["run_id"] != json!(foreign.to_string())),
        "the other suite's run must not appear: {view}"
    );
    let rate = points[0]["rate"].as_f64().expect("a rate");
    assert!(
        (rate - 2.0 / 13.0).abs() < 1e-12,
        "the rate is n_resolved / n_graded: {view}"
    );
}
