#![allow(dead_code)]
//! T5 (#77) fixtures: a real organisation spine and a locked eval instrument.
//!
//! The kata is an ordinary job on the V2 spine, so its fixture is the same one
//! `apps/server/tests/organisation_gate.rs` builds: an industry, its primary department, a job,
//! a published release and an ACTIVE binding. The instrument is an `eval`/`eval_version` with a
//! locked split, one `pass` quality metric and two complete runs whose graded cases are opposite
//! on the train and test sides — so a measurement that reads the wrong side produces a different
//! number, not a passing test.

use conveyor_domain::TenantId;
use conveyor_domain::executor::{ExecutorKind, Registry};
use conveyor_domain::organisation::BindingStatus;
use conveyor_store::actions::{NewDepartment, department_create};
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_create_tx, job_release_publish_tx,
};
use serde_json::json;
use uuid::Uuid;

use super::{Pg, Running, Seeded, seed};

/// The seeded tenant plus its active binding — what `kata.open` requires.
pub struct Anchor {
    pub seeded: Seeded,
    pub binding_id: Uuid,
    pub release_hash: String,
    pub job_id: Uuid,
}

/// The organisation spine `kata.open` requires, on an already-seeded tenant.
pub struct Spine {
    pub binding_id: Uuid,
    pub release_hash: String,
    pub job_id: Uuid,
}

/// Seed `dev-a`, its organisation spine and one ACTIVE `internal_agent` binding.
pub async fn anchor(pg: &Pg) -> Anchor {
    let seeded = seed(pg, "dev-a", "alice", "Alice").await;
    let spine = spine(pg, &seeded).await;
    Anchor {
        seeded,
        binding_id: spine.binding_id,
        release_hash: spine.release_hash,
        job_id: spine.job_id,
    }
}

/// One ACTIVE `internal_agent` binding on an industry → department → job → release chain.
///
/// A card carries the binding a kata opens under (SPEC_V2 §4.4), so a fixture that needs a
/// measurable kata on a card of its own seeds the spine here instead of taking `anchor`'s
/// tenant.
pub async fn spine(pg: &Pg, seeded: &Seeded) -> Spine {
    let tenant: TenantId = seeded.tenant;
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");

    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: "software".to_owned(),
            name: "Software".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let delivery = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "delivery",
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(1),
            is_primary: true,
        },
    )
    .await
    .expect("department");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            job_key: "kata_facilitator".to_owned(),
            industry_id: industry.id,
            family: "improvement".to_owned(),
            title: "Kata facilitator".to_owned(),
            status: "active".to_owned(),
            definition: json!({"charter": "Improve the system by measuring it."}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: delivery.id,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    let release = job_release_publish_tx(&mut tx, tenant, job.id)
        .await
        .expect("release");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'kata', 'Kata')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = excluded.label
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role)
         VALUES ($1, $2, 'facilitation', 'router/medium', 'tool_agent') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(capability)
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    let binding = job_binding_create_tx(
        &mut tx,
        tenant,
        &NewBinding {
            job_id: job.id,
            release_id: release.id,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(agent),
            capability_id: None,
            meta: json!({}),
            status: BindingStatus::Active,
        },
        &Registry::builtin(),
    )
    .await
    .expect("binding");
    tx.commit().await.expect("commit the spine fixture");

    Spine {
        binding_id: binding.id,
        release_hash: binding.release_hash,
        job_id: job.id,
    }
}

/// A locked eval instrument plus two complete runs.
pub struct Instrument {
    pub eval_id: Uuid,
    pub version_id: Uuid,
    pub metric_key: String,
    pub candidate_run_id: Uuid,
    pub baseline_run_id: Uuid,
    /// Distinct held-out cases on each side.
    pub held_out_cases: usize,
}

/// Seed a locked instrument whose candidate and baseline grade the train/test sides
/// differently. The caller supplies the per-case scores, so "read only the test side" is a
/// number the test can predict exactly.
pub async fn instrument(
    pg: &Pg,
    seeded: &Seeded,
    candidate_test: &[f64],
    candidate_train: &[f64],
    baseline_test: &[f64],
    baseline_train: &[f64],
) -> Instrument {
    assert!(
        !candidate_test.is_empty() && candidate_test.len() == baseline_test.len(),
        "both runs need the same, non-empty held-out side"
    );
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let eval_id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval (tenant_id, name, subject_kind, entry_action)
         VALUES ($1, 'Kata instrument', 'process', 'eval_runs.start') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("eval");
    let version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_version
             (tenant_id, eval_id, version, digest, split_sha256, split_locked_at)
         VALUES ($1, $2, 1, 'kata-fixture', $3, now()) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(eval_id)
    .bind("a".repeat(64))
    .fetch_one(&mut *tx)
    .await
    .expect("eval version");
    sqlx::query(
        "INSERT INTO eval_metric
             (tenant_id, version_id, key, role, kind, better, label, grader_kind, ordinal)
         VALUES ($1, $2, 'pass', 'quality', 'binary', 'higher', 'pass', 'programmatic', 0)",
    )
    .bind(seeded.tenant_id)
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("metric");

    let mut ordinal = 0;
    // The cases are one set; the two runs grade the same case ids.
    let mut test_ids = Vec::new();
    let mut train_ids = Vec::new();
    for (split, scores) in [("test", candidate_test), ("train", candidate_train)] {
        for score in scores {
            let case_id: Uuid = sqlx::query_scalar(
                "INSERT INTO eval_case (tenant_id, version_id, ordinal, input, split)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(seeded.tenant_id)
            .bind(version_id)
            .bind(ordinal)
            .bind(format!("kata case {ordinal}: grade {score}"))
            .bind(split)
            .fetch_one(&mut *tx)
            .await
            .expect("case");
            ordinal += 1;
            if split == "test" {
                test_ids.push(case_id);
            } else {
                train_ids.push(case_id);
            }
        }
    }
    let case_count = (test_ids.len() + train_ids.len()) as i32;

    let candidate_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_run
             (tenant_id, eval_id, version_id, variant, reps, case_count, status)
         VALUES ($1, $2, $3, 'v1', 1, $4, 'complete') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(eval_id)
    .bind(version_id)
    .bind(case_count)
    .fetch_one(&mut *tx)
    .await
    .expect("candidate run");
    let baseline_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_run
             (tenant_id, eval_id, version_id, variant, reps, case_count, status)
         VALUES ($1, $2, $3, 'baseline', 1, $4, 'complete') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(eval_id)
    .bind(version_id)
    .bind(case_count)
    .fetch_one(&mut *tx)
    .await
    .expect("baseline run");

    for (run_id, test_scores, train_scores) in [
        (candidate_run_id, candidate_test, candidate_train),
        (baseline_run_id, baseline_test, baseline_train),
    ] {
        for (case_id, score) in test_ids
            .iter()
            .zip(test_scores.iter())
            .chain(train_ids.iter().zip(train_scores.iter()))
        {
            sqlx::query(
                "INSERT INTO eval_case_result
                     (tenant_id, eval_run_id, case_id, rep, status, grade, model)
                 VALUES ($1, $2, $3, 0, 'ok', $4, 'fixture')",
            )
            .bind(seeded.tenant_id)
            .bind(run_id)
            .bind(case_id)
            .bind(json!({ "pass": score }))
            .execute(&mut *tx)
            .await
            .expect("case result");
        }
    }
    tx.commit().await.expect("commit the instrument");
    Instrument {
        eval_id,
        version_id,
        metric_key: "pass".to_owned(),
        candidate_run_id,
        baseline_run_id,
        held_out_cases: test_ids.len(),
    }
}

/// A procedure key with two versions: v1 (retired, the pre-kata state) and v2 (published, the
/// candidate). Returned as `(baseline, candidate)`.
pub async fn published_pair(pg: &Pg, seeded: &Seeded, key: &str) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let baseline: Uuid = sqlx::query_scalar(
        "INSERT INTO procedure (tenant_id, key, title, status, version)
         VALUES ($1, $2, 'Kata procedure v1', 'published', 1) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .expect("the pre-kata procedure version");
    let candidate: Uuid = sqlx::query_scalar(
        "INSERT INTO procedure (tenant_id, key, title, status, version)
         VALUES ($1, $2, 'Kata procedure v2', 'draft', 2) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .expect("the candidate procedure version");
    // Publish the candidate the way `procedure::publish_tx` does: retire the previous published
    // row, then flip this one. The partial unique index permits exactly that order.
    sqlx::query(
        "UPDATE procedure SET status = 'retired'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(baseline)
    .execute(&mut *tx)
    .await
    .expect("retire the pre-kata version");
    sqlx::query(
        "UPDATE procedure SET status = 'published'
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(candidate)
    .execute(&mut *tx)
    .await
    .expect("publish the candidate");
    tx.commit().await.expect("commit the procedure pair");
    (baseline, candidate)
}

/// Open a measurable kata (P4's instrument + threshold) and return its id.
pub async fn open(
    running: &Running,
    anchor: &Anchor,
    instrument_version: Uuid,
    signature: &str,
    target_ref: &str,
    threshold: f64,
) -> String {
    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": signature,
                "title": "Kata fixture",
                "target_surface": "procedure",
                "target_ref": target_ref,
                "instrument_version_id": instrument_version,
                "threshold": threshold,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    body["output"]["id"]
        .as_str()
        .expect("the kata id")
        .to_owned()
}

/// Record the first four Gets, in the procedure's order.
pub async fn to_proposed(
    running: &Running,
    kata_id: &str,
    instrument_version: Uuid,
    candidate_ref: &str,
) {
    for (action, input) in [
        (
            "kata.observe",
            json!({"kata_id": kata_id, "observation": "the work and its evidence"}),
        ),
        (
            "kata.analyse",
            json!({"kata_id": kata_id, "problem_statement": "the problem, with data"}),
        ),
        (
            "kata.hypothesise",
            json!({
                "kata_id": kata_id,
                "prediction": "the candidate raises the held-out mean",
                "instrument_version_id": instrument_version,
            }),
        ),
        (
            "kata.propose",
            json!({"kata_id": kata_id, "candidate_ref": candidate_ref}),
        ),
    ] {
        let (status, body) = running
            .invoke(action, "dev:alice", input, None, None, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
    }
}

/// Open a kata and drive it to `proposed`, ready to measure.
pub async fn proposed(
    running: &Running,
    anchor: &Anchor,
    instrument_version: Uuid,
    signature: &str,
    target_ref: &str,
    candidate_ref: &str,
    threshold: f64,
) -> String {
    let kata_id = open(
        running,
        anchor,
        instrument_version,
        signature,
        target_ref,
        threshold,
    )
    .await;
    to_proposed(running, &kata_id, instrument_version, candidate_ref).await;
    kata_id
}
