//! S19 Part C step 13 — `eval_runs.report`: the run's numbers, recomputed from its rows.
//!
//! The report is the instrument's read side, so every test here is about honesty rather than a
//! happy path: a mean that must not be weighted by surviving reps, a truncated or parked slot
//! that must be counted rather than averaged, a cost that must say "unpriced" instead of "$0",
//! and an empty run that must answer an empty report rather than invent a zero.
//!
//! The fixture seeds the run and its result rows through the store (the driver's own proof is
//! `apps/server/tests/eval_runner.rs`); the report is then read through the ordinary action
//! surface, over HTTP, as a member.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::RunStatus;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// One slot to seed.
struct Slot {
    ordinal: usize,
    rep: i32,
    status: &'static str,
    /// The `pass` grade; `None` writes no value at all (a truncated or parked row).
    score: Option<f64>,
    usage: Value,
    judge_usage: Option<Value>,
}

impl Slot {
    fn ok(ordinal: usize, rep: i32, score: f64) -> Self {
        Self {
            ordinal,
            rep,
            status: "ok",
            score: Some(score),
            usage: json!({}),
            judge_usage: None,
        }
    }

    fn blank(ordinal: usize, rep: i32, status: &'static str) -> Self {
        Self {
            ordinal,
            rep,
            status,
            score: None,
            usage: json!({}),
            judge_usage: None,
        }
    }
}

/// An eval, a version, the given cases (one per tag) and one binary quality metric `pass`.
async fn instrument(running: &support::Running, tags: &[&str]) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "report fixture",
                "purpose": "prove the report recomputes from the rows",
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

    for (ordinal, tag) in tags.iter().enumerate() {
        let (status, body) = running
            .invoke(
                "eval_cases.create",
                TOKEN,
                json!({
                    "version_id": version_id,
                    "input": format!("classify: case {ordinal}"),
                    "tags": [tag],
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

    // A perf column: measured, never graded (`grader_kind = none`), and it must land in `perf[]`
    // rather than among the quality metrics.
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "key": "latency",
                "role": "perf",
                "kind": "float",
                "better": "lower",
                "label": "latency",
                "unit": "ms",
                "grader_kind": "none",
                "grader": {},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    (eval_id, version_id)
}

/// Insert a run in the caller's tenant and return its id.
async fn run_row(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: &str,
    version_id: &str,
    reps: i32,
    case_count: i32,
) -> Uuid {
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
            reps,
            case_count,
            environment: &json!({}),
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit");
    run.id
}

/// Fill the given slots, then drive the run to `complete`.
async fn fill(pg: &support::Pg, tenant: TenantId, version_id: &str, run_id: Uuid, slots: &[Slot]) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let cases: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("version uuid"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");

    for slot in slots {
        let grade = match slot.score {
            Some(score) => json!({ "pass": score, "latency": 120.0 }),
            None => json!({}),
        };
        let explanation = json!({});
        store::eval::insert_case_result_tx(
            &mut tx,
            tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run_id,
                case_id: cases[slot.ordinal],
                rep: slot.rep,
                status: slot.status,
                stop_reason: None,
                grade: &grade,
                explanation: &explanation,
                model: "stub-model",
                usage: &slot.usage,
                latency_ms: 3,
                tool_calls: 0,
                action_invocation_id: None,
                subject_run_id: None,
                judge_model: None,
                judge_usage: slot.judge_usage.as_ref(),
                human_score: None,
            },
        )
        .await
        .expect("insert a result");
    }
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Running)
        .await
        .expect("to running");
    store::eval::transition_run_tx(&mut tx, tenant, run_id, RunStatus::Complete)
        .await
        .expect("to complete");
    tx.commit().await.expect("commit");
}

/// Write the split directly: a drawn split is random, and these tests name exact counts.
async fn set_split(pg: &support::Pg, tenant: TenantId, version_id: &str, test: &[usize]) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("version uuid"))
    .fetch_all(&mut *tx)
    .await
    .expect("read the cases");
    for (ordinal, id) in ids.iter().enumerate() {
        let split = if test.contains(&ordinal) {
            "test"
        } else {
            "train"
        };
        sqlx::query("UPDATE eval_case SET split = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(split)
            .execute(&mut *tx)
            .await
            .expect("write the split");
    }
    tx.commit().await.expect("commit");
}

/// Set (or clear) the version's price document.
async fn set_prices(pg: &support::Pg, tenant: TenantId, version_id: &str, prices: Value) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("UPDATE eval_version SET prices = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(Uuid::parse_str(version_id).expect("version uuid"))
        .bind(&prices)
        .execute(&mut *tx)
        .await
        .expect("write the prices");
    tx.commit().await.expect("commit");
}

/// One failed attempt, written beside the slot it never filled.
#[allow(clippy::too_many_arguments)]
async fn attempt(
    pg: &support::Pg,
    tenant: TenantId,
    version_id: &str,
    run_id: Uuid,
    ordinal: usize,
    rep: i32,
    number: i32,
    class: &str,
    usage: Option<Value>,
) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 AND ordinal = $3",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(version_id).expect("version uuid"))
    .bind(ordinal as i32)
    .fetch_one(&mut *tx)
    .await
    .expect("read the case");
    store::eval::insert_attempt_error_tx(
        &mut tx,
        tenant,
        &store::eval::NewAttemptError {
            eval_run_id: run_id,
            case_id,
            rep,
            attempt: number,
            failure_class: class,
            retry_count: 0,
            model: Some("stub-model"),
            usage: usage.as_ref(),
            message: "the stub could not answer",
            deadline_at: None,
        },
    )
    .await
    .expect("insert a failed attempt");
    tx.commit().await.expect("commit");
}

async fn report(running: &support::Running, run_id: Uuid, group_by: Option<&str>) -> (u16, Value) {
    let mut input = json!({ "run_id": run_id });
    if let Some(group_by) = group_by {
        input["group_by"] = json!(group_by);
    }
    running
        .invoke("eval_runs.report", TOKEN, input, None, None, None)
        .await
}

fn close(actual: &Value, expected: f64) -> bool {
    let actual = actual.as_f64().expect("a number");
    (actual - expected).abs() < 1e-9
}

#[tokio::test]
async fn report_recomputes_mean_and_ci_from_raw_rows() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) =
        instrument(&running, &["triage", "triage", "billing", "billing"]).await;
    set_split(&pg, seeded.tenant, &version_id, &[0, 1]).await;
    let run_id = run_row(&pg, seeded.tenant, &eval_id, &version_id, 2, 4).await;

    // Case 0: 1.0 twice. Case 1: 0.0 and 1.0 -> .5. Case 2: 1.0 twice. Case 3: truncated twice.
    fill(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        &[
            Slot::ok(0, 0, 1.0),
            Slot::ok(0, 1, 1.0),
            Slot::ok(1, 0, 0.0),
            Slot::ok(1, 1, 1.0),
            Slot::ok(2, 0, 1.0),
            Slot::ok(2, 1, 1.0),
            Slot::blank(3, 0, "truncated"),
            Slot::blank(3, 1, "truncated"),
        ],
    )
    .await;

    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let out = &body["output"];
    let metric = &out["metrics"][0];
    assert_eq!(metric["key"], "pass");
    assert_eq!(metric["n"], 3, "three cases carry a value");
    assert_eq!(metric["missing"], 1, "the truncated case has none");
    // The mean of the case means is (1.0 + 0.5 + 1.0) / 3, never 3.5 / 5 (the row mean).
    assert!(close(&metric["mean"], 5.0 / 6.0), "{metric}");
    // A perf metric is a measured column of its own, never mixed into the quality metrics.
    assert_eq!(
        out["metrics"].as_array().expect("metrics").len(),
        1,
        "{out}"
    );
    let perf = &out["perf"][0];
    assert_eq!(perf["key"], "latency");
    assert_eq!(perf["role"], "perf");
    assert_eq!(perf["n"], 3);
    assert!(close(&perf["mean"], 120.0), "{perf}");
    assert!(close(
        &metric["ci_low"],
        5.0 / 6.0 - 1.96 * (0.5 / 6.0_f64).sqrt() / 3.0_f64.sqrt()
    ));
    assert!(close(
        &metric["ci_high"],
        5.0 / 6.0 + 1.96 * (0.5 / 6.0_f64).sqrt() / 3.0_f64.sqrt()
    ));
    assert_eq!(out["truncated"], 2);
    assert_eq!(out["graded"], 6);
    // The floor and the headroom are the HELD-OUT numbers: test cases 0 and 1, two reps each,
    // and a held-out mean of (1.0 + 0.5) / 2 — not the whole-run mean.
    assert!(close(&out["noise_floor"], 0.5), "{out}");
    assert!(close(&out["headroom"], 0.25), "{out}");
    assert_eq!(out["errors"]["total"], 0);
}

#[tokio::test]
async fn cost_follows_the_recorded_usage_and_reports_tokens_when_unpriced() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = instrument(&running, &["triage"]).await;
    let run_id = run_row(&pg, seeded.tenant, &eval_id, &version_id, 1, 1).await;

    let mut slot = Slot::ok(0, 0, 1.0);
    slot.usage = json!({ "prompt_tokens": 1000, "completion_tokens": 500 });
    slot.judge_usage = Some(json!({ "prompt_tokens": 200, "completion_tokens": 100 }));
    fill(&pg, seeded.tenant, &version_id, run_id, &[slot]).await;
    attempt(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        0,
        0,
        1,
        "serving",
        Some(json!({ "prompt_tokens": 10 })),
    )
    .await;
    attempt(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        0,
        0,
        2,
        "serving",
        Some(json!({ "no_tokens": true })),
    )
    .await;

    // ── priced: cents from the version's rates, never a stored number ──
    set_prices(
        &pg,
        seeded.tenant,
        &version_id,
        json!({ "input": 3.0, "output": 15.0 }),
    )
    .await;
    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let cost = &body["output"]["cost"];
    assert_eq!(cost["unit"], "cents");
    assert!(close(&cost["subject"], 1.05), "{cost}"); // (1000·3 + 500·15) / 10 000
    assert!(close(&cost["judge"], 0.21), "{cost}"); // (200·3 + 100·15) / 10 000
    assert!(close(&cost["failed"], 0.003), "{cost}"); // (10·3) / 10 000
    assert!(close(&cost["total"], 1.263), "{cost}");
    assert_eq!(
        cost["unpriced"], 1,
        "a usage with no token counts is not free"
    );

    // ── unpriced: tokens, so a version without rates still reports a spend ──
    set_prices(&pg, seeded.tenant, &version_id, json!({})).await;
    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let cost = &body["output"]["cost"];
    assert_eq!(cost["unit"], "tokens");
    assert!(close(&cost["subject"], 1500.0), "{cost}");
    assert!(close(&cost["judge"], 300.0), "{cost}");
    assert!(close(&cost["failed"], 10.0), "{cost}");
    assert_eq!(cost["unpriced"], 0);
}

#[tokio::test]
async fn truncated_and_failed_attempts_are_counted_never_averaged() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = instrument(&running, &["triage"]).await;
    let run_id = run_row(&pg, seeded.tenant, &eval_id, &version_id, 2, 1).await;

    fill(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        &[Slot::ok(0, 0, 1.0), Slot::blank(0, 1, "pending_label")],
    )
    .await;
    attempt(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        0,
        1,
        1,
        "timeout",
        None,
    )
    .await;

    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let out = &body["output"];
    // The single scored rep is the mean; the parked rep is not a zero.
    assert!(close(&out["metrics"][0]["mean"], 1.0), "{out}");
    assert_eq!(out["metrics"][0]["n"], 1);
    assert_eq!(out["truncated"], 0);
    assert_eq!(out["graded"], 1);
    assert_eq!(out["errors"]["total"], 1);
    assert_eq!(out["errors"]["failed_slots"], 1);
    assert_eq!(out["errors"]["by_class"]["timeout"], 1);
}

#[tokio::test]
async fn an_empty_run_reports_empty_honestly() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = instrument(&running, &["triage", "triage"]).await;
    let run_id = run_row(&pg, seeded.tenant, &eval_id, &version_id, 2, 2).await;

    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let out = &body["output"];
    let metric = &out["metrics"][0];
    assert_eq!(metric["n"], 0);
    assert!(metric["mean"].is_null(), "{metric}");
    assert!(metric["ci_low"].is_null(), "{metric}");
    assert!(metric["ci_high"].is_null(), "{metric}");
    assert!(
        out["noise_floor"].is_null(),
        "an empty run has no floor: {out}"
    );
    assert!(out["headroom"].is_null(), "{out}");
    assert!(close(&out["cost"]["total"], 0.0));
    assert_eq!(out["cost"]["unpriced"], 0);

    // An unknown run is a 404, never an empty report.
    let (status, body) = report(&running, Uuid::now_v7(), None).await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test]
async fn per_tag_groups_by_first_tag_and_honours_group_by_split() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = instrument(&running, &["triage", "triage", "billing"]).await;
    set_split(&pg, seeded.tenant, &version_id, &[0, 1]).await;
    let run_id = run_row(&pg, seeded.tenant, &eval_id, &version_id, 1, 3).await;
    fill(
        &pg,
        seeded.tenant,
        &version_id,
        run_id,
        &[
            Slot::ok(0, 0, 1.0),
            Slot::ok(1, 0, 0.0),
            Slot::ok(2, 0, 1.0),
        ],
    )
    .await;

    let (status, body) = report(&running, run_id, None).await;
    assert_eq!(status, 200, "{body}");
    let groups = body["output"]["per_tag"].as_array().expect("groups");
    let names: Vec<&str> = groups.iter().filter_map(|g| g["group"].as_str()).collect();
    assert_eq!(names, vec!["billing", "triage"], "{body}");
    let billing = &groups[0];
    assert_eq!(billing["n"], 1);
    assert!(close(&billing["metrics"][0]["mean"], 1.0), "{billing}");
    let triage = &groups[1];
    assert_eq!(triage["n"], 2);
    assert!(close(&triage["metrics"][0]["mean"], 0.5), "{triage}");

    // Grouping by split names the split, not the tag.
    let (status, body) = report(&running, run_id, Some("split")).await;
    assert_eq!(status, 200, "{body}");
    let groups = body["output"]["per_tag"].as_array().expect("groups");
    let names: Vec<&str> = groups.iter().filter_map(|g| g["group"].as_str()).collect();
    assert_eq!(names, vec!["test", "train"], "{body}");

    // Anything else is refused by name.
    let (status, body) = report(&running, run_id, Some("bucket")).await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().contains("group_by"), "{body}");
}
