//! T9 / #526 — the ADM profile is a board-adopted instrument, not a client-supplied map.
//!
//! Findings v1-p81-1, v1-p81-3 and v1-u5a-t9-1: only an adopted profile governs a phase gate,
//! the standard's `artifact_type.mandatory` rows always join the profile's sets, `profile_tailor`
//! refuses an unknown key / a phase outside `phase_set` / a dropped mandatory type, an adopted
//! profile is immutable, and adoption cites an approved architecture-board decision about it.
//!
//! `cargo test -p conveyor-actions --test togaf_profile_gates`.

mod support;

#[path = "../../api/tests/architecture_support/mod.rs"]
mod arch;

use serde_json::{Value, json};
use uuid::Uuid;

/// The seeded standard's mandatory set for Preliminary through D.
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

fn tailor_input(key: &str, phase_set: Value, requirements: Value, kinds: Value) -> Value {
    json!({
        "key": key,
        "name": format!("Tailored ADM {key}"),
        "phase_set": phase_set,
        "artifact_requirements": requirements,
        "contract_kinds": kinds,
    })
}

async fn tailor(f: &arch::Fixture, input: Value) -> (u16, Value) {
    arch::call(&f.running, "architecture.profile_tailor", input, None).await
}

/// The approved board decision about one profile, returning its approval id.
async fn board_decision(f: &arch::Fixture, profile_id: &str) -> String {
    let board = arch::open_board(&f.running).await;
    let board_id = arch::id_of(&board);
    let decision = arch::created(
        &f.running,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
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

/// Tailor a draft, take its board decision, and adopt it through the same action.
async fn adopt(
    f: &arch::Fixture,
    key: &str,
    phase_set: Value,
    requirements: Value,
    kinds: Value,
) -> Value {
    let input = tailor_input(key, phase_set.clone(), requirements.clone(), kinds.clone());
    let draft = arch::ok(&f.running, "architecture.profile_tailor", input, None).await;
    let decision = board_decision(f, &arch::id_of(&draft)).await;
    let mut input = tailor_input(key, phase_set, requirements, kinds);
    input["adopt_with_decision_id"] = json!(decision);
    arch::ok(&f.running, "architecture.profile_tailor", input, None).await
}

/// Create one approved artefact of a seeded type.
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

/// Write an adopted profile past the action, exactly as the finding's actor does.
async fn insert_profile(
    f: &arch::Fixture,
    key: &str,
    phase_set: &Value,
    requirements: Value,
    kinds: Value,
) {
    sqlx::query(
        "INSERT INTO adm_profile
            (tenant_id, key, name, phase_set, artifact_requirements, contract_kinds,
             status, adopted_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'adopted', now())",
    )
    .bind(f.seeded.tenant_id)
    .bind(key)
    .bind(format!("Inserted {key}"))
    .bind(phase_set)
    .bind(requirements)
    .bind(kinds)
    .execute(&f.pg.super_pool)
    .await
    .expect("insert an adopted profile");
}

async fn close_preliminary(f: &arch::Fixture, project: &str) {
    for (key, kind) in [
        ("principles_catalog", "principles_catalog"),
        ("principle_derivation", "diagram"),
        ("principle_realization", "diagram"),
    ] {
        approved(f, project, key, "preliminary", "psa", kind).await;
    }
    let (status, out) = arch::advance(&f.running, project, "preliminary").await;
    assert_eq!(status, 200, "Preliminary: {out}");
}

#[tokio::test]
async fn an_absent_or_partial_profile_set_does_not_satisfy_the_gate() {
    let f = arch::fixture().await;
    adopt(
        &f,
        "adm",
        json!(["preliminary"]),
        json!({ "preliminary": { "psa": P } }),
        json!({}),
    )
    .await;
    let project = arch::project(&f.running, "architecture").await;
    close_preliminary(&f, &project).await;

    // The finding's actor: an adopted profile with no requirement set at all, written past the
    // action (the action refuses to create it), then advanced by key.
    let phases = json!(["preliminary", "a", "b", "c", "d"]);
    insert_profile(&f, "empty-adm", &phases, json!({}), json!({})).await;
    insert_profile(
        &f,
        "partial-adm",
        &phases,
        json!({ "a": { "psa": ["stakeholder_map"] } }),
        json!({}),
    )
    .await;
    approved(&f, &project, "stakeholder_map", "a", "psa", "matrix").await;

    let (status, refused) = arch::call(
        &f.running,
        "architecture.phases_advance",
        json!({ "project_id": project, "phase_code": "a", "profile_key": "empty-adm" }),
        None,
    )
    .await;
    assert_eq!(
        status, 409,
        "an empty phase entry satisfied the gate: {refused}"
    );
    assert_eq!(
        refused["type"], "architecture_artifact_missing",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("no requirement set"),
        "{refused}"
    );

    let (status, refused) = arch::call(
        &f.running,
        "architecture.phases_advance",
        json!({ "project_id": project, "phase_code": "a", "profile_key": "partial-adm" }),
        None,
    )
    .await;
    assert_eq!(
        status, 409,
        "the profile subtracted the standard's mandatory rows: {refused}"
    );
    assert_eq!(
        refused["type"], "architecture_artifact_missing",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("driver_goal_objective"),
        "artifact_type.mandatory was not read: {refused}"
    );
}

#[tokio::test]
async fn a_draft_profile_cannot_govern_the_gate() {
    let f = arch::fixture().await;
    let draft = arch::ok(
        &f.running,
        "architecture.profile_tailor",
        tailor_input(
            "adm",
            json!(["preliminary"]),
            json!({ "preliminary": { "psa": P } }),
            json!({}),
        ),
        None,
    )
    .await;
    assert_eq!(draft["status"], "draft", "{draft}");
    let project = arch::project(&f.running, "architecture").await;
    for (key, kind) in [
        ("principles_catalog", "principles_catalog"),
        ("principle_derivation", "diagram"),
        ("principle_realization", "diagram"),
    ] {
        approved(&f, &project, key, "preliminary", "psa", kind).await;
    }

    let (status, refused) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(
        status, 409,
        "a draft governed the default profile: {refused}"
    );
    assert_eq!(
        refused["type"], "architecture_profile_not_adopted",
        "{refused}"
    );

    let (status, refused) = arch::call(
        &f.running,
        "architecture.phases_advance",
        json!({ "project_id": project, "phase_code": "preliminary", "profile_key": "adm" }),
        None,
    )
    .await;
    assert_eq!(status, 409, "a named draft governed the gate: {refused}");
    assert_eq!(
        refused["type"], "architecture_profile_not_adopted",
        "{refused}"
    );
}

#[tokio::test]
async fn profile_tailor_refuses_an_empty_phase_set() {
    let f = arch::fixture().await;
    let (status, refused) = tailor(
        &f,
        tailor_input("no-phases", json!([]), json!({}), json!({})),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "phase_set", "{refused}");

    // An omitted phase_set is the same empty tailoring, not an accepted profile.
    let (status, refused) = tailor(&f, json!({ "key": "no-phases", "name": "No phases" })).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "phase_set", "{refused}");
}

#[tokio::test]
async fn profile_tailor_refuses_an_unknown_requirement_key() {
    let f = arch::fixture().await;
    let (status, refused) = tailor(
        &f,
        tailor_input(
            "bad-key",
            json!(["a"]),
            json!({ "a": { "psa": ["stakeholder_map", "not_a_standard_type"] } }),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["field"], "artifact_requirements",
        "{refused}"
    );
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("not_a_standard_type"),
        "{refused}"
    );
}

#[tokio::test]
async fn profile_tailor_refuses_a_phase_outside_the_set() {
    let f = arch::fixture().await;
    let (status, refused) = tailor(
        &f,
        tailor_input(
            "phase-outside",
            json!(["preliminary", "a"]),
            json!({
                "a": { "psa": A },
                "b": { "hld": ["business_capability_map"] },
            }),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["field"], "artifact_requirements",
        "{refused}"
    );
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("phase_set"),
        "{refused}"
    );
}

#[tokio::test]
async fn profile_tailor_refuses_a_dropped_mandatory_type() {
    let f = arch::fixture().await;
    let (status, refused) = tailor(
        &f,
        tailor_input(
            "dropped-mandatory",
            json!(["preliminary", "a"]),
            json!({
                "preliminary": { "psa": P },
                "a": { "psa": ["stakeholder_map"] },
            }),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["field"], "artifact_requirements",
        "{refused}"
    );
    assert!(
        refused["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("driver_goal_objective"),
        "{refused}"
    );
}

#[tokio::test]
async fn an_adopted_profile_cannot_be_rewritten() {
    let f = arch::fixture().await;
    let adopted = adopt(
        &f,
        "adm",
        json!(["preliminary"]),
        json!({ "preliminary": { "psa": P } }),
        json!({}),
    )
    .await;
    assert_eq!(adopted["status"], "adopted", "{adopted}");

    // A valid replacement payload still may not rewrite an adopted profile.
    let (status, refused) = tailor(
        &f,
        tailor_input(
            "adm",
            json!(["preliminary"]),
            json!({ "preliminary": { "psa": P } }),
            json!({ "preliminary": "statement_of_architecture_work" }),
        ),
    )
    .await;
    assert_eq!(status, 409, "an adopted profile was rewritten: {refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("immutable"),
        "{refused}"
    );

    let read = arch::ok(
        &f.running,
        "architecture.profile_get",
        json!({ "key": "adm" }),
        None,
    )
    .await;
    assert_eq!(
        read["profile"]["artifact_requirements"]["preliminary"]["psa"][0], "principles_catalog",
        "the adopted requirements changed: {read}"
    );
    assert_eq!(
        read["profile"]["contract_kinds"],
        json!({}),
        "the adopted contract kinds changed: {read}"
    );
    assert_eq!(read["profile"]["status"], "adopted", "{read}");
}

#[tokio::test]
async fn adoption_requires_an_approved_decision_about_this_profile() {
    let f = arch::fixture().await;
    let input = tailor_input(
        "adm",
        json!(["preliminary"]),
        json!({ "preliminary": { "psa": P } }),
        json!({}),
    );
    let draft = arch::ok(
        &f.running,
        "architecture.profile_tailor",
        input.clone(),
        None,
    )
    .await;
    let profile_id = arch::id_of(&draft);

    // Any UUID is not a board decision.
    let mut random = input.clone();
    random["adopt_with_decision_id"] = json!(Uuid::now_v7().to_string());
    let (status, refused) =
        arch::call(&f.running, "architecture.profile_tailor", random, None).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_profile_decision_required",
        "{refused}"
    );

    // A decision about another profile does not adopt this one.
    arch::ok(
        &f.running,
        "architecture.profile_tailor",
        tailor_input(
            "other",
            json!(["preliminary"]),
            json!({ "preliminary": { "psa": P } }),
            json!({}),
        ),
        None,
    )
    .await;
    let decision = board_decision(&f, &profile_id).await;
    let mut wrong = tailor_input(
        "other",
        json!(["preliminary"]),
        json!({ "preliminary": { "psa": P } }),
        json!({}),
    );
    wrong["adopt_with_decision_id"] = json!(decision);
    let (status, refused) =
        arch::call(&f.running, "architecture.profile_tailor", wrong, None).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_profile_decision_required",
        "{refused}"
    );

    // A rejected decision is not an adoption.
    let board = arch::open_board(&f.running).await;
    let rejected = arch::created(
        &f.running,
        "architecture.board_decide",
        json!({
            "body_id": arch::id_of(&board),
            "decision": "rejected",
            "evidence": { "charter": "PF-1" },
            "subject_kind": "adm_profile",
            "subject_id": profile_id,
        }),
        None,
    )
    .await;
    let mut refused_adoption = input.clone();
    refused_adoption["adopt_with_decision_id"] = rejected["approval_id"].clone();
    let (status, refused) = arch::call(
        &f.running,
        "architecture.profile_tailor",
        refused_adoption,
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_profile_decision_required",
        "{refused}"
    );

    // The approved decision about THIS profile adopts it, and the adopted profile opens the gate.
    let mut accepted = input;
    accepted["adopt_with_decision_id"] = json!(decision);
    let adopted = arch::ok(&f.running, "architecture.profile_tailor", accepted, None).await;
    assert_eq!(adopted["status"], "adopted", "{adopted}");
    assert_eq!(adopted["board_decision_id"], decision, "{adopted}");

    let project = arch::project(&f.running, "architecture").await;
    close_preliminary(&f, &project).await;
}
