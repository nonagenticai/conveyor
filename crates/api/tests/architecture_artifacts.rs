//! T9 (#81) D1 — the artefact standard, the typed repository and reuse.
//!
//! `rbuild -- test -p conveyor-api --test architecture_artifacts`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;
#[path = "architecture_support/mod.rs"]
mod arch;

use serde_json::json;

#[tokio::test]
async fn a_catalog_is_queryable_rows() {
    let f = arch::fixture().await;
    arch::tailor(&f.running, json!({}), json!({})).await;
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
    arch::tailor(&f.running, json!({}), json!({})).await;
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
    assert_eq!(refused["type"], "architecture_approved_immutable", "{refused}");

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
    arch::tailor(&f.running, json!({}), json!({})).await;

    let enterprise_type = arch::type_id(&f.running, "business_capability_map").await;
    let enterprise = arch::artifact(
        &f.running,
        "b",
        "hld",
        "diagram",
        "Enterprise capability map",
        json!({ "artifact_type_id": enterprise_type }),
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
        json!({ "artifact_type_id": solution_type }),
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
