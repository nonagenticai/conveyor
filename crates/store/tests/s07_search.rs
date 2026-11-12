//! S07 §5.6 — the three ranked search arms.
//!
//! - a **stemmed** word finds an asset through the full-text arm even though the query text
//!   never appears in the title;
//! - a **single-token typo** finds it too, and the trigram arm is load-bearing: at the
//!   extension's default threshold the same row is invisible, and `tsv @@ tsquery` alone
//!   returns nothing for it;
//! - an **exact key** finds its row when *both* lexical arms are blind to it (which is what
//!   makes the key-exact bonus reachable at all);
//! - a second tenant gets **zero** hits for the first tenant's exact title and exact key;
//! - `arms` routes: a card-only request returns no asset, a card is found by name, a
//!   catalogue entry by label, and the `kind`/`status` filters narrow governed assets only.
//!
//! `cargo test -p conveyor-store --test s07_search -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{AssetKind, TenantId};
use conveyor_store::begin_scoped;
use conveyor_store::governance::{NewAsset, insert_asset_tx};
use conveyor_store::search::{
    KEY_EXACT_BONUS, SearchArm, SearchHit, SearchRequest, WORD_SIMILARITY_THRESHOLD, search_tx,
};
use sqlx::PgConnection;
use uuid::Uuid;

/// One asset with a chosen key, title and body.
async fn seed_asset(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: AssetKind,
    key: &str,
    title: &str,
    body: &str,
) -> Uuid {
    let mut input = NewAsset::new(kind, key, title);
    input.body = body.to_owned();
    insert_asset_tx(&mut *conn, tenant, &input)
        .await
        .unwrap_or_else(|error| panic!("insert asset {key:?}: {error:?}"))
        .id
}

/// A single scalar count over the caller's scoped transaction.
async fn count(conn: &mut PgConnection, sql: &str, q: &str, tenant: TenantId) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(q)
        .fetch_one(&mut *conn)
        .await
        .expect("count")
}

/// The rows the full-text arm alone would return.
const FTS_ROWS: &str = "SELECT count(*)::bigint FROM governed_asset \
                        WHERE tenant_id = $1 AND tsv @@ websearch_to_tsquery('english', $2::text)";

/// The rows the trigram arm alone would return.
const TRGM_ROWS: &str = "SELECT count(*)::bigint FROM governed_asset \
                         WHERE tenant_id = $1 AND $2::text <% title";

/// `pg_trgm.word_similarity_threshold` as this backend sees it.
///
/// The variable belongs to the extension, so it is only registered once `pg_trgm`'s library
/// has been loaded **in this backend** — `current_setting` for an unregistered name is an
/// error, not `NULL`. One innocuous similarity call warms the backend up first; both
/// statements run on the caller's connection, so they cannot land on different backends.
///
/// The reading is returned as parsed (PostgreSQL renders `0.40` as `0.4`).
async fn threshold(conn: &mut PgConnection) -> f64 {
    let probe: f64 = sqlx::query_scalar("SELECT word_similarity('probe', 'probe')::float8")
        .fetch_one(&mut *conn)
        .await
        .expect("load pg_trgm");
    assert_eq!(probe, 1.0);
    let raw: String =
        sqlx::query_scalar("SELECT current_setting('pg_trgm.word_similarity_threshold')")
            .fetch_one(&mut *conn)
            .await
            .expect("read the threshold");
    raw.parse().expect("the threshold is a number")
}

/// One request over the given arms.
fn request(q: &str, arms: &[SearchArm]) -> SearchRequest {
    SearchRequest {
        q: q.to_owned(),
        arms: arms.to_vec(),
        kind: None,
        status: None,
        project_id: None,
        limit: 20,
    }
}

#[tokio::test]
async fn a_stemmed_word_and_a_single_token_typo_both_find_the_asset() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "stem").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");
    let asset = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant,
            AssetKind::Requirement,
            "req-mig",
            "Database migration plan",
            "The migration moves every tenant to the new schema.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };

    // (a) The query word is not in the title — the match is the stem `migrat`.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let fts = count(&mut tx, FTS_ROWS, "migrated", tenant).await;
    let literal: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM governed_asset \
          WHERE tenant_id = $1 AND title ILIKE '%migrated%'",
    )
    .bind(seed.tenant)
    .fetch_one(&mut *tx)
    .await
    .expect("count the literal substring");
    let hits = search_tx(
        &mut tx,
        tenant,
        &request("migrated", &[SearchArm::GovernedAsset]),
    )
    .await
    .expect("search");
    tx.rollback().await.expect("roll back");
    println!(
        "stemmed: fts {fts}, literal-substring rows {literal}, hits {}",
        hits.len()
    );
    assert_eq!(fts, 1, "the stemmed query matches the full-text arm");
    assert_eq!(literal, 0, "the word itself is nowhere in the title");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].id, asset);
    assert_eq!(hits[0].entity_type, "governed_asset");
    assert_eq!(hits[0].title, "Database migration plan");
    assert_eq!(hits[0].key.as_deref(), Some("req-mig"));
    assert_eq!(hits[0].kind.as_deref(), Some("requirement"));
    assert_eq!(hits[0].status.as_deref(), Some("draft"));
    let snippet = hits[0].snippet.as_deref().expect("a body excerpt");
    assert!(
        snippet.contains("migration"),
        "ts_headline highlights the stemmed word: {snippet:?}"
    );
    assert!(hits[0].score > 0.0, "score {}", hits[0].score);

    // (b) `paymnet` is one transposed token away. At the extension's DEFAULT threshold the
    // trigram arm cannot see it, and `tsv @@ tsquery` matches nothing — so only the lowered
    // threshold inside `search_tx` can find the row.
    let asset_pay = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant,
            AssetKind::Requirement,
            "req-pay",
            "Payment reconciliation ledger",
            "Reconciles payment records against the ledger nightly.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let default_threshold = threshold(&mut tx).await;
    let fts_typo = count(&mut tx, FTS_ROWS, "paymnet", tenant).await;
    let trgm_typo = count(&mut tx, TRGM_ROWS, "paymnet", tenant).await;
    let hits = search_tx(
        &mut tx,
        tenant,
        &request("paymnet", &[SearchArm::GovernedAsset]),
    )
    .await
    .expect("search");
    let in_transaction = threshold(&mut tx).await;
    tx.rollback().await.expect("roll back");
    println!(
        "typo: default {default_threshold}, fts {fts_typo}, trigram-at-default {trgm_typo}, \
         hits {}, threshold inside the search {in_transaction}",
        hits.len()
    );
    assert_eq!(
        fts_typo, 0,
        "websearch_to_tsquery alone finds nothing for a transposed token"
    );
    assert_eq!(
        trgm_typo, 0,
        "the {default_threshold} default threshold is why the typo arm needs lowering"
    );
    assert!(
        default_threshold > WORD_SIMILARITY_THRESHOLD.parse::<f64>().unwrap(),
        "the default {default_threshold} is above the threshold the arm runs at"
    );
    assert_eq!(in_transaction, 0.40, "search_tx lowered the threshold");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].id, asset_pay);
    assert!(
        hits[0].score > 0.0 && hits[0].score < 1.0,
        "the typo matched on trigram only, not on rank: {}",
        hits[0].score
    );

    // The lowered threshold is transaction-local: a fresh transaction is back on the default.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let after = threshold(&mut tx).await;
    tx.rollback().await.expect("roll back");
    println!("threshold after the search's transaction: {after}");
    assert_eq!(
        after, default_threshold,
        "set_config(..., true) died with its transaction"
    );
}

#[tokio::test]
async fn an_exact_key_finds_its_row_when_no_lexical_arm_can() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "exactkey").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // `a` is a legal key under `domain::Code`'s grammar (one lowercase letter) and an
    // `english` stopword, so `websearch_to_tsquery('english','a')` is EMPTY and the title's
    // trigrams are nothing like it. Both lexical arms are blind to the row, which is exactly
    // the case that makes the `key = $2` disjunct load-bearing.
    let asset = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant,
            AssetKind::Principle,
            "a",
            "Quarterly budget review",
            "The board runs the review each quarter.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let fts = count(&mut tx, FTS_ROWS, "a", tenant).await;
    let trgm = count(&mut tx, TRGM_ROWS, "a", tenant).await;
    let hits = search_tx(&mut tx, tenant, &request("a", &[SearchArm::GovernedAsset]))
        .await
        .expect("search");
    let by_code = search_tx(&mut tx, tenant, &request("a", &[SearchArm::CatalogueEntry]))
        .await
        .expect("search the catalogue");
    tx.rollback().await.expect("roll back");
    println!(
        "exact key: fts {fts}, trigram {trgm}, asset hits {}, catalogue hits {}",
        hits.len(),
        by_code.len()
    );
    assert_eq!(fts, 0, "the full-text arm cannot see the key");
    assert_eq!(trgm, 0, "the trigram arm cannot see the key either");
    assert_eq!(
        hits.len(),
        1,
        "the key equality is what admits the row: {hits:?}"
    );
    assert_eq!(hits[0].id, asset);
    assert_eq!(hits[0].key.as_deref(), Some("a"));
    assert_eq!(
        hits[0].score, KEY_EXACT_BONUS,
        "an exact key scores the bonus and nothing else"
    );
    assert!(
        by_code.is_empty(),
        "no catalogue entry has that code: {by_code:?}"
    );
}

#[tokio::test]
async fn a_second_tenant_gets_zero_hits_for_the_first_tenants_exact_title() {
    let db = support::TestDb::fresh().await;
    let alpha = support::seed_tenant(&db.super_pool, "alpha-s").await;
    let beta = support::seed_tenant(&db.super_pool, "beta-s").await;
    let tenant_a = TenantId::from_uuid(alpha.tenant).expect("non-nil");
    let tenant_b = TenantId::from_uuid(beta.tenant).expect("non-nil");

    let asset = {
        let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant_a,
            AssetKind::Requirement,
            "req-ledger",
            "Payment reconciliation ledger",
            "Reconciles payment records against the ledger nightly.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };

    // Tenant B: the exact title, the exact key, and every arm.
    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope");
    let by_title = search_tx(
        &mut tx,
        tenant_b,
        &request("Payment reconciliation ledger", &SearchArm::ALL),
    )
    .await
    .expect("search by title");
    let by_key = search_tx(&mut tx, tenant_b, &request("req-ledger", &SearchArm::ALL))
        .await
        .expect("search by key");
    tx.rollback().await.expect("roll back");

    // Positive control: tenant A finds its own row through the same code path.
    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope");
    let own = search_tx(
        &mut tx,
        tenant_a,
        &request("Payment reconciliation ledger", &SearchArm::ALL),
    )
    .await
    .expect("search as the owner");
    tx.rollback().await.expect("roll back");

    println!(
        "cross-tenant: beta by title {}, by key {}, alpha {}",
        by_title.len(),
        by_key.len(),
        own.len()
    );
    assert!(
        by_title.is_empty(),
        "another tenant's exact title returns no hit: {by_title:?}"
    );
    assert!(
        by_key.is_empty(),
        "another tenant's exact key returns no hit: {by_key:?}"
    );
    assert!(
        own.iter().any(|hit| hit.id == asset),
        "the owner sees the same row: {own:?}"
    );
}

#[tokio::test]
async fn the_arms_route_and_each_arms_label_is_its_own() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "arms").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let asset = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant,
            AssetKind::Requirement,
            "req-mig",
            "Database migration plan",
            "The migration moves every tenant to the new schema.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };
    // A card and a catalogue row with the word `gateway` in their own label column; the
    // fixture's own card and `gate`/`review` entry stay as the negative controls.
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description, \
                           position, created_by) \
         VALUES ($1, $2, $3, $4, 'Payment gateway migration', \
                 'Move the payment gateway onto the new cluster.', 2048, $5) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.board)
    .bind(seed.list)
    .bind(seed.card_type)
    .bind(seed.user)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the gateway card");
    let entry: Uuid = sqlx::query_scalar(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label) \
         VALUES ($1, 'gateway', 'pay_gw', 'Payment gateway standard') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the catalogue entry");

    // (d) `card` only: the asset is not reachable, whatever it matches.
    let only_cards = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let hits = search_tx(
            &mut tx,
            tenant,
            &request("Database migration plan", &[SearchArm::Card]),
        )
        .await
        .expect("search cards only");
        tx.rollback().await.expect("roll back");
        hits
    };
    assert!(
        only_cards.iter().all(|hit| hit.id != asset),
        "a card-only request returns no asset: {only_cards:?}"
    );

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    // (e) a card is found by name, and reports the card arm's own shape.
    let cards = search_tx(&mut tx, tenant, &request("gateway", &[SearchArm::Card]))
        .await
        .expect("search cards");
    // (f) a catalogue entry is found by label.
    let entries = search_tx(
        &mut tx,
        tenant,
        &request("gateway", &[SearchArm::CatalogueEntry]),
    )
    .await
    .expect("search catalogue entries");
    // The same word over every arm, to show the two are separate rows of one page.
    let both = search_tx(&mut tx, tenant, &request("gateway", &SearchArm::ALL))
        .await
        .expect("search every arm");
    // `limit` is honoured for the page as a whole.
    let limited = search_tx(
        &mut tx,
        tenant,
        &SearchRequest {
            limit: 1,
            ..request("gateway", &SearchArm::ALL)
        },
    )
    .await
    .expect("search with limit 1");
    tx.rollback().await.expect("roll back");

    println!(
        "arms: cards {} entries {} all {} limited {}",
        cards.len(),
        entries.len(),
        both.len(),
        limited.len()
    );
    assert_eq!(cards.len(), 1, "{cards:?}");
    let card_hit: &SearchHit = &cards[0];
    assert_eq!(card_hit.id, card);
    assert_eq!(card_hit.entity_type, "card");
    assert_eq!(card_hit.title, "Payment gateway migration");
    assert_eq!(card_hit.key, None, "a card has no key");
    assert_eq!(card_hit.kind, None, "a card has no kind");
    assert_eq!(card_hit.status.as_deref(), Some("active"));
    assert!(
        card_hit
            .snippet
            .as_deref()
            .expect("a description excerpt")
            .contains("gateway"),
        "ts_headline reads the description: {card_hit:?}"
    );
    assert!(card_hit.score > 0.0);

    assert_eq!(entries.len(), 1, "{entries:?}");
    let entry_hit: &SearchHit = &entries[0];
    assert_eq!(entry_hit.id, entry);
    assert_eq!(entry_hit.entity_type, "catalogue_entry");
    assert_eq!(entry_hit.title, "Payment gateway standard");
    assert_eq!(entry_hit.key.as_deref(), Some("pay_gw"));
    assert_eq!(entry_hit.kind.as_deref(), Some("gateway"));
    assert_eq!(entry_hit.status.as_deref(), Some("active"));
    assert_eq!(entry_hit.snippet, None, "a catalogue entry has no body");
    assert!(entry_hit.score > 0.0);

    assert_eq!(both.len(), 2, "one hit per arm: {both:?}");
    assert_eq!(limited.len(), 1, "the limit bounds the whole page");
}

#[tokio::test]
async fn the_filters_narrow_governed_assets_and_nothing_else() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "filters").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let asset = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let id = seed_asset(
            &mut tx,
            tenant,
            AssetKind::Requirement,
            "req-ledger",
            "Payment reconciliation ledger",
            "Reconciles payment records against the ledger nightly.",
        )
        .await;
        tx.commit().await.expect("commit");
        id
    };
    // A card that matches the same word, so the filters can be seen NOT to touch it.
    sqlx::query(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description, \
                           position, created_by) \
         VALUES ($1, $2, $3, $4, 'Ledger cleanup', 'Retire the old ledger tables.', 2048, $5)",
    )
    .bind(seed.tenant)
    .bind(seed.board)
    .bind(seed.list)
    .bind(seed.card_type)
    .bind(seed.user)
    .execute(&db.super_pool)
    .await
    .expect("seed the ledger card");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let unfiltered = search_tx(&mut tx, tenant, &request("ledger", &SearchArm::ALL))
        .await
        .expect("search everything");
    let mismatched = search_tx(
        &mut tx,
        tenant,
        &SearchRequest {
            kind: Some("requirement".to_owned()),
            status: Some("approved".to_owned()),
            ..request("ledger", &SearchArm::ALL)
        },
    )
    .await
    .expect("search with a status the asset does not hold");
    let matched = search_tx(
        &mut tx,
        tenant,
        &SearchRequest {
            kind: Some("requirement".to_owned()),
            status: Some("draft".to_owned()),
            ..request("ledger", &SearchArm::ALL)
        },
    )
    .await
    .expect("search with the stored status");
    tx.rollback().await.expect("roll back");

    println!(
        "filters: unfiltered {}, mismatched {}, matched {}",
        unfiltered.len(),
        mismatched.len(),
        matched.len()
    );
    assert!(
        unfiltered.iter().any(|hit| hit.id == asset),
        "{unfiltered:?}"
    );
    assert!(
        unfiltered
            .iter()
            .any(|hit| hit.entity_type == "card" && hit.title == "Ledger cleanup"),
        "the card matches the same word: {unfiltered:?}"
    );
    // The asset is `draft`, so an `approved` filter drops it — and the card survives, because
    // `kind`/`status` are governed-asset filters (S07 §5.6 names them under that arm).
    assert!(
        matched.iter().any(|hit| hit.id == asset),
        "the stored status keeps the asset: {matched:?}"
    );
    assert!(
        !mismatched.iter().any(|hit| hit.id == asset),
        "a different status drops the asset: {mismatched:?}"
    );
    assert!(
        mismatched
            .iter()
            .any(|hit| hit.entity_type == "card" && hit.title == "Ledger cleanup"),
        "the filters do not reach the card arm: {mismatched:?}"
    );
    assert!(
        matched
            .iter()
            .all(|hit| hit.entity_type != "governed_asset"
                || hit.kind.as_deref() == Some("requirement")),
        "the kind filter holds: {matched:?}"
    );
}
