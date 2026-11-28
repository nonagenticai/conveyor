//! The coach's `move` remedy and the lane kinds it may enter (SPEC_V2 §6.2; conveyor#935).
//!
//! The remedy applies its effect with its own UPDATE, not through `cards.move`, so it owes the
//! `done`-lane rule itself: `done` and `archive` are gate outcomes — a card reaches `done` only
//! through an approved `card_done` gate — and a raw placement there would bypass the gate the
//! coach exists to protect. A working lane stays a legal destination.
//!
//! `cargo test -p conveyor-store --test coach_remedy`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::coach::{AppliedRemedy, CoachStoreError, RemedyEffect, apply_remedy_tx};
use serde_json::json;
use uuid::Uuid;

/// One lane on the seeded card's board.
async fn lane(
    pool: &sqlx::PgPool,
    seeded: &support::Seeded,
    name: &str,
    kind: &str,
    position: i32,
) -> Uuid {
    // `list.position` is `numeric`; `$5::numeric` binds the test's own constant, so the query
    // stays a literal (the workspace's dynamic-SQL lint refuses a formatted statement).
    sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, $3, $5::numeric, $4) RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.board)
    .bind(name)
    .bind(kind)
    .bind(position.to_string())
    .fetch_one(pool)
    .await
    .expect("the lane")
}

/// An open interruption on the seeded card and one `move` remedy pointing at `target`.
async fn record_move(
    pool: &sqlx::PgPool,
    seeded: &support::Seeded,
    target: Uuid,
    signature: &str,
) -> Uuid {
    let interruption: Uuid = sqlx::query_scalar(
        "INSERT INTO coach_interruption
             (tenant_id, coach_card_id, interrupted_card_id, failed_run_id, signature,
              window_ends_at)
         VALUES ($1, $2, $3, $4, $5, now() + interval '1 hour')
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.card)
    .bind(seeded.card)
    .bind(seeded.run)
    .bind(signature)
    .fetch_one(pool)
    .await
    .expect("the interruption");
    sqlx::query_scalar(
        "INSERT INTO coach_remedy (tenant_id, interruption_id, remedy_class, detail)
         VALUES ($1, $2, 'move', $3) RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(interruption)
    .bind(json!({ "list_id": target.to_string() }))
    .fetch_one(pool)
    .await
    .expect("the move remedy")
}

/// A `move` remedy may return a card to a working lane; it may not place it in `done` or
/// `archive`, which are the `card_done` gate's outcomes.
#[tokio::test]
async fn a_move_remedy_cannot_target_a_done_or_archive_lane() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "coach-move").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil");

    let waiting = lane(&db.super_pool, &seeded, "Waiting", "waiting", 2048).await;
    let done = lane(&db.super_pool, &seeded, "Done", "done", 3072).await;
    let archive = lane(&db.super_pool, &seeded, "Archive", "archive", 4096).await;
    let to_waiting = record_move(&db.super_pool, &seeded, waiting, "move|waiting").await;
    let to_done = record_move(&db.super_pool, &seeded, done, "move|done").await;
    let to_archive = record_move(&db.super_pool, &seeded, archive, "move|archive").await;

    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("tenant scope");

    let applied = apply_remedy_tx(&mut tx, tenant, to_waiting)
        .await
        .expect("a working lane is a legal destination");
    assert!(
        matches!(
            &applied,
            Some(AppliedRemedy {
                effect: RemedyEffect::Moved { list_id },
                ..
            }) if *list_id == waiting
        ),
        "the control move must apply: {applied:?}"
    );

    for (remedy, lane) in [(to_done, "done"), (to_archive, "archive")] {
        let refused = apply_remedy_tx(&mut tx, tenant, remedy)
            .await
            .expect_err("a gate outcome is not a move destination");
        assert!(
            matches!(
                &refused,
                CoachStoreError::RemedyInvalid(message) if message.contains("done or archive lane")
            ),
            "the {lane} target must be refused as a gate outcome: {refused}"
        );
    }

    tx.commit().await.expect("commit the control move");
    let placed: Uuid =
        sqlx::query_scalar("SELECT list_id FROM card WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant)
            .bind(seeded.card)
            .fetch_one(&db.super_pool)
            .await
            .expect("the card's lane");
    assert_eq!(
        placed, waiting,
        "the refused moves left the card where the control move put it"
    );
}
