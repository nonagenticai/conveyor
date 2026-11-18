//! T10 (#82) P4.5 and P5 — benefits realization is a procedure after delivery.
//!
//! A dependency slip names the benefits the map puts at risk; a measured baseline enables the
//! realization (post-measurement, delta, legacy retirement); a baseline that was never measured
//! cannot claim an improvement; and a programme closes only with a realization plan that names
//! who continues the measurement.
//!
//! `cargo test -p conveyor-store --test benefit_transition`.

#[path = "support/mod.rs"]
mod support;

use chrono::Utc;
use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError, TransitionPatch};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    programme: Uuid,
    benefit: Uuid,
    dependency: Uuid,
    project: Uuid,
    user: Uuid,
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Benefits') RETURNING id")
            .bind(format!("benefit-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the programme");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'owner@dev.invalid', 'Owner') RETURNING id",
    )
    .bind(format!("owner-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the user");
    let mut projects = Vec::new();
    for key in ["A", "B"] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name, programme_id)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(tenant_id)
        .bind(format!("{key}-{}", Uuid::now_v7().simple()))
        .bind(format!("Project {key}"))
        .bind(programme)
        .fetch_one(&db.super_pool)
        .await
        .expect("insert the project");
        projects.push(id);
    }
    let output: Uuid = sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status)
         VALUES ($1, $2, 0, 'deliverable', 'the new capability', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(projects[1])
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the output");
    let benefit: Uuid = sqlx::query_scalar(
        "INSERT INTO benefit_profile (tenant_id, programme_id, reference, description,
                                      baseline, operational_area, status)
         VALUES ($1, $2, $3, 'a measurable improvement', $4, 'operations', 'active')
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(format!("B-{}", Uuid::now_v7().simple()))
    .bind(json!({ "value": 10.0 }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the benefit profile");
    sqlx::query(
        "INSERT INTO benefit_link (tenant_id, programme_id, output_ref, capability_ref,
                                   operational_outcome, benefit_id, strategic_objective)
         VALUES ($1, $2, $3, 'the capability', 'faster delivery', $4, 'the objective')",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(output.to_string())
    .bind(benefit)
    .execute(&db.super_pool)
    .await
    .expect("insert the benefit-map edge");
    let dependency: Uuid = sqlx::query_scalar(
        "INSERT INTO project_dependency (tenant_id, programme_id, from_project, to_project, kind)
         VALUES ($1, $2, $3, $4, 'blocks') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(projects[0])
    .bind(projects[1])
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the dependency");
    Fixture {
        tenant,
        programme,
        benefit,
        dependency,
        project: projects[0],
        user,
    }
}

async fn patch(
    db: &support::TestDb,
    f: &Fixture,
    patch: TransitionPatch,
) -> Result<(), PlanStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::upsert_transition_tx(&mut tx, f.tenant, f.benefit, None, &patch)
        .await
        .map(|_| ());
    match &outcome {
        Ok(()) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

#[tokio::test]
async fn a_dependency_slip_names_the_benefits_at_risk() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let slip = spine::record_dependency_slip_tx(&mut tx, f.tenant, f.dependency)
        .await
        .expect("record the slip");
    tx.commit().await.expect("commit");
    assert_eq!(
        slip.benefits,
        vec![f.benefit],
        "the map's edge names the benefit at risk"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM project_dependency WHERE id = $1")
        .bind(f.dependency)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the dependency");
    assert_eq!(status, "slipped");
}

#[tokio::test]
async fn a_measured_baseline_enables_realization() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let now = Utc::now();
    patch(
        &db,
        &f,
        TransitionPatch {
            baseline_measured_at: Some(now),
            readiness: Some(json!({ "ready": true })),
            enacted_at: Some(now),
            post_measured_at: Some(now),
            retired_at: Some(now),
            evidence: Some(json!({ "baseline_value": 10.0, "post_value": 14.0 })),
        },
    )
    .await
    .expect("the measured baseline enables the realization");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let realization = spine::benefit_realization_tx(&mut tx, f.tenant, f.benefit)
        .await
        .expect("read the realization");
    tx.commit().await.expect("commit");
    assert_eq!(realization.operational_area, "operations");
    assert_eq!(realization.delta, Some(4.0), "post - baseline");
    assert!(
        realization.retired_at.is_some(),
        "the legacy practice retires"
    );
}

#[tokio::test]
async fn a_missing_baseline_cannot_claim_realization() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let now = Utc::now();
    let refused = patch(
        &db,
        &f,
        TransitionPatch {
            post_measured_at: Some(now),
            evidence: Some(json!({ "post_value": 14.0 })),
            ..TransitionPatch::default()
        },
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::BenefitBaselineMissing))
        ),
        "a benefit with no measured baseline cannot report a realized improvement: {refused:?}"
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM benefit_transition WHERE tenant_id = $1")
            .bind(f.tenant.as_uuid())
            .fetch_one(&db.super_pool)
            .await
            .expect("count transitions");
    assert_eq!(rows, 0, "no realization row is written");
}

#[tokio::test]
async fn a_close_without_a_post_closure_owner_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    // No plan at all: a different refusal from a plan with no owner.
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::close_programme_tx(&mut tx, f.tenant, f.programme).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::RealizationPlanMissing))
        ),
        "a programme with no realization plan cannot close: {refused:?}"
    );

    sqlx::query(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, project_id, status, meta)
         VALUES ($1, 'benefits_realization_plan', $2, 'Realization plan', $3, 'approved',
                 '{\"post_closure_owner\": \"\"}'::jsonb)",
    )
    .bind(f.tenant.as_uuid())
    .bind(format!("RP-{}", Uuid::now_v7().simple()))
    .bind(f.project)
    .execute(&db.super_pool)
    .await
    .expect("insert the plan");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::close_programme_tx(&mut tx, f.tenant, f.programme).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(PlanStoreError::Illegal(PlanError::PostClosureOwnerMissing))
        ),
        "a plan without a post-closure owner is refused: {refused:?}"
    );

    sqlx::query(
        "UPDATE governed_asset SET meta = jsonb_build_object('post_closure_owner', $2)
          WHERE tenant_id = $1 AND kind = 'benefits_realization_plan'",
    )
    .bind(f.tenant.as_uuid())
    .bind(format!("ops lead ({})", f.user))
    .execute(&db.super_pool)
    .await
    .expect("name the owner");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    spine::close_programme_tx(&mut tx, f.tenant, f.programme)
        .await
        .expect("the named owner closes the programme");
    tx.commit().await.expect("commit");
    let (status,): (String,) = sqlx::query_as("SELECT status FROM programme WHERE id = $1")
        .bind(f.programme)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the programme");
    assert_eq!(status, "closed");
}
