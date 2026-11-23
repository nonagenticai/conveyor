//! `storage.put` / `storage.get` — the live job-storage path (T13 finding `v1-p1110-1`).
//!
//! Runs against the in-memory `object_store` backend the suite uses (`CONVEYOR_S3_BACKEND=memory`);
//! the same adapter trait, the tenant prefix and the hash cross the call path. The oversize cap
//! is the adapter's own check and is exercised at the adapter boundary.

mod support;

use base64::Engine as _;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_object_round_trips_through_the_actions_and_a_cross_tenant_name_is_refused() {
    support::set_env("CONVEYOR_S3_BACKEND", "memory");
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let body = base64::engine::general_purpose::STANDARD.encode(b"a job artefact");

    let (status, stored) = running
        .invoke(
            "storage.put",
            "dev:alice",
            json!({"name": "report.txt", "body_base64": body}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "the storage path stores: {stored}");
    let key = stored["output"]["key"].as_str().expect("a key");
    assert!(
        key.starts_with(&format!("tenant/{}/", seeded.tenant.as_uuid())),
        "the key is tenant-prefixed: {key}"
    );
    let sha = stored["output"]["sha256"].as_str().expect("a hash");
    assert_eq!(sha.len(), 64, "the adapter records the content hash");

    let (status, read) = running
        .invoke(
            "storage.get",
            "dev:alice",
            json!({"name": "report.txt"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "the storage path reads: {read}");
    assert_eq!(read["output"]["sha256"].as_str(), Some(sha));
    assert_eq!(
        read["output"]["body_base64"].as_str(),
        Some(body.as_str()),
        "the bytes round-trip"
    );

    let (status, refused) = running
        .invoke(
            "storage.put",
            "dev:alice",
            json!({"name": "../another-tenant/secret", "body_base64": body}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "a cross-tenant name is refused: {refused}");
    assert!(
        refused.to_string().contains("object_name_invalid"),
        "the refusal names the invalid segment: {refused}"
    );

    running.shutdown().await;
}

/// The adapter's own cap is the action's enforcement point: the bytes are refused before the
/// store is touched, and the action maps the error to `413 object_too_large`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_adapter_refuses_an_object_over_the_cap() {
    use bytes::Bytes;
    use conveyor_actions::storage::{JobStorage, MAX_OBJECT_BYTES, StorageError};
    use uuid::Uuid;

    support::set_env("CONVEYOR_S3_BACKEND", "memory");
    let storage = JobStorage::from_env().expect("the in-memory backend is configured");
    let error = storage
        .put(
            Uuid::now_v7(),
            None,
            None,
            "too-big.bin",
            Bytes::from(vec![0u8; MAX_OBJECT_BYTES + 1]),
        )
        .await
        .expect_err("an object over the cap must be refused");
    assert!(matches!(error, StorageError::TooLarge { .. }), "{error}");
}
