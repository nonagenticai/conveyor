//! T9 (#81) D3 — the architecture project, its phase nodes and the contract gate.
//!
//! `rbuild -- test -p conveyor-api --test togaf_architecture_phases`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;

/// Create one artefact of a seeded standard type and approve it; returns its id.
async fn approved(
    f: &arch::Fixture,
    project: &str,
    key: &str,
    phase: &str,
    plateau: &str,
    kind: &str,
) -> String {
    let artifact_type_id = arch::type_id(&f.running, key).await;
    let created = arch::artifact(
        &f.running,
        phase,
        plateau,
        kind,
        key,
        json!({ "project_id": project, "artifact_type_id": artifact_type_id }),
    )
    .await;
    let id = arch::id_of(&created);
    arch::approve_artifact(&f.running, &id).await;
    id
}

#[tokio::test]
async fn phases_a_to_f_pass_in_order() {
    let f = arch::fixture().await;
    arch::tailor_adopted(
        &f,
        json!({
            "preliminary": { "psa": ["principles_catalog"] },
            "a": { "psa": ["stakeholder_map"] },
            "b": { "hld": ["business_capability_map"] },
            "c": { "hld": ["data_entity_catalog"] },
            "d": { "hld": ["technology_standards"] },
            // Explicit empty sets: the adopted profile states that E and F carry no artefacts.
            // An absent key would fall back to the standard's mandatory rows (v1-p81-1).
            "e": [],
            "f": [],
        }),
        json!({
            "a": "statement_of_architecture_work",
            "f": "delivery_contract",
        }),
    )
    .await;
    let project = arch::project(&f.running, "architecture").await;

    // The project is born with its nine phase nodes; the first open phase is Preliminary.
    let (status, out) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(
        status, 409,
        "Preliminary's mandatory catalogue is not approved yet: {out}"
    );
    assert_eq!(out["type"], "architecture_artifact_missing", "{out}");
    approved(
        &f,
        &project,
        "principles_catalog",
        "preliminary",
        "psa",
        "principles_catalog",
    )
    .await;
    let (status, out) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(status, 200, "Preliminary: {out}");
    assert_eq!(out["next_phase"], "a", "{out}");

    approved(&f, &project, "stakeholder_map", "a", "psa", "matrix").await;
    arch::live_contract(&f, &project, "statement_of_architecture_work", "a").await;
    let (status, out) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 200, "A: {out}");
    assert_eq!(out["closed_phase"], "a", "{out}");
    assert_eq!(out["next_phase"], "b", "{out}");

    approved(
        &f,
        &project,
        "business_capability_map",
        "b",
        "hld",
        "diagram",
    )
    .await;
    let (status, out) = arch::advance(&f.running, &project, "b").await;
    assert_eq!(status, 200, "B: {out}");
    assert_eq!(out["next_phase"], "c", "{out}");

    approved(&f, &project, "data_entity_catalog", "c", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "c").await;
    assert_eq!(status, 200, "C: {out}");
    assert_eq!(out["next_phase"], "d", "{out}");

    approved(&f, &project, "technology_standards", "d", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "d").await;
    assert_eq!(status, 200, "D: {out}");
    assert_eq!(out["next_phase"], "e", "{out}");

    let (status, out) = arch::advance(&f.running, &project, "e").await;
    assert_eq!(status, 200, "E: {out}");
    assert_eq!(out["next_phase"], "f", "{out}");

    arch::live_contract(&f, &project, "delivery_contract", "f").await;
    let (status, out) = arch::advance(&f.running, &project, "f").await;
    assert_eq!(status, 200, "F: {out}");
    assert_eq!(out["next_phase"], "g", "{out}");
}

#[tokio::test]
async fn a_draft_artefact_refuses_the_contract() {
    let f = arch::fixture().await;
    arch::tailor_adopted(
        &f,
        json!({ "preliminary": [], "a": { "psa": ["stakeholder_map"] } }),
        json!({ "a": "statement_of_architecture_work" }),
    )
    .await;
    let project = arch::project(&f.running, "architecture").await;
    let (status, _) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(status, 200);

    // The artefact exists but is still a draft: the gate names it and writes nothing.
    let artifact_type_id = arch::type_id(&f.running, "stakeholder_map").await;
    arch::artifact(
        &f.running,
        "a",
        "psa",
        "matrix",
        "Stakeholder map",
        json!({ "project_id": project, "artifact_type_id": artifact_type_id }),
    )
    .await;
    let contract = arch::live_contract(&f, &project, "statement_of_architecture_work", "a").await;

    let (status, refused) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_artifact_missing",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("stakeholder_map"),
        "the refusal does not name the missing artefact: {refused}"
    );

    let listed = arch::ok(
        &f.running,
        "architecture.contracts_list",
        json!({ "project_id": project }),
        None,
    )
    .await;
    assert_eq!(
        listed["items"].as_array().expect("contracts").len(),
        1,
        "the refused advance wrote a contract row: {listed}"
    );
    let _ = contract;
}

#[tokio::test]
async fn a_contract_change_reopens_the_gate() {
    let f = arch::fixture().await;
    arch::tailor_adopted(
        &f,
        json!({ "preliminary": [], "a": { "psa": ["stakeholder_map"] } }),
        json!({ "a": "statement_of_architecture_work" }),
    )
    .await;
    let project = arch::project(&f.running, "architecture").await;
    let (status, _) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(status, 200);
    let artifact_type_id = arch::type_id(&f.running, "stakeholder_map").await;
    let artifact = arch::artifact(
        &f.running,
        "a",
        "psa",
        "matrix",
        "Stakeholder map",
        json!({ "project_id": project, "artifact_type_id": artifact_type_id }),
    )
    .await;
    arch::approve_artifact(&f.running, &arch::id_of(&artifact)).await;
    let contract = arch::live_contract(&f, &project, "statement_of_architecture_work", "a").await;

    let retired = arch::ok(
        &f.running,
        "architecture.contracts_supersede",
        json!({ "id": contract }),
        Some("\"2\""),
    )
    .await;
    assert_eq!(retired["status"], "superseded", "{retired}");

    let (status, refused) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_contract_not_live",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("statement_of_architecture_work"),
        "the refusal does not name the contract that reopens the gate: {refused}"
    );

    let replacement = arch::created(
        &f.running,
        "architecture.contracts_request",
        json!({
            "project_id": project,
            "kind": "statement_of_architecture_work",
            "phase_code": "a",
            "title": "Statement of architecture work — A (v2)",
            "body": { "assignment": "revised" },
            "supersedes_id": contract,
        }),
        None,
    )
    .await;
    let replacement_id = arch::id_of(&replacement);
    let old = arch::ok(
        &f.running,
        "architecture.contracts_get",
        json!({ "id": contract }),
        None,
    )
    .await;
    assert_eq!(old["superseded_by_id"], replacement_id, "{old}");
    arch::approve_contract(&f, &replacement_id).await;

    let (status, out) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 200, "the replacement contract is live: {out}");
    assert_eq!(out["closed_phase"], "a", "{out}");
}
