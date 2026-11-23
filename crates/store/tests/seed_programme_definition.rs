//! `seed_programme_definition`: the buildout's own MSP definition seeds as rows.
//!
//! A fresh tenant gets the six benefit profiles and their map, the fourteen tranches, and each
//! project's ADM phase/plateau in `meta`. A second run inserts nothing and never overwrites a
//! published row.
//!
//! `cargo test -p conveyor-store --test seed_programme_definition`.

#[path = "support/mod.rs"]
mod support;

use conveyor_store::seed_programme::seed_programme;

#[tokio::test]
async fn a_fresh_tenant_seeds_its_msp_definition() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "msp").await;

    let first = seed_programme(&db.super_pool, Some("msp"))
        .await
        .expect("first run");
    let counts = first[0].1;
    assert_eq!(counts.benefits, 7, "the seven benefit profiles");
    assert_eq!(counts.links, 8, "the benefits map");
    assert_eq!(counts.tranches, 14, "fourteen tranches");

    let benefits: Vec<(String, String)> = sqlx::query_as(
        "SELECT reference, status FROM benefit_profile
          WHERE tenant_id = $1 ORDER BY reference",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("benefits");
    assert_eq!(benefits.len(), 7);
    assert_eq!(benefits[0].0, "B1");
    assert!(
        benefits.iter().all(|(_, status)| status == "draft"),
        "the seeded profiles are drafts: {benefits:?}"
    );
    let baseline: String = sqlx::query_scalar(
        "SELECT baseline #>> '{}' FROM benefit_profile
          WHERE tenant_id = $1 AND reference = 'B1'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("B1 baseline");
    assert!(
        baseline.contains("not measured"),
        "the baseline is honest: {baseline}"
    );

    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM benefit_link WHERE tenant_id = $1")
        .bind(seeded.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("links");
    assert_eq!(links, 8);

    let tranches: Vec<(i32, String)> = sqlx::query_as(
        "SELECT number, state FROM programme_tranche
          WHERE tenant_id = $1 ORDER BY number",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("tranches");
    assert_eq!(tranches.len(), 14);
    assert_eq!(tranches[0], (0, "closed".to_owned()));
    assert_eq!(
        tranches[1],
        (1, "authorised".to_owned()),
        "the tranche-0 continue review authorises tranche 1: {tranches:?}"
    );
    assert!(
        tranches[2..].iter().all(|(_, state)| state == "planned"),
        "tranches 2–13 are planned: {tranches:?}"
    );
    let reviewed: Option<String> = sqlx::query_scalar(
        "SELECT review_at::text FROM programme_tranche
          WHERE tenant_id = $1 AND number = 0",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("review date");
    assert!(reviewed.is_some(), "tranche 0 carries its review date");

    for (key, phase, plateau) in [
        ("T11", "G", "capability"),
        ("P0", "Preliminary", "definition"),
    ] {
        let row: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT meta ->> 'adm_phase', meta ->> 'plateau'
               FROM project WHERE tenant_id = $1 AND key = $2",
        )
        .bind(seeded.tenant)
        .bind(key)
        .fetch_one(&db.super_pool)
        .await
        .expect("project meta");
        assert_eq!(row.0.as_deref(), Some(phase), "{key} phase");
        assert_eq!(row.1.as_deref(), Some(plateau), "{key} plateau");
    }
}

#[tokio::test]
async fn a_repeated_seed_is_a_no_op_and_never_overwrites_a_published_row() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "msp-repeat").await;
    let first = seed_programme(&db.super_pool, Some("msp-repeat"))
        .await
        .expect("first run");
    assert_eq!(first[0].1.benefits, 7);

    // A profile moved out of draft is not the seed's to rewrite.
    sqlx::query(
        "UPDATE benefit_profile SET status = 'active', version = version + 1
          WHERE tenant_id = $1 AND reference = 'B1'",
    )
    .bind(seeded.tenant)
    .execute(&db.super_pool)
    .await
    .expect("activate B1");

    let second = seed_programme(&db.super_pool, Some("msp-repeat"))
        .await
        .expect("second run");
    let counts = second[0].1;
    assert_eq!(counts.benefits, 0, "a repeated seed inserts no benefit");
    assert_eq!(counts.links, 0, "a repeated seed inserts no link");
    assert_eq!(counts.tranches, 0, "a repeated seed inserts no tranche");

    let status: String = sqlx::query_scalar(
        "SELECT status FROM benefit_profile WHERE tenant_id = $1 AND reference = 'B1'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("B1 status");
    assert_eq!(status, "active", "the non-draft row is untouched");
    let benefits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM benefit_profile WHERE tenant_id = $1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("benefit count");
    assert_eq!(benefits, 7);
    let tranches: i64 =
        sqlx::query_scalar("SELECT count(*) FROM programme_tranche WHERE tenant_id = $1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("tranche count");
    assert_eq!(tranches, 14);
}

/// P1.2 — the seed's authorised tranche 1 is what the start gate reads: a seeded tranche-1
/// project starts, and the gate keeps refusing while the tranche is still planned.
#[tokio::test]
async fn a_seeded_tranche_one_project_starts_after_the_continue_review() {
    use conveyor_domain::TenantId;
    use conveyor_store::begin_scoped;
    use conveyor_store::spine::assert_project_tranche_authorised_tx;

    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "msp-gate").await;
    seed_programme(&db.super_pool, Some("msp-gate"))
        .await
        .expect("seed");

    let project: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM project
          WHERE tenant_id = $1 AND key = 'T1' AND programme_id IS NOT NULL",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the seeded T1 project");
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant id");

    // The seed asserts no authorisation: every tranche starts planned, so the tranche-1
    // project is refused until a review authorises it.
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let refused = assert_project_tranche_authorised_tx(&mut tx, tenant, project).await;
    tx.rollback().await.expect("rollback");
    assert!(
        refused.is_err(),
        "a seeded planned tranche cannot start: {refused:?}"
    );

    // The continue review's effect: a review row of the tranche, named by
    // authorisation_review_id, is what authorises the start. The review's own evidence
    // resolution is proved in tranche_review.rs.
    let reviewer: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE tenant_id = $1 ORDER BY id LIMIT 1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("a seeded app_user");
    let review: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche_review
             (tenant_id, tranche_id, decision, evidence, reviewed_by)
         SELECT $1, id, 'continue', '{\"fixture\": true}'::jsonb, $2
           FROM programme_tranche WHERE tenant_id = $1 AND number = 1
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(reviewer)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the authorising review");
    sqlx::query(
        "UPDATE programme_tranche
            SET state = 'authorised', authorisation_review_id = $2, authorised_at = now()
          WHERE tenant_id = $1 AND number = 1",
    )
    .bind(seeded.tenant)
    .bind(review)
    .execute(&db.super_pool)
    .await
    .expect("authorise tranche 1");
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let allowed = assert_project_tranche_authorised_tx(&mut tx, tenant, project).await;
    tx.commit().await.expect("commit");
    assert!(
        allowed.is_ok(),
        "the reviewed tranche-1 project starts: {allowed:?}"
    );
}

/// T10 — every seeded benefit resolves both job owners, and the governance roles are real
/// `job_catalogue` rows rather than text in `operational_area`.
#[tokio::test]
async fn every_seeded_benefit_resolves_both_job_owners() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "msp-jobs").await;
    seed_programme(&db.super_pool, Some("msp-jobs"))
        .await
        .expect("seed");

    let owners: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT b.reference, owner.job_key, benefit_owner.job_key,
                b.owner_job_id = b.benefit_owner_job_id
           FROM benefit_profile b
           JOIN job_catalogue owner ON owner.id = b.owner_job_id
           JOIN job_catalogue benefit_owner ON benefit_owner.id = b.benefit_owner_job_id
          WHERE b.tenant_id = $1
          ORDER BY b.reference",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("every benefit joins both owner jobs");
    assert_eq!(
        owners.len(),
        6,
        "all six benefits have both owners: {owners:?}"
    );
    for (reference, owner, benefit_owner, same) in owners {
        assert_eq!(owner, "bcm", "{reference} owner is the BCM job");
        assert_eq!(
            benefit_owner, "bcm",
            "{reference} benefit owner is the BCM job"
        );
        assert!(
            same,
            "{reference} owner and benefit owner resolve the same job id"
        );
    }

    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT job_key FROM job_catalogue
          WHERE tenant_id = $1 AND job_key IN ('bcm', 'board', 'sro', 'sponsoring_group')
          ORDER BY job_key",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("governance roles");
    assert_eq!(
        roles,
        vec!["bcm", "board", "sponsoring_group", "sro"],
        "SRO, BCM, sponsoring group and board are seeded as job keys"
    );
}
