//! T11 — the metrics acceptance slice: the C0–C5 ladder (P3), the never-refused rule (P4) and
//! the MSP programme metrics (P5).
//!
//! The source rows are inserted directly: these tests are about the metrics derivation, not about
//! the actions that create a benefit, an approval or a dependency (those are T1/T3/T10's tests).
//
//! `rbuild -- test -p conveyor-actions --test metrics`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}


async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn programme(pg: &support::Pg, seeded: &support::Seeded) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("Metrics {}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("programme")
}

async fn project(
    pg: &support::Pg,
    seeded: &support::Seeded,
    programme: Uuid,
    key: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status, programme_id)
         VALUES ($1, $2, $3, 'active', $4) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(key)
    .bind(format!("Project {key}"))
    .bind(programme)
    .fetch_one(&pg.super_pool)
    .await
    .expect("project")
}

/// One decided approval. `rejected` picks the decision; `evidence` fills `meta.evidence_ref`.
async fn approval(pg: &support::Pg, seeded: &support::Seeded, gate: &str, rejected: bool, evidence: bool) {
    let status = if rejected { "rejected" } else { "approved" };
    let meta = if evidence {
        json!({"evidence_ref": "doc-1"})
    } else {
        json!({})
    };
    sqlx::query(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta, created_at, updated_at)
         VALUES ($1, $2, 'gate', $3, $4, now() - interval '1 hour', now())",
    )
    .bind(seeded.tenant_id)
    .bind(gate)
    .bind(status)
    .bind(meta)
    .execute(&pg.super_pool)
    .await
    .expect("approval");
}

async fn benefit(
    pg: &support::Pg,
    seeded: &support::Seeded,
    programme: Uuid,
    reference: &str,
    baseline: Option<Value>,
    target: Option<Value>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO benefit_profile
            (tenant_id, programme_id, reference, description, baseline, target,
             measurement_method, status)
         VALUES ($1, $2, $3, $4, $5, $6, 'timestamps, weekly', 'draft') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .bind(reference)
    .bind(format!("{reference} benefit"))
    .bind(baseline)
    .bind(target)
    .fetch_one(&pg.super_pool)
    .await
    .expect("benefit")
}

async fn threshold(running: &support::Running, level: &str, metrics: Value) -> Value {
    let (status, body) = running
        .invoke(
            "metrics.threshold_update",
            TOKEN,
            json!({"level": level, "metrics": metrics}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

async fn maturity(running: &support::Running) -> Value {
    let (status, body) = running
        .invoke("metrics.maturity", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

async fn programme_metrics(running: &support::Running, programme: Uuid) -> Value {
    let (status, body) = running
        .invoke(
            "metrics.programme",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_level_moves_only_when_the_evidence_crosses_the_recorded_threshold() {
    let (pg, running, seeded) = fixture().await;
    threshold(&running, "C0", json!({"runs.total": 0})).await;
    let c1 = threshold(&running, "C1", json!({"approvals.approval_rate": 0.8})).await;
    assert_eq!(c1["version"], 1, "the first C1 version");

    for _ in 0..5 {
        approval(&pg, &seeded, "quality_gate", false, true).await;
    }
    let first = maturity(&running).await;
    assert_eq!(first["level"], "C1", "{first}");
    assert_eq!(first["threshold_version"], 1);
    assert_eq!(first["evidence"]["approvals.approval_rate"], 1.0);

    // P3.1 — one rejection drops the rate to 0.833; the 0.8 threshold is still met.
    approval(&pg, &seeded, "quality_gate", true, true).await;
    let unchanged = maturity(&running).await;
    assert_eq!(unchanged["level"], "C1", "{unchanged}");

    // P3.3 — the SAME evidence against a raised threshold (a new version, no code change).
    let raised = threshold(&running, "C1", json!({"approvals.approval_rate": 0.9})).await;
    assert_eq!(raised["version"], 2);
    let below = maturity(&running).await;
    assert_eq!(below["level"], "C0", "{below}");
    assert_eq!(below["levels"][1]["met"], false);
    assert_eq!(below["levels"][1]["version"], 2);

    // P3.2 — lowering it again moves the level and names the version that authorised it.
    let lowered = threshold(&running, "C1", json!({"approvals.approval_rate": 0.8})).await;
    assert_eq!(lowered["version"], 3);
    let above = maturity(&running).await;
    assert_eq!(above["level"], "C1", "{above}");
    assert_eq!(above["threshold_version"], 3);

    // Reading a metric writes no metric row: only the three threshold versions exist.
    let thresholds: i64 = sqlx::query_scalar("SELECT count(*) FROM metric_threshold WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count thresholds");
    assert_eq!(thresholds, 4, "C0 v1 + C1 v1/v2/v3");
    let reconciliations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM metric_reconciliation WHERE tenant_id = $1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count reconciliations");
    assert_eq!(reconciliations, 0, "a read writes no reconciliation");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_with_no_refusal_is_flagged_and_one_refusal_clears_it() {
    let (pg, running, seeded) = fixture().await;
    let programme = programme(&pg, &seeded).await;
    for _ in 0..3 {
        approval(&pg, &seeded, "tranche_review", false, true).await;
    }
    for _ in 0..2 {
        approval(&pg, &seeded, "benefit_review", false, false).await;
    }

    let first = programme_metrics(&running, programme).await;
    let never = first["never_refused"]
        .as_array()
        .expect("never_refused")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(never.contains(&"tranche_review"), "{first}");
    assert!(never.contains(&"benefit_review"), "{first}");
    let tranche = first["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .find(|gate| gate["gate"] == "tranche_review")
        .expect("the tranche gate");
    assert_eq!(tranche["approved"], 3);
    assert_eq!(tranche["rejected"], 0);
    assert_eq!(tranche["never_refused"], true);
    assert_eq!(tranche["with_evidence"], 3);
    assert_eq!(tranche["latency_seconds"], 3600.0);

    // P4.2 — one rejection clears the flag; the other gate is untouched.
    approval(&pg, &seeded, "tranche_review", true, true).await;
    let second = programme_metrics(&running, programme).await;
    let never = second["never_refused"]
        .as_array()
        .expect("never_refused")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(!never.contains(&"tranche_review"), "{second}");
    assert!(never.contains(&"benefit_review"), "{second}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn realised_versus_planned_recomputes_and_an_unmeasured_baseline_is_not_zero() {
    let (pg, running, seeded) = fixture().await;
    let programme = programme(&pg, &seeded).await;
    let measured = benefit(
        &pg,
        &seeded,
        programme,
        "B1",
        Some(json!({"value": 14})),
        Some(json!({"value": 8})),
    )
    .await;
    benefit(
        &pg,
        &seeded,
        programme,
        "B2",
        None,
        Some(json!({"value": 95})),
    )
    .await;
    sqlx::query(
        "INSERT INTO benefit_transition
            (tenant_id, benefit_id, baseline_measured_at, post_measured_at, evidence)
         VALUES ($1, $2, now(), now(), '[]')",
    )
    .bind(seeded.tenant_id)
    .bind(measured)
    .execute(&pg.super_pool)
    .await
    .expect("transition");

    let metrics = programme_metrics(&running, programme).await;
    assert_eq!(metrics["realised"], 1, "{metrics}");
    assert_eq!(metrics["planned"], 2, "{metrics}");
    assert_eq!(metrics["benefit_ratio"], 0.5, "{metrics}");
    let benefits = metrics["benefits"].as_array().expect("benefits");
    let b1 = benefits.iter().find(|row| row["reference"] == "B1").expect("B1");
    let b2 = benefits.iter().find(|row| row["reference"] == "B2").expect("B2");
    assert_eq!(b1["baseline_state"], "measured");
    assert_eq!(b1["realised"], true);
    // P5.4 — a null baseline is reported unmeasured, never counted as a realised zero.
    assert_eq!(b2["baseline_state"], "unmeasured");
    assert_eq!(b2["realised"], false);
    assert!(b2["baseline"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dependency_slip_names_the_benefits_it_puts_at_risk() {
    let (pg, running, seeded) = fixture().await;
    let programme = programme(&pg, &seeded).await;
    let from = project(&pg, &seeded, programme, "PDEP-A").await;
    let to = project(&pg, &seeded, programme, "PDEP-B").await;
    let at_risk = benefit(
        &pg,
        &seeded,
        programme,
        "B3",
        Some(json!({"value": 4})),
        Some(json!({"value": 12})),
    )
    .await;
    sqlx::query(
        "INSERT INTO benefit_link
            (tenant_id, programme_id, output_ref, capability_ref, operational_outcome,
             benefit_id, strategic_objective)
         VALUES ($1, $2, 'PDEP-B', 'the capability', 'the outcome', $3, 'the objective')",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .bind(at_risk)
    .execute(&pg.super_pool)
    .await
    .expect("link");
    let dependency: Uuid = sqlx::query_scalar(
        "INSERT INTO project_dependency
            (tenant_id, programme_id, from_project, to_project, kind, status)
         VALUES ($1, $2, $3, $4, 'needs', 'slipped') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .bind(from)
    .bind(to)
    .fetch_one(&pg.super_pool)
    .await
    .expect("dependency");

    let metrics = programme_metrics(&running, programme).await;
    let risk = metrics["benefit_at_risk"].as_array().expect("risk");
    assert_eq!(risk.len(), 1, "{metrics}");
    assert_eq!(risk[0]["reference"], "B3");
    assert_eq!(risk[0]["via_project"], "PDEP-B");

    // The edge follows the dependency's state: a met dependency names nothing.
    sqlx::query("UPDATE project_dependency SET status = 'met' WHERE id = $1")
        .bind(dependency)
        .execute(&pg.super_pool)
        .await
        .expect("meet the dependency");
    let met = programme_metrics(&running, programme).await;
    assert_eq!(met["benefit_at_risk"].as_array().expect("risk").len(), 0);
}
