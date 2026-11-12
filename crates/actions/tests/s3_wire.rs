//! S09 §5.2's one unproven leg: **the S3 wire itself** — endpoint, path-style addressing and SigV4.
//!
//! The suite normally runs `object_store`'s in-memory backend (`CONVEYOR_S3_BACKEND=memory`): the same trait,
//! the same key scheme, so put/get/head/delete and the hash check are real. What it cannot prove is the part
//! that only exists on a socket. This test is `#[ignore]`d because it needs a real S3-compatible server:
//!
//! ```text
//! scripts/prove-s3.sh          # starts one, runs this test, tears it down
//! ```
//!
//! ⚠️ It refuses to run against the in-memory backend on purpose: a green result here must mean bytes crossed
//! a socket and were signed, never that a fake agreed with itself.

use bytes::Bytes;
use conveyor_actions::integrations::objects::Objects;
use uuid::Uuid;

fn configured() -> Objects {
    assert_ne!(
        std::env::var("CONVEYOR_S3_BACKEND").as_deref(),
        Ok("memory"),
        "this test proves the S3 wire; the in-memory backend proves nothing about it"
    );
    Objects::from_env().expect(
        "CONVEYOR_S3_ENDPOINT, CONVEYOR_S3_ACCESS_KEY and CONVEYOR_S3_SECRET_KEY must be set",
    )
}

#[tokio::test]
#[ignore = "needs a real S3 endpoint: scripts/prove-s3.sh"]
async fn the_s3_wire_round_trips_over_http_with_sigv4() {
    let objects = configured();
    let key = format!("wire-proof/{}.txt", Uuid::now_v7().simple());
    let body = Bytes::from_static(b"the S3 wire, not the trait");

    objects.put(&key, body.clone()).await.expect("put");
    assert!(objects.head(&key).await.expect("head"), "the object exists");

    let read = objects.get(&key).await.expect("get");
    assert_eq!(read, body, "what came back is what was stored");

    // A second put replaces rather than appends — the key scheme depends on it.
    let replacement = Bytes::from_static(b"replaced");
    objects
        .put(&key, replacement.clone())
        .await
        .expect("overwrite");
    assert_eq!(objects.get(&key).await.expect("get"), replacement);

    objects.delete(&key).await.expect("delete");
    assert!(
        !objects.head(&key).await.expect("head after delete"),
        "a deleted object is gone"
    );
}

// ⚠️ There used to be a second test here asserting that an unconfigured endpoint yields `None`. It returned
// early in BOTH configurations it could run in (the suite sets `CONVEYOR_S3_BACKEND=memory`, and `prove-s3.sh`
// sets the endpoint), so it asserted nothing and read as coverage. The refusal it described is proven where it
// matters — `documents_roundtrip` runs every object operation through `Objects::from_env()` — and a test that
// can only pass is worse than no test.
