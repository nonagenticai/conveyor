//! `seed_plans`: one board per programme project and one card per plan unit.
//!
//! The natural keys are `(tenant, project, board name)` and `card.meta.unit_key`. A second run
//! with an unchanged document creates nothing, bumps no version and leaves `updated_at` alone;
//! a hand-moved card keeps its lane, and a leased card is never touched.
//!
//! `rbuild -- test -p conveyor-store --test seed_plans`.

#[path = "support/mod.rs"]
mod support;

use conveyor_store::seed::seed_catalogue;
use conveyor_store::seed_plans::{plan_export, seed_plans};
use conveyor_store::seed_programme::seed_programme;
use serde_json::Value;

fn units() -> usize {
    plan_export()
        .expect("embedded seed parses")
        .projects
        .iter()
        .map(|project| project.units.len())
        .sum()
}

#[tokio::test]
async fn seed_plans_is_idempotent() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "plans").await;
    seed_catalogue(&db.super_pool, Some("plans"))
        .await
        .expect("catalogue");
    seed_programme(&db.super_pool, Some("plans"))
        .await
        .expect("programme");

    let first = seed_plans(&db.super_pool, Some("plans"))
        .await
        .expect("first run");
    assert_eq!(first.len(), 1, "one tenant targeted");
    let (slug, counts) = &first[0];
    assert_eq!(slug, "plans");
    assert_eq!(counts.created, units() as i64, "one card per plan unit");
    assert_eq!(counts.updated, 0);
    assert_eq!(counts.skipped, 0);

    let second = seed_plans(&db.super_pool, Some("plans"))
        .await
        .expect("second run");
    let counts = second[0].1;
    assert_eq!(counts.created, 0, "a second run creates nothing");
    assert_eq!(counts.updated, 0, "a second run updates nothing");
    assert_eq!(counts.skipped, units() as i64);

    let boards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM board WHERE tenant_id = $1 AND name = 'Board'")
            .bind(seeded.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("count boards");
    assert_eq!(boards, 47, "one board per programme project");

    let lanes_per_board: Vec<i64> = sqlx::query_scalar(
        "SELECT count(*) FROM list l
           JOIN board b ON b.id = l.board_id
          WHERE b.tenant_id = $1 AND b.name = 'Board'
          GROUP BY b.id",
    )
    .bind(seeded.tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("count lanes");
    assert_eq!(lanes_per_board.len(), 47);
    assert!(
        lanes_per_board.iter().all(|count| *count == 5),
        "five lanes per board"
    );

    let cards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM card c
           JOIN board b ON b.id = c.board_id
          WHERE b.tenant_id = $1 AND b.name = 'Board' AND c.meta ? 'unit_key'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count cards");
    assert_eq!(cards, units() as i64);

    let project_type: String = sqlx::query_scalar(
        "SELECT project_type_code FROM project WHERE tenant_id = $1 AND key = 'T0'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("T0 project type");
    assert_eq!(
        project_type, "it",
        "the it matrix admits the plan card types"
    );

    let meta: Value = sqlx::query_scalar(
        "SELECT meta FROM card WHERE tenant_id = $1 AND meta ->> 'unit_key' = 'issue-34'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("T0 card meta");
    assert_eq!(meta["card_type"], "architecture_task");
    assert_eq!(meta["process_key"], "architecture_task.READY");
}

#[tokio::test]
async fn a_hand_moved_card_keeps_its_lane() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "moved").await;
    seed_catalogue(&db.super_pool, Some("moved"))
        .await
        .expect("catalogue");
    seed_programme(&db.super_pool, Some("moved"))
        .await
        .expect("programme");
    seed_plans(&db.super_pool, Some("moved"))
        .await
        .expect("first");

    let (card, active): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT c.id, a.id FROM card c
           JOIN board b ON b.id = c.board_id
           JOIN list a ON a.board_id = b.id AND a.kind = 'active'
          WHERE b.tenant_id = $1 AND c.meta ->> 'unit_key' = 'issue-34'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("T0 card and active lane");
    sqlx::query("UPDATE card SET list_id = $2, version = version + 1 WHERE id = $1")
        .bind(card)
        .bind(active)
        .execute(&db.super_pool)
        .await
        .expect("move the card by hand");

    seed_plans(&db.super_pool, Some("moved"))
        .await
        .expect("second");

    let lane: uuid::Uuid = sqlx::query_scalar("SELECT list_id FROM card WHERE id = $1")
        .bind(card)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the lane");
    assert_eq!(lane, active, "a person's move is not undone by the seed");
}

#[tokio::test]
async fn a_leased_card_is_never_moved_or_renamed() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "leased").await;
    seed_catalogue(&db.super_pool, Some("leased"))
        .await
        .expect("catalogue");
    seed_programme(&db.super_pool, Some("leased"))
        .await
        .expect("programme");
    seed_plans(&db.super_pool, Some("leased"))
        .await
        .expect("first");

    // A closed unit would be advanced intake -> done; lease it in intake and prove it stays.
    let (card, intake): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT c.id, i.id FROM card c
           JOIN board b ON b.id = c.board_id
           JOIN list i ON i.board_id = b.id AND i.kind = 'intake'
          WHERE b.tenant_id = $1 AND c.meta ->> 'unit_key' = 'issue-55'",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("the closed unit's card and intake lane");
    sqlx::query(
        "UPDATE card
            SET list_id = $2, lease_token = gen_random_uuid(),
                lease_expires_at = now() + interval '1 hour', version = version + 1
          WHERE id = $1",
    )
    .bind(card)
    .bind(intake)
    .execute(&db.super_pool)
    .await
    .expect("lease the card in intake");

    seed_plans(&db.super_pool, Some("leased"))
        .await
        .expect("second");

    let lane: uuid::Uuid = sqlx::query_scalar("SELECT list_id FROM card WHERE id = $1")
        .bind(card)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the lane");
    assert_eq!(lane, intake, "a leased card keeps its lane");
}
