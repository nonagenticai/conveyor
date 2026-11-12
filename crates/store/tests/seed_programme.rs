//! `seed_programme`: the buildout programme and its projects are idempotent rows.
//!
//! The natural keys are `(tenant, programme.name)` and `(tenant, project.key)`. A second run
//! with an unchanged document creates nothing, bumps no version and leaves `updated_at` alone;
//! the programme owns its named meta keys and preserves every other key a later job wrote.
//!
//! `cargo test -p conveyor-store --test seed_programme`.

#[path = "support/mod.rs"]
mod support;

use conveyor_store::seed_programme::seed_programme;

#[tokio::test]
async fn seed_programme_is_idempotent() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prog").await;

    let first = seed_programme(&db.super_pool, Some("prog"))
        .await
        .expect("first run");
    assert_eq!(first.len(), 1, "one tenant targeted");
    let (slug, counts) = &first[0];
    assert_eq!(slug, "prog");
    assert_eq!(counts.created, 48, "the programme and its 47 projects");
    assert_eq!(counts.updated, 0);
    assert_eq!(counts.skipped, 0);

    let second = seed_programme(&db.super_pool, Some("prog"))
        .await
        .expect("second run");
    let counts = second[0].1;
    assert_eq!(counts.created, 0, "a second run creates nothing");
    assert_eq!(counts.updated, 0, "a second run updates nothing");
    assert_eq!(counts.skipped, 48);

    let programmes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM programme WHERE tenant_id = $1 AND name = 'conveyor buildout'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count programmes");
    assert_eq!(programmes, 1);

    let projects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project
          WHERE tenant_id = $1 AND programme_id IS NOT NULL AND meta ? 'track'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count projects");
    assert_eq!(projects, 47);

    let versions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project
          WHERE tenant_id = $1 AND meta ? 'track' AND version <> 1",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count bumped versions");
    assert_eq!(versions, 0, "a repeated seed does not bump a version");
}

#[tokio::test]
async fn seed_programme_links_the_registered_issues() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "links").await;

    seed_programme(&db.super_pool, Some("links"))
        .await
        .expect("seed");

    let issues: serde_json::Value = sqlx::query_scalar(
        "SELECT meta -> 'issues' FROM project WHERE tenant_id = $1 AND key = 'S12'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("S12 issues");
    assert_eq!(issues, serde_json::json!([16, 41, 42, 43]));

    let spec: serde_json::Value = sqlx::query_scalar(
        "SELECT meta -> 'issues' FROM project WHERE tenant_id = $1 AND key = 'SPEC-V2'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("SPEC-V2 issues");
    assert_eq!(spec, serde_json::json!([38]));

    // Every project carries the plan and track that let the design site render the programme.
    let missing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project
          WHERE tenant_id = $1 AND meta ? 'track'
            AND (meta ->> 'plan' IS NULL OR meta ->> 'track' IS NULL)",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count incomplete projects");
    assert_eq!(missing, 0);
}

#[tokio::test]
async fn seed_programme_preserves_meta_the_seed_does_not_own() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "meta").await;

    seed_programme(&db.super_pool, Some("meta"))
        .await
        .expect("seed");
    sqlx::query(
        "UPDATE project SET meta = meta || '{\"owner_note\":\"written by a later job\"}'::jsonb
          WHERE tenant_id = $1 AND key = 'T0'",
    )
    .bind(seeded.tenant)
    .execute(&db.super_pool)
    .await
    .expect("add a later key");

    let second = seed_programme(&db.super_pool, Some("meta"))
        .await
        .expect("second seed");
    assert_eq!(
        second[0].1.updated, 0,
        "the seed does not rewrite its own rows"
    );
    assert_eq!(second[0].1.skipped, 48);

    let (plan, note): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT meta ->> 'plan', meta ->> 'owner_note'
           FROM project WHERE tenant_id = $1 AND key = 'T0'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the merged meta");
    assert_eq!(plan.as_deref(), Some("docs/SPEC_V2.md#12"));
    assert_eq!(note.as_deref(), Some("written by a later job"));
}

#[tokio::test]
async fn seed_programme_refuses_an_unknown_tenant() {
    let db = support::TestDb::fresh().await;
    let error = seed_programme(&db.super_pool, Some("no-such-tenant"))
        .await
        .expect_err("an unknown slug is a typed refusal");
    assert!(error.to_string().contains("no tenant with slug"));
}
