//! The kata union's benchmark instrument (SPEC_V2 §6.3, D-T5-2; finding F-T5-1, #526).
//!
//! A kata may pre-register an eval version OR a benchmark suite. A benchmark instrument's
//! held-out guarantee is the suite's locked dataset (`seed_sha256` pinned at open); its
//! measurement reads two COMPLETE runs of that suite — the candidate on the `conveyor` arm, the
//! baseline on the `baseline` arm — and computes each side's resolved rate from the grader's
//! counts. A run from another suite, a control arm, or an unfinished run refuses.

mod support;

use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn instance(id: &str) -> store::benchmark::SeedInstance {
    store::benchmark::SeedInstance {
        instance_id: id.to_owned(),
        repo: "django/django".to_owned(),
        base_commit: "0".repeat(40),
        image: format!("registry.example.com:5000/swebench/{id}@sha256:abc"),
        problem_statement: "kata benchmark".to_owned(),
        oracle_files: vec!["django/core.py".to_owned()],
        gold_patch: "diff --git a/x b/x\n".to_owned(),
        test_patch: "diff --git a/t b/t\n".to_owned(),
        fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
        pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
        eval_script: "#!/bin/bash\necho kata\n".to_owned(),
        parser: "parse_log_django".to_owned(),
    }
}

fn seed_document(slug: &str, seed_sha: &str) -> store::benchmark::SeedDocument {
    store::benchmark::SeedDocument {
        schema: 1,
        slug: slug.to_owned(),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "kata-rev".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: "kata-lock".to_owned(),
        seed_sha256: seed_sha.to_owned(),
        instances: vec![
            instance("repo__a-1"),
            instance("repo__b-2"),
            instance("repo__c-3"),
        ],
    }
}

/// One finished run of `suite` on `arm`, graded `resolved` for the first `resolved` cases.
async fn graded_run(
    pg: &support::Pg,
    seeded: &support::Seeded,
    suite: Uuid,
    arm: &str,
    resolved: usize,
    agent: Uuid,
) -> Uuid {
    let ids: Vec<String> = ["repo__a-1", "repo__b-2", "repo__c-3"]
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the run");
    let run = store::benchmark::create_run_tx(
        &mut tx,
        seeded.tenant,
        &store::benchmark::NewRun {
            suite_id: suite,
            arm,
            subject_kind: "capability_agent",
            capability_agent_id: Some(agent),
            process_id: None,
            subject_version: None,
            label: "kata benchmark arm",
            options: &json!({}),
            instance_ids: &ids,
        },
    )
    .await
    .expect("create the run");
    for (index, id) in ids.iter().enumerate() {
        let verdict = if index < resolved {
            "resolved"
        } else {
            "unresolved"
        };
        store::benchmark::finalise_case_tx(
            &mut tx,
            seeded.tenant,
            run.id,
            id,
            &store::benchmark::CaseOutcome {
                produce_status: "ok",
                patch: Some("diff --git a/x b/x\n"),
                patch_sha256: Some(&"0".repeat(64)),
                verdict,
                attempts: 1,
                seconds: None,
                sandbox_ref: None,
                log_document_id: None,
                detail: &json!({}),
            },
        )
        .await
        .expect("grade the case");
    }
    store::benchmark::refresh_run_counts_tx(&mut tx, seeded.tenant, run.id)
        .await
        .expect("counts");
    store::benchmark::finish_run_tx(&mut tx, seeded.tenant, run.id, "succeeded", None, None)
        .await
        .expect("finish the run");
    tx.commit().await.expect("commit the run");
    run.id
}

#[tokio::test]
async fn a_benchmark_instrument_opens_measures_and_closes() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let seeded = &anchor.seeded;
    let seed_sha = "a".repeat(64);
    let agent: Uuid = sqlx::query_scalar(
        "SELECT id FROM capability_agent WHERE tenant_id = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the fixture's capability agent");
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the suite");
    let (suite, _) = store::benchmark::seed_suite_tx(
        &mut tx,
        seeded.tenant,
        &seed_document("kata-bench", &seed_sha),
    )
    .await
    .expect("seed the suite");
    tx.commit().await.expect("commit the suite");
    let candidate = graded_run(&pg, seeded, suite.id, "conveyor", 2, agent).await;
    let baseline = graded_run(&pg, seeded, suite.id, "baseline", 1, agent).await;

    let running = support::Running::start(&pg).await;
    let (status, opened) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "benchmark-instrument",
                "title": "Benchmark instrument",
                "target_surface": "prompt",
                "target_ref": "prompt:parity",
                "instrument_kind": "benchmark",
                "instrument_id": suite.id,
                "instrument_version": "kata-rev",
                "split": "test",
                "split_sha256": seed_sha,
                "prediction": {"threshold": 0.05},
                "threshold": 0.05,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{opened}");
    let kata = text(output(&opened), "id");

    for (action, input) in [
        (
            "kata.observe",
            json!({"kata_id": kata, "observation": "the resolved rate is low"}),
        ),
        (
            "kata.analyse",
            json!({"kata_id": kata, "problem_statement": "the prompt omits the repro step"}),
        ),
        (
            "kata.hypothesise",
            json!({"kata_id": kata, "prediction": "the rewritten prompt doubles the resolved rate"}),
        ),
        (
            "kata.propose",
            json!({"kata_id": kata, "candidate_ref": "prompt:parity:v2"}),
        ),
    ] {
        let (status, body) = running
            .invoke(action, "dev:alice", input, None, None, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
    }

    let (status, measured) = running
        .invoke(
            "kata.measure",
            "dev:alice",
            json!({"kata_id": kata, "candidate_run_id": candidate, "baseline_run_id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{measured}");
    let measurement = &output(&measured)["measurement"];
    assert_eq!(measurement["instrument_kind"], "benchmark", "{measured}");
    assert_eq!(measurement["metric_key"], "resolved_rate", "{measured}");
    assert_eq!(measurement["split"], "test", "{measured}");
    assert_eq!(measurement["threshold"], 0.05, "{measured}");
    assert_eq!(measurement["verdict"], "improved", "{measured}");
    assert!(
        measurement["delta"].as_f64().unwrap_or_default() > 0.3,
        "{measured}"
    );
    assert_eq!(output(&measured)["status"], "measured", "{measured}");

    let (status, closed) = running
        .invoke(
            "kata.close",
            "dev:alice",
            json!({"kata_id": kata, "outcome": "kept", "reason": "the measurement held"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{closed}");
    running.shutdown().await;
}

#[tokio::test]
async fn a_benchmark_run_from_another_suite_is_refused() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let seeded = &anchor.seeded;
    let seed_sha = "a".repeat(64);
    let agent: Uuid = sqlx::query_scalar(
        "SELECT id FROM capability_agent WHERE tenant_id = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("agent");
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    let (suite, _) = store::benchmark::seed_suite_tx(
        &mut tx,
        seeded.tenant,
        &seed_document("kata-bench", &seed_sha),
    )
    .await
    .expect("suite");
    let (other, _) = store::benchmark::seed_suite_tx(
        &mut tx,
        seeded.tenant,
        &seed_document("kata-other", &"b".repeat(64)),
    )
    .await
    .expect("other suite");
    tx.commit().await.expect("commit");
    let candidate = graded_run(&pg, seeded, suite.id, "conveyor", 2, agent).await;
    let foreign = graded_run(&pg, seeded, other.id, "baseline", 1, agent).await;

    let running = support::Running::start(&pg).await;
    let (status, opened) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "benchmark-foreign-run",
                "title": "Benchmark foreign run",
                "target_surface": "prompt",
                "target_ref": "prompt:parity",
                "instrument_kind": "benchmark",
                "instrument_id": suite.id,
                "instrument_version": "kata-rev",
                "split": "test",
                "split_sha256": seed_sha,
                "prediction": {"threshold": 0.05},
                "threshold": 0.05,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{opened}");
    let kata = text(output(&opened), "id");
    for (action, input) in [
        (
            "kata.observe",
            json!({"kata_id": kata, "observation": "the rate is low"}),
        ),
        (
            "kata.analyse",
            json!({"kata_id": kata, "problem_statement": "the prompt omits the step"}),
        ),
        (
            "kata.hypothesise",
            json!({"kata_id": kata, "prediction": "the rewrite doubles the rate"}),
        ),
        (
            "kata.propose",
            json!({"kata_id": kata, "candidate_ref": "prompt:parity:v2"}),
        ),
    ] {
        let (status, body) = running
            .invoke(action, "dev:alice", input, None, None, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
    }
    let (status, refused) = running
        .invoke(
            "kata.measure",
            "dev:alice",
            json!({
                "kata_id": kata,
                "candidate_run_id": candidate,
                "baseline_run_id": foreign
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "benchmark_run_not_measurable",
        "{refused}"
    );
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("another suite"),
        "the refusal names the mismatch: {refused}"
    );
    running.shutdown().await;
}
