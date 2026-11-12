//! S15 steps 8–10: work-package memory store.
//!
//! The properties, each a way this could go wrong silently:
//!
//! 1. **Tenant isolation is two-sided** — tenant B sees zero of A's rows, and a write that names
//!    A's card from B's scope is a typed parent miss, not an FK-accepted cross-tenant row.
//! 2. **The project is derived** — a caller cannot name a project; it comes from the card's board.
//! 3. **A supersede is same-card only** — the table has no unique pair for this, so the store
//!    checks it.
//! 4. **A digest covers a row exactly once** — after `write_digest_tx` the un-consolidated set is
//!    empty, and the digest row is itself stamped so it is never consolidated.
//!
//! `rbuild -- test -p conveyor-store --test memory -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::memory::{MemoryKind, MemorySource, digest};
use conveyor_store::begin_scoped;
use conveyor_store::memory::{
    MemoryStoreError, NewMemory, context_for_card_tx, hot_for_card_tx, insert_memory_tx,
    unconsolidated_for_card_tx, write_digest_tx,
};
use serde_json::json;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

async fn remember(
    db: &support::TestDb,
    seed: &support::Seeded,
    card: Uuid,
    body: &str,
    supersedes: Option<Uuid>,
) -> Uuid {
    let mut tx = begin_scoped(&db.runtime, tenant(seed))
        .await
        .expect("scope");
    let record = insert_memory_tx(
        &mut tx,
        tenant(seed),
        &NewMemory {
            card_id: card,
            run_id: None,
            capability_id: None,
            kind: MemoryKind::Fact,
            source: MemorySource::Agent,
            body,
            supersedes_id: supersedes,
            meta: json!({}),
        },
    )
    .await
    .expect("insert memory");
    tx.commit().await.expect("commit");
    record.id
}

async fn second_card(db: &support::TestDb, seed: &support::Seeded) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'second', 2048) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.board)
    .bind(seed.list)
    .bind(seed.card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed second card")
}

#[tokio::test]
async fn memory_is_tenant_scoped_and_newest_first() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s15-mem-a").await;
    let first = remember(&db, &a, a.card, "first", None).await;
    let second = remember(&db, &a, a.card, "second", None).await;

    let mut tx = begin_scoped(&db.runtime, tenant(&a)).await.expect("scope");
    let hot = hot_for_card_tx(&mut tx, tenant(&a), a.card, 50)
        .await
        .expect("hot rows");
    tx.commit().await.expect("commit");
    assert_eq!(hot.len(), 2);
    assert_eq!(hot[0].id, second);
    assert_eq!(hot[1].id, first);
    assert_eq!(
        hot[0].project_id, a.project,
        "the project is derived from the card"
    );

    // Tenant B cannot see A's card or rows.
    let b = support::seed_tenant(&db.super_pool, "s15-mem-b").await;
    let mut tx = begin_scoped(&db.runtime, tenant(&b)).await.expect("scope");
    let seen = hot_for_card_tx(&mut tx, tenant(&b), a.card, 50)
        .await
        .expect("scoped read");
    assert!(seen.is_empty(), "tenant B sees tenant A's memory");
    tx.rollback().await.expect("rollback");

    // A write from B's scope that names A's card is refused before the FK.
    let mut tx = begin_scoped(&db.runtime, tenant(&b)).await.expect("scope");
    let refused = insert_memory_tx(
        &mut tx,
        tenant(&b),
        &NewMemory {
            card_id: a.card,
            run_id: None,
            capability_id: None,
            kind: MemoryKind::Fact,
            source: MemorySource::Agent,
            body: "forged",
            supersedes_id: None,
            meta: json!({}),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(MemoryStoreError::Parent(_))),
        "a cross-tenant card must be a parent miss: {refused:?}"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn a_supersede_must_be_on_the_same_card() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-mem-super").await;
    let other_card = second_card(&db, &seed).await;
    let other_row = remember(&db, &seed, other_card, "other card", None).await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let refused = insert_memory_tx(
        &mut tx,
        tenant(&seed),
        &NewMemory {
            card_id: seed.card,
            run_id: None,
            capability_id: None,
            kind: MemoryKind::Decision,
            source: MemorySource::Human,
            body: "supersedes the wrong card",
            supersedes_id: Some(other_row),
            meta: json!({}),
        },
    )
    .await;
    assert!(matches!(
        refused,
        Err(MemoryStoreError::SupersedesOtherCard)
    ));
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn a_digest_covers_the_unconsolidated_rows_once() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s15-mem-digest").await;
    remember(&db, &seed, seed.card, "found the cause", None).await;
    remember(&db, &seed, seed.card, "tried a fix", None).await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let rows = unconsolidated_for_card_tx(&mut tx, tenant(&seed), seed.card)
        .await
        .expect("unconsolidated");
    assert_eq!(rows.len(), 2);
    let digest = digest(seed.card, &rows).expect("digest");
    let context = context_for_card_tx(&mut tx, tenant(&seed), seed.card)
        .await
        .expect("context");
    assert_eq!(context.project_id, seed.project);

    let digest_id = write_digest_tx(&mut tx, tenant(&seed), seed.card, &digest)
        .await
        .expect("write digest");
    let after = unconsolidated_for_card_tx(&mut tx, tenant(&seed), seed.card)
        .await
        .expect("unconsolidated after");
    assert!(
        after.is_empty(),
        "a covered row must not be consolidated again"
    );

    let hot = hot_for_card_tx(&mut tx, tenant(&seed), seed.card, 50)
        .await
        .expect("hot rows");
    tx.commit().await.expect("commit");

    // Two hot rows plus the digest, newest first.
    assert_eq!(hot.len(), 3);
    assert_eq!(hot[0].id, digest_id);
    assert_eq!(hot[0].kind, MemoryKind::Digest);
    assert_eq!(hot[0].source, MemorySource::System);
    assert!(hot[0].consolidated_at.is_some(), "the digest is stamped");
    assert!(
        hot[0]
            .body
            .starts_with(&format!("work-package: {}", seed.card))
    );
    assert!(hot[1].consolidated_at.is_some());
    assert!(hot[2].consolidated_at.is_some());
}
