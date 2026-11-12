//! S07 §5.3 — the revision model over `governed_asset`.
//!
//! - create → update → transition write exactly three revisions, `seq` 1, 2, 3, and the
//!   asset ends at version 3;
//! - a stale `expected_version` writes no revision at all;
//! - two assets of different kinds may share a key (the unique key is `(tenant, kind, key)`),
//!   and the same kind twice may not;
//! - tenant B cannot read tenant A's assets or revisions through any store path.
//!
//! `cargo test -p conveyor-store --test s07_governance -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::compare::{content_hash, diff};
use conveyor_domain::{AssetKind, AssetStatus, TenantId, Version};
use conveyor_store::begin_scoped;
use conveyor_store::governance::{
    AssetFilter, AssetPatch, GovernanceStoreError, NewAsset, PageRequest, get_asset_tx,
    insert_asset_tx, list_assets_tx, snapshot, transition_asset_tx, update_asset_tx,
};
use conveyor_store::revision::{get_revision_by_seq_tx, get_revision_tx, list_revisions_tx};
use serde_json::json;
use uuid::Uuid;

fn requirement(key: &str, title: &str, project_id: Option<Uuid>) -> NewAsset {
    let mut input = NewAsset::new(AssetKind::Requirement, key, title);
    input.project_id = project_id;
    input.body = "Every mutating request is idempotent.".to_owned();
    input
}

#[tokio::test]
async fn create_update_transition_write_exactly_three_revisions() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "rev3").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let asset = insert_asset_tx(
            &mut tx,
            tenant,
            &requirement(
                "req-three",
                "A requirement title long enough",
                Some(seed.project),
            ),
        )
        .await
        .expect("insert the asset");
        tx.commit().await.expect("commit");
        asset
    };
    assert_eq!(created.version.get(), 1, "a created asset is version 1");

    let patch = AssetPatch {
        title: Some("An updated requirement title".to_owned()),
        acceptance: Some(json!([{
            "id": "a1",
            "text": "Given a board, when a card moves, then it appears"
        }])),
        ..AssetPatch::default()
    };
    let updated = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = update_asset_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &patch,
            Some(seed.user),
        )
        .await
        .expect("apply the edit");
        // The revision the edit wrote is the full post-mutation snapshot, hashed in Rust.
        let revision = get_revision_by_seq_tx(
            &mut tx,
            tenant,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            created.id,
            2,
        )
        .await
        .expect("read revision 2")
        .expect("the edit wrote revision 2");
        assert_eq!(revision.seq, 2);
        assert_eq!(revision.label, "updated");
        assert_eq!(revision.content, snapshot(&record));
        assert_eq!(revision.content_hash, content_hash(&snapshot(&record)));
        assert_eq!(
            revision.entity_type,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE
        );
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(updated.version.get(), 2);
    assert_eq!(updated.title, "An updated requirement title");
    assert_eq!(updated.body, "Every mutating request is idempotent.");
    assert_eq!(
        updated.acceptance,
        json!([{"id": "a1", "text": "Given a board, when a card moves, then it appears"}])
    );

    let transitioned = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let asset = transition_asset_tx(
            &mut tx,
            tenant,
            created.id,
            AssetStatus::InReview,
            Some(seed.user),
        )
        .await
        .expect("move the asset to in_review");
        tx.commit().await.expect("commit");
        asset
    };
    assert_eq!(transitioned.version.get(), 3);

    let (revisions, read_back, first, last) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let revisions = list_revisions_tx(
            &mut tx,
            tenant,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            created.id,
        )
        .await
        .expect("list the asset's revisions");
        let read_back = get_asset_tx(&mut tx, tenant, created.id)
            .await
            .expect("read the asset")
            .expect("it exists");
        let first = get_revision_by_seq_tx(
            &mut tx,
            tenant,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            created.id,
            1,
        )
        .await
        .expect("read revision 1")
        .expect("revision 1 resolves");
        let last = get_revision_by_seq_tx(
            &mut tx,
            tenant,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            created.id,
            3,
        )
        .await
        .expect("read revision 3")
        .expect("revision 3 resolves");
        tx.rollback().await.expect("roll back the read");
        (revisions, read_back, first, last)
    };

    // `list_revisions_tx` is newest-first (S03's contract); oldest-first is 1, 2, 3.
    let seqs: Vec<i32> = revisions
        .iter()
        .rev()
        .map(|revision| revision.seq)
        .collect();
    let labels: Vec<&str> = revisions
        .iter()
        .rev()
        .map(|revision| revision.label.as_str())
        .collect();
    println!("revision seqs (oldest first): {seqs:?}, labels {labels:?}");
    assert_eq!(seqs, vec![1, 2, 3], "exactly one revision per mutation");
    assert_eq!(
        labels,
        vec!["created", "updated", "status:in_review"],
        "the label vocabulary is S07 §5.3's"
    );
    assert_eq!(read_back.version.get(), 3, "the version is the last seq");
    for revision in &revisions {
        assert_eq!(
            revision.entity_type,
            conveyor_store::governance::GOVERNED_ASSET_ENTITY_TYPE
        );
        assert_eq!(revision.content_hash.len(), 64, "SHA-256 hex");
        assert!(
            revision
                .content_hash
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "lowercase hex: {}",
            revision.content_hash
        );
        let content = revision
            .content
            .as_object()
            .expect("the snapshot is an object");
        assert_eq!(content.len(), 14, "the full 14-key snapshot: {content:?}");
    }
    assert_eq!(first.seq, 1);
    assert_eq!(first.content["status"], json!("draft"));
    assert_eq!(last.seq, 3);
    assert_eq!(last.content["version"], json!(3));
    assert_eq!(last.content["status"], json!("in_review"));
    assert_eq!(last.content["title"], json!("An updated requirement title"));

    // `compare 1..3`: both documents are the same 14-key snapshot, so the only `added`
    // element is the criterion the edit introduced — matched by id (`acceptance.a1`), never
    // a whole-array replacement — and the status change is reported as a changed field.
    let change = diff(&first.content, &last.content);
    assert_eq!(change.added.len(), 1, "{change:?}");
    assert_eq!(change.added[0].field, "acceptance.a1");
    assert!(change.removed.is_empty(), "{change:?}");
    assert!(
        !change
            .changed
            .iter()
            .any(|entry| entry.field == "acceptance"),
        "the acceptance array was matched by id: {change:?}"
    );
    let status = change
        .changed
        .iter()
        .find(|entry| entry.field == "status")
        .expect("rev 1 vs rev 3 reports the status change");
    assert_eq!(status.from, Some(json!("draft")));
    assert_eq!(status.to, Some(json!("in_review")));
}

#[tokio::test]
async fn a_stale_expected_version_writes_no_revision() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "stale").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let asset = insert_asset_tx(
            &mut tx,
            tenant,
            &requirement("req-stale-edit", "A requirement title long enough", None),
        )
        .await
        .expect("insert the asset");
        tx.commit().await.expect("commit");
        asset
    };

    // One successful edit: version 1 → 2, revision 2.
    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = update_asset_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &AssetPatch {
                body: Some("A different body.".to_owned()),
                ..AssetPatch::default()
            },
            Some(seed.user),
        )
        .await
        .expect("the first edit applies");
        assert_eq!(record.version.get(), 2);
        tx.commit().await.expect("commit");
    }

    // The same precondition again is now stale, and writes nothing.
    let refused = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let refused = update_asset_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &AssetPatch {
                title: Some("This title must never be stored".to_owned()),
                ..AssetPatch::default()
            },
            Some(seed.user),
        )
        .await;
        tx.rollback().await.expect("roll back the refused edit");
        refused
    };
    match refused {
        Err(GovernanceStoreError::VersionMismatch { current }) => {
            assert_eq!(current.get(), 2, "the mismatch reports the stored version");
        }
        other => panic!("a stale precondition must be refused, got {other:?}"),
    }

    let (seqs, record) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let seqs: Vec<i32> = sqlx::query_scalar(
            "SELECT seq FROM revision WHERE tenant_id = $1 AND entity_id = $2 ORDER BY seq",
        )
        .bind(seed.tenant)
        .bind(created.id)
        .fetch_all(&mut *tx)
        .await
        .expect("read every revision of the asset");
        let record = conveyor_store::governance::get_asset_tx(&mut tx, tenant, created.id)
            .await
            .expect("read the asset")
            .expect("it exists");
        tx.rollback().await.expect("roll back the read");
        (seqs, record)
    };
    println!(
        "after the stale edit: seqs {seqs:?}, version {}",
        record.version
    );
    assert_eq!(seqs, vec![1, 2], "the refused edit wrote no revision");
    assert_eq!(record.version.get(), 2);
    assert_eq!(
        record.title, created.title,
        "the refused title was not stored"
    );
}

#[tokio::test]
async fn the_unique_key_is_scoped_by_kind_and_two_kinds_share_a_key() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "kinds").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let (requirement_asset, adr) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let requirement_asset = insert_asset_tx(
            &mut tx,
            tenant,
            &requirement("shared-key", "A requirement title long enough", None),
        )
        .await
        .expect("the requirement takes the key");
        let adr = insert_asset_tx(
            &mut tx,
            tenant,
            &NewAsset::new(
                AssetKind::Adr,
                "shared-key",
                "An architecture decision title",
            ),
        )
        .await
        .expect("an ADR of another kind may hold the same key");
        tx.commit().await.expect("commit");
        (requirement_asset, adr)
    };
    assert_ne!(requirement_asset.id, adr.id);
    assert_eq!(requirement_asset.key, adr.key);
    assert_ne!(requirement_asset.kind, adr.kind);

    // Positive control: the constraint exists, so the same kind twice is refused.
    let duplicate = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let duplicate = insert_asset_tx(
            &mut tx,
            tenant,
            &requirement("shared-key", "Another requirement title", None),
        )
        .await;
        tx.rollback().await.expect("roll back the duplicate");
        duplicate
    };
    assert!(
        matches!(
            &duplicate,
            Err(GovernanceStoreError::Database(error))
                if error
                    .as_database_error()
                    .is_some_and(|database| database.is_unique_violation())
        ),
        "(tenant, kind, key) is unique: {duplicate:?}"
    );

    // Filters and the keyset cursor over the same table.
    let (page_one, cursor) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let page = list_assets_tx(
            &mut tx,
            tenant,
            &AssetFilter {
                kind: Some(AssetKind::Requirement),
                ..AssetFilter::default()
            },
            &PageRequest::new(1),
        )
        .await
        .expect("first page");
        tx.rollback().await.expect("roll back the read");
        page
    };
    assert_eq!(page_one.len(), 1);
    assert!(
        page_one
            .iter()
            .all(|record| record.kind == AssetKind::Requirement),
        "the kind filter excludes the ADR"
    );
    let cursor = cursor.expect("a second requirement exists, so there is another page");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let (page_two, next) = list_assets_tx(
        &mut tx,
        tenant,
        &AssetFilter {
            kind: Some(AssetKind::Requirement),
            ..AssetFilter::default()
        },
        &PageRequest {
            cursor: Some(cursor),
            limit: 1,
        },
    )
    .await
    .expect("second page");
    let bad_cursor = list_assets_tx(
        &mut tx,
        tenant,
        &AssetFilter::default(),
        &PageRequest {
            cursor: Some("not a cursor".to_owned()),
            limit: 1,
        },
    )
    .await;
    tx.rollback().await.expect("roll back");
    assert_eq!(page_two.len(), 1);
    assert_ne!(page_two[0].id, page_one[0].id);
    assert!(next.is_none(), "two requirements are exactly one page");
    assert!(
        matches!(bad_cursor, Err(GovernanceStoreError::BadCursor)),
        "a malformed cursor is refused: {bad_cursor:?}"
    );
}

#[tokio::test]
async fn tenant_b_cannot_read_tenant_a_assets_or_revisions() {
    let db = support::TestDb::fresh().await;
    let alpha = support::seed_tenant(&db.super_pool, "alpha").await;
    let beta = support::seed_tenant(&db.super_pool, "beta").await;
    let tenant_a = TenantId::from_uuid(alpha.tenant).expect("non-nil");
    let tenant_b = TenantId::from_uuid(beta.tenant).expect("non-nil");

    let revision_a: Uuid = sqlx::query_scalar(
        "SELECT id FROM revision WHERE tenant_id = $1 AND entity_id = $2 ORDER BY seq LIMIT 1",
    )
    .bind(alpha.tenant)
    .bind(alpha.asset)
    .fetch_one(&db.super_pool)
    .await
    .expect("the seed wrote one revision");

    // Tenant B, scoped: every path resolves tenant A's ids to nothing.
    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope");
    assert!(
        get_asset_tx(&mut tx, tenant_b, alpha.asset)
            .await
            .expect("read the asset")
            .is_none(),
        "another tenant's asset is invisible"
    );
    assert!(
        get_revision_tx(&mut tx, tenant_b, revision_a)
            .await
            .expect("read the revision")
            .is_none(),
        "another tenant's revision is invisible by id"
    );
    assert!(
        get_revision_by_seq_tx(&mut tx, tenant_b, "requirement", alpha.asset, 1)
            .await
            .expect("read the revision by seq")
            .is_none(),
        "another tenant's revision is invisible by seq"
    );
    let (rows, next) = list_assets_tx(
        &mut tx,
        tenant_b,
        &AssetFilter::default(),
        &PageRequest::new(50),
    )
    .await
    .expect("list tenant B's assets");
    assert!(next.is_none());
    assert!(rows.iter().all(|record| record.id != alpha.asset));
    assert_eq!(rows.len(), 1, "tenant B sees exactly its own seeded asset");
    tx.rollback()
        .await
        .expect("roll back the cross-tenant read");

    // Positive control: tenant A sees the same rows it owns.
    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope");
    assert!(
        get_asset_tx(&mut tx, tenant_a, alpha.asset)
            .await
            .expect("read the asset")
            .is_some()
    );
    assert!(
        get_revision_tx(&mut tx, tenant_a, revision_a)
            .await
            .expect("read the revision")
            .is_some()
    );
    let (rows, _) = list_assets_tx(
        &mut tx,
        tenant_a,
        &AssetFilter::default(),
        &PageRequest::new(50),
    )
    .await
    .expect("list tenant A's assets");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, alpha.asset);
    tx.rollback().await.expect("roll back");
}
