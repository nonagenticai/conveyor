//! Property (f), registry half: a breaking refresh is refused (`422`, CLI exit 2) and the
//! approved version keeps serving until an explicit approval.
//!
//! The documents name `widgets.list`, which is **not** compiled into this binary, so the
//! test can go through the production path the CLI runs (`ingest_production`: discover
//! against the compiled names, then apply). A document whose operationId collides with a
//! compiled action is refused by that same path, which is the rule that makes the fixture
//! usable rather than a parallel, lenient path.

mod support;

use conveyor_actions::ingest::{self, IngestError};
use conveyor_store::begin_scoped;
use serde_json::{Value, json};

const WIDGETS_V1: &[u8] = include_bytes!("fixtures/external/widgets_v1.json");
const WIDGETS_V2: &[u8] = include_bytes!("fixtures/external/widgets_v2.json");

#[tokio::test]
async fn a_removed_field_is_breaking_while_the_approved_version_keeps_serving() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // 1. The first ingest of a service is approved immediately (nothing to protect).
    let first = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V1, false)
        .await
        .expect("first ingest");
    assert!(!first.has_breaking(), "{first:?}");
    assert_eq!(first.approved, vec![("widgets.list".to_owned(), 1)]);
    assert_eq!(first.exit_code(), None);

    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let approved = conveyor_store::actions::approved_registry(&mut tx)
        .await
        .expect("approved");
    tx.commit().await.expect("commit");
    let widget = approved
        .iter()
        .find(|row| row.name == "widgets.list")
        .expect("widgets.list approved");
    assert_eq!(widget.spec_version, 1);
    let approved_hash = widget.schema_hash.clone();

    // 2. A removed input member is breaking: written UNAPPROVED, and the CLI exits 2.
    let second = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V2, false)
        .await
        .expect("refresh");
    assert!(
        second.has_breaking(),
        "the removal must be breaking: {second:?}"
    );
    assert_eq!(
        second.exit_code(),
        Some(2),
        "a breaking refresh is the CLI's exit-2 path, through ingest_production"
    );
    let (_name, version, changes) = second
        .breaking
        .iter()
        .find(|(name, _, _)| name == "widgets.list")
        .expect("widgets.list is in the breaking set");
    assert_eq!(*version, 2);
    assert!(
        changes
            .iter()
            .any(|line| line.contains("query") && line.contains("removed")),
        "{changes:?}"
    );

    // 3. The approved version still serves: v1, with its original schema hash.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let approved = conveyor_store::actions::approved_registry(&mut tx)
        .await
        .expect("approved");
    let versions = conveyor_store::actions::registry_versions(&mut tx, "widgets.list")
        .await
        .expect("versions");
    tx.rollback().await.expect("rollback");
    let widget = approved
        .iter()
        .find(|row| row.name == "widgets.list")
        .expect("widgets.list still approved");
    assert_eq!(widget.spec_version, 1, "the unapproved v2 must not serve");
    assert_eq!(widget.schema_hash, approved_hash);
    assert_eq!(versions.len(), 2, "both versions are stored");
    assert!(versions[0].approved_at.is_none(), "v2 is unapproved");

    // 4. An explicit approval (the `--approve` path) switches what serves.
    let approved_run =
        ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V2, true)
            .await
            .expect("approving ingest");
    assert!(!approved_run.has_breaking());
    assert_eq!(approved_run.exit_code(), None);
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let approved = conveyor_store::actions::approved_registry(&mut tx)
        .await
        .expect("approved");
    tx.commit().await.expect("commit");
    let widget = approved
        .iter()
        .find(|row| row.name == "widgets.list")
        .expect("approved");
    assert_eq!(widget.spec_version, 2);
}

#[tokio::test]
async fn the_production_path_refuses_an_external_name_that_is_compiled() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // The same widgets document, renamed to a compiled action: the CLI (and only the CLI)
    // is where a caller would hit this, so the production path must refuse it.
    let mut document: Value = serde_json::from_slice(WIDGETS_V1).expect("widgets v1");
    document["paths"]["/widgets/search"]["post"]["operationId"] = json!("members.set_role");
    let bytes = serde_json::to_vec(&document).expect("serialize");

    let error = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", &bytes, false)
        .await
        .expect_err("a compiled name must be refused");
    assert!(
        matches!(&error, IngestError::NameConflict(name) if name == "members.set_role"),
        "{error}"
    );

    // Nothing was written.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let approved = conveyor_store::actions::approved_registry(&mut tx)
        .await
        .expect("approved");
    tx.commit().await.expect("commit");
    assert!(approved.is_empty());
}
