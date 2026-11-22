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

/// A complete profile the benefits map can link to (baseline, target, method and owner are all
/// mandatory, so the inline fixture would be forty lines in every test).
async fn create_measured_benefit(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    programme: &str,
    reference: &str,
) -> String {
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
                "reference": reference,
                "baseline": {"value": 14, "measured_at": "2026-11-01"},
                "target": {"value": 8},
                "measurement_method": "card and run timestamps, weekly",
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
    id_of(output(&profile), "id")
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

/// An approved scope baseline: `baselines.take` creates the row and the `baseline` gate's
/// decision is what `approved_by` records, so the fixture writes that decision directly (the
/// same shape `spine_gates.rs` builds).
async fn create_approved_baseline(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
) -> String {
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
    sqlx::query("UPDATE project_baseline SET approved_by = $2 WHERE id = $1::uuid")
        .bind(&baseline)
        .bind(seeded.user_id)
        .execute(&pg.super_pool)
        .await
        .expect("approve the baseline");
    baseline
}

/// A `kind='change'` register row citing `baseline`. `approved` writes the `change` gate's
/// decision — the only record that makes a change approved; the row's `status` never is one.
async fn create_change(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
    baseline: &str,
    approved: bool,
) -> String {
    let (status, body) = running
        .invoke(
            "registers.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Move the scope",
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": "a deliverable moves"}}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let change = id_of(output(&body), "id");
    if approved {
        sqlx::query(
            "INSERT INTO approval (tenant_id, project_id, gate, kind, status, requested_by, meta)
             VALUES ($1, $2::uuid, 'change', 'pm', 'approved', $3, $4)",
        )
        .bind(seeded.tenant_id)
        .bind(project)
        .bind(seeded.user_id)
        .bind(json!({"change_id": change}))
        .execute(&pg.super_pool)
        .await
        .expect("insert the approved change gate");
    }
    change
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

/// v1-p5-4 — after an approved scope baseline a direct `wbs.update` is a re-baseline: it needs
/// the approved change request that cites that baseline, and nothing is written without one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wbs_edit_after_a_baseline_requires_an_approved_change() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Guarded baseline").await;
    let node = create_wbs(&running, &project, "The promised deliverable").await;
    let baseline = create_approved_baseline(&pg, &running, &seeded, &project).await;

    let (_, headers, _) = running
        .invoke_full(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the node carries an ETag");

    // No change id at all: the edit would move the scope an approved baseline records.
    let (status, body) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({"id": node, "title": "Renamed without a change"}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");

    // A change row without its gate's decision is pending, not approved.
    let pending = create_change(&pg, &running, &seeded, &project, &baseline, false).await;
    let (status, body) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({"id": node, "title": "Renamed under a pending change", "change_id": pending}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");

    // An approved change that cites ANOTHER project's baseline does not authorise this one.
    let other = create_project(&running, "Foreign baseline").await;
    let foreign_baseline = create_approved_baseline(&pg, &running, &seeded, &other).await;
    let foreign = create_change(&pg, &running, &seeded, &project, &foreign_baseline, true).await;
    let (status, body) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({"id": node, "title": "Renamed under a foreign change", "change_id": foreign}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");

    // Every refusal wrote nothing: the row is still the promise the baseline froze.
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
    assert_eq!(output(&read)["title"], "The promised deliverable");
    assert_eq!(output(&read)["version"], 1, "a refusal moved the row");

    // The approved change that cites THIS baseline is the authorisation.
    let change = create_change(&pg, &running, &seeded, &project, &baseline, true).await;
    let (status, updated) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({
                "id": node,
                "title": "Renamed under the approved change",
                "change_id": change
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(
        output(&updated)["title"],
        "Renamed under the approved change"
    );
}

/// v1-p5-4 — the same guard covers create and delete; a project with no approved baseline is
/// untouched (every other WBS test in this file is that half).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wbs_create_after_a_baseline_requires_an_approved_change() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Guarded create").await;
    let baseline = create_approved_baseline(&pg, &running, &seeded, &project).await;

    let (status, body) = running
        .invoke(
            "wbs.create",
            "dev:alice",
            json!({"project_id": project, "kind": "deliverable", "title": "Unsanctioned"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM wbs_node WHERE project_id = $1::uuid")
        .bind(&project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the project's nodes");
    assert_eq!(rows, 0, "the refused create wrote a row");

    let pending = create_change(&pg, &running, &seeded, &project, &baseline, false).await;
    let (status, body) = running
        .invoke(
            "wbs.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "deliverable",
                "title": "Under a pending change",
                "change_id": pending
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");

    let change = create_change(&pg, &running, &seeded, &project, &baseline, true).await;
    let (status, created) = running
        .invoke(
            "wbs.create",
            "dev:alice",
            json!({
                "project_id": project,
                "kind": "deliverable",
                "title": "Under the approved change",
                "change_id": change
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let node = id_of(output(&created), "id");

    // Delete is the same mutation: refused without the change, allowed with it.
    let (_, headers, _) = running
        .invoke_full(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the node carries an ETag");
    let (status, body) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "wbs.delete",
            "dev:alice",
            json!({"id": node}),
            Some(etag.as_str()),
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "rebaseline_requires_change", "{body}");

    let (status, deleted) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "wbs.delete",
            "dev:alice",
            json!({"id": node, "change_id": change}),
            Some(etag.as_str()),
        )
        .await;
    assert_eq!(status, 204, "{deleted}");
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
    let (pg, running, seeded) = fixture().await;
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
    // The portfolio is projects AND programmes: the active programme has no projects, and the
    // closed one is history, so exactly the first is in flight on its own status.
    let active_programme = create_programme(&running, "In-flight programme").await;
    let closed_programme = create_programme(&running, "Closed programme").await;
    sqlx::query("UPDATE programme SET status = 'closed' WHERE id = $1::uuid")
        .bind(&closed_programme)
        .execute(&pg.super_pool)
        .await
        .expect("close the programme");

    // The view is computed, never stored: the read appends no domain event.
    let events_before: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the tenant's events");

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

    let programme_ids: Vec<&str> = output(&body)["programmes"]
        .as_array()
        .expect("programmes")
        .iter()
        .map(|row| row["id"].as_str().unwrap_or_default())
        .collect();
    assert!(
        programme_ids.contains(&active_programme.as_str()),
        "the active programme with no projects is in flight: {programme_ids:?}"
    );
    assert!(
        !programme_ids.contains(&closed_programme.as_str()),
        "the closed programme is history: {programme_ids:?}"
    );
    assert_eq!(programme_ids.len(), 1, "{programme_ids:?}");

    let events_after: i64 = sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the tenant's events");
    assert_eq!(
        events_after, events_before,
        "portfolio.in_flight writes no row"
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
            json!({
                "programme_id": programme,
                "reference": "BEN-01",
                "capabilities": ["delivery"],
                "operational_area": "delivery"
            }),
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
    let project = create_programme_project(&running, &programme, "Map project").await;
    let node = create_wbs(&running, &project, "T1 plan spine").await;
    let benefit = create_measured_benefit(&pg, &running, &seeded, &programme, "BEN-01").await;

    let (status, refused) = running
        .invoke(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": programme,
                "benefit_id": benefit,
                "output_ref": node,
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
    assert_eq!(output(&linked)["benefit_id"], benefit);
    assert_eq!(output(&linked)["output_ref"], node);
}

/// v1-w2b-5 — an output is a `wbs_node` id; a label is refused and leaves no edge.
#[tokio::test(flavor = "multi_thread")]
async fn a_benefit_link_refuses_a_non_uuid_output_ref() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Label output programme").await;
    create_programme_project(&running, &programme, "Label output project").await;
    let benefit = create_measured_benefit(&pg, &running, &seeded, &programme, "BEN-01").await;

    let (status, refused) = running
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
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "action-invalid-input", "{refused}");
    assert_eq!(refused["errors"][0]["field"], "output_ref", "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "benefit_output_unresolved",
        "{refused}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_link")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count edges");
    assert_eq!(rows, 0, "a refused edge left a row");
}

/// v1-w2b-5 — a real node of another programme is still not this map's output.
#[tokio::test(flavor = "multi_thread")]
async fn a_benefit_link_refuses_a_node_from_another_programme() {
    let (pg, running, seeded) = fixture().await;
    let home = create_programme(&running, "Home programme").await;
    let other = create_programme(&running, "Other programme").await;
    let other_project = create_programme_project(&running, &other, "Other project").await;
    let foreign = create_wbs(&running, &other_project, "Another programme's output").await;
    let benefit = create_measured_benefit(&pg, &running, &seeded, &home, "BEN-01").await;

    let (status, refused) = running
        .invoke(
            "benefits.link",
            "dev:alice",
            json!({
                "programme_id": home,
                "benefit_id": benefit,
                "output_ref": foreign,
                "capability_ref": "WBS and baseline",
                "operational_outcome": "the plan is managed",
                "strategic_objective": "faster IT delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "output_ref", "{refused}");
    assert_eq!(
        refused["errors"][0]["code"], "benefit_output_unresolved",
        "{refused}"
    );

    // The node is real and in this tenant: the refusal is the programme, not existence.
    let (status, node) = running
        .invoke(
            "wbs.get",
            "dev:alice",
            json!({"id": foreign}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{node}");
}

/// v1-w2b-5 — a node of this programme is accepted and stored as its id.
#[tokio::test(flavor = "multi_thread")]
async fn a_benefit_link_accepts_a_node_in_its_programme() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Node output programme").await;
    let project = create_programme_project(&running, &programme, "Node output project").await;
    let node = create_wbs(&running, &project, "The measured output").await;
    let benefit = create_measured_benefit(&pg, &running, &seeded, &programme, "BEN-01").await;

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
    assert_eq!(output(&linked)["output_ref"], node);

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
    assert_eq!(output(&listed)["items"][0]["output_ref"], node);
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

    let delegation = support::delegation_ref(&running, "dev:alice").await;
    let input =
        json!({"id": id, "decision": "continue", "delegation_ref": delegation, "evidence": {}});
    let gate = support::review_gate(&running, "dev:alice", &input).await;
    let (status, refused) = running
        .invoke_approved("tranches.review", "dev:alice", input, None, gate)
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(
        refused.to_string().contains("tranche_evidence_missing"),
        "{refused}"
    );

    // U4: a `continue` authorises the next tranche, so the plan needs one to authorise.
    let (status, next) = running
        .invoke(
            "tranches.create",
            "dev:alice",
            json!({"programme_id": programme, "number": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{next}");

    let input = json!({
        "id": id,
        "decision": "continue",
        "delegation_ref": delegation,
        "evidence": {
            // v1-tr2-2: the review names the snapshot it judged.
            "snapshot_sha": "0123456789abcdef0123456789abcdef01234567",
            "baselines": ["B0"],
            "benefit_status": {"BEN-01": "tracking"},
            "risk_register": ["risk-1"]
        }
    });
    let gate = support::review_gate(&running, "dev:alice", &input).await;
    let (status, reviewed) = running
        .invoke_approved("tranches.review", "dev:alice", input, None, gate)
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

/// v1-p5-2 — acceptance is evidence, not a checkbox.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepting_a_wbs_node_without_evidence_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Evidence gate").await;
    let node = create_wbs(&running, &project, "Unproven deliverable").await;

    let (_, headers, _) = running
        .invoke_full(
            "wbs.get",
            "dev:alice",
            json!({"id": node}),
            None,
            None,
            None,
        )
        .await;
    let etag = headers
        .get("etag")
        .cloned()
        .expect("the node carries an ETag");

    let (status, body) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({"id": node, "status": "accepted", "evidence": []}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["field"], "evidence", "{body}");
    assert_eq!(body["errors"][0]["code"], "required", "{body}");

    let (status, body) = running
        .invoke(
            "wbs.update",
            "dev:alice",
            json!({
                "id": node,
                "status": "accepted",
                "evidence": [{"manifest": "acceptance/programme/T1.toml", "result": "pass"}]
            }),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "accepted", "{body}");
}
