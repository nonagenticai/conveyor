//! T9 (#81) D5 — Phase H records exactly one typed outcome, on the project's live delivery
//! contract, and an unrecorded or elsewhere-recorded review refuses the gate.
//!
//! `rbuild -- test -p conveyor-api --test togaf_h_change`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The full tailored ADM: every phase the test advances names a requirement set.
fn blueprint() -> Value {
    json!({
        "preliminary": {
            "psa": ["principles_catalog", "principle_derivation", "principle_realization"]
        },
        "a": {
            "psa": ["stakeholder_map", "driver_goal_objective", "requirements_catalog",
                    "solution_concept"]
        },
        "b": { "hld": ["business_capability_map"] },
        "c": { "hld": ["data_entity_catalog"] },
        "d": { "hld": ["technology_standards"] },
        "e": { "sd": ["solution_concept"] },
        "f": { "sd": ["business_capability_map"] },
        "g": { "sd": ["technology_standards"] },
        "h": { "sd": ["solution_concept"] },
    })
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

async fn process_id(pool: &PgPool, tenant: Uuid, key: &str) -> Uuid {
    sqlx::query(
        "INSERT INTO process (tenant_id, key, title, status, version)
         SELECT $1, $2, 'Kata process', 'published',
                (SELECT COALESCE(MAX(version), 0) + 1
                   FROM process WHERE tenant_id = $1 AND key = $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(tenant)
    .bind(key)
    .execute(pool)
    .await
    .expect("insert the published process");
    sqlx::query_scalar(
        "SELECT id FROM process
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'",
    )
    .bind(tenant)
    .bind(key)
    .fetch_one(pool)
    .await
    .expect("read the published process")
}

async fn run_of(pool: &PgPool, tenant: Uuid, project: Uuid, process: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, process_id, status)
         VALUES ($1, $2, $3, 'succeeded')
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(process)
    .fetch_one(pool)
    .await
    .expect("insert the run")
}

/// The project's live kata candidate: a `kata.open` run of THIS project.
async fn kata_candidate(f: &arch::Fixture, project: Uuid) -> Uuid {
    let kata = process_id(&f.pg.super_pool, f.seeded.tenant_id, "kata.open").await;
    run_of(&f.pg.super_pool, f.seeded.tenant_id, project, kata).await
}

/// Walk a project through Preliminary..G: every phase's approved set (the profile's set plus the
/// standard's mandatory E–H rows), the Phase-G live delivery contract and its compliance review
/// (a level, not a phase-H outcome). Returns the project id; its delivery contract is live.
async fn walk_to_phase_g(f: &arch::Fixture) -> String {
    arch::tailor(f, blueprint(), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;

    for (key, kind) in [
        ("principles_catalog", "principles_catalog"),
        ("principle_derivation", "diagram"),
        ("principle_realization", "diagram"),
    ] {
        approved(f, &project, key, "preliminary", "psa", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "preliminary").await;
    assert_eq!(status, 200, "advance preliminary: {out}");

    let (status, out) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 409, "phase A has no approved set yet: {out}");
    assert_eq!(out["type"], "architecture_artifact_missing", "{out}");
    for (key, kind) in [
        ("stakeholder_map", "matrix"),
        ("driver_goal_objective", "catalog"),
        ("requirements_catalog", "catalog"),
        ("solution_concept", "diagram"),
    ] {
        approved(f, &project, key, "a", "psa", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "a").await;
    assert_eq!(status, 200, "advance a: {out}");

    approved(
        f,
        &project,
        "business_capability_map",
        "b",
        "hld",
        "diagram",
    )
    .await;
    let (status, out) = arch::advance(&f.running, &project, "b").await;
    assert_eq!(status, 200, "advance b: {out}");

    approved(f, &project, "data_entity_catalog", "c", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "c").await;
    assert_eq!(status, 200, "advance c: {out}");

    approved(f, &project, "technology_standards", "d", "hld", "catalog").await;
    let (status, out) = arch::advance(&f.running, &project, "d").await;
    assert_eq!(status, 200, "advance d: {out}");

    // E–H: the standard's mandatory rows join every set, so the phase needs them approved too.
    for (key, kind) in [
        ("solution_concept", "diagram"),
        ("gap_analysis", "matrix"),
        ("candidate_solutions", "catalog"),
        ("project_context_diagram", "diagram"),
        ("benefits_diagram", "diagram"),
    ] {
        approved(f, &project, key, "e", "hld", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "e").await;
    assert_eq!(status, 200, "advance e: {out}");

    for (key, kind) in [
        ("business_capability_map", "diagram"),
        ("implementation_migration_plan", "catalog"),
    ] {
        approved(f, &project, key, "f", "hld", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "f").await;
    assert_eq!(status, 200, "advance f: {out}");

    // Phase G is a gate for every delivery project: a live delivery contract and its compliance
    // review are what open it.
    let contract = arch::live_contract(f, &project, "delivery_contract", "f").await;
    arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": contract,
            "level": "compliant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the delivery is judged at Phase G" }],
            "evidence": { "ref": "phase G" },
        }),
        None,
    )
    .await;
    for (key, kind) in [
        ("technology_standards", "catalog"),
        ("compliance_review", "catalog"),
    ] {
        approved(f, &project, key, "g", "hld", kind).await;
    }
    let (status, out) = arch::advance(&f.running, &project, "g").await;
    assert_eq!(status, 200, "advance g: {out}");
    project
}

/// Phase H's approved set: the profile's type plus the standard's two mandatory H rows.
async fn approve_phase_h(f: &arch::Fixture, project: &str) {
    for (key, kind) in [
        ("solution_concept", "diagram"),
        ("architecture_change_request", "catalog"),
        ("architecture_requirement", "catalog"),
    ] {
        approved(f, project, key, "h", "hld", kind).await;
    }
}

#[tokio::test]
async fn a_scope_change_produces_a_change_register_row() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;
    let project_uuid = Uuid::parse_str(&project).expect("project id");
    let contract =
        arch::request_contract(&f.running, &project, "request_for_architecture", "h").await;
    let contract_id = arch::id_of(&contract);
    let change =
        arch::register_change(&f.pg, f.seeded.tenant_id, project_uuid, f.seeded.user_id).await;

    let review = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": contract_id,
            "level": "non_conformant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the current architecture no longer meets its purpose" }],
            "evidence": {"ref": "change register", "delegation_ref": f.delegation},
            "outcome": { "kind": "change", "change_id": change },
        }),
        None,
    )
    .await;
    assert_eq!(review["outcome"]["kind"], "change", "{review}");
    assert_eq!(review["outcome"]["change_id"], change, "{review}");

    // A systemic finding is a kata candidate and writes no change row of its own. One outcome
    // review may govern a project (#526), so the kata case seeds its own project.
    let before = arch::change_rows(&f.pg, f.seeded.tenant_id).await;
    let kata_project = arch::project(&f.running, "architecture").await;
    let kata_project_uuid = Uuid::parse_str(&kata_project).expect("project id");
    let kata_contract =
        arch::request_contract(&f.running, &kata_project, "statement_of_intent", "h").await;
    let candidate = kata_candidate(&f, kata_project_uuid).await;
    let kata = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": kata_project,
            "contract_id": arch::id_of(&kata_contract),
            "level": "non_conformant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "systemic: the review ritual itself failed" }],
            "outcome": { "kind": "kata", "kata_candidate_id": candidate },
        }),
        None,
    )
    .await;
    assert_eq!(kata["outcome"]["kind"], "kata", "{kata}");
    assert_eq!(
        kata["outcome"]["kata_candidate_id"],
        candidate.to_string(),
        "the kata outcome does not name its candidate: {kata}"
    );
    assert_eq!(
        arch::change_rows(&f.pg, f.seeded.tenant_id).await,
        before,
        "a kata verdict wrote a change register row"
    );
}

#[tokio::test]
async fn an_unrecorded_review_refuses_the_gate() {
    let f = arch::fixture().await;
    let project = walk_to_phase_g(&f).await;
    let project_uuid = Uuid::parse_str(&project).expect("project id");
    approve_phase_h(&f, &project).await;

    // Zero outcomes: the Phase-G review records a level, not a phase-H outcome.
    let (status, refused) = arch::advance(&f.running, &project, "h").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_outcome_required",
        "{refused}"
    );

    // The one outcome must sit on the project's LIVE delivery contract. A delivery contract
    // requested for phase H is approved live and carries the kata outcome; the gate opens.
    let contract = arch::live_contract(&f, &project, "delivery_contract", "h").await;
    let candidate = kata_candidate(&f, project_uuid).await;
    arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": contract,
            "level": "compliant",
            "delegation_ref": f.delegation,
            "outcome": { "kind": "kata", "kata_candidate_id": candidate },
        }),
        None,
    )
    .await;

    let (status, out) = arch::advance(&f.running, &project, "h").await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["closed_phase"], "h", "{out}");
    assert!(out["next_phase"].is_null(), "H is the last phase: {out}");
}

#[tokio::test]
async fn an_outcome_on_a_non_delivery_contract_does_not_satisfy_h() {
    let f = arch::fixture().await;
    let project = walk_to_phase_g(&f).await;
    let project_uuid = Uuid::parse_str(&project).expect("project id");
    approve_phase_h(&f, &project).await;

    // A typed outcome on a draft request-for-architecture contract is a real review row, but it
    // is not the design gate's instrument: the contract is neither live nor a delivery contract.
    let contract =
        arch::request_contract(&f.running, &project, "request_for_architecture", "h").await;
    let candidate = kata_candidate(&f, project_uuid).await;
    let review = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": arch::id_of(&contract),
            "level": "compliant",
            "delegation_ref": f.delegation,
            "outcome": { "kind": "kata", "kata_candidate_id": candidate },
        }),
        None,
    )
    .await;
    assert_eq!(review["outcome"]["kind"], "kata", "{review}");

    let (status, refused) = arch::advance(&f.running, &project, "h").await;
    assert_eq!(status, 409, "a non-delivery outcome opened H: {refused}");
    assert_eq!(
        refused["type"], "architecture_outcome_not_on_delivery_contract",
        "{refused}"
    );
}
