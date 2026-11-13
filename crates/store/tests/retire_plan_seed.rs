//! The plan-seed retirement scan.
//!
//! The scan is the only query the `admin retire-plan-seed` command trusts. It must find the
//! cards the retired seeder stamped (`meta.seed_name`), never a card without the marker, never
//! a card another tenant owns, and it must partition the seeded set into
//! candidates / already-archived / live so the command cannot archive work in flight.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::retire::scan_seeded_cards;
use serde_json::json;
use uuid::Uuid;

/// Insert one card on the fixture board. `meta` is the JSONB object the seeder would have
/// written; `lease` and `archived` drive the two skip partitions.
async fn card(
    pool: &sqlx::PgPool,
    seeded: &support::Seeded,
    name: &str,
    meta: serde_json::Value,
    lease: Option<Uuid>,
    archived: bool,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position, meta,
                           lease_token, lease_expires_at, archived_at)
         VALUES ($1, $2, $3, $4, $5, 2048, $6, $7,
                 CASE WHEN $7::uuid IS NULL THEN NULL ELSE now() + interval '1 hour' END,
                 CASE WHEN $8 THEN now() ELSE NULL END)
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.board)
    .bind(seeded.list)
    .bind(seeded.card_type)
    .bind(name)
    .bind(meta)
    .bind(lease)
    .bind(archived)
    .fetch_one(pool)
    .await
    .expect("insert the card")
}

#[tokio::test]
async fn the_scan_partitions_seeded_cards_and_ignores_other_tenants() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "retire-a").await;
    let b = support::seed_tenant(&db.super_pool, "retire-b").await;
    let tenant = TenantId::from_uuid(a.tenant).expect("tenant id");

    let active_a = card(
        &db.super_pool,
        &a,
        "Seeded active A",
        json!({"seed_name": "Seeded active A", "issue": 1}),
        None,
        false,
    )
    .await;
    let active_b = card(
        &db.super_pool,
        &a,
        "Seeded active B",
        json!({"seed_name": "Seeded active B", "issue": 2}),
        None,
        false,
    )
    .await;
    let leased = card(
        &db.super_pool,
        &a,
        "Seeded leased",
        json!({"seed_name": "Seeded leased"}),
        Some(Uuid::now_v7()),
        false,
    )
    .await;
    let live_run = card(
        &db.super_pool,
        &a,
        "Seeded live run",
        json!({"seed_name": "Seeded live run"}),
        None,
        false,
    )
    .await;
    sqlx::query("INSERT INTO run (tenant_id, card_id, project_id) VALUES ($1, $2, $3)")
        .bind(a.tenant)
        .bind(live_run)
        .bind(a.project)
        .execute(&db.super_pool)
        .await
        .expect("the live run");
    let archived = card(
        &db.super_pool,
        &a,
        "Seeded archived",
        json!({"seed_name": "Seeded archived"}),
        None,
        true,
    )
    .await;
    // A real card (no seed marker) and another tenant's seeded card are both invisible.
    let real = card(
        &db.super_pool,
        &a,
        "Real card",
        json!({"issue": 34}),
        None,
        false,
    )
    .await;
    let other = card(
        &db.super_pool,
        &b,
        "Other tenant seeded",
        json!({"seed_name": "Other tenant seeded"}),
        None,
        false,
    )
    .await;

    let scan = scan_seeded_cards(&db.super_pool, tenant)
        .await
        .expect("scan");
    let mut candidates: Vec<Uuid> = scan.candidates.iter().map(|card| card.id).collect();
    candidates.sort();
    let mut expected = vec![active_a, active_b];
    expected.sort();
    assert_eq!(candidates, expected, "only the two live seeded cards");
    assert_eq!(scan.already_archived, 1, "the archived seeded card");
    assert_eq!(scan.live, 2, "the leased card and the live run");
    for card in [leased, live_run, archived, real, other] {
        assert!(
            !candidates.contains(&card),
            "{card} must not be a candidate"
        );
    }

    // A second scan is stable: nothing changed.
    let again = scan_seeded_cards(&db.super_pool, tenant)
        .await
        .expect("second scan");
    assert_eq!(again.candidates.len(), 2);
    assert_eq!(again.already_archived, 1);
    assert_eq!(again.live, 2);
}
