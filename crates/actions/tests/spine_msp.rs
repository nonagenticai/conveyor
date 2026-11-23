//! #305 — the MSP edit/delete surface over real HTTP and real Postgres.
//!
//! T1 built the MSP rows and their create/read paths; this unit makes every stored record
//! correctable and removable, and makes a tranche review's decision and evidence durable. Each
//! test builds its own tenant through [`support::seed`]; every behaviour under test goes through
//! an action, and the superuser pool is used only to count rows a cascade should have removed.

mod support;

use serde_json::{Value, json};

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            "dev:alice",
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

async fn create_project(running: &support::Running, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// A project inside a programme: only such a project's WBS nodes are the programme's outputs.
async fn create_programme_project(
    running: &support::Running,
    programme: &str,
    title: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({
                "title": title,
                "project_type_code": "delivery",
                "programme_id": programme
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// The output a benefits-map edge names: a `wbs_node` of the programme.
async fn create_output(running: &support::Running, project: &str, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "wbs.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "deliverable",
                "title": title,
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
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// A job the benefit profile can name as its BCM owner (the completeness rule requires one).
async fn create_owner_job(pg: &support::Pg, seeded: &support::Seeded) -> uuid::Uuid {
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

async fn create_benefit(
    running: &support::Running,
    programme: &str,
    job: uuid::Uuid,
) -> (String, String) {
    let (status, headers, body) = running
        .invoke_full(
            "benefits.create",
            "dev:alice",
            json!({
                "programme_id": programme,
                "reference": "BEN-T",
                "description": "A measured benefit",
                "baseline": {"value": 14},
                "target": {"value": 8},
                "measurement_method": "weekly timestamps",
                "benefit_owner_job_id": job,
                "capabilities": ["delivery"],
                "operational_area": "delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the benefit carries an ETag");
    (id_of(output(&body), "id"), etag)
}

async fn create_tranche(
    running: &support::Running,
    programme: &str,
    number: i32,
) -> (String, String) {
    let (status, headers, body) = running
        .invoke_full(
            "tranches.create",
            "dev:alice",
            json!({"programme_id": programme, "number": number}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the tranche carries an ETag");
    (id_of(output(&body), "id"), etag)
}

/// P1.1 — an edit needs the current version; a stale or absent precondition writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn benefit_update_requires_current_version() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Edit programme").await;
    let job = create_owner_job(&pg, &seeded).await;
    let (benefit, etag) = create_benefit(&running, &programme, job).await;

    let (status, stale) = running
        .invoke(
            "benefits.update",
            "dev:alice",
            json!({"id": benefit, "category": "efficiency"}),
            Some("\"999\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
    assert_eq!(
        stale["status"], 412,
        "the refusal carries its status: {stale}"
    );

    let (status, missing) = running
        .invoke(
            "benefits.update",
            "dev:alice",
            json!({"id": benefit, "category": "efficiency"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{missing}");

    let (status, headers, updated) = running
        .invoke_full(
            "benefits.update",
            "dev:alice",
            json!({
                "id": benefit,
                "category": "efficiency",
                "description": "A corrected benefit",
                "status": "active"
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["category"], "efficiency");
    assert_eq!(output(&updated)["status"], "active");
    assert_eq!(output(&updated)["description"], "A corrected benefit");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"2\""),
        "the version increments once"
    );
}

/// P1.2 — deleting a benefit removes its links and its realization record.
#[tokio::test(flavor = "multi_thread")]
async fn delete_benefit_cascades_links_and_transition() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Cascade programme").await;
    let project = create_programme_project(&running, &programme, "Cascade project").await;
    let node = create_output(&running, &project, "T1 plan spine").await;
    let job = create_owner_job(&pg, &seeded).await;
    let (benefit, etag) = create_benefit(&running, &programme, job).await;

    let (status, linked) = running
        .invoke(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": programme,
                "benefit_id": benefit,
                "output_ref": node,
                "capability_ref": "WBS and baseline",
                "operational_outcome": "the plan is managed",
                "strategic_objective": "faster IT delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{linked}");

    let (status, transition) = running
        .invoke(
            "benefits.transition_upsert",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "baseline_measured_at": "2026-11-01T00:00:00Z",
                "readiness": {"state": "ready"}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{transition}");

    // #283: destructive; the gate names this object.
    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "benefits.delete",
            "dev:alice",
            json!({"id": benefit}),
            Some(etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");

    let (status, gone) = running
        .invoke(
            "benefits.get",
            "dev:alice",
            json!({"id": benefit}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{gone}");
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_link")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count links");
    let transitions: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_transition")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count transitions");
    assert_eq!((links, transitions), (0, 0), "the cascade left rows");
}

/// P1.3 — the benefits map is readable, and an edge can be removed.
#[tokio::test(flavor = "multi_thread")]
async fn benefit_links_are_listable_and_removable() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Map read programme").await;
    let project = create_programme_project(&running, &programme, "Map read project").await;
    let node = create_output(&running, &project, "T1 plan spine").await;
    let job = create_owner_job(&pg, &seeded).await;
    let (benefit, _etag) = create_benefit(&running, &programme, job).await;

    let (status, headers, linked) = running
        .invoke_full(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": programme,
                "benefit_id": benefit,
                "output_ref": node,
                "capability_ref": "WBS and baseline",
                "operational_outcome": "the plan is managed",
                "strategic_objective": "faster IT delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{linked}");
    assert_eq!(output(&linked)["output_ref"], node);
    let link = id_of(output(&linked), "id");
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the edge carries an ETag");

    let (status, listed) = running
        .invoke(
            "benefits.link_list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        output(&listed)["items"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
    assert_eq!(output(&listed)["items"][0]["benefit_id"], benefit);

    // #283: destructive; the gate names this object.
    let (status, unlinked) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "benefits.unlink",
            "dev:alice",
            json!({"id": link}),
            Some(etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{unlinked}");
    let (status, listed) = running
        .invoke(
            "benefits.link_list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(output(&listed)["items"].as_array().map(Vec::len), Some(0));
}

/// P1.4 — a tranche can be corrected, its review persists the decision and evidence, and a
/// reviewed tranche cannot be deleted.
#[tokio::test(flavor = "multi_thread")]
async fn tranche_review_persists_decision_and_evidence() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Review programme").await;
    let (tranche, etag) = create_tranche(&running, &programme, 0).await;
    // U4: a `continue` authorises the next tranche, so the plan needs one to authorise.
    let _next = create_tranche(&running, &programme, 1).await;

    let (status, updated) = running
        .invoke(
            "tranches.update",
            "dev:alice",
            json!({
                "id": tranche,
                "entry_criteria": ["mandate approved"],
                "exit_criteria": ["definition set accepted"],
                "review_at": "2026-12-04T00:00:00Z"
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(
        output(&updated)["entry_criteria"][0],
        "mandate approved",
        "{updated}"
    );
    let delegation = support::delegation_ref(&running, "dev:alice").await;
    let mut evidence = support::tranche_review_evidence(
        &pg,
        &seeded,
        uuid::Uuid::parse_str(&tranche).expect("tranche id"),
    )
    .await;
    // v1-tr2-2: the review names the snapshot it judged.
    evidence["snapshot_sha"] = json!("0123456789abcdef0123456789abcdef01234567");
    let baseline = evidence["baselines"][0].clone();
    let input = json!({
        "id": tranche,
        "decision": "continue",
        "delegation_ref": delegation,
        "evidence": evidence
    });
    let gate = support::review_gate(&running, "dev:alice", &input).await;
    let (status, reviewed) = running
        .invoke_approved("tranches.review", "dev:alice", input, None, gate)
        .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(output(&reviewed)["state"], "closed");
    let review_version = headers_version(&reviewed);

    let (status, reviews) = running
        .invoke(
            "tranches.review_list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{reviews}");
    let rows = output(&reviews)["items"].as_array().expect("review items");
    assert_eq!(rows.len(), 1, "{reviews}");
    assert_eq!(rows[0]["tranche_id"], tranche);
    assert_eq!(rows[0]["decision"], "continue");
    assert_eq!(rows[0]["evidence"]["baselines"][0], baseline);

    let tranche_etag = format!("\"{review_version}\"");
    // #283: the gate fires before the handler, so the refusal under test is reached through one.
    let (status, refused) = running
        .invoke_gated(
            &_pg,
            _seeded.tenant,
            "tranches.delete",
            "dev:alice",
            json!({"id": tranche}),
            Some(tranche_etag.as_str()),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "conflict");
}

fn headers_version(body: &Value) -> i64 {
    body["version"]
        .as_i64()
        .or_else(|| output(body)["version"].as_i64())
        .expect("the row carries a version")
}

/// P1.5 — cadence, WBS, register and stakeholder profile can be corrected and removed; the
/// two dependent-row refusals answer 409.
#[tokio::test(flavor = "multi_thread")]
async fn msp_deletes_and_refusals() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Delete programme").await;
    let project = create_project(&running, "Delete project").await;
    let job = create_owner_job(&pg, &seeded).await;

    // Cadence: create → update → delete.
    let (status, headers, cadence) = running
        .invoke_full(
            "cadences.create",
            "dev:alice",
            json!({"programme_id": programme, "kind": "pi", "period": "PI-2027.1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{cadence}");
    let cadence_id = id_of(output(&cadence), "id");
    let (status, updated) = running
        .invoke(
            "cadences.update",
            "dev:alice",
            json!({"id": cadence_id, "period": "PI-2027.2", "state": "active"}),
            Some(headers.get("etag").expect("cadence etag").as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["period"], "PI-2027.2");
    let cadence_version = headers_version(&updated);
    let cadence_etag = format!("\"{cadence_version}\"");
    // #283: destructive; the gate names this object.
    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "cadences.delete",
            "dev:alice",
            json!({"id": cadence_id}),
            Some(cadence_etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");

    // WBS: a parent with a child cannot be deleted; the child can.
    let (status, headers, parent) = running
        .invoke_full(
            "wbs.create",
            "dev:alice",
            json!({
                "project_id": project, "kind": "deliverable", "title": "Parent",
                "business_value": 1, "time_criticality": 1, "risk_reduction": 1, "job_size": 1
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{parent}");
    let parent_id = id_of(output(&parent), "id");
    let parent_etag = headers.get("etag").cloned().expect("parent etag");
    let (status, headers, child) = running
        .invoke_full(
            "wbs.create",
            "dev:alice",
            json!({
                "project_id": project, "parent_id": parent_id, "kind": "work_package",
                "title": "Child", "business_value": 1, "time_criticality": 1,
                "risk_reduction": 1, "job_size": 1
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{child}");
    let child_id = id_of(output(&child), "id");
    let child_etag = headers.get("etag").cloned().expect("child etag");
    // #283: destructive; the gate names this object.
    let (status, refused) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "wbs.delete",
            "dev:alice",
            json!({"id": parent_id}),
            Some(parent_etag.as_str()),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    // #283: destructive; the gate names this object.
    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "wbs.delete",
            "dev:alice",
            json!({"id": child_id}),
            Some(child_etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");

    // Register: a row a benefit profile indexes is refused; an unindexed row deletes.
    let (status, headers, register) = running
        .invoke_full(
            "registers.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "risk",
                "title": "Indexed risk",
                "probability": "medium",
                "impact": 3,
                "owner_job_id": job,
                "review_at": "2026-12-01",
                "links": [{"kind": "objective", "ref": "reduce cycle time"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{register}");
    let register_id = id_of(output(&register), "id");
    let register_etag = headers.get("etag").cloned().expect("register etag");
    let (status, profile) = running
        .invoke(
            "benefits.create",
            "dev:alice",
            json!({
                "programme_id": programme,
                "register_item_id": register_id,
                "reference": "BEN-R",
                "baseline": {"value": 1},
                "target": {"value": 2},
                "measurement_method": "weekly",
                "benefit_owner_job_id": job,
                "capabilities": ["delivery"],
                "operational_area": "delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{profile}");
    // #283: the gate fires before the handler, so the refusal under test is reached through one.
    let (status, refused) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "registers.delete",
            "dev:alice",
            json!({"id": register_id}),
            Some(register_etag.as_str()),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    let (status, headers, free) = running
        .invoke_full(
            "registers.create",
            "dev:alice",
            json!({"project_id": project, "kind": "issue", "title": "Free issue"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{free}");
    let free_id = id_of(output(&free), "id");
    let free_etag = headers.get("etag").cloned().expect("free register etag");
    // #283: destructive; the gate names this object.
    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "registers.delete",
            "dev:alice",
            json!({"id": free_id}),
            Some(free_etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");

    // Stakeholder profile: full update, then delete.
    let (status, headers, profile) = running
        .invoke_full(
            "stakeholders.profile_create",
            "dev:alice",
            json!({
                "programme_id": programme,
                "name": "Sponsoring group",
                "importance": "high",
                "influence": "high",
                "interest": "high",
                "support_level": "medium"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{profile}");
    let profile_id = id_of(output(&profile), "id");
    let (status, updated) = running
        .invoke(
            "stakeholders.profile_update",
            "dev:alice",
            json!({"id": profile_id, "importance": "critical", "support_level": "high"}),
            Some(headers.get("etag").expect("profile etag").as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["importance"], "critical");
    let profile_version = headers_version(&updated);
    let profile_etag = format!("\"{profile_version}\"");
    // #283: destructive; the gate names this object.
    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "stakeholders.profile_delete",
            "dev:alice",
            json!({"id": profile_id}),
            Some(profile_etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");
}

/// P1.6 — a realization record cannot claim a post-transition measurement without a measured
/// baseline, and the timestamp alone is not the reading: the `post` measurement must exist.
#[tokio::test(flavor = "multi_thread")]
async fn transition_requires_baseline_before_post() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Realization programme").await;
    let job = create_owner_job(&pg, &seeded).await;
    let (benefit, _etag) = create_benefit(&running, &programme, job).await;

    let (status, refused) = running
        .invoke(
            "benefits.transition_upsert",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "post_measured_at": "2026-12-01T00:00:00Z",
                "evidence": ["weekly measure"]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused.to_string().contains("benefit_baseline_missing"),
        "{refused}"
    );

    let (status, headers, created) = running
        .invoke_full(
            "benefits.transition_upsert",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "baseline_measured_at": "2026-11-01T00:00:00Z",
                "enacted_at": "2026-11-20T00:00:00Z",
                "readiness": {"state": "ready"}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{created}");
    let etag = headers.get("etag").cloned().expect("transition etag");

    // A post timestamp no reading backs is refused: the evaluation cannot reproduce it.
    let (status, refused) = running
        .invoke(
            "benefits.transition_upsert",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "post_measured_at": "2026-12-01T00:00:00Z",
                "evidence": ["weekly measure"]
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("benefit_post_measurement_missing"),
        "{refused}"
    );

    let (status, measured) = running
        .invoke(
            "benefits.measure",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "kind": "post",
                "target_value": 8.0,
                "actual_value": 9.0,
                "unit": "hours",
                "method": "weekly measure",
                "measured_at": "2026-11-25T00:00:00Z"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{measured}");

    let (status, updated) = running
        .invoke(
            "benefits.transition_upsert",
            "dev:alice",
            json!({
                "benefit_id": benefit,
                "post_measured_at": "2026-12-01T00:00:00Z",
                "evidence": ["weekly measure"]
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["post_measured_at"], "2026-12-01T00:00:00Z");

    let (status, listed) = running
        .invoke(
            "benefits.transition_list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(output(&listed)["items"].as_array().map(Vec::len), Some(1));
}

/// P1.7 — the new rows are tenant-scoped: dev-b cannot read, edit or delete dev-a's.
#[tokio::test(flavor = "multi_thread")]
async fn another_tenants_msp_rows_are_invisible() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Tenant A programme").await;
    let job = create_owner_job(&pg, &seeded).await;
    let (benefit, etag) = create_benefit(&running, &programme, job).await;

    let (status, body) = running
        .invoke(
            "benefits.get",
            "dev:bob",
            json!({"id": benefit}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = running
        .invoke(
            "benefits.update",
            "dev:bob",
            json!({"id": benefit, "description": "hijacked"}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    // #283: bob is in ANOTHER tenant, so his gate is opened there — then his delete still has to
    // answer 404: the approval buys nothing outside the tenant that holds the object.
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let bob_gate = support::approved_gate(
        &pg,
        bob.tenant,
        &running,
        "dev:bob",
        "benefits.delete",
        &json!({"id": benefit}),
    )
    .await;
    let (status, body) = running
        .invoke_approved(
            "benefits.delete",
            "dev:bob",
            json!({"id": benefit}),
            Some(etag.as_str()),
            bob_gate,
        )
        .await;
    assert_eq!(status, 404, "{body}");
}
