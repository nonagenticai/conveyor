//! v1-w1a-4 (T0) — a job goes live only through an approved activation (SPEC_V2 §4.1, §4.3).
//!
//! `cargo test -p conveyor-actions --test job_activation`.
//!
//! The finding: `job_catalogue.create` hard-wrote `status = 'draft'`, `JobUpdateIn` had no
//! status field and no activation existed, so `active` was unreachable through the API and
//! `job_releases.publish` compiled an uncurated draft into an immutable release. Now a
//! `status: "active"` update opens a `job_activation` gate, only an approved decision (by a
//! principal other than the requester) flips the job to `active`, and publishing or binding a
//! job that is not active is refused `409 job_not_live`.
//!
//! Everything runs through the public action surface (`conveyor_actions::invoke`'s HTTP door),
//! with the fixture rows the organisation spine needs seeded through the store.

mod support;

use conveyor_store::begin_scoped;
use conveyor_store::organisation::{NewIndustry, industry_create_tx};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

struct Fixture {
    pg: support::Pg,
    running: support::Running,
    seeded: support::Seeded,
    industry: Uuid,
    department: Uuid,
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// A tenant, its industry's one department, and a running app.
async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (industry, department) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let industry = industry_create_tx(
            &mut tx,
            seeded.tenant,
            &NewIndustry {
                key: "software".to_owned(),
                name: "Software".to_owned(),
                meta: json!({}),
            },
        )
        .await
        .expect("industry");
        let department = conveyor_store::actions::department_create(
            &mut tx,
            seeded.tenant,
            &conveyor_store::actions::NewDepartment {
                name: "delivery",
                parent_id: None,
                industry_id: Some(industry.id),
                ordinal: Some(1),
                is_primary: true,
            },
        )
        .await
        .expect("department");
        tx.commit().await.expect("commit");
        (industry.id, department.id)
    };
    Fixture {
        pg,
        running,
        seeded,
        industry,
        department,
    }
}

/// Create one draft position through the action door; return its id and version.
async fn create_job(fixture: &Fixture, key: &str) -> (String, i64) {
    let (status, body) = fixture
        .running
        .invoke(
            "job_catalogue.create",
            TOKEN,
            json!({
                "job_key": key,
                "industry_id": fixture.industry,
                "family": "engineering",
                "title": format!("{key} position"),
                "departments": [{
                    "department_id": fixture.department,
                    "ordinal": 1,
                    "is_primary": true
                }],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "draft", "{body}");
    let id = output(&body)["id"].as_str().expect("id").to_owned();
    let version = output(&body)["version"].as_i64().expect("version");
    (id, version)
}

/// The tenant's one pending job-activation gate.
async fn pending_activation(running: &support::Running) -> Value {
    let (status, body) = running
        .invoke(
            "approvals.list",
            TOKEN,
            json!({"status": "pending", "limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    let gates: Vec<&Value> = items
        .iter()
        .filter(|item| item["gate"] == "job_catalogue.update" && item["kind"] == "job_activation")
        .collect();
    assert_eq!(
        gates.len(),
        1,
        "expected exactly one activation gate: {body}"
    );
    gates[0].clone()
}

#[tokio::test]
async fn a_draft_job_cannot_be_published() {
    let fixture = fixture().await;
    let (job_id, _version) = create_job(&fixture, "draft_publish").await;

    let (status, body) = fixture
        .running
        .invoke(
            "job_releases.publish",
            TOKEN,
            json!({"job_id": job_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"].as_str().unwrap_or_default(),
        "job_not_live",
        "an uncurated draft never compiles into a pinned release: {body}"
    );
}

#[tokio::test]
async fn an_approved_activation_flips_draft_to_active_then_publish_succeeds() {
    let fixture = fixture().await;
    let (job_id, version) = create_job(&fixture, "curated_job").await;

    // Requesting activation opens the gate and leaves the job a draft.
    let (status, body) = fixture
        .running
        .invoke(
            "job_catalogue.update",
            TOKEN,
            json!({"id": job_id, "version": version, "status": "active"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["status"],
        "draft",
        "activation waits for another principal: {body}"
    );
    let draft_version = output(&body)["version"].as_i64().expect("version");

    let gate = pending_activation(&fixture.running).await;
    assert_eq!(gate["status"], "pending", "{gate}");
    assert_eq!(
        gate["requested_by"],
        fixture.seeded.user_id.to_string(),
        "the gate names the person who requested activation: {gate}"
    );
    // The gate's meta is what the approved decision flips; it must name this job.
    let named: Option<String> =
        sqlx::query_scalar("SELECT meta->>'job_id' FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(fixture.seeded.tenant_id)
            .bind(Uuid::parse_str(gate["id"].as_str().expect("gate id")).expect("uuid"))
            .fetch_one(&fixture.pg.super_pool)
            .await
            .expect("gate row");
    assert_eq!(named.as_deref(), Some(job_id.as_str()));

    // The requester cannot decide their own activation (§5.4, unconditional on this lane).
    let (status, refused) = fixture
        .running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("requester"),
        "{refused}"
    );

    let (status, decided) = fixture
        .running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(output(&decided)["status"], "approved", "{decided}");

    let (status, read) = fixture
        .running
        .invoke(
            "job_catalogue.get",
            TOKEN,
            json!({"id": job_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["status"], "active", "{read}");
    let active_version = output(&read)["version"].as_i64().expect("version");
    assert_eq!(
        active_version,
        draft_version + 1,
        "activation moves the version, so a stale If-Match cannot pass: {read}"
    );

    let (status, published) = fixture
        .running
        .invoke(
            "job_releases.publish",
            TOKEN,
            json!({"job_id": job_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert!(
        output(&published)["release_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty()),
        "{published}"
    );

    // Activating a job that is already active is refused `409 job_not_draft`.
    let (status, refused) = fixture
        .running
        .invoke(
            "job_catalogue.update",
            TOKEN,
            json!({"id": job_id, "version": active_version, "status": "active"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"].as_str().unwrap_or_default(),
        "job_not_draft",
        "{refused}"
    );
}

#[tokio::test]
async fn the_requester_cannot_decide_their_own_activation() {
    let fixture = fixture().await;
    let (job_id, version) = create_job(&fixture, "self_approval").await;

    let (status, body) = fixture
        .running
        .invoke(
            "job_catalogue.update",
            TOKEN,
            json!({"id": job_id, "version": version, "status": "active"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let gate = pending_activation(&fixture.running).await;
    assert_eq!(
        gate["requested_by"],
        fixture.seeded.user_id.to_string(),
        "{gate}"
    );

    let (status, refused) = fixture
        .running
        .invoke(
            "approvals.decide",
            TOKEN,
            json!({"approval_id": gate["id"], "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("requester"),
        "{refused}"
    );

    let (status, read) = fixture
        .running
        .invoke(
            "job_catalogue.get",
            TOKEN,
            json!({"id": job_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        output(&read)["status"],
        "draft",
        "a refused self-approval activates nothing: {read}"
    );
}

#[tokio::test]
async fn an_unknown_status_is_refused() {
    let fixture = fixture().await;
    let (job_id, version) = create_job(&fixture, "status_probe").await;

    for status in ["bogus", "retired", "draft"] {
        let (http, body) = fixture
            .running
            .invoke(
                "job_catalogue.update",
                TOKEN,
                json!({"id": job_id, "version": version, "status": status}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(http, 422, "{status}: {body}");
        assert!(
            body["errors"].as_array().is_some_and(|rows| rows
                .iter()
                .any(|row| row["field"] == "status" && row["code"] == "invalid_value")),
            "{status}: {body}"
        );
    }
}
