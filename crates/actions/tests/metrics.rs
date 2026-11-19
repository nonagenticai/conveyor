//! T11 — the metrics acceptance slice: the C0–C5 ladder (P3), the never-refused rule (P4) and
//! the MSP programme metrics (P5).
//!
//! The source rows are inserted directly: these tests are about the metrics derivation, not about
//! the actions that create a benefit, an approval or a dependency (those are T1/T3/T10's tests).
//
//! `rbuild -- test -p conveyor-actions --test metrics`.

mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{Duration as ChronoDuration, Utc};
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
    sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(seeded.tenant_id)
        .bind(format!("Metrics {}", Uuid::now_v7().simple()))
        .fetch_one(&pg.super_pool)
        .await
        .expect("programme")
}

async fn project(pg: &support::Pg, seeded: &support::Seeded, programme: Uuid, key: &str) -> Uuid {
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
async fn approval(
    pg: &support::Pg,
    seeded: &support::Seeded,
    gate: &str,
    rejected: bool,
    evidence: bool,
) {
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
    let thresholds: i64 =
        sqlx::query_scalar("SELECT count(*) FROM metric_threshold WHERE tenant_id = $1")
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
    let b1 = benefits
        .iter()
        .find(|row| row["reference"] == "B1")
        .expect("B1");
    let b2 = benefits
        .iter()
        .find(|row| row["reference"] == "B2")
        .expect("B2");
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

// ── P2: the billing reconciliation ───────────────────────────────────────────────────────

/// A controllable billing_service: every path answers the current JSON body.
async fn billing_server(body: Arc<Mutex<Value>>) -> String {
    let app = axum::Router::new().fallback(axum::routing::any(move || {
        let body = body.clone();
        async move { axum::Json(body.lock().expect("billing body").clone()) }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind billing stub");
    let addr = listener.local_addr().expect("billing addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

async fn price(pg: &support::Pg, model: &str) {
    sqlx::query(
        "INSERT INTO commercial_price
            (model, equivalent, input_usd_per_1m, output_usd_per_1m, source)
         VALUES ($1, 'metrics-test equivalent', 1.0, 2.0, 'test')
         ON CONFLICT (model) DO NOTHING",
    )
    .bind(model)
    .execute(&pg.super_pool)
    .await
    .expect("price");
}

#[allow(clippy::too_many_arguments)]
async fn inference_call(
    pg: &support::Pg,
    seeded: &support::Seeded,
    model: &str,
    run_id: Option<Uuid>,
    step_id: Option<Uuid>,
    prompt: i32,
    completion: i32,
    caller_kind: &str,
    created_at: chrono::DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO inference_call
            (tenant_id, caller_kind, model, prompt_tokens, completion_tokens,
             run_id, run_step_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(caller_kind)
    .bind(model)
    .bind(prompt)
    .bind(completion)
    .bind(run_id)
    .bind(step_id)
    .bind(created_at)
    .fetch_one(&pg.super_pool)
    .await
    .expect("inference call")
}

async fn usage(running: &support::Running, level: &str) -> Vec<Value> {
    let (status, body) = running
        .invoke(
            "metrics.usage",
            TOKEN,
            json!({"level": level}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["rows"].as_array().expect("rows").clone()
}

fn calls_total(rows: &[Value]) -> i64 {
    rows.iter()
        .map(|row| row["calls"].as_i64().unwrap_or(0))
        .sum()
}

fn tokens_total(rows: &[Value]) -> i64 {
    rows.iter()
        .map(|row| row["tokens"].as_i64().unwrap_or(0))
        .sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_metering_level_sums_to_its_children() {
    let (pg, running, seeded) = fixture().await;
    let programme = programme(&pg, &seeded).await;
    let project = project(&pg, &seeded, programme, "METER").await;
    price(&pg, "metrics-test").await;
    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'succeeded') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("run");
    let now = Utc::now();
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        Some(run),
        None,
        1000,
        500,
        "agent_run",
        now,
    )
    .await;
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        Some(run),
        None,
        1000,
        500,
        "agent_run",
        now,
    )
    .await;
    // An unattributed call: caller_kind 'other' with no run. It must appear in its own bucket.
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        None,
        None,
        100,
        50,
        "other",
        now,
    )
    .await;

    let tenant = usage(&running, "tenant").await;
    assert_eq!(calls_total(&tenant), 3, "{tenant:?}");
    assert_eq!(tokens_total(&tenant), 3150, "{tenant:?}");

    let call_level = usage(&running, "call").await;
    assert_eq!(calls_total(&call_level), 3, "the calls are the leaf rows");
    let project_rows = usage(&running, "project").await;
    assert_eq!(calls_total(&project_rows), 3);
    let programme_rows = usage(&running, "programme").await;
    assert_eq!(calls_total(&programme_rows), 3);
    let run_rows = usage(&running, "run").await;
    assert_eq!(calls_total(&run_rows), 3);
    assert!(run_rows.iter().any(|row| row["key"] == "unattributed"));
    let department_rows = usage(&running, "department").await;
    assert_eq!(calls_total(&department_rows), 3);
    let job_rows = usage(&running, "job").await;
    assert_eq!(calls_total(&job_rows), 3);
    // Every level sums to the tenant: no call is dropped.
    for level in ["project", "programme", "run", "department", "job", "call"] {
        assert_eq!(
            tokens_total(&usage(&running, level).await),
            3150,
            "level {level}"
        );
    }

    // P1.1 — a new ledger row changes the figure; the read itself wrote no metric row.
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        Some(run),
        None,
        10,
        5,
        "agent_run",
        now,
    )
    .await;
    let after = usage(&running, "tenant").await;
    assert_eq!(calls_total(&after), 4, "the new call is counted");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM metric_threshold WHERE tenant_id = $1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("thresholds");
    assert_eq!(rows, 0, "a metrics read never writes a threshold");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_run_drilldown_sums_to_the_call_rows() {
    let (pg, running, seeded) = fixture().await;
    let programme = programme(&pg, &seeded).await;
    let project = project(&pg, &seeded, programme, "DRILL").await;
    price(&pg, "metrics-test").await;
    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'succeeded') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("run");
    let first: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step (tenant_id, run_id, position, action) VALUES ($1, $2, 1, 'agent.run') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(run)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step 1");
    let second: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step (tenant_id, run_id, position, action) VALUES ($1, $2, 2, 'agent.run') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(run)
    .fetch_one(&pg.super_pool)
    .await
    .expect("step 2");
    let now = Utc::now();
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        Some(run),
        Some(first),
        1000,
        500,
        "agent_run",
        now,
    )
    .await;
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        Some(run),
        Some(second),
        2000,
        1000,
        "agent_run",
        now,
    )
    .await;

    let (status, body) = running
        .invoke(
            "metrics.job",
            TOKEN,
            json!({"run_id": run}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let out = output(&body);
    let out = out.get("run").expect("the run block");
    assert_eq!(out["tokens"], 4500, "{out}");
    let steps = out["steps"].as_array().expect("steps");
    assert_eq!(
        steps
            .iter()
            .map(|s| s["tokens"].as_i64().unwrap())
            .sum::<i64>(),
        4500,
        "the steps sum to the run"
    );
    let calls = out["calls"].as_array().expect("calls");
    assert_eq!(
        calls
            .iter()
            .map(|c| c["tokens"].as_i64().unwrap())
            .sum::<i64>(),
        4500,
        "the calls sum to the run"
    );
    let step_cost: f64 = steps.iter().map(|s| s["cost_usd"].as_f64().unwrap()).sum();
    assert!((step_cost - out["cost_usd"].as_f64().unwrap()).abs() < 1e-9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flow_recomputes_and_a_missing_ledger_is_unavailable() {
    let (pg, running, seeded) = fixture().await;
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Flow project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project = output(&body)["id"].as_str().expect("project id").to_owned();
    let project_id: Uuid = project.parse().expect("uuid");
    let board: Uuid = sqlx::query_scalar("SELECT id FROM board WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("board");
    let mut lists: HashMap<String, Uuid> = HashMap::new();
    for (kind, id) in
        sqlx::query_as::<_, (String, Uuid)>("SELECT kind, id FROM list WHERE board_id = $1")
            .bind(board)
            .fetch_all(&pg.super_pool)
            .await
            .expect("lists")
    {
        lists.insert(kind, id);
    }
    for (kind, name, pos) in [
        ("intake", "Intake", 0.0),
        ("active", "Active", 1.0),
        ("waiting", "Waiting", 2.0),
        ("done", "Done", 3.0),
    ] {
        if !lists.contains_key(kind) {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO list (tenant_id, board_id, name, position, kind)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(seeded.tenant_id)
            .bind(board)
            .bind(name)
            .bind(pos)
            .bind(kind)
            .fetch_one(&pg.super_pool)
            .await
            .expect("list");
            lists.insert(kind.to_owned(), id);
        }
    }
    let card_type: Uuid =
        sqlx::query_scalar("SELECT id FROM card_type WHERE tenant_id = $1 LIMIT 1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("card type");
    let now = Utc::now();
    let created = now - ChronoDuration::minutes(120);
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position, created_at)
         VALUES ($1, $2, $3, $4, 'flow card', 0, $5) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(board)
    .bind(lists["intake"])
    .bind(card_type)
    .bind(created)
    .fetch_one(&pg.super_pool)
    .await
    .expect("card");
    for (from, to, minutes) in [
        ("intake", "active", 90),
        ("active", "waiting", 75),
        ("waiting", "active", 66),
        ("active", "done", 60),
    ] {
        sqlx::query(
            "INSERT INTO event
                (tenant_id, topic, type, aggregate_type, aggregate_id, payload, created_at)
             VALUES ($1, $2, 'card.moved', 'card', $3, $4, $5)",
        )
        .bind(seeded.tenant_id)
        .bind(format!("board:{board}"))
        .bind(card)
        .bind(json!({
            "to_list_id": lists[to].to_string(),
            "from_list_id": lists[from].to_string(),
        }))
        .bind(now - ChronoDuration::minutes(minutes))
        .execute(&pg.super_pool)
        .await
        .expect("move event");
    }
    let (status, body) = running
        .invoke("metrics.flow", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let flow = output(&body);
    assert_eq!(flow["throughput"], 1, "{flow}");
    assert_eq!(flow["wip"], 0, "{flow}");
    let lead = flow["lead_time_seconds"].as_f64().expect("lead");
    assert!((lead - 3600.0).abs() < 90.0, "lead {lead}");
    let cycle = flow["cycle_time_seconds"].as_f64().expect("cycle");
    assert!((cycle - 1800.0).abs() < 90.0, "cycle {cycle}");
    let queue = flow["queue_time_seconds"].as_f64().expect("queue");
    assert!((queue - 1800.0).abs() < 90.0, "queue {queue}");
    let block = flow["block_time_seconds"].as_f64().expect("block");
    assert!((block - 540.0).abs() < 60.0, "block {block}");
    let efficiency = flow["flow_efficiency"].as_f64().expect("efficiency");
    assert!((efficiency - 0.35).abs() < 0.05, "efficiency {efficiency}");
    // P1.4 — the knowledge figures have no ledger and are unavailable with a reason, not zero.
    let unavailable = flow["unavailable"].as_array().expect("unavailable");
    assert!(
        unavailable
            .iter()
            .any(|row| row["metric"] == "knowledge.lesson_promotion_rate"),
        "{flow}"
    );
    assert!(
        flow["wip"].as_i64().is_some(),
        "the other figures are not affected"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconciliation_is_exact_or_opens_an_incident() {
    let (pg, running, seeded) = fixture().await;
    // The light catalogue seed carries only the project types; the full export carries
    // `incident`, and importing it here also proves the seed file really has it.
    let export = conveyor_store::seed::catalogue_export().expect("the catalogue export");
    let incident: Vec<_> = export
        .card_types
        .into_iter()
        .filter(|card_type| card_type.code == "incident")
        .collect();
    assert_eq!(
        incident.len(),
        1,
        "the catalogue seed carries the incident type"
    );
    let mut card_type_tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::catalogue::import_card_types_tx(&mut card_type_tx, seeded.tenant, &incident)
        .await
        .expect("import the incident type");
    card_type_tx
        .commit()
        .await
        .expect("commit the incident type");
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Reconcile project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project: Uuid = output(&body)["id"]
        .as_str()
        .expect("project id")
        .parse()
        .expect("uuid");
    let board: Uuid = sqlx::query_scalar("SELECT id FROM board WHERE project_id = $1")
        .bind(project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("board");
    let list: Uuid =
        sqlx::query_scalar("SELECT id FROM list WHERE board_id = $1 ORDER BY position LIMIT 1")
            .bind(board)
            .fetch_one(&pg.super_pool)
            .await
            .expect("list");
    price(&pg, "metrics-test").await;
    inference_call(
        &pg,
        &seeded,
        "metrics-test",
        None,
        None,
        1000,
        500,
        "other",
        Utc::now(),
    )
    .await;
    let body = Arc::new(Mutex::new(json!({"total_usd": 0.002})));
    let base = billing_server(body.clone()).await;
    support::set_env("CONVEYOR_EXTERNAL_BILLING_SERVICE_URL", &base);
    support::set_env("CONVEYOR_EXTERNAL_BILLING_SERVICE_TIMEOUT_MS", "500");
    let today = Utc::now().date_naive();
    let (status, matched) = running
        .invoke(
            "metrics.reconcile",
            TOKEN,
            json!({"period_start": today, "period_end": today, "board_id": board, "list_id": list}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{matched}");
    let matched = output(&matched);
    assert_eq!(matched["state"], "matched", "{matched}");
    assert_eq!(matched["delta_usd"], 0.0);
    assert!(matched["incident_card_id"].is_null());

    *body.lock().expect("body") = json!({"total_usd": 0.003});
    let (status, different) = running
        .invoke(
            "metrics.reconcile",
            TOKEN,
            json!({"period_start": today, "period_end": today, "board_id": board, "list_id": list}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{different}");
    let different = output(&different);
    assert_eq!(different["state"], "difference", "{different}");
    let incident_id = different["incident_card_id"]
        .as_str()
        .expect("incident card");
    let (code, meta): (String, Value) = sqlx::query_as(
        "SELECT ct.code, c.meta FROM card c JOIN card_type ct ON ct.id = c.card_type_id
          WHERE c.id = $1",
    )
    .bind(Uuid::parse_str(incident_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("incident card");
    assert_eq!(code, "incident");
    assert_eq!(meta["conveyor_usd"], 0.002);
    assert_eq!(meta["billing_usd"], 0.003);

    support::set_env(
        "CONVEYOR_EXTERNAL_BILLING_SERVICE_URL",
        "http://127.0.0.1:1",
    );
    let yesterday = today - ChronoDuration::days(1);
    let (status, unavailable) = running
        .invoke(
            "metrics.reconcile",
            TOKEN,
            json!({"period_start": yesterday, "period_end": yesterday}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{unavailable}");
    let unavailable = output(&unavailable);
    assert_eq!(unavailable["state"], "unavailable", "{unavailable}");
    assert!(
        unavailable["upstream_error"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "{unavailable}"
    );
    assert!(unavailable["conveyor_usd"].is_null(), "never a zero");
    let records: i64 =
        sqlx::query_scalar("SELECT count(*) FROM metric_reconciliation WHERE tenant_id = $1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("records");
    assert_eq!(
        records, 2,
        "today (re-run, now difference) + yesterday (unavailable)"
    );
}
