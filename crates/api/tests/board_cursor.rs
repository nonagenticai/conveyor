//! One 1,000-card board: the keyset page and the partial index that serves it.
//!
//! Property (c) of S05's gate is a load claim, but the part that can be asserted without a
//! load generator is the SHAPE: `GET /api/boards/{id}/cards` must be a keyset scan on
//! `card_board_position_idx` (`WHERE archived_at IS NULL`), never a sequential scan and never
//! an `OFFSET`. The same test walks every page through the real store function, so the
//! cursor arithmetic is exercised on the data the index was built for.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

struct Seeded {
    tenant: conveyor_domain::TenantId,
    tenant_id: Uuid,
    board: Uuid,
}

async fn board_with_cards(pool: &PgPool, slug: &str, cards: i64) -> Seeded {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .expect("tenant");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("KEY-{slug}"))
    .fetch_one(pool)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(pool)
    .await
    .expect("board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .fetch_one(pool)
    .await
    .expect("list");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("card_type");
    sqlx::query(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         SELECT $1, $2, $3, $4, 'Card ' || g, 1024 * g FROM generate_series(1, $5::int) AS g",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .bind(i32::try_from(cards).expect("card count fits i32"))
    .execute(pool)
    .await
    .expect("cards");
    Seeded {
        tenant: conveyor_domain::TenantId::from_uuid(tenant_id).expect("tenant"),
        tenant_id,
        board,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_thousand_card_board_pages_by_the_partial_index() {
    let db = common::TestDb::fresh().await;
    let seeded = board_with_cards(&db.super_pool, "cursor-a", 1_000).await;

    // ── the plan: the exact keyset predicate and ORDER BY the store function uses ──────
    // Nothing is disabled here on purpose. `enable_seqscan = off` would make the assertion
    // below true by construction; the property worth proving is that at the gate's size the
    // planner PICKS the partial index on its own — no full scan, and no sort node, because
    // the index already supplies `(position, id)` order.
    let anchor = Uuid::now_v7();
    let explain = format!(
        "EXPLAIN (FORMAT JSON)
         SELECT c.id, c.position
           FROM card c
          WHERE c.tenant_id = '{}'::uuid
            AND c.board_id = '{}'::uuid
            AND c.archived_at IS NULL
            AND (c.position, c.id) > (500000::numeric, '{}'::uuid)
          ORDER BY c.position, c.id
          LIMIT 50",
        seeded.tenant_id, seeded.board, anchor
    );
    let plan: serde_json::Value = sqlx::query_scalar(AssertSqlSafe(explain))
        .fetch_one(&db.super_pool)
        .await
        .expect("EXPLAIN");
    let rendered = plan.to_string();
    assert!(
        rendered.contains("card_board_position_idx"),
        "the board card page must scan the partial index, plan was {rendered}"
    );
    assert!(
        !rendered.contains("\"Seq Scan\""),
        "a 1,000-card keyset page must not scan the whole card table: {rendered}"
    );
    assert!(
        !rendered.contains("\"Sort\""),
        "ORDER BY (position, id) must come from the index, not a sort: {rendered}"
    );
    let access = &plan[0]["Plan"]["Plans"][0];
    assert_eq!(
        access["Index Name"], "card_board_position_idx",
        "the access path must be the partial index, not {access}"
    );
    assert!(
        access["Index Cond"]
            .as_str()
            .is_some_and(|cond| cond.contains("ROW(\"position\", id)")),
        "the keyset comparison must be an index condition, not a post-filter: {access}"
    );

    // ── the pages: 1,000 cards in 20 keyset pages, no row skipped or repeated ──────────
    let mut tx = conveyor_store::begin_scoped(&db.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    let mut cursor = None;
    let mut seen = Vec::new();
    loop {
        let page =
            conveyor_store::card::card_page_tx(&mut tx, seeded.tenant, seeded.board, cursor, 50)
                .await
                .expect("page");
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 50, "a page never exceeds its limit");
        for record in &page {
            seen.push(record.card.id);
        }
        let last = page.last().expect("a non-empty page");
        // The cursor is opaque on the wire; this is the round-trip the API performs.
        let encoded = conveyor_store::position::encode_cursor(last.position, last.card.id);
        cursor = conveyor_store::position::decode_cursor(&encoded);
        assert_eq!(cursor, Some((last.position, last.card.id)));
    }
    tx.rollback().await.expect("rollback");

    assert_eq!(seen.len(), 1_000, "every card is on exactly one page");
    let unique: std::collections::BTreeSet<Uuid> = seen.iter().copied().collect();
    assert_eq!(unique.len(), 1_000, "no card repeats across pages");
}
