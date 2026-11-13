//! `conveyor admin retire-plan-seed` — the seeded cards are archived through `cards.archive`.
//!
//! The command must move each seeded card into the board's archive lane, write the normal
//! `card.archived` event, skip a leased/live-run card, leave a non-seeded card alone, stay
//! idempotent, and never cross a tenant boundary.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::admin::retire_plan_seed::run;
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: Uuid,
    project: Uuid,
    board: Uuid,
    intake: Uuid,
    archive: Uuid,
    card_type: Uuid,
}

/// A tenant with a delivery project, a board, an intake and an archive lane, and a feature
/// card type. The lanes are inserted directly: the board came from the template in production,
/// and only their kind matters to `cards.archive`.
async fn fixture(db: &common::TestDb, slug: &str) -> Fixture {
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("user-{slug}"))
            .fetch_one(&db.super_pool)
            .await
            .expect("user");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant)
        .bind(user)
        .execute(&db.super_pool)
        .await
        .expect("membership");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, $2) RETURNING id")
            .bind(tenant)
            .bind(format!("Programme {slug}"))
            .fetch_one(&db.super_pool)
            .await
            .expect("programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id, project_type_code)
         VALUES ($1, 'P1', 'Project', $2, 'delivery') RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("board");
    let intake: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Intake', 1024, 'intake') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("intake lane");
    let archive: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Archive', 5120, 'archive') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("archive lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("card type");
    Fixture {
        tenant,
        project,
        board,
        intake,
        archive,
        card_type,
    }
}

async fn card(
    db: &common::TestDb,
    fixture: &Fixture,
    name: &str,
    meta: serde_json::Value,
    lease: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position, meta,
                           lease_token, lease_expires_at)
         VALUES ($1, $2, $3, $4, $5, 1024, $6, $7,
                 CASE WHEN $7::uuid IS NULL THEN NULL ELSE now() + interval '1 hour' END)
         RETURNING id",
    )
    .bind(fixture.tenant)
    .bind(fixture.board)
    .bind(fixture.intake)
    .bind(fixture.card_type)
    .bind(name)
    .bind(meta)
    .bind(lease)
    .fetch_one(&db.super_pool)
    .await
    .expect("card")
}

async fn archived(db: &common::TestDb, id: Uuid) -> (Option<Uuid>, bool) {
    sqlx::query_as("SELECT list_id, archived_at IS NOT NULL FROM card WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("the card")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seeded_cards_are_archived_through_the_action_and_the_rest_are_left_alone() {
    let db = common::TestDb::fresh().await;
    let f = fixture(&db, "retire").await;
    let other = fixture(&db, "retire-other").await;
    let tenant = TenantId::from_uuid(f.tenant).expect("tenant id");

    let first = card(
        &db,
        &f,
        "Seeded one",
        json!({"seed_name": "Seeded one", "issue": 1}),
        None,
    )
    .await;
    let second = card(
        &db,
        &f,
        "Seeded two",
        json!({"seed_name": "Seeded two", "issue": 2}),
        None,
    )
    .await;
    let leased = card(
        &db,
        &f,
        "Seeded leased",
        json!({"seed_name": "Seeded leased"}),
        Some(Uuid::now_v7()),
    )
    .await;
    let live_run = card(
        &db,
        &f,
        "Seeded live run",
        json!({"seed_name": "Seeded live run"}),
        None,
    )
    .await;
    sqlx::query("INSERT INTO run (tenant_id, card_id, project_id) VALUES ($1, $2, $3)")
        .bind(f.tenant)
        .bind(live_run)
        .bind(f.project)
        .execute(&db.super_pool)
        .await
        .expect("live run");
    let real = card(&db, &f, "Real card", json!({"issue": 34}), None).await;
    let foreign = card(
        &db,
        &other,
        "Other tenant seeded",
        json!({"seed_name": "Other tenant seeded"}),
        None,
    )
    .await;

    let report = run(&db.super_pool, tenant, "conveyor")
        .await
        .expect("retire");
    assert_eq!(report.archived, 2, "the two live seeded cards");
    assert_eq!(report.already_archived, 0);
    assert_eq!(report.skipped_live, 2, "the leased card and the live run");
    assert!(report.ok(), "{:?}", report.failed);

    for id in [first, second] {
        let (list_id, stamped) = archived(&db, id).await;
        assert_eq!(list_id, Some(f.archive), "the archive lane");
        assert!(stamped, "the card is archived");
    }
    for id in [leased, live_run, real] {
        let (list_id, stamped) = archived(&db, id).await;
        assert_eq!(list_id, Some(f.intake), "{id} stays in intake");
        assert!(!stamped, "{id} is not archived");
    }
    let (foreign_list, foreign_archived) = archived(&db, foreign).await;
    assert_eq!(
        foreign_list,
        Some(other.intake),
        "the other tenant's card keeps its own lane"
    );
    assert!(!foreign_archived, "the other tenant is untouched");

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'card.archived'",
    )
    .bind(f.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("events");
    assert_eq!(events, 2, "one card.archived event per archived card");

    // Idempotent: the second run archives nothing and does not touch the live cards.
    let again = run(&db.super_pool, tenant, "conveyor")
        .await
        .expect("second retire");
    assert_eq!(again.archived, 0);
    assert_eq!(again.already_archived, 2);
    assert_eq!(again.skipped_live, 2);
    assert!(again.ok());
    let events_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'card.archived'",
    )
    .bind(f.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("events");
    assert_eq!(events_after, 2, "a no-op run writes no event");
}
