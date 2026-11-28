//! T9 (#81) D1 — the artefact standard, the typed repository and reuse.
//!
//! `rbuild -- test -p conveyor-api --test architecture_artifacts`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn a_catalog_is_queryable_rows() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;
    let document = arch::document(
        &f.pg,
        f.seeded.tenant_id,
        Some(&project),
        "Principles catalog",
    )
    .await;
    let entries: Vec<_> = (1..=10)
        .map(|i| {
            json!({
                "axis_a": format!("PR-{i}"),
                "value": { "statement": format!("Principle {i}"), "rationale": "because" },
                "position": i,
            })
        })
        .collect();
    let artifact_type_id = arch::type_id(&f.running, "principles_catalog").await;
    let created = arch::artifact(
        &f.running,
        "preliminary",
        "psa",
        "principles_catalog",
        "Principles catalog",
        json!({
            "project_id": project,
            "artifact_type_id": artifact_type_id,
            "document_id": document,
            "entries": entries,
        }),
    )
    .await;
    let id = arch::id_of(&created);
    arch::approve_artifact(&f.running, &id).await;

    let got = arch::ok(
        &f.running,
        "architecture.artifacts_get",
        json!({ "id": id }),
        None,
    )
    .await;
    assert_eq!(got["artifact_kind"], "principles_catalog", "{got}");
    assert_eq!(got["artifact_version"], 1, "{got}");
    assert_eq!(got["author_id"], f.seeded.user_id.to_string(), "{got}");
    assert!(
        got["reviewer_id"].is_string(),
        "the reviewer is not named after approval: {got}"
    );
    assert_eq!(got["document_id"], document, "{got}");

    let listed = arch::ok(
        &f.running,
        "architecture.artifacts_entries",
        json!({ "id": id }),
        None,
    )
    .await;
    let rows = listed["items"].as_array().expect("entries");
    assert_eq!(rows.len(), 10, "{listed}");
    assert_eq!(rows[0]["axis_a"], "PR-1", "{listed}");
    assert_eq!(rows[0]["artifact_id"], id, "{listed}");
}

#[tokio::test]
async fn an_approved_artefact_is_immutable() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;
    let artifact_type_id = arch::type_id(&f.running, "stakeholder_map").await;
    let created = arch::artifact(
        &f.running,
        "a",
        "psa",
        "matrix",
        "Stakeholder map",
        json!({
            "project_id": project,
            "artifact_type_id": artifact_type_id,
            "entries": [{ "axis_a": "Sponsor", "axis_b": "Interest", "value": "high" }],
        }),
    )
    .await;
    let id = arch::id_of(&created);
    arch::approve_artifact(&f.running, &id).await;

    let (status, refused) = arch::call(
        &f.running,
        "architecture.artifacts_update",
        json!({ "id": id, "title": "Changed after approval" }),
        Some("\"2\""),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_approved_immutable",
        "{refused}"
    );

    let next = arch::created(
        &f.running,
        "architecture.artifacts_supersede",
        json!({ "id": id }),
        Some("\"2\""),
    )
    .await;
    assert_eq!(next["artifact_version"], 2, "{next}");
    assert_eq!(next["status"], "draft", "{next}");
    assert_eq!(next["supersedes_id"], id, "{next}");

    let old = arch::ok(
        &f.running,
        "architecture.artifacts_get",
        json!({ "id": id }),
        None,
    )
    .await;
    assert_eq!(old["status"], "superseded", "{old}");
    assert_eq!(old["title"], "Stakeholder map", "{old}");
    assert_eq!(old["superseded_by_id"], arch::id_of(&next), "{old}");
}

#[tokio::test]
async fn reuse_is_searchable_across_plateaus() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    // A link joins two artefacts of ONE project (#526); the reuse property under test is the
    // plateau search, not a project boundary.
    let project = arch::project(&f.running, "architecture").await;

    let enterprise_type = arch::type_id(&f.running, "business_capability_map").await;
    let enterprise = arch::artifact(
        &f.running,
        "b",
        "hld",
        "diagram",
        "Enterprise capability map",
        json!({ "project_id": project, "artifact_type_id": enterprise_type }),
    )
    .await;
    let enterprise_id = arch::id_of(&enterprise);
    arch::approve_artifact(&f.running, &enterprise_id).await;

    let solution_type = arch::type_id(&f.running, "solution_concept").await;
    let solution = arch::artifact(
        &f.running,
        "a",
        "sd",
        "diagram",
        "Solution concept reuse probe",
        json!({ "project_id": project, "artifact_type_id": solution_type }),
    )
    .await;
    let solution_id = arch::id_of(&solution);
    arch::approve_artifact(&f.running, &solution_id).await;

    arch::ok(
        &f.running,
        "architecture.artifacts_link",
        json!({
            "from_artifact_id": solution_id,
            "to_artifact_id": enterprise_id,
            "kind": "reuse",
        }),
        None,
    )
    .await;

    let found = arch::ok(
        &f.running,
        "architecture.repository_search",
        json!({ "query": "reuse probe", "plateau": "sd" }),
        None,
    )
    .await;
    let ids: Vec<&str> = found["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(ids.contains(&solution_id.as_str()), "{found}");
    assert!(
        ids.contains(&enterprise_id.as_str()),
        "the reused HLD artefact is not returned across plateaus: {found}"
    );

    let links = arch::ok(
        &f.running,
        "architecture.artifacts_links",
        json!({ "id": solution_id }),
        None,
    )
    .await;
    assert_eq!(links["items"][0]["kind"], "reuse", "{links}");
}

/// Every link row of the tenant, read as the superuser so a refused insert cannot hide behind
/// the request's own scope.
async fn link_rows(f: &arch::Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM architecture_artifact_link WHERE tenant_id = $1")
        .bind(f.seeded.tenant_id)
        .fetch_one(&f.pg.super_pool)
        .await
        .expect("count the tenant's links")
}

#[tokio::test]
async fn a_link_across_projects_is_refused() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let home = arch::project(&f.running, "architecture").await;
    let other = arch::project(&f.running, "architecture").await;
    let artifact_type_id = arch::type_id(&f.running, "solution_concept").await;
    let from = arch::artifact(
        &f.running,
        "a",
        "sd",
        "diagram",
        "Home solution",
        json!({ "project_id": home, "artifact_type_id": artifact_type_id }),
    )
    .await;
    let to = arch::artifact(
        &f.running,
        "a",
        "sd",
        "diagram",
        "Other project's solution",
        json!({ "project_id": other, "artifact_type_id": artifact_type_id }),
    )
    .await;

    let (status, refused) = arch::call(
        &f.running,
        "architecture.artifacts_link",
        json!({
            "from_artifact_id": arch::id_of(&from),
            "to_artifact_id": arch::id_of(&to),
            "kind": "reuse",
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "a cross-project link was written: {refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("same project"),
        "the refusal does not name the project boundary: {refused}"
    );
    assert_eq!(link_rows(&f).await, 0, "a refused link wrote a row");
}

#[tokio::test]
async fn a_link_across_tenants_is_refused() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let home = arch::project(&f.running, "architecture").await;
    let artifact_type_id = arch::type_id(&f.running, "solution_concept").await;
    let from = arch::artifact(
        &f.running,
        "a",
        "sd",
        "diagram",
        "Home solution",
        json!({ "project_id": home, "artifact_type_id": artifact_type_id }),
    )
    .await;

    // A second tenant's artefact is invisible to this request's scope: the join cannot see it,
    // so the link is refused exactly as a project-boundary crossing is.
    let foreign_tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('link-foreign', 'Foreign') RETURNING id",
    )
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the foreign tenant");
    let foreign_project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'FOREIGN', 'Foreign') RETURNING id",
    )
    .bind(foreign_tenant)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the foreign project");
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO architecture_artifact
             (tenant_id, project_id, phase_code, plateau, artifact_kind, title)
         VALUES ($1, $2, 'a', 'sd', 'diagram', 'Foreign solution')
         RETURNING id",
    )
    .bind(foreign_tenant)
    .bind(foreign_project)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the foreign artefact");

    let (status, refused) = arch::call(
        &f.running,
        "architecture.artifacts_link",
        json!({
            "from_artifact_id": arch::id_of(&from),
            "to_artifact_id": foreign,
            "kind": "reuse",
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "a cross-tenant link was written: {refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");
    assert_eq!(link_rows(&f).await, 0, "a refused link wrote a row");
}
