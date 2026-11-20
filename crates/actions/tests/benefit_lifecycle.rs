//! #303 — the benefit profile's life-cycle: edit under change control, retire with a reason.
//!
//! A draft profile is editable; once approved, its four measurement fields (baseline, target,
//! method, BCM owner) change only through an approved change request, and the profile and the
//! change then cite each other. Retirement is terminal and never a delete: the row leaves the
//! default list and map but remains readable by id.
//!
//! The change row and its pending gate are seeded directly because `changes.request` needs T3's PM
//! job binding, which is not this unit's subject; the gate is then approved through
//! `approvals.decide` as the second principal (`dev:reviewer`), so the approval that authorises
//! the measurement change is the real one.
//!
//! `rbuild -- test -p conveyor-actions --test benefit_lifecycle`.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";
/// The second admin in the primary tenant (`support::seed`), so the change gate is decided by a
/// principal other than the one that would own the profile.
const REVIEWER: &str = "dev:reviewer";

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

async fn create_project(running: &support::Running, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
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

/// Create one complete profile (the four measurement fields are mandatory).
async fn create_benefit(
    running: &support::Running,
    programme: &str,
    project: Option<&str>,
    reference: &str,
    status: &str,
    owner: uuid::Uuid,
) -> (String, i64) {
    let (status_code, body) = running
        .invoke(
            "benefits.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "project_id": project,
                "reference": reference,
                "description": format!("{reference} benefit"),
                "baseline": {"value": 14, "measured_at": "2026-11-01"},
                "target": {"value": 8},
                "measurement_method": "card and run timestamps, weekly",
                "benefit_owner_job_id": owner,
                "status": status,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status_code, 201, "{body}");
    let out = output(&body);
    (text(out, "id"), out["version"].as_i64().expect("a version"))
}

async fn get_benefit(running: &support::Running, id: &str) -> Value {
    let (status, body) = running
        .invoke("benefits.get", TOKEN, json!({"id": id}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

async fn list_benefits(
    running: &support::Running,
    programme: &str,
    include_retired: bool,
) -> Vec<Value> {
    let (status, body) = running
        .invoke(
            "benefits.list",
            TOKEN,
            json!({"programme_id": programme, "include_retired": include_retired}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["items"].as_array().expect("a page").clone()
}

/// An approved change request on `project`: the change row names its baseline and impact (0044's
/// CHECK), and its `change` gate is approved by the second principal through the real action.
async fn approved_change(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
) -> String {
    let (status, wbs) = running
        .invoke(
            "wbs.create",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "deliverable",
                "title": "The deliverable the change protects",
                "business_value": 3,
                "time_criticality": 2,
                "risk_reduction": 1,
                "job_size": 2
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{wbs}");
    let (status, baseline) = running
        .invoke(
            "baselines.take",
            TOKEN,
            json!({"project_id": project, "kind": "scope"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{baseline}");
    let baseline = text(output(&baseline), "id");

    let change: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item
            (tenant_id, project_id, kind, title, impact, baseline_ref, meta)
         VALUES ($1, $2::uuid, 'change', 'Change the baseline', 3, $3::uuid,
                 '{\"impact\": 3}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(&baseline)
    .fetch_one(&pg.super_pool)
    .await
    .expect("change row");
    let approval: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, required_decisions, meta)
         VALUES ($1, $2::uuid, 'change', 'pm', 1, $3)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(json!({"change_id": change.to_string(), "description": "Approve the change"}))
    .fetch_one(&pg.super_pool)
    .await
    .expect("approval row");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    change.to_string()
}

/// P1.1 — a draft profile is editable, and no other row moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_profile_is_editable() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Draft programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, version) =
        create_benefit(&running, &programme, None, "BEN-01", "draft", owner).await;
    let (other, _) = create_benefit(&running, &programme, None, "BEN-02", "draft", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({
                "id": benefit,
                "description": "Edited on a draft",
                "target": {"value": 6},
                "measurement_method": "monthly",
                "benefit_owner_job_id": owner,
            }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(version, 1);
    assert_eq!(output(&body)["version"], 2, "{body}");
    assert_eq!(output(&body)["description"], "Edited on a draft", "{body}");
    assert_eq!(output(&body)["target"]["value"], 6, "{body}");
    assert_eq!(output(&body)["measurement_method"], "monthly", "{body}");

    let untouched = get_benefit(&running, &other).await;
    assert_eq!(untouched["version"], 1, "no other row moves: {untouched}");
    assert_eq!(
        untouched["description"], "BEN-02 benefit",
        "no other row moves: {untouched}"
    );
}

/// P1.2 — an approved profile's measurement fields need an approved change request; the refusal
/// writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_profile_needs_a_change_request() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Approved programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, _) = create_benefit(&running, &programme, None, "BEN-01", "active", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "baseline": {"value": 12}}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "benefit_change_required", "{body}");

    let after = get_benefit(&running, &benefit).await;
    assert_eq!(after["version"], 1, "the refusal wrote a row: {after}");
    assert_eq!(
        after["baseline"]["value"], 14,
        "the refusal moved the baseline: {after}"
    );
}

/// P1.2 (the approval-row reading) — an approved `benefit_profile` gate protects a profile that
/// is still `draft`: the approval primitive is a record of the decision, not only the status.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_benefit_profile_gate_approval_also_protects_the_measurement() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Gate programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, _) = create_benefit(&running, &programme, None, "BEN-01", "draft", owner).await;
    sqlx::query(
        "INSERT INTO approval (tenant_id, gate, kind, status, required_decisions, meta)
         VALUES ($1, 'benefit_profile', 'pm', 'approved', 1, $2)",
    )
    .bind(seeded.tenant_id)
    .bind(json!({"benefit_id": benefit}))
    .execute(&pg.super_pool)
    .await
    .expect("approved profile gate");

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "baseline": {"value": 12}}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "benefit_change_required", "{body}");
    assert_eq!(
        get_benefit(&running, &benefit).await["version"],
        1,
        "the refusal wrote a row"
    );
}

/// P1.3 — an approved change request advances the measurement, and the profile and the change
/// cite each other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_request_advances_the_profile() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Change programme").await;
    let project = create_project(&running, "Change project").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, _) = create_benefit(
        &running,
        &programme,
        Some(&project),
        "BEN-01",
        "active",
        owner,
    )
    .await;
    let change = approved_change(&pg, &running, &seeded, &project).await;

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "baseline": {"value": 12}, "change_id": change}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["change_ref"], json!(change), "{body}");
    assert_eq!(output(&body)["baseline"]["value"], 12, "{body}");

    let links: Value =
        sqlx::query_scalar("SELECT links FROM project_register_item WHERE id = $1::uuid")
            .bind(&change)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the change row");
    assert!(
        links.to_string().contains(&benefit),
        "the change must cite the profile back: {links}"
    );
}

/// P2.1 — retirement is terminal and leaves the default map; the row stays readable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retired_profile_leaves_the_map() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Retire programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, _) = create_benefit(&running, &programme, None, "BEN-01", "active", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.retire",
            TOKEN,
            json!({"id": benefit, "reason": "superseded by BEN-02"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "retired", "{body}");
    assert_eq!(
        output(&body)["retired_reason"],
        "superseded by BEN-02",
        "{body}"
    );

    let listed = list_benefits(&running, &programme, false).await;
    assert!(
        !listed.iter().any(|item| item["id"] == json!(benefit)),
        "a retired profile is in the default map: {listed:?}"
    );
    let all = list_benefits(&running, &programme, true).await;
    assert!(
        all.iter().any(|item| item["id"] == json!(benefit)),
        "include_retired must return it: {all:?}"
    );
    let read = get_benefit(&running, &benefit).await;
    assert_eq!(read["status"], "retired", "{read}");
    assert_eq!(read["retired_reason"], "superseded by BEN-02", "{read}");
}

/// P2.2 — a profile in an open transition cannot be retired; once the transition retires, it can.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_in_transition_cannot_retire() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Transition programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, _) = create_benefit(&running, &programme, None, "BEN-01", "active", owner).await;
    let transition: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO benefit_transition (tenant_id, benefit_id) VALUES ($1, $2::uuid) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(&benefit)
    .fetch_one(&pg.super_pool)
    .await
    .expect("transition row");

    let (status, body) = running
        .invoke(
            "benefits.retire",
            TOKEN,
            json!({"id": benefit, "reason": "superseded"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "benefit_in_transition", "{body}");
    assert_eq!(get_benefit(&running, &benefit).await["status"], "active");

    sqlx::query("UPDATE benefit_transition SET retired_at = now() WHERE id = $1::uuid")
        .bind(transition)
        .execute(&pg.super_pool)
        .await
        .expect("retire the transition");
    let (status, body) = running
        .invoke(
            "benefits.retire",
            TOKEN,
            json!({"id": benefit, "reason": "superseded"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "retired", "{body}");
}

/// P1.1/P1.2 — `realized` is gated on the realization record: both the baseline and the
/// post-transition measurement must exist, and a refused update leaves the row untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_realized_status_needs_a_complete_transition() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Realization programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (benefit, version) =
        create_benefit(&running, &programme, None, "BEN-01", "active", owner).await;

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "status": "realized"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_baseline_missing",
        "{body}"
    );
    let unchanged = get_benefit(&running, &benefit).await;
    assert_eq!(unchanged["status"], "active", "{unchanged}");
    assert_eq!(unchanged["version"].as_i64(), Some(version), "{unchanged}");

    sqlx::query(
        "INSERT INTO benefit_transition (tenant_id, benefit_id, baseline_measured_at)
         VALUES ($1, $2::uuid, now())",
    )
    .bind(seeded.tenant_id)
    .bind(&benefit)
    .execute(&pg.super_pool)
    .await
    .expect("transition row");

    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "status": "realized"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "a baseline alone cannot claim realization: {body}"
    );
    assert_eq!(
        body["errors"][0]["code"], "benefit_baseline_missing",
        "{body}"
    );

    sqlx::query(
        "UPDATE benefit_transition SET post_measured_at = now() WHERE benefit_id = $1::uuid",
    )
    .bind(&benefit)
    .execute(&pg.super_pool)
    .await
    .expect("post-transition measurement");
    let (status, body) = running
        .invoke(
            "benefits.update",
            TOKEN,
            json!({"id": benefit, "status": "realized"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "realized", "{body}");
}

/// P2.1 — a profile is never born realized: the realization record can only reference a row that
/// already exists, so creation refuses the status outright.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_created_benefit_cannot_start_realized() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Born-realized programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (status, body) = running
        .invoke(
            "benefits.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "reference": "BEN-01",
                "description": "BEN-01 benefit",
                "baseline": {"value": 14, "measured_at": "2026-11-01"},
                "target": {"value": 8},
                "measurement_method": "card and run timestamps, weekly",
                "benefit_owner_job_id": owner,
                "status": "realized",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "benefit_baseline_missing",
        "{body}"
    );
    let listed = list_benefits(&running, &programme, true).await;
    assert!(
        listed.is_empty(),
        "a profile must not exist after the refusal: {listed:?}"
    );
}
