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
    assert_eq!(counts.benefits, 6, "the six benefit profiles");
    assert_eq!(counts.links, 6, "the benefits map");
    assert_eq!(counts.tranches, 14, "fourteen tranches");

    let benefits: Vec<(String, String)> = sqlx::query_as(
        "SELECT reference, status FROM benefit_profile
          WHERE tenant_id = $1 ORDER BY reference",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("benefits");
    assert_eq!(benefits.len(), 6);
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
    assert_eq!(links, 6);

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
    assert!(
        tranches[1..].iter().all(|(_, state)| state == "planned"),
        "tranches 1–13 are planned: {tranches:?}"
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
    assert_eq!(first[0].1.benefits, 6);

    // A published profile is not the seed's to rewrite.
    sqlx::query(
        "UPDATE benefit_profile SET status = 'approved', version = version + 1
          WHERE tenant_id = $1 AND reference = 'B1'",
    )
    .bind(seeded.tenant)
    .execute(&db.super_pool)
    .await
    .expect("publish B1");

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
    assert_eq!(status, "approved", "the published row is untouched");
    let benefits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM benefit_profile WHERE tenant_id = $1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("benefit count");
    assert_eq!(benefits, 6);
    let tranches: i64 =
        sqlx::query_scalar("SELECT count(*) FROM programme_tranche WHERE tenant_id = $1")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("tranche count");
    assert_eq!(tranches, 14);
}
