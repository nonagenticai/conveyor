//! The tranche-4 architecture residuals (v1-p81-1/2/3; T9 · SPEC_V2 §8.1).
//!
//! The phase gate reads the adopted profile and falls back to the standard's mandatory rows;
//! Phase G is a real gate (live delivery contract + compliance review + waiver when the review
//! is non-conformant); an adopted profile is immutable and its adoption cites a real board
//! decision.
//!
//! `cargo test -p conveyor-store --test architecture_gates`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::architecture::{self, ArchitectureRule, ArchitectureStoreError, NewProfile};
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    project: Uuid,
    reviewer: Uuid,
}

async fn fixture(db: &support::TestDb, phases: &[(&str, i64)]) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ($1, 'Architecture gates') RETURNING id",
    )
    .bind(format!("arch-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let reviewer: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'reviewer@dev.invalid', 'Reviewer') RETURNING id",
    )
    .bind(format!("reviewer-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the reviewer");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status)
         VALUES ($1, $2, 'Architecture project', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    // The phase nodes the gates advance over. Only the requested node is created per test, so
    // it is the first (and only) open phase and the order check passes by construction.
    for (code, position) in phases {
        sqlx::query(
            "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status, meta)
             VALUES ($1, $2, $3, 'phase', $4, 'open', $5)",
        )
        .bind(tenant_id)
        .bind(project)
        .bind(position)
        .bind(format!("Phase {}", code.to_uppercase()))
        .bind(json!({ "phase_code": code }))
        .execute(&db.super_pool)
        .await
        .expect("insert the phase node");
    }
    // The starter standard: phases preliminary–f carry their mandatory rows.
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    architecture::seed_standard_tx(&mut tx, tenant)
        .await
        .expect("seed the standard");
    tx.commit().await.expect("commit the standard");
    Fixture {
        tenant,
        project,
        reviewer,
    }
}

async fn upsert(
    db: &support::TestDb,
    f: &Fixture,
    key: &str,
    requirements: serde_json::Value,
) -> Result<architecture::ProfileRecord, ArchitectureStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let result = architecture::upsert_profile_tx(
        &mut tx,
        f.tenant,
        &NewProfile {
            key: key.to_owned(),
            name: format!("Profile {key}"),
            phase_set: json!({}),
            artifact_requirements: requirements,
            contract_kinds: json!({}),
        },
    )
    .await;
    if result.is_ok() {
        tx.commit().await.expect("commit the profile");
    } else {
        tx.rollback().await.expect("rollback the profile");
    }
    result
}

async fn adopt(
    db: &support::TestDb,
    f: &Fixture,
    profile: &architecture::ProfileRecord,
    decision: Uuid,
) -> Result<architecture::ProfileRecord, ArchitectureStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let result =
        architecture::adopt_profile_tx(&mut tx, f.tenant, profile.id, profile.version, decision)
            .await;
    if result.is_ok() {
        tx.commit().await.expect("commit the adoption");
    } else {
        tx.rollback().await.expect("rollback the adoption");
    }
    result
}

/// An approved architecture-board decision about this profile.
async fn board_decision(db: &support::TestDb, f: &Fixture, profile: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, required_decisions, meta)
         VALUES ($1, 'architecture_board', 'architecture_board', 'approved', 1, $2)
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(json!({ "subject_kind": "adm_profile", "subject_id": profile.to_string() }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the board decision")
}

async fn advance(
    db: &support::TestDb,
    f: &Fixture,
    phase: &str,
    profile_key: Option<&str>,
) -> Result<architecture::PhaseAdvance, ArchitectureStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let result =
        architecture::advance_phase_tx(&mut tx, f.tenant, f.project, phase, profile_key).await;
    if result.is_ok() {
        tx.commit().await.expect("commit the advance");
    } else {
        tx.rollback().await.expect("rollback the advance");
    }
    result
}

/// An approved artefact of the standard's mandatory type for `phase`.
async fn approve_standard_artifact(
    db: &support::TestDb,
    f: &Fixture,
    phase: &str,
    key: &str,
) -> Uuid {
    let (type_id, plateau, kind): (Uuid, String, String) = sqlx::query_as(
        "SELECT id, plateau, artifact_kind FROM artifact_type
          WHERE tenant_id = $1 AND key = $2 AND phase_code = $3 LIMIT 1",
    )
    .bind(f.tenant.as_uuid())
    .bind(key)
    .bind(phase)
    .fetch_one(&db.super_pool)
    .await
    .expect("the standard type");
    sqlx::query_scalar(
        "INSERT INTO architecture_artifact
            (tenant_id, project_id, artifact_type_id, phase_code, plateau, artifact_kind,
             title, status, reviewer_id, reviewed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'approved', $8, now())
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .bind(type_id)
    .bind(phase)
    .bind(plateau)
    .bind(kind)
    .bind(format!("{key} ({phase})"))
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the approved artefact")
}

async fn live_contract(db: &support::TestDb, f: &Fixture) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO architecture_contract (tenant_id, project_id, kind, phase_code, title, status)
         VALUES ($1, $2, 'delivery_contract', 'g', 'The delivery contract', 'live')
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the live delivery contract")
}

async fn compliance_review(db: &support::TestDb, f: &Fixture, contract: Uuid, level: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO architecture_compliance_review
            (tenant_id, project_id, contract_id, level, reviewed_by)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .bind(contract)
    .bind(level)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the compliance review")
}

async fn active_waiver(db: &support::TestDb, f: &Fixture, review: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO architecture_waiver
            (tenant_id, review_id, owner_user_id, reason, reassessment_at)
         VALUES ($1, $2, $3, 'The deviation is accepted until the reassessment',
                 now() + interval '30 days')
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(review)
    .bind(f.reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the waiver")
}

/// v1-p81-1: a named draft is refused; an absent phase key falls back to the standard's
/// mandatory rows, so a phase cannot close on nothing.
#[tokio::test]
async fn a_phase_closes_on_the_adopted_profile_or_the_standard() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, &[("a", 1)]).await;

    // A caller-named draft is not an authority.
    upsert(
        &db,
        &f,
        "draft",
        json!({ "a": { "psa": ["stakeholder_map"] } }),
    )
    .await
    .expect("insert the draft");
    let refused = advance(&db, &f, "a", Some("draft")).await;
    assert!(
        matches!(
            refused,
            Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::ProfileNotAdopted
            ))
        ),
        "a draft profile cannot gate a phase: {refused:?}"
    );

    // An adopted profile that does not name phase A: the standard's mandatory set applies.
    let profile = upsert(&db, &f, "adopted", json!({}))
        .await
        .expect("insert the adopted draft");
    let decision = board_decision(&db, &f, profile.id).await;
    adopt(&db, &f, &profile, decision)
        .await
        .expect("adopt the profile");
    let refused = advance(&db, &f, "a", None).await;
    assert!(
        matches!(
            &refused,
            Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::MissingMandatory(_)
            ))
        ),
        "the standard's mandatory artefact is required: {refused:?}"
    );

    // Every mandatory artefact of phase A's plateau is approved: the phase closes.
    for key in [
        "stakeholder_map",
        "driver_goal_objective",
        "requirements_catalog",
        "solution_concept",
    ] {
        approve_standard_artifact(&db, &f, "a", key).await;
    }
    let advanced = advance(&db, &f, "a", None).await;
    assert!(
        advanced.is_ok(),
        "the adopted profile plus the standard closes phase A: {advanced:?}"
    );
}

/// v1-p81-2: Phase G needs the live contract, its compliance review and — for a
/// non-conformant review — an active waiver.
#[tokio::test]
async fn phase_g_needs_the_contract_the_review_and_a_waiver() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, &[("g", 7)]).await;

    let refused = advance(&db, &f, "g", None).await;
    assert!(
        matches!(
            refused,
            Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::DesignContractMissing
            ))
        ),
        "phase G without a live delivery contract is refused: {refused:?}"
    );

    let contract = live_contract(&db, &f).await;
    let refused = advance(&db, &f, "g", None).await;
    assert!(
        matches!(
            refused,
            Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::ComplianceReviewMissing
            ))
        ),
        "phase G without a compliance review is refused: {refused:?}"
    );

    let review = compliance_review(&db, &f, contract, "non_conformant").await;
    let refused = advance(&db, &f, "g", None).await;
    assert!(
        matches!(
            refused,
            Err(ArchitectureStoreError::Illegal(
                ArchitectureRule::ComplianceWaiverMissing
            ))
        ),
        "a non-conformant review needs an active waiver: {refused:?}"
    );

    active_waiver(&db, &f, review).await;
    let advanced = advance(&db, &f, "g", None).await;
    assert!(
        advanced.is_ok(),
        "the live contract, the review and the waiver close phase G: {advanced:?}"
    );
}

/// v1-p81-3: an adopted profile is immutable, and an adoption cites an approved board decision
/// about that profile.
#[tokio::test]
async fn an_adopted_profile_is_immutable_and_adoption_needs_its_decision() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, &[("a", 1)]).await;

    let profile = upsert(&db, &f, "p3", json!({}))
        .await
        .expect("insert the draft");
    let refused = adopt(&db, &f, &profile, Uuid::now_v7()).await;
    assert!(
        matches!(refused, Err(ArchitectureStoreError::Conflict { .. })),
        "a random approval id is not an adoption decision: {refused:?}"
    );

    let decision = board_decision(&db, &f, profile.id).await;
    adopt(&db, &f, &profile, decision)
        .await
        .expect("the real board decision adopts the profile");

    let refused = upsert(
        &db,
        &f,
        "p3",
        json!({ "a": { "psa": ["stakeholder_map"] } }),
    )
    .await;
    assert!(
        matches!(refused, Err(ArchitectureStoreError::Conflict { .. })),
        "an adopted profile cannot be rewritten: {refused:?}"
    );
}
