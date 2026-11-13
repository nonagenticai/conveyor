//! `conveyor admin intake` — a GitHub issue becomes a real card on a project's board.
//!
//! The command resolves the project, its board and the intake lane, then calls `cards.create`
//! through the same engine as HTTP. The card carries the issue link in `meta`, writes the
//! normal `card.created` event, and a second intake of the same issue is refused.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::admin::intake::{IntakeError, run};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fixture {
    tenant: Uuid,
    board: Uuid,
    intake: Uuid,
}

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
    sqlx::query(
        "INSERT INTO project (tenant_id, key, name, programme_id, project_type_code)
         VALUES ($1, 'T0', 'Model and contracts', $2, 'delivery')",
    )
    .bind(tenant)
    .bind(programme)
    .execute(&db.super_pool)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         SELECT $1, id, 'Board', 0 FROM project WHERE tenant_id = $1 AND key = 'T0'
         RETURNING id",
    )
    .bind(tenant)
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
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'architecture_task', 'Architecture task')",
    )
    .bind(tenant)
    .execute(&db.super_pool)
    .await
    .expect("card type");
    Fixture {
        tenant,
        board,
        intake,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_issue_becomes_one_card_with_its_link_and_a_second_run_refuses() {
    let db = common::TestDb::fresh().await;
    let f = fixture(&db, "intake").await;
    let tenant = TenantId::from_uuid(f.tenant).expect("tenant id");

    let report = run(
        &db.super_pool,
        tenant,
        "T0",
        34,
        "T0 — the organisation spine",
        "architecture_task",
        Some("architecture_task.READY"),
        Some("docs/plans/slices/T0.md"),
        "conveyor",
    )
    .await
    .expect("intake");
    assert!(!report.card_id.is_empty(), "{:?}", report.output);
    assert_eq!(report.board_id, f.board);
    assert_eq!(report.list_id, f.intake);

    let card_id: Uuid = report.card_id.parse().expect("card id");
    let (list_id, name, meta): (Uuid, String, Value) =
        sqlx::query_as("SELECT list_id, name, meta FROM card WHERE id = $1")
            .bind(card_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("the card");
    assert_eq!(list_id, f.intake, "born in the intake lane");
    assert_eq!(name, "T0 — the organisation spine");
    assert_eq!(meta["issue"], json!(34));
    assert_eq!(meta["unit_key"], json!("T0"));
    assert_eq!(meta["process_key"], json!("architecture_task.READY"));
    assert_eq!(meta["plan"], json!("docs/plans/slices/T0.md"));

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'card.created'",
    )
    .bind(f.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("events");
    assert_eq!(events, 1, "the real action wrote its event");

    // A second intake of the same issue is refused, not duplicated.
    let again = run(
        &db.super_pool,
        tenant,
        "T0",
        34,
        "T0 — the organisation spine",
        "architecture_task",
        None,
        None,
        "conveyor",
    )
    .await
    .expect_err("a second intake must refuse");
    assert!(
        matches!(again, IntakeError::AlreadyIntaken { issue: 34, .. }),
        "{again}"
    );
    let cards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM card WHERE tenant_id = $1 AND board_id = $2")
            .bind(f.tenant)
            .bind(f.board)
            .fetch_one(&db.super_pool)
            .await
            .expect("cards");
    assert_eq!(cards, 1);

    // An unknown project fails typed, before any write.
    let unknown = run(
        &db.super_pool,
        tenant,
        "NOPE",
        35,
        "Nope",
        "architecture_task",
        None,
        None,
        "conveyor",
    )
    .await
    .expect_err("unknown project");
    assert!(
        matches!(unknown, IntakeError::UnknownProject(_)),
        "{unknown}"
    );
}
