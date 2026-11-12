//! S05 §7 property (c)'s deterministic half, and step 4's position arithmetic.
//!
//! Two assertions live here because both are about the ORDER the board is read and written
//! in, and neither needs a browser or a load generator:
//!
//! 1. **exact midpoints under the board's advisory lock** — two concurrent appends must see
//!    each other, so their positions differ by exactly one `APPEND_STEP` (1024). A float or
//!    an unlocked read gives the same position twice, which is the drag that silently loses
//!    a card.
//! 2. **1,000 cards in five 200-card pages in under 500 ms** — the gate's property (c). The
//!    query is the store's keyset page, so the assertion also proves nothing sorts in Rust:
//!    the rows arrive already ordered by `(position, id)`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::time::{Duration, Instant};

use sqlx::PgPool;
use uuid::Uuid;

use conveyor_domain::TenantId;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_appends_are_exactly_one_step_apart() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");

    let board_id = make_board(&pg, seeded.tenant).await;

    let (a, b) = tokio::join!(
        append_lane(&pg, seeded.tenant, board_id, "left"),
        append_lane(&pg, seeded.tenant, board_id, "right"),
    );
    let (a, b) = (a.expect("left"), b.expect("right"));
    assert_ne!(a, b, "concurrent appends must not share a position");
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    assert_eq!(
        high - low,
        rust_decimal::Decimal::from(1024),
        "{low} {high}"
    );
}

async fn make_board(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'POS', 'Positions') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project)
    .fetch_one(&mut *tx)
    .await
    .expect("board");
    tx.commit().await.expect("commit");
    board
}

/// Append one lane through the store's exact path, under the board lock, in its own
/// transaction — the same three statements `lists.create` runs.
async fn append_lane(
    pg: &support::Pg,
    tenant: TenantId,
    board_id: Uuid,
    name: &str,
) -> Result<rust_decimal::Decimal, String> {
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .map_err(|e| e.to_string())?;
    conveyor_store::position::lock_board_tx(&mut tx, tenant, board_id)
        .await
        .map_err(|e| e.to_string())?;
    let position = conveyor_store::position::next_list_position_tx(&mut tx, tenant, board_id)
        .await
        .map_err(|e| e.to_string())?;
    conveyor_store::list::insert_list_tx(
        &mut tx,
        tenant,
        &conveyor_store::list::NewList {
            board_id,
            name: name.to_owned(),
            position,
            kind: conveyor_domain::ListKind::Active,
            color: None,
            wip_limit: None,
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(position)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_thousand_cards_fit_in_five_pages_under_500ms() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let board = seed_thousand_cards(&pg.super_pool, seeded.tenant.as_uuid()).await;

    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    let started = Instant::now();
    let mut cursor = None;
    let mut seen = 0usize;
    for page in 0..5 {
        let rows = conveyor_store::card::card_page_tx(&mut tx, seeded.tenant, board, cursor, 200)
            .await
            .expect("card page");
        assert!(!rows.is_empty(), "page {page} is unexpectedly empty");
        seen += rows.len();
        if let Some(last) = rows.last() {
            cursor = Some((last.position, last.card.id));
        }
    }
    let elapsed = started.elapsed();
    assert_eq!(seen, 1_000, "five 200-card pages cover the whole board");
    assert!(
        elapsed < Duration::from_millis(500),
        "1,000 cards in five pages took {elapsed:?}"
    );
    println!("1,000 cards / 5 pages in {elapsed:?}");
}

async fn seed_thousand_cards(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'LOAD', 'Load') RETURNING id",
    )
    .bind(tenant_id)
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
         SELECT $1, $2, $3, $4, 'Card ' || g, 1024 * g FROM generate_series(1, 1000) AS g",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .execute(pool)
    .await
    .expect("cards");
    board
}
