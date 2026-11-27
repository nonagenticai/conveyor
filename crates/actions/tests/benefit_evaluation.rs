//! Benefit evaluation (0071) — a realization cites measurements, not timestamps.
//!
//! A benefit is realisable only when it has a `baseline` reading and a `post` reading taken
//! after the transition was enacted. `benefits.transition_upsert` refuses to record a
//! post-transition timestamp or retirement that no reading backs, and `metrics.programme`
//! reports the latest reading's variance and evaluation status.
//!
//! `cargo-guard.py check -p conveyor-actions --tests` compiles these; run with
//! `cargo-guard.py test -p conveyor-actions --test benefit_evaluation` for the behavioural gate.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
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

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            TOKEN,
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// The BCM owner is a real `job_catalogue` row; §8.3 refuses a profile without one.
async fn owner_job(pg: &support::Pg, seeded: &support::Seeded) -> uuid::Uuid {
    let industry: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("industry");
    sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, title)
         VALUES ($1, 'bcm', $2, 'Benefit owner') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("job")
}

/// A complete profile: baseline, target, method, owner, capabilities and area are mandatory.
async fn create_benefit(
    running: &support::Running,
    programme: &str,
    reference: &str,
    owner: uuid::Uuid,
) -> String {
    let (status, body) = running
        .invoke(
            "benefits.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "reference": reference,
                "baseline": {"value": 14, "measured_at": "2026-11-01"},
                "target": {"value": 8},
                "measurement_method": "card and run timestamps, weekly",
                "benefit_owner_job_id": owner,
                "capabilities": ["delivery"],
                "operational_area": "delivery",
                "status": "active",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn measure(
    running: &support::Running,
    benefit: &str,
    kind: &str,
    target: f64,
    actual: f64,
    measured_at: &str,
) -> (u16, Value) {
    running
        .invoke(
            "benefits.measure",
            TOKEN,
            json!({
                "benefit_id": benefit,
                "kind": kind,
                "target_value": target,
                "actual_value": actual,
                "unit": "hours",
                "method": "median of the weekly run timestamps",
                "measured_at": measured_at,
                "evidence": {"ref": format!("{kind}-reading")}
            }),
            None,
            None,
            None,
        )
        .await
}

async fn realise(pg: &support::Pg, running: &support::Running, benefit: &str) -> (u16, Value) {
    // `benefits.measure` bumps the profile's version; a hardcoded `"1"` makes every update
    // after a measurement a 412 version_mismatch. Send the version that is actually current.
    let version: i64 = sqlx::query_scalar("SELECT version FROM benefit_profile WHERE id = $1")
        .bind(uuid::Uuid::parse_str(benefit).expect("the benefit id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the benefit version");
    let if_match = format!("\"{version}\"");
    running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "status": "realized"}),
            Some(&if_match),
            None,
            None,
        )
        .await
}

// ── P1: a benefit with no baseline measurement is not realisable ─────────────────────────

/// A post reading alone is not an improvement: without the baseline reading the claim names
/// nothing to improve on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_benefit_with_no_baseline_measurement_is_not_realisable() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Unmeasured baseline programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let benefit = create_benefit(&running, &programme, "BEN-01", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.transition_upsert",
            TOKEN,
            json!({"benefit_id": benefit, "enacted_at": "2026-11-20T00:00:00Z"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    // A post reading after the enactment, but no baseline reading.
    let (status, body) =
        measure(&running, &benefit, "post", 8.0, 9.0, "2026-11-21T00:00:00Z").await;
    assert_eq!(status, 201, "{body}");

    let (status, body) = realise(&pg, &running, &benefit).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_baseline_missing",
        "{body}"
    );
}

// ── P2: a baseline reading alone is not realisation; a post after enactment is ──────────

/// The baseline reading exists, but the only post reading is absent/before the enactment:
/// the update is refused. A post reading after `enacted_at` realises the benefit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_post_measurement_after_enactment_realises_the_benefit() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Realization programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let benefit = create_benefit(&running, &programme, "BEN-01", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.transition_upsert",
            TOKEN,
            json!({"benefit_id": benefit, "enacted_at": "2026-11-20T00:00:00Z"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = measure(
        &running,
        &benefit,
        "baseline",
        8.0,
        14.0,
        "2026-11-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");

    let (status, body) = realise(&pg, &running, &benefit).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_post_measurement_missing",
        "{body}"
    );

    // A post reading BEFORE the enactment is not the post-transition measurement either.
    let (status, body) = measure(
        &running,
        &benefit,
        "post",
        8.0,
        10.0,
        "2026-11-19T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = realise(&pg, &running, &benefit).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_post_measurement_missing",
        "{body}"
    );

    let (status, body) =
        measure(&running, &benefit, "post", 8.0, 9.0, "2026-11-21T00:00:00Z").await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = realise(&pg, &running, &benefit).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "realized", "{body}");
}

// ── P3: transition_upsert cannot record a post timestamp the readings do not back ───────

/// `benefits.transition_upsert` refuses a post-transition timestamp (and a retirement) when no
/// `post` reading follows the enactment, and accepts one when it does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transition_cannot_assert_a_post_measurement() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Transition programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let benefit = create_benefit(&running, &programme, "BEN-01", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.transition_upsert",
            TOKEN,
            json!({
                "benefit_id": benefit,
                "baseline_measured_at": "2026-11-01T00:00:00Z",
                "enacted_at": "2026-11-20T00:00:00Z",
                "post_measured_at": "2026-11-25T00:00:00Z"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_post_measurement_missing",
        "{body}"
    );

    let (status, body) =
        measure(&running, &benefit, "post", 8.0, 9.0, "2026-11-21T00:00:00Z").await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = running
        .invoke(
            "benefits.transition_upsert",
            TOKEN,
            json!({
                "benefit_id": benefit,
                "baseline_measured_at": "2026-11-01T00:00:00Z",
                "enacted_at": "2026-11-20T00:00:00Z",
                "post_measured_at": "2026-11-25T00:00:00Z"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["post_measured_at"],
        "2026-11-25T00:00:00Z",
        "{body}"
    );
}

// ── P4: metrics.programme carries variance and the evaluation status ────────────────────

/// A reduction target (8 below the 14 baseline) is on track at or below target and at risk
/// above it; a realized benefit reports `realised`; an unmeasured profile reports
/// `unmeasured`. Variance is `actual - target`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metrics_programme_reports_variance_and_evaluation() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Metrics programme").await;
    let owner = owner_job(&pg, &seeded).await;

    // Realised: baseline 14 → post 9 against target 8.
    let realised = create_benefit(&running, &programme, "BEN-01", owner).await;
    running
        .invoke(
            "benefits.transition_upsert",
            TOKEN,
            json!({"benefit_id": realised, "enacted_at": "2026-11-20T00:00:00Z"}),
            None,
            None,
            None,
        )
        .await;
    let (status, body) = measure(
        &running,
        &realised,
        "baseline",
        8.0,
        14.0,
        "2026-11-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = measure(
        &running,
        &realised,
        "post",
        8.0,
        9.0,
        "2026-11-21T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = realise(&pg, &running, &realised).await;
    assert_eq!(status, 200, "{body}");

    // Reduced below target: baseline 14 → interim 7 against target 8, on track.
    let on_track = create_benefit(&running, &programme, "BEN-02", owner).await;
    let (status, body) = measure(
        &running,
        &on_track,
        "baseline",
        8.0,
        14.0,
        "2026-11-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = measure(
        &running,
        &on_track,
        "interim",
        8.0,
        7.0,
        "2026-11-10T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");

    // Above target: baseline 14 → interim 11 against target 8, at risk.
    let at_risk = create_benefit(&running, &programme, "BEN-03", owner).await;
    let (status, body) = measure(
        &running,
        &at_risk,
        "baseline",
        8.0,
        14.0,
        "2026-11-01T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = measure(
        &running,
        &at_risk,
        "interim",
        8.0,
        11.0,
        "2026-11-10T00:00:00Z",
    )
    .await;
    assert_eq!(status, 201, "{body}");

    // No measurement at all.
    create_benefit(&running, &programme, "BEN-04", owner).await;

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
    let metrics = output(&body);
    let benefits = metrics["benefits"].as_array().expect("a benefit list");

    let by_reference = |reference: &str| {
        benefits
            .iter()
            .find(|row| row["reference"] == reference)
            .unwrap_or_else(|| panic!("no {reference} in {body}"))
    };

    let realised_row = by_reference("BEN-01");
    assert_eq!(realised_row["evaluation_status"], "realised", "{body}");
    assert_eq!(realised_row["variance"], 1.0, "{body}");

    let on_track_row = by_reference("BEN-02");
    assert_eq!(on_track_row["evaluation_status"], "on_track", "{body}");
    assert_eq!(on_track_row["variance"], -1.0, "{body}");

    let at_risk_row = by_reference("BEN-03");
    assert_eq!(at_risk_row["evaluation_status"], "at_risk", "{body}");
    assert_eq!(at_risk_row["variance"], 3.0, "{body}");

    let unmeasured_row = by_reference("BEN-04");
    assert_eq!(unmeasured_row["evaluation_status"], "unmeasured", "{body}");
    assert_eq!(unmeasured_row["variance"], Value::Null, "{body}");
}
