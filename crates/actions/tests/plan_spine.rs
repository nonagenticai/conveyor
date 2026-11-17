//! T1 — the plan spine's ten acceptance scenarios (issue #72) over real HTTP and real Postgres.
//!
//! Each test builds its own tenant through [`support::seed`]; the fixture is the S05/S08 shape:
//! an ephemeral Postgres 18, the two estate roles, the embedded migrations and a real server.
//! The assertions are the issue's: a baseline does not move, a change without impact is refused,
//! WSJF recomputes, the portfolio lists only the in-flight set, a cycle leaves no row, and the
//! MSP rows refuse a wish.
//!
//! The superuser pool (`pg.super_pool`) is used only to build state the ten scenarios assume
//! (a paused/closed project, an owner job) and to prove a column does not exist; every
//! behaviour under test goes through an action.

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

async fn create_wbs(running: &support::Running, project: &str, title: &str) -> String {
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

/// P1.1 — a taken baseline is frozen; later row changes do not move it.
#[tokio::test(flavor = "multi_thread")]
async fn a_baseline_does_not_move_when_rows_change() {
    let (pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Baseline project").await;
    let node = create_wbs(&running, &project, "The promised deliverable").await;

    let (status, body) = running
        .invoke(
            "baselines.take",
            "dev:alice",
            json!({"project_id": project, "kind": "scope"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let baseline = id_of(output(&body), "id");
    let snapshot = output(&body)["snapshot"].clone();
    assert_eq!(snapshot["wbs"][0]["title"], "The promised deliverable");

    // The threat model: the rows change afterwards. A direct row edit is the strongest form.
    sqlx::query("UPDATE wbs_node SET title = 'Renamed after the baseline', version = version + 1 WHERE id = $1::uuid")
        .bind(&node)
        .execute(&pg.super_pool)
        .await
        .expect("rename the node");

    let (status, read) = running
        .invoke(
            "baselines.get",
            "dev:alice",
            json!({"id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        output(&read)["snapshot"]["wbs"][0]["title"],
        "The promised deliverable",
        "the frozen snapshot moved with the rows"
    );
}

/// P1.2 — a change needs an impact block and the baseline it changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_change_request_without_impact_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Change project").await;
    let (status, body) = running
        .invoke(
            "baselines.take",
            "dev:alice",
            json!({"project_id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let baseline = id_of(output(&body), "id");

    let (status, refused) = running
        .invoke(
            "registers.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Move the gate",
                "baseline_ref": baseline
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");

    let (status, created) = running
        .invoke(
            "registers.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Move the gate",
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": 0, "schedule_days": 3, "cost": 0, "risk": "low"}}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(output(&created)["kind"], "change");
    assert_eq!(output(&created)["baseline_ref"], baseline);
}

/// P2.1 — WSJF is computed on read from the four inputs; no score column exists.
#[tokio::test(flavor = "multi_thread")]
async fn wsjf_recomputes_when_an_input_changes() {
    let (pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "WSJF project").await;
    let node = create_wbs(&running, &project, "Scored node").await;

    let (status, read) = running
        .invoke(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["wsjf"], 3.0, "{read}");

    // The ETag of the read is the If-Match an edit needs.
    let (status, headers, full) = running
        .invoke_full(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{full}");
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the read carries an ETag");

    let (status, updated) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({
                "id": node,
                "business_value": 6,
                "time_criticality": 2,
                "risk_reduction": 1,
                "job_size": 2
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["wsjf"], 4.5, "{updated}");

    let (status, read) = running
        .invoke(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["wsjf"], 4.5);

    // The claim is that the score is never stored: no column may carry it.
    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'wbs_node' AND column_name = 'wsjf'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the catalogue");
    assert_eq!(columns, 0, "a wsjf column exists; the score is stored");
}

/// P3.1 — the portfolio lists exactly the tenant's in-flight set.
#[tokio::test(flavor = "multi_thread")]
async fn the_portfolio_lists_only_the_in_flight_set() {
    let (pg, running, _seeded) = fixture().await;
    let active = create_project(&running, "Active").await;
    let paused = create_project(&running, "Paused").await;
    let closed = create_project(&running, "Closed").await;
    let archived = create_project(&running, "Archived").await;
    for (project, status) in [
        (paused.as_str(), "paused"),
        (closed.as_str(), "closed"),
        (archived.as_str(), "archived"),
    ] {
        sqlx::query("UPDATE project SET status = $1 WHERE id = $2::uuid")
            .bind(status)
            .bind(project)
            .execute(&pg.super_pool)
            .await
            .expect("set the project status");
    }

    let (status, body) = running
        .invoke(
            "portfolio.in_flight",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let ids: Vec<&str> = output(&body)["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default())
        .collect();
    assert!(ids.contains(&active.as_str()), "active is listed: {ids:?}");
    assert!(ids.contains(&paused.as_str()), "paused is listed: {ids:?}");
    assert!(
        !ids.contains(&closed.as_str()),
        "closed is not listed: {ids:?}"
    );
    assert!(
        !ids.contains(&archived.as_str()),
        "archived is not listed: {ids:?}"
    );
}

/// P4.1 — a two-node dependency cycle is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_two_node_dependency_cycle_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let a = create_project(&running, "A").await;
    let b = create_project(&running, "B").await;
    let (status, edge) = running
        .invoke(
            "project_dependencies.create",
            "dev:alice",
            json!({"from_project": a, "to_project": b, "kind": "blocks"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{edge}");
    let (status, refused) = running
        .invoke(
            "project_dependencies.create",
            "dev:alice",
            json!({"from_project": b, "to_project": a, "kind": "blocks"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused.to_string().contains("dependency_cycle"),
        "{refused}"
    );
}

/// P4.2 — a deep cycle is refused and leaves no partial row.
#[tokio::test(flavor = "multi_thread")]
async fn a_deep_dependency_cycle_leaves_no_row() {
    let (_pg, running, _seeded) = fixture().await;
    let a = create_project(&running, "A").await;
    let b = create_project(&running, "B").await;
    let c = create_project(&running, "C").await;
    let d = create_project(&running, "D").await;
    for (from, to) in [
        (a.as_str(), b.as_str()),
        (a.as_str(), c.as_str()),
        (b.as_str(), d.as_str()),
        (c.as_str(), d.as_str()),
    ] {
        let (status, body) = running
            .invoke(
                "project_dependencies.create",
                "dev:alice",
                json!({"from_project": from, "to_project": to, "kind": "blocks"}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }
    let (status, refused) = running
        .invoke(
            "project_dependencies.create",
            "dev:alice",
            json!({"from_project": d, "to_project": a, "kind": "blocks"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");

    let (status, list) = running
        .invoke(
            "project_dependencies.list",
            "dev:alice",
            json!({"project_id": a}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(
        output(&list)["items"].as_array().map(|items| items.len()),
        Some(2),
        "the refused edge left a row: {list}"
    );
}

/// P5.1 — a benefit without baseline, target, method or owner is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_benefit_without_baseline_target_method_or_owner_is_refused() {
    let (pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Benefits programme").await;
    let (status, refused) = running
        .invoke(
            "benefits.create",
            "dev:alice",
            json!({"programme_id": programme, "reference": "BEN-01"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused.to_string().contains("benefit_profile_incomplete"),
        "{refused}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_profile")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count profiles");
    assert_eq!(rows, 0, "a refused profile left a row");
}

/// P5.2 — a benefits-map edge names all five levels; an incomplete edge is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_benefit_map_edge_names_its_five_levels() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Map programme").await;
    let industry: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("industry");
    let job: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, title)
         VALUES ($1, 'bcm', $2, 'Benefit owner') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("job");
    let (status, profile) = running
        .invoke(
            "benefits.create",
            "dev:alice",
            json!({
                "programme_id": programme,
                "reference": "BEN-01",
                "baseline": {"value": 14, "measured_at": "2026-11-01"},
                "target": {"value": 8},
                "measurement_method": "card and run timestamps, weekly",
                "benefit_owner_job_id": job
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{profile}");
    let benefit = id_of(output(&profile), "id");

    let (status, refused) = running
        .invoke(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": programme,
                "benefit_id": benefit,
                "output_ref": "T1 plan spine",
                "capability_ref": "WBS and baseline",
                "operational_outcome": "the plan is managed"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused.to_string().contains("benefit_map_incomplete"),
        "{refused}"
    );

    let (status, linked) = running
        .invoke(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": programme,
                "benefit_id": benefit,
                "output_ref": "T1 plan spine",
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
    assert_eq!(output(&linked)["benefit_id"], benefit);
}

/// P5.3 — a tranche review without evidence is refused; with evidence it records the decision.
#[tokio::test(flavor = "multi_thread")]
async fn a_tranche_review_without_evidence_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Tranche programme").await;
    let (status, tranche) = running
        .invoke(
            "tranches.create",
            "dev:alice",
            json!({"programme_id": programme, "number": 0}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{tranche}");
    let id = id_of(output(&tranche), "id");

    let (status, refused) = running
        .invoke(
            "tranches.review",
            "dev:alice",
            json!({"id": id, "decision": "continue", "evidence": {}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused.to_string().contains("tranche_evidence_missing"),
        "{refused}"
    );

    let (status, reviewed) = running
        .invoke(
            "tranches.review",
            "dev:alice",
            json!({
                "id": id,
                "decision": "continue",
                "evidence": {
                    "baselines": ["B0"],
                    "benefit_status": {"BEN-01": "tracking"},
                    "risk_register": ["risk-1"]
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(output(&reviewed)["state"], "closed");
}

/// The cadence row and the stakeholder profile are T1's MSP rows; their actions are ordinary
/// CRUD over the same tenant-scoped envelope. This is a smoke test, not one of the ten
/// acceptance scenarios (which are the parts of #72).
#[tokio::test(flavor = "multi_thread")]
async fn cadences_and_stakeholder_profiles_are_rows() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Cadence programme").await;

    let (status, cadence) = running
        .invoke(
            "cadences.create",
            "dev:alice",
            json!({"programme_id": programme, "kind": "pi", "period": "PI-2027.1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{cadence}");
    let (status, listed) = running
        .invoke(
            "cadences.list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        output(&listed)["items"].as_array().map(|items| items.len()),
        Some(1)
    );

    let (status, headers, profile) = running
        .invoke_full(
            "stakeholders.profile_create",
            "dev:alice",
            json!({
                "programme_id": programme,
                "name": "Sponsoring group",
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
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the profile carries an ETag");
    let id = id_of(output(&profile), "id");
    let (status, updated) = running
        .invoke(
            "stakeholders.profile_update",
            "dev:alice",
            json!({"id": id, "support_level": "high"}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["support_level"], "high");
}
