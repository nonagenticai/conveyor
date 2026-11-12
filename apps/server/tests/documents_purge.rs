//! `document.purge`: the half of a delete that removes the bytes.
//!
//! `documents.delete` writes a tombstone and enqueues this job; the job deletes the object and clears
//! `object_key`. Asserted here, not in the API test, because the worker is this crate's: the action test can only
//! see that the tombstone kept its key, which is exactly the state this job consumes.
//!
//! The object backend is `object_store`'s in-memory one (`CONVEYOR_S3_BACKEND=memory`) — the same trait and key
//! scheme as S3, without a MinIO image this box cannot pull. What is NOT covered here is the S3 wire; that is
//! recorded as UNVERIFIED in the slice plan.

mod common;

use conveyor_actions::integrations::objects::{Objects, key_for};
use conveyor_domain::TenantId;
use conveyor_server::worker;
use serde_json::json;
use sha2::{Digest, Sha256};

/// The worker's config: the same defaults `prune.rs` uses.
fn local_config() -> conveyor_api::Config {
    conveyor_api::Config::from_lookup(|_| None).expect("a default config")
}
use uuid::Uuid;

/// Configure the two S09 services for this test process. Edition 2024 made `set_var` unsafe; the test binary is
/// single-threaded until the runtime starts, and this runs before the first call that reads them.
fn init_env() {
    unsafe {
        std::env::set_var(
            "CONVEYOR_SECRET_KEK",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        );
        if std::env::var("CONVEYOR_S3_ENDPOINT").is_err() {
            std::env::set_var("CONVEYOR_S3_BACKEND", "memory");
        }
    }
}

async fn seed_deleted_document(db: &common::TestDb) -> (TenantId, Uuid, String) {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('purge', 'Purge') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let id = Uuid::now_v7();
    let bytes = b"bytes that must not outlive the row".to_vec();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let key = key_for(tenant_id, id, &hash);
    Objects::from_env()
        .expect("the test object backend")
        .put(&key, bytes.into())
        .await
        .expect("store the object");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, kind, title, status, object_key, content_hash, meta)
         VALUES ($1, $2, 'document', 'Purge me', 'deleted', $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(&key)
    .bind(&hash)
    .bind(json!({"content_type": "text/plain", "size_bytes": 36}))
    .execute(&db.super_pool)
    .await
    .expect("seed a deleted document");
    (tenant, id, key)
}

#[tokio::test]
async fn the_purge_removes_the_object_and_clears_the_key() {
    init_env();
    let db = common::TestDb::fresh().await;
    let (tenant, document, key) = seed_deleted_document(&db).await;
    let objects = Objects::from_env().expect("the test object backend");
    assert!(
        objects.head(&key).await.expect("head"),
        "the object starts there"
    );

    worker::run_kind(
        &db.runtime,
        &local_config(),
        None,
        tenant,
        "document.purge",
        &json!({"document_id": document}),
    )
    .await
    .expect("document.purge");

    assert!(
        !objects.head(&key).await.expect("head"),
        "the object is gone from the store"
    );
    let leftover: Option<String> =
        sqlx::query_scalar("SELECT object_key FROM document WHERE id = $1")
            .bind(document)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the row");
    assert!(leftover.is_none(), "the pointer is cleared");

    // The row survives as a tombstone: the id, the hash and the audit trail are still facts.
    let (status, hash): (String, Option<String>) =
        sqlx::query_as("SELECT status, content_hash FROM document WHERE id = $1")
            .bind(document)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the tombstone");
    assert_eq!(status, "deleted");
    assert!(hash.is_some(), "the hash outlives the bytes");

    // And it announced exactly one update.
    let events: Vec<(String,)> =
        sqlx::query_as("SELECT type FROM event WHERE tenant_id = $1 AND aggregate_id = $2")
            .bind(tenant.as_uuid())
            .bind(document)
            .fetch_all(&db.super_pool)
            .await
            .expect("read the outbox");
    assert_eq!(
        events,
        vec![("document.updated".to_owned(),)],
        "the purge announces one document.updated"
    );
}

#[tokio::test]
async fn a_purge_of_a_live_document_removes_nothing() {
    init_env();
    let db = common::TestDb::fresh().await;
    let (tenant, document, key) = seed_deleted_document(&db).await;
    // Flip the row back to active: the job must refuse to remove live bytes.
    sqlx::query("UPDATE document SET status = 'active' WHERE id = $1")
        .bind(document)
        .execute(&db.super_pool)
        .await
        .expect("reactivate");

    worker::run_kind(
        &db.runtime,
        &local_config(),
        None,
        tenant,
        "document.purge",
        &json!({"document_id": document}),
    )
    .await
    .expect("document.purge runs without error");

    let objects = Objects::from_env().expect("the test object backend");
    assert!(
        objects.head(&key).await.expect("head"),
        "an active document keeps its bytes even if a purge job names it"
    );
    let kept: Option<String> = sqlx::query_scalar("SELECT object_key FROM document WHERE id = $1")
        .bind(document)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the row");
    assert!(kept.is_some(), "the pointer survives");
}
