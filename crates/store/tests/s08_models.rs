//! S08 Part B store coverage: the seven model tables, publish, and the validate job.
//!
//! One fresh migrated database per test. Everything that writes runs as `conveyor_runtime` in
//! a [`begin_scoped`] transaction; the fixtures are superuser SQL, because the superuser is
//! the positive control that bypasses FORCE RLS.
//!
//! `cargo test -p conveyor-store --test s08_models -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::model::REVISION_CONTENT_MAX_BYTES;
use conveyor_store::begin_scoped;
use conveyor_store::model::{
    ModelPage, ModelPatch, ModelStoreError, NewElement, NewModel, NewRelationship,
    create_element_tx, create_model_tx, create_relationship_tx, delete_element_tx, delete_model_tx,
    enqueue_validate_tx, get_element_tx, get_model_tx, list_elements_tx, list_models_tx,
    load_snapshot_tx, publish_model_tx, update_model_tx,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TABLES: [&str; 7] = [
    "model",
    "model_element",
    "model_relationship",
    "model_validation_run",
    "model_view",
    "view_connection",
    "view_object",
];

fn first_page() -> ModelPage {
    ModelPage::first(50)
}

fn archimate(title: &str) -> NewModel {
    NewModel {
        project_id: None,
        language: "archimate".to_owned(),
        language_version: "3.2".to_owned(),
        title: title.to_owned(),
        description: String::new(),
        meta: json!({}),
    }
}

fn element(model_id: Uuid, kind: &str, name: &str) -> NewElement {
    NewElement {
        model_id,
        kind: kind.to_owned(),
        name: name.to_owned(),
        documentation: String::new(),
        parent_element_id: None,
        meta: json!({}),
    }
}

/// The columns the schema test reads: RLS enabled, forced, and exactly one policy per table.
async fn rls_flags(pool: &PgPool) -> Vec<(String, bool, bool)> {
    sqlx::query_as(
        "SELECT relname, relrowsecurity, relforcerowsecurity
           FROM pg_class WHERE relname = ANY($1) ORDER BY relname",
    )
    .bind(&TABLES[..])
    .fetch_all(pool)
    .await
    .expect("read pg_class")
}

#[tokio::test]
async fn the_seven_model_tables_force_rls_with_one_policy_each() {
    let db = support::TestDb::fresh().await;
    let flags = rls_flags(&db.super_pool).await;
    assert_eq!(flags.len(), 7, "all seven tables must exist");
    for (table, enabled, forced) in &flags {
        assert!(enabled, "{table} must ENABLE ROW LEVEL SECURITY");
        assert!(forced, "{table} must FORCE ROW LEVEL SECURITY");
    }
    let policies: Vec<(String, i64)> = sqlx::query_as(
        "SELECT tablename, count(*) FROM pg_policies WHERE tablename = ANY($1) GROUP BY 1",
    )
    .bind(&TABLES[..])
    .fetch_all(&db.super_pool)
    .await
    .expect("read pg_policies");
    assert_eq!(
        policies.len(),
        7,
        "every table needs a policy: {policies:?}"
    );
    for (table, count) in policies {
        assert_eq!(count, 1, "{table} must carry exactly one policy");
    }
}

#[tokio::test]
async fn a_model_is_invisible_to_another_tenant() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s08m-a").await;
    let b = support::seed_tenant(&db.super_pool, "s08m-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("tenant a");
    let tenant_b = TenantId::from_uuid(b.tenant).expect("tenant b");

    let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope a");
    let model = create_model_tx(&mut tx, tenant_a, &archimate("A's model"))
        .await
        .expect("create in a");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope b");
    let seen = get_model_tx(&mut tx, tenant_b, model.id)
        .await
        .expect("read from b");
    assert!(seen.is_none(), "tenant b must not read tenant a's model");
    let (rows, _) = list_models_tx(&mut tx, tenant_b, &first_page(), None)
        .await
        .expect("list from b");
    assert!(rows.is_empty(), "tenant b must not list tenant a's models");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn update_is_version_guarded_and_a_stale_write_changes_nothing() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-version").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("v1"))
        .await
        .expect("create");
    let bumped = update_model_tx(
        &mut tx,
        tenant,
        model.id,
        model.version,
        &ModelPatch {
            title: Some("v2".to_owned()),
            ..ModelPatch::default()
        },
    )
    .await
    .expect("first update");
    assert_eq!(bumped.version.get(), model.version.get() + 1);

    let stale = update_model_tx(
        &mut tx,
        tenant,
        model.id,
        model.version,
        &ModelPatch {
            title: Some("v3".to_owned()),
            ..ModelPatch::default()
        },
    )
    .await;
    assert!(matches!(
        stale,
        Err(ModelStoreError::VersionMismatch { .. })
    ));
    let still = get_model_tx(&mut tx, tenant, model.id)
        .await
        .expect("read")
        .expect("exists");
    assert_eq!(still.title, "v2", "a stale update writes nothing");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn an_element_with_children_cannot_be_deleted() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-children").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("tree"))
        .await
        .expect("create model");
    let parent = create_element_tx(&mut tx, tenant, &element(model.id, "grouping", "Group"))
        .await
        .expect("create parent");
    let mut child = element(model.id, "node", "Server");
    child.parent_element_id = Some(parent.id);
    create_element_tx(&mut tx, tenant, &child)
        .await
        .expect("create child");

    let refused = delete_element_tx(&mut tx, tenant, parent.id, parent.version).await;
    assert!(
        matches!(refused, Err(ModelStoreError::HasChildren { count: 1 })),
        "{refused:?}"
    );

    // The child can go; then the parent.
    let (rows, _) = list_elements_tx(&mut tx, tenant, model.id, None, &first_page())
        .await
        .expect("list");
    let child_id = rows
        .iter()
        .find(|row| row.id != parent.id)
        .expect("the child is listed")
        .id;
    let child = get_element_tx(&mut tx, tenant, child_id)
        .await
        .expect("read child")
        .expect("child exists");
    delete_element_tx(&mut tx, tenant, child.id, child.version)
        .await
        .expect("delete child");
    let parent = get_element_tx(&mut tx, tenant, parent.id)
        .await
        .expect("read parent")
        .expect("parent exists");
    delete_element_tx(&mut tx, tenant, parent.id, parent.version)
        .await
        .expect("delete parent once childless");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn publish_freezes_one_revision_and_supersedes_the_previous() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-publish").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("publishable"))
        .await
        .expect("create model");
    let api = create_element_tx(
        &mut tx,
        tenant,
        &element(model.id, "application_component", "API"),
    )
    .await
    .expect("create element");
    let db_node = create_element_tx(&mut tx, tenant, &element(model.id, "node", "Server"))
        .await
        .expect("create element");
    create_relationship_tx(
        &mut tx,
        tenant,
        &NewRelationship {
            model_id: model.id,
            kind: "serving".to_owned(),
            source_id: api.id,
            target_id: db_node.id,
            meta: json!({}),
        },
    )
    .await
    .expect("create relationship");

    let first = publish_model_tx(&mut tx, tenant, model.id, model.version, Some(seed.user))
        .await
        .expect("first publish");
    assert_eq!(first.seq, 1);
    assert_eq!(first.model.head_revision, 1);
    assert_eq!(first.model.published_revision_id, Some(first.revision_id));
    assert_eq!(first.content_hash.len(), 64, "content_hash is sha-256 hex");

    // The published snapshot has to contain what was frozen, not what is there later.
    let snapshot = load_snapshot_tx(&mut tx, tenant, model.id)
        .await
        .expect("load snapshot");
    assert_eq!(snapshot.elements.len(), 2);
    assert_eq!(snapshot.relationships.len(), 1);
    assert_eq!(snapshot.content_hash(), first.content_hash);

    // A second publish supersedes the first revision and advances `head_revision`.
    let second = publish_model_tx(
        &mut tx,
        tenant,
        model.id,
        first.model.version,
        Some(seed.user),
    )
    .await
    .expect("second publish");
    assert_eq!(second.seq, 2);
    assert_ne!(second.revision_id, first.revision_id);

    let statuses: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2
          ORDER BY seq",
    )
    .bind(seed.tenant)
    .bind(model.id)
    .fetch_all(&mut *tx)
    .await
    .expect("read revisions");
    assert_eq!(
        statuses,
        vec![
            (first.revision_id, "superseded".to_owned()),
            (second.revision_id, "published".to_owned()),
        ]
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn the_revision_cap_is_a_refusal_not_a_truncation() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-cap").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("big"))
        .await
        .expect("create model");
    // ~1.6 KiB of documentation per element; 200 of them clear the 256 KiB cap.
    let filler = "x".repeat(1600);
    for index in 0..200 {
        let mut element = element(model.id, "node", &format!("node-{index:03}"));
        element.documentation = filler.clone();
        create_element_tx(&mut tx, tenant, &element)
            .await
            .expect("create element");
    }
    let snapshot = load_snapshot_tx(&mut tx, tenant, model.id)
        .await
        .expect("load snapshot");
    assert!(
        snapshot.content_len() > REVISION_CONTENT_MAX_BYTES,
        "the fixture must actually exceed the cap: {} bytes",
        snapshot.content_len()
    );

    let refused = publish_model_tx(&mut tx, tenant, model.id, model.version, None).await;
    assert!(
        matches!(
            refused,
            Err(ModelStoreError::RevisionTooLarge { bytes }) if bytes > REVISION_CONTENT_MAX_BYTES
        ),
        "{refused:?}"
    );
    let still = get_model_tx(&mut tx, tenant, model.id)
        .await
        .expect("read")
        .expect("exists");
    assert_eq!(
        still.head_revision, 0,
        "a refused publish writes no revision"
    );
    assert_eq!(still.published_revision_id, None);
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn the_validate_job_is_deduped_on_the_content_hash() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-validate").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("validated"))
        .await
        .expect("create model");
    let snapshot = load_snapshot_tx(&mut tx, tenant, model.id)
        .await
        .expect("load snapshot");
    let hash = snapshot.content_hash();

    let first = enqueue_validate_tx(&mut tx, tenant, model.id, &hash)
        .await
        .expect("enqueue");
    assert!(first.is_some(), "the first request writes a job");
    let second = enqueue_validate_tx(&mut tx, tenant, model.id, &hash)
        .await
        .expect("enqueue again");
    assert!(
        second.is_none(),
        "an identical live job is reused, not duplicated"
    );

    // A different graph is a different key, so it gets its own job.
    let third = enqueue_validate_tx(&mut tx, tenant, model.id, "deadbeef")
        .await
        .expect("enqueue a different hash");
    assert!(third.is_some());

    let kinds: Vec<(String,)> =
        sqlx::query_as("SELECT kind FROM job WHERE tenant_id = $1 ORDER BY created_at")
            .bind(seed.tenant)
            .fetch_all(&mut *tx)
            .await
            .expect("read jobs");
    assert_eq!(kinds.len(), 2, "two distinct keys, two jobs: {kinds:?}");
    assert!(kinds.iter().all(|(kind,)| kind == "model.validate"));
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn deleting_a_model_removes_its_whole_graph_and_revisions() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-delete").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("doomed"))
        .await
        .expect("create model");
    create_element_tx(&mut tx, tenant, &element(model.id, "node", "Server"))
        .await
        .expect("create element");
    let published = publish_model_tx(&mut tx, tenant, model.id, model.version, None)
        .await
        .expect("publish");

    delete_model_tx(&mut tx, tenant, model.id, published.model.version)
        .await
        .expect("delete the model");

    for (table, column) in [
        ("model", "id"),
        ("model_element", "model_id"),
        ("model_relationship", "model_id"),
        ("model_view", "model_id"),
        ("model_validation_run", "model_id"),
    ] {
        let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1 AND {column} = $2");
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(seed.tenant)
            .bind(model.id)
            .fetch_one(&mut *tx)
            .await
            .expect("count rows");
        assert_eq!(count, 0, "{table} must be empty after the model is deleted");
    }
    let revisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2",
    )
    .bind(seed.tenant)
    .bind(model.id)
    .fetch_one(&mut *tx)
    .await
    .expect("count revisions");
    assert_eq!(revisions, 0, "the model's history goes with it");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn a_relationship_cannot_name_the_same_element_twice() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-self").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let model = create_model_tx(&mut tx, tenant, &archimate("self"))
        .await
        .expect("create model");
    let node = create_element_tx(&mut tx, tenant, &element(model.id, "node", "Server"))
        .await
        .expect("create element");
    let refused = create_relationship_tx(
        &mut tx,
        tenant,
        &NewRelationship {
            model_id: model.id,
            kind: "association".to_owned(),
            source_id: node.id,
            target_id: node.id,
            meta: json!({}),
        },
    )
    .await;
    assert!(
        matches!(refused, Err(ModelStoreError::Database(_))),
        "0009's CHECK must refuse it: {refused:?}"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn a_first_page_is_empty_for_a_fresh_tenant() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08m-empty").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let (rows, next) = list_models_tx(&mut tx, tenant, &first_page(), None)
        .await
        .expect("list");
    assert!(rows.is_empty());
    assert_eq!(next, None, "an empty page has no cursor");
    tx.rollback().await.expect("rollback");
}
