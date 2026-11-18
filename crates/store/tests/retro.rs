//! T10 (#82) P3 — the retro opens one coach card and closes only on its lessons.
//!
//! Inspect & adapt is a cadence event, not a meeting note: one retro card owned by the coach,
//! and a close is refused while any finding it recorded has no `lesson` proposal carrying the
//! evidence refs and the cadence.
//!
//! `cargo test -p conveyor-store --test retro`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    programme: Uuid,
    cadence: Uuid,
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Retro') RETURNING id")
            .bind(format!("retro-{}", Uuid::now_v7().simple()))
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
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Delivery') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
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
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Intake', 0, 'intake')",
    )
    .bind(tenant_id)
    .bind(board)
    .execute(&db.super_pool)
    .await
    .expect("insert the intake lane");
    sqlx::query("INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'retro', 'Retro')")
        .bind(tenant_id)
        .execute(&db.super_pool)
        .await
        .expect("insert the card type");
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'retro', 'PI 1 retro', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(json!({ "board_id": board, "card_type_code": "retro" }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the retro cadence");
    Fixture {
        tenant,
        programme,
        cadence,
    }
}

async fn open(db: &support::TestDb, f: &Fixture) -> Uuid {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let card = spine::open_retro_tx(&mut tx, f.tenant, f.cadence)
        .await
        .expect("open the retro");
    tx.commit().await.expect("commit");
    card
}

async fn findings(db: &support::TestDb, f: &Fixture, rows: serde_json::Value) {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    spine::record_retro_findings_tx(&mut tx, f.tenant, f.cadence, &rows)
        .await
        .expect("record findings");
    tx.commit().await.expect("commit");
}

async fn lesson(db: &support::TestDb, f: &Fixture, key: &str) -> Uuid {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let id = spine::propose_lesson_tx(
        &mut tx,
        f.tenant,
        f.programme,
        Some(f.cadence),
        key,
        &format!("lesson for {key}"),
        &json!([{ "run_id": "r-1" }]),
    )
    .await
    .expect("propose a lesson");
    tx.commit().await.expect("commit");
    id
}

async fn close(db: &support::TestDb, f: &Fixture) -> Result<(), PlanStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::close_retro_tx(&mut tx, f.tenant, f.cadence).await;
    match &outcome {
        Ok(()) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

#[tokio::test]
async fn retro_card_is_the_coachs() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let card = open(&db, &f).await;
    let again = open(&db, &f).await;
    assert_eq!(card, again, "a second open returns the same card");
    let meta: serde_json::Value = sqlx::query_scalar("SELECT meta FROM card WHERE id = $1")
        .bind(card)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the card meta");
    assert_eq!(meta["owner_kind"], "coach", "the card is the coach's");
    let cards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM card WHERE tenant_id = $1 AND meta->>'cadence_id' = $2",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.cadence.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("count retro cards");
    assert_eq!(cards, 1, "exactly one retro card per cadence");
}

#[tokio::test]
async fn every_finding_becomes_a_lesson() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    open(&db, &f).await;
    findings(
        &db,
        &f,
        json!([{ "key": "F1", "text": "the demo slipped" }, { "key": "F2", "text": "review was late" }]),
    )
    .await;
    lesson(&db, &f, "F1").await;
    lesson(&db, &f, "F2").await;

    close(&db, &f).await.expect("every finding has its lesson");
    let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT state, finding_key, evidence FROM lesson WHERE tenant_id = $1 AND cadence_id = $2
          ORDER BY finding_key",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.cadence)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the lessons");
    assert_eq!(rows.len(), 2);
    for (state, _key, evidence) in &rows {
        assert_eq!(state, "proposed", "a lesson starts proposed");
        assert!(
            !evidence.as_array().unwrap_or(&vec![]).is_empty(),
            "the lesson carries its evidence refs"
        );
    }
}

#[tokio::test]
async fn retro_cannot_close_without_lessons() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    open(&db, &f).await;
    findings(
        &db,
        &f,
        json!([{ "key": "F1", "text": "one" }, { "key": "F2", "text": "two" }]),
    )
    .await;
    lesson(&db, &f, "F1").await;

    match close(&db, &f).await {
        Err(PlanStoreError::Illegal(PlanError::RetroFindingWithoutLesson(key))) => {
            assert_eq!(key, "F2", "the refusal names the finding without a lesson")
        }
        other => panic!("expected the F2 refusal, got {other:?}"),
    }
    let closed: bool = sqlx::query_scalar(
        "SELECT coalesce((meta->>'retro_closed')::boolean, false) FROM card
          WHERE tenant_id = $1 AND meta->>'cadence_id' = $2 LIMIT 1",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.cadence.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("read the card");
    assert!(!closed, "the refused close wrote nothing");
}
