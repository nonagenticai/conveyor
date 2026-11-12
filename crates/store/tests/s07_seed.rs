//! S07 seed: the checked-in catalogue export reaches the database through the one importer.
//!
//! The export (`seed/catalogue.json`) is a one-time read-only read of RM, so the assertions
//! are about what it must contain, not about a shape this workspace invented: `project_type`
//! rows carry the move matrix and the card types it is keyed by, `phase` rows exist, and the
//! import is idempotent — a second run rewrites nothing and reports every row as skipped.
//!
//! `cargo test -p conveyor-store --test s07_seed -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_store::seed::{SeedError, capability_export, catalogue_export, seed_catalogue};
use uuid::Uuid;

/// First run creates, second run skips exactly those rows; nothing is ever rewritten twice.
#[tokio::test]
async fn the_catalogue_seed_imports_every_exported_row_and_is_idempotent() {
    let db = support::TestDb::fresh().await;
    // The real dev seed, not a fixture: `dev-a` is the tenant `migrate --dev-seed` leaves.
    conveyor_store::seed::dev(&db.owner)
        .await
        .expect("seed the dev tenants");

    let export = catalogue_export().expect("the embedded export parses");
    let capabilities = capability_export().expect("the embedded capability seed parses");
    assert_eq!(export.entries.len(), 39, "the export's catalogue entries");
    assert_eq!(export.card_types.len(), 74, "the export's card types");
    assert_eq!(
        capabilities.capabilities.len(),
        18,
        "the 18 capability rows moved out of the catalogue"
    );
    let exported = (export.entries.len()
        + export.card_types.len()
        + capabilities.capabilities.len()
        + capabilities.relations.len()) as u64;

    let first = seed_catalogue(&db.runtime, Some("dev-a"))
        .await
        .expect("the first run imports the catalogue");
    assert_eq!(first.len(), 1, "one slug named, one tenant seeded");
    let (slug, first_counts) = first.first().expect("one tenant");
    assert_eq!(slug, "dev-a");
    println!("first run: {} {first_counts:?}", slug);
    assert!(
        first_counts.created > 0,
        "the export creates rows: {first_counts:?}"
    );
    assert_eq!(
        first_counts.created + first_counts.updated + first_counts.skipped,
        exported,
        "every exported card type and entry is accounted for exactly once"
    );

    let second = seed_catalogue(&db.runtime, Some("dev-a"))
        .await
        .expect("the second run is a no-op");
    let (slug, second_counts) = second.first().expect("one tenant");
    assert_eq!(slug, "dev-a");
    println!("second run: {} {second_counts:?}", slug);
    assert_eq!(second_counts.created, 0, "nothing is created twice");
    assert_eq!(second_counts.updated, 0, "nothing is rewritten");
    // Every exported row is skipped on the second run — including the one the DEV seed had
    // already written (`board_template/standard`), which the first run therefore skipped too.
    assert_eq!(
        second_counts.skipped, exported,
        "the second run skips exactly the exported rows"
    );
    assert_eq!(
        second_counts.skipped, exported,
        "the importer covers the whole export on a re-run"
    );

    // The CLI's default (no `--tenant`) resolves every active tenant: dev-a is already
    // imported and skips, dev-b is fresh and creates — the counts are per tenant, not global.
    let all = seed_catalogue(&db.runtime, None)
        .await
        .expect("the no-slug run covers every active tenant");
    println!("all-active run: {all:?}");
    let slugs: Vec<&str> = all.iter().map(|(slug, _)| slug.as_str()).collect();
    assert_eq!(
        slugs,
        vec!["dev-a", "dev-b", "kata", "execution"],
        "every active tenant is seeded"
    );
    let dev_a = all[0].1;
    assert_eq!(
        (dev_a.created, dev_a.updated, dev_a.skipped),
        (0, 0, exported),
        "dev-a was already imported"
    );
    let dev_b = all[1].1;
    // dev-b's first run accounts for every exported row. The DEV seed already wrote
    // `board_template/standard`; the import brings that one row to the export's hand-authored
    // content (an UPDATE when they differ, a SKIP when they do not), so the invariant is that
    // the three counts sum to the export — not that `updated` is zero.
    assert_eq!(
        dev_b.created + dev_b.updated + dev_b.skipped,
        exported,
        "dev-b's first import accounts for every exported row"
    );
    assert!(
        dev_b.updated <= 1,
        "at most the dev-seeded board template is rewritten: {dev_b:?}"
    );

    // And the re-run is still idempotent: the one updated row is now identical and skipped.
    let again = seed_catalogue(&db.runtime, Some("dev-b"))
        .await
        .expect("a re-run of dev-b");
    let (_, second_b) = again.first().expect("one tenant");
    assert_eq!(
        (second_b.created, second_b.updated, second_b.skipped),
        (0, 0, exported),
        "a second dev-b run creates and rewrites nothing"
    );
}

/// The matrix is keyed by card types, so both must be stored — and the derived codes present.
#[tokio::test]
async fn the_seeded_project_type_carries_its_matrix_and_the_derived_codes_are_present() {
    let db = support::TestDb::fresh().await;
    conveyor_store::seed::dev(&db.owner)
        .await
        .expect("seed the dev tenants");
    seed_catalogue(&db.runtime, Some("dev-a"))
        .await
        .expect("import the catalogue");

    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = 'dev-a'")
        .fetch_one(&db.super_pool)
        .await
        .expect("dev-a exists");

    let meta: serde_json::Value = sqlx::query_scalar(
        "SELECT meta FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = 'it'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("project_type/it is imported");
    println!(
        "project_type/it meta keys: {:?}",
        meta.as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>())
    );
    assert!(
        meta.get("matrix").is_some_and(serde_json::Value::is_object),
        "meta.matrix is the from-kind -> to-kinds map: {meta}"
    );
    assert!(
        meta.get("card_types")
            .is_some_and(serde_json::Value::is_array),
        "meta.card_types lists the codes the matrix is keyed by: {meta}"
    );

    // The export MUST carry the `standard` board template. `projects.create` and
    // `project_requests.promote` both default to it (S05's DEFAULT_TEMPLATE_CODE), and it
    // used to come only from the development seed — so a tenant seeded from the export alone
    // could not create a project at all. RM ships only `default`, so this row is
    // hand-authored in `seed/catalogue.json`.
    let lanes: serde_json::Value = sqlx::query_scalar(
        "SELECT meta -> 'lists' FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'board_template' AND code = 'standard'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("board_template/standard is imported");
    assert_eq!(
        lanes.as_array().map(Vec::len),
        Some(5),
        "the standard template must list the five lane kinds: {lanes}"
    );

    // The RM export's own codes, in the vocabularies `catalogue_entry` stores.
    for (kind, code) in [("project_type", "it"), ("phase", "design")] {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM catalogue_entry
                  WHERE tenant_id = $1 AND kind = $2 AND code = $3
             )",
        )
        .bind(tenant)
        .bind(kind)
        .bind(code)
        .fetch_one(&db.super_pool)
        .await
        .expect("existence probe");
        assert!(
            present,
            "{kind}/{code} comes from the RM export and must be imported"
        );
    }

    // `card_type` is its own table, not one of the eleven catalogue kinds (S07 §5.4): the
    // matrix's codes live here, which is why the seed imports the types before the entries.
    let card_types: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM card_type
          WHERE tenant_id = $1 AND code IN ('architecture_epic', 'architecture_task')
          ORDER BY code",
    )
    .bind(tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the derived card types");
    println!("derived card types imported: {card_types:?}");
    assert_eq!(card_types, vec!["architecture_epic", "architecture_task"]);
}

/// `--tenant` is typed by hand: an unknown slug must fail naming it, never mean "all".
#[tokio::test]
async fn an_unknown_tenant_slug_is_an_error_that_names_it() {
    let db = support::TestDb::fresh().await;
    let refused = seed_catalogue(&db.runtime, Some("dev-z")).await;
    assert!(
        matches!(&refused, Err(SeedError::UnknownTenant(slug)) if slug == "dev-z"),
        "an unknown slug is refused when it names no tenant: {refused:?}"
    );
    let message = refused.expect_err("the call failed").to_string();
    println!("refusal: {message}");
    assert!(
        message.contains("dev-z"),
        "the error names the slug that failed: {message}"
    );
}
