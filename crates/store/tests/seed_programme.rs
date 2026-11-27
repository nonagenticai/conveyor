//! `seed_programme`: the buildout programme and its projects are idempotent rows.
//!
//! The natural keys are `(tenant, programme.name)` and `(tenant, project.key)`. A second run
//! with an unchanged document creates nothing, bumps no version and leaves `updated_at` alone;
//! the programme owns its named meta keys and preserves every other key a later job wrote.
//!
//! `cargo test -p conveyor-store --test seed_programme`.

#[path = "support/mod.rs"]
mod support;

use conveyor_store::seed_programme::{seed_programme, seed_programme_tx};
use uuid::Uuid;

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
    assert_eq!(counts.created, 53, "the programme and its 52 projects");
    assert_eq!(counts.updated, 0);
    assert_eq!(counts.skipped, 0);

    let second = seed_programme(&db.super_pool, Some("prog"))
        .await
        .expect("second run");
    let counts = second[0].1;
    assert_eq!(counts.created, 0, "a second run creates nothing");
    assert_eq!(counts.updated, 0, "a second run updates nothing");
    assert_eq!(counts.skipped, 53);

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
    assert_eq!(projects, 52);

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
    // The seed is FROZEN: `seed/programme.json` is the bootstrap of record, so the live row must
    // carry exactly the seed's S12 list. The registry export (`docs/design/data/issues.json`) is
    // still refreshed, but it no longer regenerates the seed; a link below only has to name an
    // issue that still exists — it is not a set that must track every new S12-labelled unit.
    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("../../../seed/programme.json"))
            .expect("the frozen seed parses");
    let mut expected: Vec<i64> = frozen["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .find(|project| project["key"].as_str() == Some("S12"))
        .expect("S12 in the frozen seed")["meta"]["issues"]
        .as_array()
        .expect("S12 issues")
        .iter()
        .filter_map(|number| number.as_i64())
        .collect();
    expected.sort_unstable();
    assert!(!expected.is_empty(), "S12 links its units");
    let mut live: Vec<i64> = issues
        .as_array()
        .expect("the live S12 issues array")
        .iter()
        .filter_map(|number| number.as_i64())
        .collect();
    live.sort_unstable();
    assert_eq!(
        live, expected,
        "the live row carries the frozen seed's S12 list"
    );

    // A link is only real if the issue exists: the registry still carries every linked number.
    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/design/data/issues.json"
        ))
        .expect("the exported issue registry"),
    )
    .expect("the registry is JSON");
    let known: std::collections::HashSet<i64> = registry["issues"]
        .as_array()
        .expect("issues")
        .iter()
        .filter_map(|issue| issue["number"].as_i64())
        .collect();
    let missing: Vec<i64> = expected
        .iter()
        .copied()
        .filter(|number| !known.contains(number))
        .collect();
    assert!(
        missing.is_empty(),
        "S12 links issues the registry does not carry: {missing:?}"
    );

    let spec: serde_json::Value = sqlx::query_scalar(
        "SELECT meta -> 'issues' FROM project WHERE tenant_id = $1 AND key = 'SPEC-V2'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("SPEC-V2 issues");
    assert_eq!(spec, serde_json::json!([24]));

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
    assert_eq!(second[0].1.skipped, 53);

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

#[tokio::test]
async fn seed_programme_tx_is_idempotent_and_rolls_back_whole() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "progtx").await;
    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");

    let mut tx = conveyor_store::begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let first = seed_programme_tx(&mut tx, tenant).await.expect("first run");
    assert_eq!(first.created, 53, "the programme and its 52 projects");
    assert_eq!(first.skipped, 0);
    tx.commit().await.expect("commit");

    let mut tx = conveyor_store::begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let second = seed_programme_tx(&mut tx, tenant)
        .await
        .expect("second run");
    assert_eq!(second.created, 0, "a second run creates nothing");
    assert_eq!(second.updated, 0, "a second run updates nothing");
    assert_eq!(second.skipped, 53);
    tx.commit().await.expect("commit");

    // A rollback publishes nothing: the caller owns the transaction.
    let other = support::seed_tenant(&db.super_pool, "progtx-rollback").await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(other.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the tenant's fixture rows");
    let other_tenant = conveyor_domain::TenantId::from_uuid(other.tenant).expect("non-nil tenant");
    let mut tx = conveyor_store::begin_scoped(&db.super_pool, other_tenant)
        .await
        .expect("scoped tx");
    let pending = seed_programme_tx(&mut tx, other_tenant)
        .await
        .expect("pending run");
    assert_eq!(pending.created, 53);
    tx.rollback().await.expect("rollback");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(other.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count projects after the rollback");
    assert_eq!(
        after, before,
        "a rolled-back seed adds no project row to the tenant"
    );
}

/// #782 P5: the bootstrap seed and the live rows agree, a live unit row never rewrites the
/// seed, and a re-seed leaves the unit alone. The snapshot's `delivery` section is the row
/// path; `seed/programme.json` stays the frozen bootstrap.
#[tokio::test]
async fn the_frozen_seed_matches_the_live_rows_and_a_unit_never_rewrites_it() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "parity").await;
    let first = seed_programme(&db.super_pool, Some("parity"))
        .await
        .expect("seed the programme");
    assert_eq!(first[0].1.created, 53);

    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("../../../seed/programme.json"))
            .expect("the frozen seed parses");
    let mut seed_keys: Vec<String> = frozen["projects"]
        .as_array()
        .expect("the seed's projects")
        .iter()
        .filter_map(|row| {
            row.get("key")
                .and_then(|key| key.as_str())
                .map(str::to_owned)
        })
        .collect();
    seed_keys.sort();
    let live_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM project
          WHERE tenant_id = $1 AND programme_id IS NOT NULL AND meta ? 'track'
          ORDER BY key",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("the live project keys");
    assert_eq!(
        live_keys, seed_keys,
        "the frozen seed and the live rows agree"
    );

    // A unit is a live row on its own table; chartering one must not edit the seed's project.
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(seeded.tenant)
        .bind(seeded.user)
        .execute(&db.super_pool)
        .await
        .expect("membership");
    let project: Uuid =
        sqlx::query_scalar("SELECT id FROM project WHERE tenant_id = $1 AND key = 'S00'")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("the S00 project");
    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");
    let mut conn = db.super_pool.acquire().await.expect("connection");
    let unit = conveyor_store::development::insert_tx(
        &mut conn,
        tenant,
        &conveyor_store::development::NewDevelopmentProject {
            project_id: project,
            repo: "nonagenticai/conveyor".to_owned(),
            lane: "issue/782-unit-record-site".to_owned(),
            issue_ref: Some("nonagenticai/conveyor#782".to_owned()),
            implementer_id: seeded.user,
            gate_set: serde_json::json!(["review", "merge", "deploy"]),
            instrument_ref: None,
            acceptance_command: "bash scripts/ci/gate.sh".to_owned(),
            created_by: Some(seeded.user),
        },
    )
    .await
    .expect("charter the unit");
    drop(conn);

    // The re-seed is a no-op and the live unit is untouched: the seed never rewrites the row.
    let second = seed_programme(&db.super_pool, Some("parity"))
        .await
        .expect("second seed");
    assert_eq!(second[0].1.created, 0, "a re-seed creates nothing");
    assert_eq!(second[0].1.updated, 0, "a re-seed updates nothing");
    let still: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM development_project WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(seeded.tenant)
    .bind(project)
    .fetch_optional(&db.super_pool)
    .await
    .expect("read the unit back");
    assert_eq!(still, Some(unit.id), "the live unit row is not the seed's");

    let after: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM project
          WHERE tenant_id = $1 AND programme_id IS NOT NULL AND meta ? 'track'
          ORDER BY key",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("the live project keys after the unit");
    assert_eq!(
        after, seed_keys,
        "the unit did not rewrite the seed's projects"
    );
}
