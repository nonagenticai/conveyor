//! T9 / #526 — Phase G is a gate, and the design gate reads it for every track.
//!
//! Finding v1-p81-2: `phases_advance(g)` refuses without a live delivery contract and its
//! compliance review (an active waiver when the verdict is non-conformant), and the PM's
//! `design` gate performs the same three checks.
//!
//! `cargo test -p conveyor-actions --test togaf_phase_g`.

mod support;

#[path = "../../api/tests/architecture_support/mod.rs"]
mod arch;

use serde_json::{Value, json};

const P: [&str; 3] = [
    "principles_catalog",
    "principle_derivation",
    "principle_realization",
];
const A: [&str; 4] = [
    "stakeholder_map",
    "driver_goal_objective",
    "requirements_catalog",
    "solution_concept",
];

/// A profile that carries every phase up to G; E, F and G borrow seeded types, which is legal
/// because a tailored requirement may name any type of the tenant's standard.
fn blueprint() -> Value {
    json!({
        "preliminary": { "psa": P },
        "a": { "psa": A },
        "b": { "hld": ["business_capability_map"] },
        "c": { "hld": ["data_entity_catalog"] },
        "d": { "hld": ["technology_standards"] },
        "e": { "sd": ["solution_concept"] },
        "f": { "sd": ["business_capability_map"] },
        "g": { "sd": ["technology_standards"] },
    })
}

const PHASES: [&str; 8] = ["preliminary", "a", "b", "c", "d", "e", "f", "g"];

async fn board_decision(f: &arch::Fixture, profile_id: &str) -> String {
    let board = arch::open_board(&f.running).await;
    let decision = arch::created(
        &f.running,
        "architecture.board_decide",
        json!({
            "body_id": arch::id_of(&board),
            "decision": "approved",
            "evidence": { "charter": "PF-1" },
            "subject_kind": "adm_profile",
            "subject_id": profile_id,
        }),
        None,
    )
    .await;
    decision["approval_id"]
        .as_str()
        .expect("the board approval id")
        .to_owned()
}

async fn adopt_blueprint(f: &arch::Fixture) -> Value {
    let input = json!({
        "key": "adm",
        "name": "Tailored ADM",
        "phase_set": PHASES,
        "artifact_requirements": blueprint(),
        "contract_kinds": { "f": "delivery_contract" },
    });
    let draft = arch::ok(
        &f.running,
        "architecture.profile_tailor",
        input.clone(),
        None,
    )
    .await;
    let decision = board_decision(f, &arch::id_of(&draft)).await;
    let mut input = input;
    input["adopt_with_decision_id"] = json!(decision);
    arch::ok(&f.running, "architecture.profile_tailor", input, None).await
}

async fn approved(
    f: &arch::Fixture,
    project: &str,
    key: &str,
    phase: &str,
    plateau: &str,
    kind: &str,
) {
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
    arch::approve_artifact(&f.running, &arch::id_of(&created)).await;
}

async fn review(f: &arch::Fixture, project: &str, contract: &str, level: &str) -> String {
    let review = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": contract,
            "level": level,
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the delivery is judged at Phase G" }],
            "evidence": { "ref": "phase G" },
        }),
        None,
    )
    .await;
    arch::id_of(&review)
}

#[tokio::test]
async fn phase_g_requires_a_live_contract_a_review_and_a_waiver() {
    let f = arch::fixture().await;
    adopt_blueprint(&f).await;
    let project = arch::project(&f.running, "architecture").await;

    // Preliminary through E: every phase's profile set approved, then the gate advances.
    for (key, kind) in [
        ("principles_catalog", "principles_catalog"),
        ("principle_derivation", "diagram"),
        ("principle_realization", "diagram"),
    ] {
        approved(&f, &project, key, "preliminary", "psa", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(status, 200, "Preliminary: {out}");
    for (key, kind) in [
        ("stakeholder_map", "matrix"),
        ("driver_goal_objective", "catalog"),
        ("requirements_catalog", "catalog"),
        ("solution_concept", "diagram"),
    ] {
        approved(&f, &project, key, "a", "psa", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 200, "A: {out}");
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
    approved(&f, &project, "data_entity_catalog", "c", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "c").await;
    assert_eq!(status, 200, "C: {out}");
    approved(&f, &project, "technology_standards", "d", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "d").await;
    assert_eq!(status, 200, "D: {out}");
    approved(&f, &project, "solution_concept", "e", "sd", "diagram").await;
    let (status, out) = arch::advance(&f.running, &project, "e").await;
    assert_eq!(status, 200, "E: {out}");

    // F closes on its profile set and the live delivery contract it names.
    approved(
        &f,
        &project,
        "business_capability_map",
        "f",
        "sd",
        "diagram",
    )
    .await;
    let contract = arch::live_contract(&f, &project, "delivery_contract", "f").await;
    let (status, out) = arch::advance(&f.running, &project, "f").await;
    assert_eq!(status, 200, "F: {out}");
    assert_eq!(out["contract_id"], contract, "{out}");

    // G: the artefact set is approved, but the contract has no Phase-G review yet.
    approved(&f, &project, "technology_standards", "g", "sd", "catalog").await;
    let (status, refused) = arch::advance(&f.running, &project, "g").await;
    assert_eq!(status, 409, "G passed without a review: {refused}");
    assert_eq!(refused["type"], "architecture_review_missing", "{refused}");

    // A non-conformant review blocks until an active waiver covers it.
    let review_id = review(&f, &project, &contract, "non_conformant").await;
    let (status, refused) = arch::advance(&f.running, &project, "g").await;
    assert_eq!(
        status, 409,
        "G passed on a non-conformant review: {refused}"
    );
    assert_eq!(
        refused["type"], "architecture_review_non_conformant",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("waiver"),
        "{refused}"
    );

    arch::created_as(
        &f.running,
        arch::OWNER,
        "architecture.reviews_waive",
        json!({
            "review_id": review_id,
            "owner_user_id": f.owner,
            "reason": "accepted for this delivery",
            "cost_impact": "none",
            "risk_impact": "low",
            "reassessment_at": "2027-01-31T00:00:00Z",
        }),
        None,
    )
    .await;
    let (status, out) = arch::advance(&f.running, &project, "g").await;
    assert_eq!(status, 200, "the waived review still blocked G: {out}");
    assert_eq!(out["closed_phase"], "g", "{out}");
    assert_eq!(out["contract_id"], contract, "{out}");
}

async fn route_design(f: &arch::Fixture, project: uuid::Uuid) -> (u16, Value) {
    arch::call(
        &f.running,
        "pm.route_gate",
        json!({
            "job_binding_id": f.binding,
            "project_id": project,
            "gate": "design",
            "kind": "design_gate",
            "evidence": [],
            // SPEC_V2 §5.4: an evidence-less gate needs a recorded waiver reason; this scenario
            // judges the contract/review half of the design gate.
            "evidence_waiver": "the live delivery contract is the design gate's evidence",
        }),
        None,
    )
    .await
}

#[tokio::test]
async fn the_design_gate_requires_the_phase_g_review() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Delivery").await;
    let contract = arch::live_contract(&f, &project.to_string(), "delivery_contract", "f").await;

    let (status, refused) = route_design(&f, project).await;
    assert_eq!(status, 409, "design passed without a review: {refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("compliance review"),
        "{refused}"
    );
    assert_eq!(
        arch::approval_count(&f.pg, project, "design").await,
        0,
        "the refused design gate wrote an approval row"
    );

    review(&f, &project.to_string(), &contract, "compliant").await;
    let (status, out) = route_design(&f, project).await;
    assert_eq!(status, 201, "the reviewed contract opens design: {out}");
    assert!(out["approval_id"].is_string(), "{out}");
}
