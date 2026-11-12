#![forbid(unsafe_code)]
//! S13's workflow store: the fork counter, the publish freeze and the two database rules.
//!
//! The properties, each a way this could go wrong silently:
//!
//! 1. **A key is versioned, not replaced** — reusing a key inserts `max(version) + 1`; the first
//!    version is 1.
//! 2. **Publish retires the previous published row first** — `uq_workflow_published` is immediate,
//!    so a publish that forgot the retire step is a unique violation, not a silent two-published
//!    state; the previous row's revision is marked `superseded`.
//! 3. **`If-Match` is the row's own counter** — a stale `expected` is refused, and only a draft can
//!    be edited or published.
//! 4. **The 256 KiB ceiling is the table's CHECK** — a hand-written INSERT is refused by name.
//! 5. **RLS is enabled AND forced** — the schema owner is subject to the tenant policy too.
//!
//! `cargo test -p conveyor-store --test workflow_publish -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::compare::content_hash;
use conveyor_domain::{TenantId, Version};
use conveyor_store::begin_scoped;
use conveyor_store::workflow::workflow::{
    NewWorkflow, WorkflowPatch, create_draft_tx, get_by_id_tx, list_tx, publish_tx, retire_tx,
    update_draft_tx,
};
use conveyor_store::workflow::{MAX_DEFINITION_BYTES, WorkflowStoreError};
use serde_json::json;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

fn new_workflow(key: &str) -> NewWorkflow {
    NewWorkflow {
        key: key.to_owned(),
        title: format!("workflow {key}"),
        definition: Some(json!({
            "schema": 1,
            "nodes": [{"id": "start", "kind": "action", "action": "cards.get"}],
            "edges": []
        })),
    }
}

#[tokio::test]
async fn a_key_is_versioned_never_replaced() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = tenant(&seed);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let v1 = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create v1");
    let v2 = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create v2");
    tx.commit().await.expect("commit");

    assert_eq!(v1.version, 1, "a new key starts at v1");
    assert_eq!(v2.version, 2, "a reused key forks max(version) + 1");
    assert_eq!(v1.key, v2.key);
    assert_ne!(v1.id, v2.id, "a fork is a new row");
    assert_eq!(v1.status, "draft");
}

#[tokio::test]
async fn publish_freezes_a_revision_and_retires_the_previous_published_row() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = tenant(&seed);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let v1 = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create v1");
    let first = publish_tx(&mut tx, tenant, v1.id, None, Some(seed.user))
        .await
        .expect("publish v1");
    tx.commit().await.expect("commit");

    assert_eq!(first.workflow.status, "published");
    assert_eq!(first.seq, 1);
    assert_eq!(
        first.content_hash,
        content_hash(&first.workflow.definition),
        "the frozen hash is the canonical JSON of the document"
    );
    assert_eq!(
        first.workflow.published_revision_id,
        Some(first.revision_id),
        "the row points at its frozen revision"
    );

    // A second version publishes: the first leaves `published` BEFORE the second enters it.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let v2 = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create v2");
    let second = publish_tx(&mut tx, tenant, v2.id, None, Some(seed.user))
        .await
        .expect("publish v2");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let retired = get_by_id_tx(&mut tx, tenant, v1.id).await.expect("read v1");
    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workflow WHERE tenant_id = $1 AND key = 'cap.x' AND status = 'published'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("count published");
    let first_revision_status: String =
        sqlx::query_scalar("SELECT status FROM revision WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(first.revision_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read v1's revision");
    tx.commit().await.expect("commit");

    assert_eq!(
        retired.status, "retired",
        "the previous published row is retired"
    );
    assert_eq!(published, 1, "exactly one published row per key");
    assert_eq!(second.workflow.status, "published");
    assert_eq!(second.seq, 2);
    assert_eq!(
        first_revision_status, "superseded",
        "a replaced revision is superseded, not left `published` beside its successor"
    );
}

#[tokio::test]
async fn if_match_is_the_rows_own_counter_and_only_a_draft_is_editable() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = tenant(&seed);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let v1 = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create v1");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let stale = update_draft_tx(
        &mut tx,
        tenant,
        v1.id,
        WorkflowPatch {
            expected: Some(Version::new(99).expect("non-zero")),
            title: Some("renamed"),
            definition: None,
        },
    )
    .await;
    assert!(
        matches!(stale, Err(WorkflowStoreError::VersionMismatch { .. })),
        "a stale If-Match is a version mismatch, got {stale:?}"
    );
    tx.rollback().await.expect("rollback");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let edited = update_draft_tx(
        &mut tx,
        tenant,
        v1.id,
        WorkflowPatch {
            expected: Some(Version::new(1).expect("non-zero")),
            title: Some("renamed"),
            definition: None,
        },
    )
    .await
    .expect("edit the draft");
    assert_eq!(edited.version, 2, "an edit increments the row's counter");
    assert_eq!(edited.title, "renamed");
    let published = publish_tx(&mut tx, tenant, v1.id, None, None)
        .await
        .expect("publish");
    assert_eq!(published.seq, 2, "publish freezes at the edited version");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = update_draft_tx(
        &mut tx,
        tenant,
        v1.id,
        WorkflowPatch {
            expected: None,
            title: Some("too late"),
            definition: None,
        },
    )
    .await;
    assert!(
        matches!(refused, Err(WorkflowStoreError::NotDraft(_))),
        "a published row is not editable, got {refused:?}"
    );
    let retire_draft = create_draft_tx(&mut tx, tenant, new_workflow("cap.y"))
        .await
        .expect("create a draft");
    let refused_retire = retire_tx(&mut tx, tenant, retire_draft.id, None).await;
    assert!(
        matches!(refused_retire, Err(WorkflowStoreError::NotPublished)),
        "a draft cannot be retired, got {refused_retire:?}"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn the_definition_ceiling_is_the_tables_check() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let tenant = tenant(&seed);

    let oversized = json!({
        "schema": 1,
        "nodes": [],
        "edges": [],
        "pad": "x".repeat(MAX_DEFINITION_BYTES + 1024),
    });

    // The Rust path refuses first, with the byte count.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = create_draft_tx(
        &mut tx,
        tenant,
        NewWorkflow {
            key: "cap.big".to_owned(),
            title: "too big".to_owned(),
            definition: Some(oversized.clone()),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(WorkflowStoreError::DefinitionTooLarge { .. })),
        "the store refuses an oversized document, got {refused:?}"
    );
    tx.rollback().await.expect("rollback");

    // The table's CHECK refuses the same document even when the Rust path is bypassed.
    let error = sqlx::query(
        "INSERT INTO workflow (tenant_id, key, title, definition) VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant.as_uuid())
    .bind("cap.raw")
    .bind("hand-written")
    .bind(&oversized)
    .execute(&db.super_pool)
    .await
    .expect_err("the CHECK must refuse an oversized definition");
    let constraint = error
        .as_database_error()
        .and_then(|error| error.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("workflow_definition_size"),
        "the refusing constraint is named, got {constraint:?} ({error})"
    );
}

#[tokio::test]
async fn workflow_is_rls_enabled_and_forced_and_a_second_tenant_sees_nothing() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "dev-a").await;
    let other = support::seed_tenant(&db.super_pool, "dev-b").await;
    let tenant = tenant(&seed);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let created = create_draft_tx(&mut tx, tenant, new_workflow("cap.x"))
        .await
        .expect("create");
    tx.commit().await.expect("commit");

    let (enabled, forced): (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE relname = 'workflow'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("read pg_class");
    assert!(enabled, "RLS must be ENABLED");
    assert!(
        forced,
        "RLS must be FORCED, or the owner reads every tenant"
    );

    let other_tenant = TenantId::from_uuid(other.tenant).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, other_tenant)
        .await
        .expect("scope");
    let hidden = get_by_id_tx(&mut tx, other_tenant, created.id).await;
    assert!(
        matches!(hidden, Err(WorkflowStoreError::NotFound)),
        "another tenant's workflow is not visible, got {hidden:?}"
    );
    let page = list_tx(&mut tx, other_tenant, None, None, 10)
        .await
        .expect("list");
    assert!(page.is_empty(), "another tenant's list is empty");
    tx.rollback().await.expect("rollback");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let page = list_tx(&mut tx, tenant, Some("draft"), None, 10)
        .await
        .expect("list");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, created.id);
    tx.commit().await.expect("commit");
}
