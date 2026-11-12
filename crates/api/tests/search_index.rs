//! S07 §5.6 — the indexes `search.query` depends on, and the three arms that read them.
//!
//! `search.query` is one ranked statement over three tables. It is only a *search* if
//! Postgres can answer each arm from an index; a dropped index turns the same statement into
//! a scan that still returns the right rows, so nothing but a plan assertion notices. This
//! file asserts the contract at three levels:
//!
//! 1. **The indexes exist** ([`the_search_indexes_exist`]) — `pg_trgm` is installed and all
//!    five indexes a freshly migrated database must carry are present, each on the table it
//!    serves. A migration that forgets one is otherwise invisible until a tenant is large.
//! 2. **Each lexical arm uses its own index** ([`a_stemmed_query_uses_the_full_text_gin`],
//!    [`a_typo_query_uses_the_trigram_gin`]) — the plan for the governed-asset arm's exact
//!    predicate names `governed_asset_tsv_idx` for a stemmed word and
//!    `governed_asset_title_trgm_idx` for a single-token typo. The typo arm runs with the
//!    same transaction-local `pg_trgm.word_similarity_threshold` `search_tx` sets, because
//!    at the extension's 0.6 default a typo never reaches the index at all.
//! 3. **The planner chooses an index unaided** ([`the_planner_picks_an_index_without_help_at_scale`])
//!    — at 50 000 rows, with `enable_seqscan` left alone, the plan names one of the arm's
//!    lexical GIN indexes. This is the gate that a dropped index fails.
//!
//! Then the routing the action itself owns: each arm returns its own row
//! ([`all_three_arms_return_their_own_rows`]) and `types` narrows the page to the arms it
//! names ([`the_types_filter_excludes_other_arms`]). The second test asserts an unfiltered
//! search first, so "only cards came back" cannot pass by matching nothing.
//!
//! ## These plans are read as the role that runs them
//!
//! Every plan assertion below opens its transaction with `store::begin_scoped` on the
//! **runtime** pool, so `app.current_tenant` is set and the tenant policy applies exactly as
//! it does in production. That matters more than it looks: the policy is a security barrier,
//! and Postgres will not use an index whose qual is not `LEAKPROOF`. `ts_match_vq` (`@@`) and
//! `word_similarity_op` (`<%`) ship with `proleakproof = false`, and with them unmarked the
//! arm degrades — measured on a 50 000-row table under `FORCE` RLS as `conveyor_runtime`:
//!
//! ```text
//! before:  Bitmap Index Scan on governed_asset_project_idx
//!            Filter: (tsv @@ '''migrat'''::tsquery OR 'migrating'::text <% title OR ...)
//! after:   BitmapOr
//!            ->  Bitmap Index Scan on governed_asset_tsv_idx
//!            ->  Bitmap Index Scan on governed_asset_title_trgm_idx
//!            ->  Bitmap Index Scan on governed_asset_tenant_kind_key_key
//! ```
//!
//! What makes the difference is `scripts/db-roles.sql` — the superuser provisioning file that
//! `support::Pg::fresh` runs before the migrations. It marks both functions leakproof and
//! raises if it cannot, so these plans are the real ones rather than a superuser's view.
//! [`the_search_operators_are_leakproof`] asserts that premise directly: a database
//! provisioned without that file must fail there, not silently serve search at O(rows).
//!
//! ## Why the plan fixtures carry a corpus instead of one row
//!
//! The governed-asset arm's predicate is a three-way `OR` over `tsv`, `title` and `key`, and
//! the tenant is also a prefix of `governed_asset_project_idx`. On a one-row table that btree
//! costs **~8** and the GIN scans price above it, so the planner answers from the tenant index
//! and filters — no GIN index appears, whatever the query. The assertion would then be red for a
//! reason that has nothing to do with the index it is meant to prove, and a smaller table
//! cannot fix it. The fixtures therefore carry a filler corpus (5 000 rows for the two "this
//! index is used" tests, 50 000 for the unaided-planner test), and the plan is then decided by
//! the lexical predicates, which is the property under test.
//!
//! The fixture also runs `VACUUM (ANALYZE)` rather than `ANALYZE` alone. `VACUUM` is what
//! flushes the GIN pending list, and `gincostestimate` charges the pending entries: measured
//! at 50 000 rows, a full pending list priced the `tsv` scan at **1 395** against **18** once
//! flushed, which is enough to hand the plan back to the tenant btree. That is a fixture
//! artefact of inserting a corpus in one statement, not a property of the index; `VACUUM` is
//! routine maintenance, not a planner hint, and no `enable_*` knob is touched by the test
//! that must prove the planner chooses unaided.
//!
//! The rows are written on the **superuser** pool with an explicit `tenant_id`: `RLS` is
//! `FORCE`d on every one of these tables, so only a superuser can seed another arm's row
//! without the scope the action would apply — and a test that seeds through the same scope it
//! is testing cannot see a cross-tenant leak.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// The governed-asset arm's wire spelling.
const GOVERNED_ASSET: &str = "governed_asset";

/// The card arm's wire spelling.
const CARD: &str = "card";

/// The catalogue arm's wire spelling.
const CATALOGUE_ENTRY: &str = "catalogue_entry";

/// The rows of the two "lexical index" fixtures: enough that the tenant btree is no longer the
/// cheapest access path, small enough to insert in a few milliseconds.
const PLAN_CORPUS: i32 = 5_000;

/// The rows of the unaided-planner fixture — S07 §5.6's gate size.
const SCALE_CORPUS: i32 = 50_000;

/// The governed-asset arm's predicate, without the ranking columns: the full-text arm
/// (`tsv`), the trigram arm (`title`) and the key equality that makes the key-exact bonus
/// reachable. This is the predicate `search_tx` binds for that arm, and the only thing the
/// plan assertions read — note that it is the *predicate* whose access path is measured, not
/// the action's plan, which the tenant policy degrades (see the module header).
const ASSET_ARM_EXPLAIN: &str = "EXPLAIN SELECT id FROM governed_asset \
     WHERE tenant_id = $1 \
       AND (tsv @@ websearch_to_tsquery('english', $2) OR $2::text <% title OR key = $2)";

/// The same predicate as a real read, for the index-usage fallback in the scale test.
const ASSET_ARM_SELECT: &str = "SELECT id FROM governed_asset \
     WHERE tenant_id = $1 \
       AND (tsv @@ websearch_to_tsquery('english', $2) OR $2::text <% title OR key = $2)";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// The plan for [`ASSET_ARM_EXPLAIN`], as one string.
///
/// Read as `conveyor_runtime` inside [`store::begin_scoped`], so the transaction carries
/// `app.current_tenant` and the tenant policy is in force — the same scope, role and policy
/// the action runs under. `steer` is the caller's `enable_seqscan = off`: the two "this index
/// is used" tests set it so the assertion is about *which* index serves the predicate, not
/// about whether the planner would have found any. `threshold` sets the arm's transaction-local
/// `pg_trgm.word_similarity_threshold`, exactly as `search_tx` does before its statement.
async fn asset_arm_plan(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    q: &str,
    steer: bool,
    threshold: bool,
) -> String {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .expect("scope the explain transaction to the tenant");
    if steer {
        sqlx::query("SET LOCAL enable_seqscan = off")
            .execute(&mut *tx)
            .await
            .expect("disable sequential scans for this transaction");
    }
    if threshold {
        sqlx::query("SELECT set_config('pg_trgm.word_similarity_threshold', '0.40', true)")
            .execute(&mut *tx)
            .await
            .expect("lower the trigram threshold for this transaction");
    }
    let lines: Vec<String> = sqlx::query_scalar(ASSET_ARM_EXPLAIN)
        .bind(tenant.as_uuid())
        .bind(q)
        .fetch_all(&mut *tx)
        .await
        .expect("EXPLAIN the asset arm");
    tx.commit().await.expect("close the explain transaction");
    lines.join("\n")
}

/// Insert `count` assets no arm of this fixture searches for, then settle the table.
///
/// `phrase` is the part of the title that decides whether a filler row can match: the typo
/// fixture must not be full of rows whose title says "payment", or `paymnet` matches every
/// row and the planner is right to scan.
async fn seed_corpus(pg: &support::Pg, tenant: Uuid, count: i32, phrase: &str) {
    sqlx::query(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, body)
         SELECT $1, 'requirement', 'BULK-' || g,
                'Bulk requirement ' || g || ' ' || $3,
                'Filler body for the bulk load number ' || g
           FROM generate_series(1, $2) g",
    )
    .bind(tenant)
    .bind(count)
    .bind(phrase)
    .execute(&pg.super_pool)
    .await
    .expect("insert the filler corpus");
    settle(pg).await;
}

/// `ANALYZE` the table, with the GIN pending list flushed first.
///
/// `ANALYZE` alone leaves every entry of a one-statement bulk insert in the pending list, and
/// `gincostestimate` prices that list — measured, it turns an 18-unit `tsv` scan into 1 395
/// and hands the plan back to the tenant btree. `VACUUM` flushes it; it is maintenance, not a
/// hint, and it leaves every `enable_*` GUC at its default.
async fn settle(pg: &support::Pg) {
    sqlx::query("VACUUM (ANALYZE) governed_asset")
        .execute(&pg.super_pool)
        .await
        .expect("vacuum and analyze governed_asset");
}

/// One asset, written as the superuser with an explicit tenant.
async fn insert_asset(pg: &support::Pg, tenant: Uuid, key: &str, title: &str, body: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, body)
         VALUES ($1, 'requirement', $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(key)
    .bind(title)
    .bind(body)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the asset")
}

/// One card, with the project → board → list → card-type rows a card cannot exist without.
async fn insert_card(pg: &support::Pg, tenant: Uuid, user: Uuid, name: &str) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'SEARCH-ARM', 'Search arm')
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the list");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the card type");
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description,
                           position, created_by)
         VALUES ($1, $2, $3, $4, $5, 'A card seeded for the card arm.', 1024, $6) RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .bind(name)
    .bind(user)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the card")
}

/// One catalogue entry, written as the superuser with an explicit tenant.
async fn insert_catalogue_entry(pg: &support::Pg, tenant: Uuid, code: &str, label: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label)
         VALUES ($1, 'standard', $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(code)
    .bind(label)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the catalogue entry")
}

/// Invoke `search.query` and return its page of hits.
async fn search(running: &support::Running, token: &str, input: Value) -> Vec<Value> {
    let (status, body) = running
        .invoke("search.query", token, input, None, None, None)
        .await;
    assert_eq!(status, 200, "search.query: {body}");
    output(&body)["items"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the page carries `items`: {body}"))
}

/// The number of scans `governed_asset_tsv_idx` has served, as the statistics collector has it.
async fn tsv_index_scans(pg: &support::Pg) -> i64 {
    sqlx::query_scalar(
        "SELECT idx_scan FROM pg_stat_user_indexes WHERE indexrelname = 'governed_asset_tsv_idx'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("read pg_stat_user_indexes for governed_asset_tsv_idx")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_search_indexes_exist() {
    let pg = support::Pg::fresh().await;

    // `pg_trgm` is a migration, not an assumption: the trigram operator the arm uses (`<%`)
    // does not exist without it, so its absence is a syntax error at runtime, not a slow read.
    let trgm: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
            .fetch_one(&pg.super_pool)
            .await
            .expect("read pg_extension");
    assert!(trgm, "the pg_trgm extension is installed by migration 0007");

    // Every index, on the table it must serve. The pair matters: an index of the right name on
    // the wrong table serves nothing the arm asks for.
    let present: Vec<(String, String)> = sqlx::query_as(
        "SELECT tablename, indexname FROM pg_indexes WHERE schemaname = current_schema()",
    )
    .fetch_all(&pg.super_pool)
    .await
    .expect("read pg_indexes");

    for (table, index) in [
        ("governed_asset", "governed_asset_tsv_idx"),
        ("governed_asset", "governed_asset_title_trgm_idx"),
        ("card", "card_tsv_idx"),
        ("card", "card_name_trgm_idx"),
        ("catalogue_entry", "catalogue_entry_label_trgm_idx"),
    ] {
        assert!(
            present
                .iter()
                .any(|(on_table, name)| on_table == table && name == index),
            "{index} must exist on {table}; pg_indexes has {present:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_search_operators_are_leakproof() {
    let pg = support::Pg::fresh().await;

    // Under `FORCE` RLS the planner refuses an index whose qual is not leakproof, so without
    // these two marked, `search.query` answers from the tenant btree and filters. The marking
    // is a superuser act and lives in scripts/db-roles.sql, which `Pg::fresh` runs before the
    // migrations; this test is the tripwire for a database provisioned without it.
    let operators: Vec<(String, bool)> = sqlx::query_as(
        "SELECT proname::text, proleakproof FROM pg_proc
          WHERE proname IN ('ts_match_vq', 'word_similarity_op')
          ORDER BY proname",
    )
    .fetch_all(&pg.super_pool)
    .await
    .expect("read pg_proc");

    for required in ["ts_match_vq", "word_similarity_op"] {
        assert!(
            operators.iter().any(|(name, _)| name == required),
            "{required} is not installed; pg_proc has {operators:?}"
        );
    }
    for (name, leakproof) in &operators {
        assert!(
            *leakproof,
            "{name} is not LEAKPROOF, so no text-search index is usable under RLS; \
             scripts/db-roles.sql marks it and raises when it cannot"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stemmed_query_uses_the_full_text_gin() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // One asset whose BODY carries the word the query stems to: `migrating` and `migrated`
    // share the `migrat` stem, and the word `migrated` is nowhere in the title.
    insert_asset(
        &pg,
        alice.tenant_id,
        "req-mig",
        "Database rollout plan",
        "The team migrated every tenant to the new schema.",
    )
    .await;
    // Filler the stemmed query does not match, so the plan is decided by the lexical arms
    // rather than by the tenant index (see the module header).
    seed_corpus(
        &pg,
        alice.tenant_id,
        PLAN_CORPUS,
        "about unrelated concerns",
    )
    .await;

    let plan = asset_arm_plan(&pg.runtime, alice.tenant, "migrating", true, false).await;
    assert!(
        plan.contains("governed_asset_tsv_idx"),
        "a stemmed word must be served by the full-text GIN index as conveyor_runtime; \
         plan:\n{plan}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_typo_query_uses_the_trigram_gin() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    insert_asset(
        &pg,
        alice.tenant_id,
        "req-pay",
        "Payment reconciliation ledger",
        "Reconciles payment records against the ledger nightly.",
    )
    .await;
    // The filler must not say "payment": `paymnet` is one transposed token away from it, so a
    // corpus full of that word is legitimately matched by every row and the index is correctly
    // not the cheapest path.
    seed_corpus(
        &pg,
        alice.tenant_id,
        PLAN_CORPUS,
        "about unrelated concerns",
    )
    .await;

    // 0.40 is the arm's own threshold; at the extension's 0.6 default `paymnet` scores 0.500
    // against `Payment reconciliation ledger` and the row is invisible to this arm.
    let plan = asset_arm_plan(&pg.runtime, alice.tenant, "paymnet", true, true).await;
    assert!(
        plan.contains("governed_asset_title_trgm_idx"),
        "a single-token typo must be served by the trigram GIN index as conveyor_runtime; \
         plan:\n{plan}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_planner_picks_an_index_without_help_at_scale() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    seed_corpus(
        &pg,
        alice.tenant_id,
        SCALE_CORPUS,
        "about payment reconciliation",
    )
    .await;

    // Nothing is steered here: no `enable_seqscan`, no threshold, no `SET` of any kind. The
    // property is that at the gate's size the runtime role reaches for the arm's own index.
    let plan = asset_arm_plan(&pg.runtime, alice.tenant, "migrating", false, false).await;
    if plan.contains("governed_asset_tsv_idx") || plan.contains("governed_asset_title_trgm_idx") {
        return;
    }

    // The prescribed fallback: the planner genuinely chose a scan. Do not fake the plan
    // assertion — prove instead whether the index is used at all, and report the plan in the
    // failure. `pg_stat_force_next_flush` makes the counter visible without waiting for the
    // statistics collector's next round.
    let before = tsv_index_scans(&pg).await;
    let mut tx = store::begin_scoped(&pg.runtime, alice.tenant)
        .await
        .expect("scope the fallback read to the tenant");
    sqlx::query(ASSET_ARM_SELECT)
        .bind(alice.tenant_id)
        .bind("migrating")
        .fetch_all(&mut *tx)
        .await
        .expect("run the arm's predicate");
    tx.commit().await.expect("commit the fallback read");
    sqlx::query("SELECT pg_stat_force_next_flush()")
        .execute(&pg.super_pool)
        .await
        .expect("flush the statistics");
    let after = tsv_index_scans(&pg).await;
    assert!(
        after > before,
        "the planner chose a scan and governed_asset_tsv_idx served no scan ({before} -> \
         {after}); plan:\n{plan}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn all_three_arms_return_their_own_rows() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // Three tokens with no trigram in common, so a query for one cannot reach another arm's
    // row through the typo predicate — each query must return exactly its own row.
    let card_token = "qwzkvbnmp";
    let catalogue_token = "xylqfjrtd";
    let asset_token = "phwgcszla";

    let asset = insert_asset(
        &pg,
        alice.tenant_id,
        "req-arm",
        &format!("Ledger note {asset_token}"),
        "The body is not what this test searches for.",
    )
    .await;
    let card = insert_card(
        &pg,
        alice.tenant_id,
        alice.user_id,
        &format!("Card {card_token} title"),
    )
    .await;
    let entry = insert_catalogue_entry(
        &pg,
        alice.tenant_id,
        "arm-standard",
        &format!("Standard {catalogue_token} label"),
    )
    .await;

    let running = support::Running::start(&pg).await;

    for (q, entity_type, id) in [
        (card_token, CARD, card),
        (catalogue_token, CATALOGUE_ENTRY, entry),
        (asset_token, GOVERNED_ASSET, asset),
    ] {
        let items = search(&running, "dev:alice", json!({"q": q, "limit": 20})).await;
        assert_eq!(items.len(), 1, "{q} reaches exactly one arm: {items:?}");
        assert_eq!(items[0]["entity_type"], entity_type, "{q}: {items:?}");
        assert_eq!(items[0]["id"], id.to_string(), "{q}: {items:?}");
    }

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_types_filter_excludes_other_arms() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // One token present in all three arms' searchable columns, so an unfiltered search really
    // does return all three rows and the filter assertions below are not vacuous.
    let shared = "gatewaymarker";
    let asset = insert_asset(
        &pg,
        alice.tenant_id,
        "req-shared",
        &format!("Ledger {shared} note"),
        "The body is not what this test searches for.",
    )
    .await;
    let card = insert_card(
        &pg,
        alice.tenant_id,
        alice.user_id,
        &format!("Card {shared} title"),
    )
    .await;
    let entry = insert_catalogue_entry(
        &pg,
        alice.tenant_id,
        "shared-standard",
        &format!("Standard {shared} label"),
    )
    .await;

    let running = support::Running::start(&pg).await;

    // Positive control: without it, "only cards came back" would also pass if nothing matched.
    let all = search(&running, "dev:alice", json!({"q": shared, "limit": 20})).await;
    let entity_types: Vec<&str> = all
        .iter()
        .map(|hit| hit["entity_type"].as_str().expect("entity_type"))
        .collect();
    for arm in [GOVERNED_ASSET, CARD, CATALOGUE_ENTRY] {
        assert!(
            entity_types.contains(&arm),
            "the unfiltered search matches the {arm} arm: {all:?}"
        );
    }
    assert_eq!(all.len(), 3, "one hit per arm: {all:?}");

    // `types` names one arm: only that arm's row, and none of the other two ids.
    let cards = search(
        &running,
        "dev:alice",
        json!({"q": shared, "types": ["card"], "limit": 20}),
    )
    .await;
    assert_eq!(
        cards.len(),
        1,
        "a card-only search returns the card: {cards:?}"
    );
    assert_eq!(cards[0]["entity_type"], CARD, "{cards:?}");
    assert_eq!(cards[0]["id"], card.to_string(), "{cards:?}");

    let assets = search(
        &running,
        "dev:alice",
        json!({"q": shared, "types": ["governed_asset"], "limit": 20}),
    )
    .await;
    assert!(
        !assets.is_empty(),
        "the asset arm still matches: {assets:?}"
    );
    assert!(
        assets
            .iter()
            .all(|hit| hit["entity_type"] == GOVERNED_ASSET),
        "a governed-asset-only search returns only assets: {assets:?}"
    );
    assert!(
        assets.iter().all(|hit| hit["id"] != card.to_string()
            && hit["id"] != entry.to_string()
            && hit["id"] == asset.to_string()),
        "and none of the other arms' rows: {assets:?}"
    );

    running.shutdown().await;
}
