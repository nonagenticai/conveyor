//! T10 (#82) P1.3 — the PI tick is idempotent.
//!
//! The first pass materialises the standing backlog into draft cards; the second sees no
//! planned cadence and creates none. The guard is the cadence's state, moved in the same
//! transaction as the drafts — not a timer and not a marker on the card.
//!
//! `cargo test -p conveyor-server --test cadence_jobs`.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::cadence;
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    cadence: Uuid,
    project: Uuid,
}

async fn fixture(db: &common::TestDb) -> Fixture {
    let slug = format!("cadence-{}", Uuid::now_v7().simple());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Cadence') RETURNING id")
            .bind(&slug)
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
    sqlx::query(
        "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
         VALUES ($1, 'conveyor', 'active', now())
         ON CONFLICT (tenant_id, app_key) DO UPDATE SET status = 'active'",
    )
    .bind(tenant_id)
    .execute(&db.super_pool)
    .await
    .expect("insert the licence");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Delivery', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the board");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Intake', 0, 'intake', NULL)",
    )
    .bind(tenant_id)
    .bind(board)
    .execute(&db.super_pool)
    .await
    .expect("insert the intake lane");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Active', 1, 'active', 4)",
    )
    .bind(tenant_id)
    .bind(board)
    .execute(&db.super_pool)
    .await
    .expect("insert the active lane");
    sqlx::query("INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')")
        .bind(tenant_id)
        .execute(&db.super_pool)
        .await
        .expect("insert the card type");
    for index in 0..2 {
        sqlx::query(
            "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status,
                                   business_value, time_criticality, risk_reduction, job_size)
             VALUES ($1, $2, $3, 'deliverable', $4, 'open', 3, 1, 1, 1)",
        )
        .bind(tenant_id)
        .bind(project)
        .bind(index)
        .bind(format!("backlog {index}"))
        .execute(&db.super_pool)
        .await
        .expect("insert a backlog row");
    }
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'pi', 'PI 1', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(json!({ "board_id": board, "card_type_code": "feature" }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the cadence");
    Fixture {
        tenant,
        cadence,
        project,
    }
}

#[tokio::test]
async fn pi_tick_is_idempotent() {
    let db = common::TestDb::fresh().await;
    let f = fixture(&db).await;

    let first = cadence::tick(&db.runtime, f.tenant)
        .await
        .expect("the first tick");
    assert_eq!(first.len(), 2, "one draft per backlog row");
    let drafted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wbs_node WHERE tenant_id = $1 AND project_id = $2 AND card_id IS NOT NULL",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("count drafted rows");
    assert_eq!(drafted, 2, "each draft names its backlog row");

    let second = cadence::tick(&db.runtime, f.tenant)
        .await
        .expect("the second tick");
    assert!(second.is_empty(), "a second tick creates none");

    let state: String = sqlx::query_scalar("SELECT state FROM programme_cadence WHERE id = $1")
        .bind(f.cadence)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the cadence");
    assert_eq!(state, "materialised", "the cadence records its pass");
}
