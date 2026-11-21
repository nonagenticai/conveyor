//! The public snapshot carries the rows an MSP/TOGAF conformance audit must read: the tranche
//! authorisation link, the compliance reviews and their waivers. The programme boundary still
//! applies — an unbound project's architecture rows are not public.
//!
//! `cargo test -p conveyor-store --test snapshot_conformance`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    programme: Uuid,
    reviewer: Uuid,
    review: Uuid,
    tranche1: Uuid,
    bound_project: Uuid,
    unbound_project: Uuid,
}

async fn project(pool: &PgPool, tenant: Uuid, programme: Option<Uuid>, key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(key)
    .bind(format!("Project {key}"))
    .bind(programme)
    .fetch_one(pool)
    .await
    .expect("insert the project")
}

/// A contract, a compliance review and its waiver for one project.
async fn architecture_tree(
    pool: &PgPool,
    tenant: Uuid,
    project: Uuid,
    reviewer: Uuid,
) -> (Uuid, Uuid) {
    let contract: Uuid = sqlx::query_scalar(
        "INSERT INTO architecture_contract
             (tenant_id, project_id, kind, phase_code, title, status, approved_by,
              requested_by, approved_at)
         VALUES ($1, $2, 'statement_of_architecture_work', 'a', 'SAW', 'live', $3, $3, now())
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(reviewer)
    .fetch_one(pool)
    .await
    .expect("insert the contract");
    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO architecture_compliance_review
             (tenant_id, project_id, contract_id, level, findings, evidence, reviewed_by)
         VALUES ($1, $2, $3, 'compliant',
                 '[{\"code\": \"ARCH-1\"}]'::jsonb, '[{\"ref\": \"saw-doc\"}]'::jsonb, $4)
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(contract)
    .bind(reviewer)
    .fetch_one(pool)
    .await
    .expect("insert the review");
    sqlx::query(
        "INSERT INTO architecture_waiver
             (tenant_id, review_id, owner_user_id, reason, reassessment_at)
         VALUES ($1, $2, $3, 'retrospective acceptance', now() + interval '90 days')",
    )
    .bind(tenant)
    .bind(review)
    .bind(reviewer)
    .execute(pool)
    .await
    .expect("insert the waiver");
    (contract, review)
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Snapshot') RETURNING id")
            .bind(format!("snapshot-{}", Uuid::now_v7().simple()))
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
    let reviewer: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'reviewer@dev.invalid', 'Reviewer') RETURNING id",
    )
    .bind(format!("reviewer-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the reviewer");
    let tranche0: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 0, 'closed') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert tranche 0");
    let tranche1: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number)
         VALUES ($1, $2, 1) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert tranche 1");
    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche_review (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, 'continue',
                 '{\"baselines\": {}, \"benefit_status\": {}, \"risk_register\": {}}'::jsonb, $3)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(tranche0)
    .bind(reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the review");
    sqlx::query(
        "UPDATE programme_tranche
            SET state = 'authorised', authorised_at = now(), authorisation_review_id = $2
          WHERE id = $1",
    )
    .bind(tranche1)
    .bind(review)
    .execute(&db.super_pool)
    .await
    .expect("authorise tranche 1");
    let bound_project = project(&db.super_pool, tenant_id, Some(programme), "T1").await;
    let unbound_project = project(&db.super_pool, tenant_id, None, "PRIVATE").await;
    architecture_tree(&db.super_pool, tenant_id, bound_project, reviewer).await;
    architecture_tree(&db.super_pool, tenant_id, unbound_project, reviewer).await;
    Fixture {
        tenant,
        tenant_id,
        programme,
        reviewer,
        review,
        tranche1,
        bound_project,
        unbound_project,
    }
}

async fn snapshot(db: &support::TestDb, tenant: TenantId) -> store::snapshot::SnapshotData {
    let mut tx = store::begin_scoped(&db.owner, tenant).await.expect("scope");
    let data = store::snapshot::snapshot_tx(&mut tx, tenant)
        .await
        .expect("snapshot");
    tx.rollback().await.expect("rollback");
    data
}

#[tokio::test]
async fn the_snapshot_carries_the_tranche_link_reviews_and_waivers() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let data = snapshot(&db, f.tenant).await;

    let tranches = data.msp["tranches"].as_array().expect("tranches");
    let tranche1 = tranches
        .iter()
        .find(|row| row["id"] == f.tranche1.to_string())
        .expect("tranche 1");
    assert!(tranche1["authorised_at"].is_string(), "{tranche1}");
    assert_eq!(
        tranche1["authorisation_review_id"],
        f.review.to_string(),
        "{tranche1}"
    );

    let reviews = data.architecture["compliance_reviews"]
        .as_array()
        .expect("compliance reviews");
    assert_eq!(
        reviews.len(),
        1,
        "only the programme-bound project's review is public"
    );
    let waivers = data.architecture["waivers"].as_array().expect("waivers");
    assert_eq!(waivers.len(), 1);
    // The bound review and its waiver survive the programme boundary; the unbound pair does not.
    assert!(
        reviews
            .iter()
            .all(|row| row["project_id"] == f.bound_project.to_string()),
        "{reviews:?}"
    );
    assert_eq!(reviews[0]["level"], "compliant");
    assert_eq!(reviews[0]["reviewed_by_name"], "Reviewer");
    assert!(waivers[0]["reassessment_at"].is_string());
    assert_eq!(waivers[0]["owner_name"], "Reviewer");

    // The keys the pack already published are still there.
    for key in [
        "profiles",
        "standard",
        "artifacts",
        "links",
        "contracts",
        "board",
        "maturity",
        "skills",
    ] {
        assert!(
            data.architecture.get(key).is_some(),
            "architecture.{key} must survive the projection change"
        );
    }
    let _ = (f.programme, f.reviewer, f.unbound_project);
}

/// The cadence projection and the audit fields: a cadence's demo credits and lessons travel in
/// the `msp` object beside it, and the contract/review rows carry the requester, findings and
/// evidence an MSP/TOGAF conformance audit must read back.
#[tokio::test]
async fn the_snapshot_carries_the_cadence_credits_lessons_and_audit_fields() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period)
         VALUES ($1, $2, 'sprint', 'S-1') RETURNING id",
    )
    .bind(f.tenant_id)
    .bind(f.programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the cadence");
    let deliverable: Uuid = sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, kind, title)
         VALUES ($1, $2, 'deliverable', 'Increment') RETURNING id",
    )
    .bind(f.tenant_id)
    .bind(f.bound_project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the deliverable");
    sqlx::query(
        "INSERT INTO programme_demo_credit (tenant_id, cadence_id, deliverable_id, evidence)
         VALUES ($1, $2, $3, '[{\"ref\": \"demo-1\"}]'::jsonb)",
    )
    .bind(f.tenant_id)
    .bind(cadence)
    .bind(deliverable)
    .execute(&db.super_pool)
    .await
    .expect("insert the demo credit");
    sqlx::query(
        "INSERT INTO lesson (tenant_id, programme_id, cadence_id, finding_key, statement)
         VALUES ($1, $2, $3, 'L-1', 'Demo arrived a day late')",
    )
    .bind(f.tenant_id)
    .bind(f.programme)
    .bind(cadence)
    .execute(&db.super_pool)
    .await
    .expect("insert the lesson");

    let data = snapshot(&db, f.tenant).await;

    let credits = data.msp["demo_credits"].as_array().expect("demo credits");
    assert_eq!(credits.len(), 1, "{credits:?}");
    assert_eq!(credits[0]["cadence_id"], cadence.to_string());
    assert_eq!(credits[0]["deliverable_id"], deliverable.to_string());
    assert_eq!(credits[0]["evidence"][0]["ref"], "demo-1");
    assert!(credits[0]["credited_at"].is_string(), "{}", credits[0]);

    let lessons = data.msp["lessons"].as_array().expect("lessons");
    assert_eq!(lessons.len(), 1, "{lessons:?}");
    assert_eq!(lessons[0]["programme_id"], f.programme.to_string());
    assert_eq!(lessons[0]["cadence_id"], cadence.to_string());
    assert_eq!(lessons[0]["finding_key"], "L-1");
    assert_eq!(lessons[0]["state"], "proposed");
    assert_eq!(lessons[0]["source"], "retro");

    let contracts = data.architecture["contracts"]
        .as_array()
        .expect("contracts");
    assert!(
        contracts
            .iter()
            .all(|row| row["requested_by"] == f.reviewer.to_string()),
        "{contracts:?}"
    );
    let reviews = data.architecture["compliance_reviews"]
        .as_array()
        .expect("reviews");
    assert!(
        reviews
            .iter()
            .all(|row| row["findings"][0]["code"] == "ARCH-1"),
        "{reviews:?}"
    );
    assert!(
        reviews
            .iter()
            .all(|row| row["evidence"][0]["ref"] == "saw-doc"),
        "{reviews:?}"
    );
}
