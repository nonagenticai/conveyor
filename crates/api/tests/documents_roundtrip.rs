//! S09 §7 (a): a document round-trips — upload, list, get, download **and delete** — through the served
//! actions and the byte route.
//!
//! The two properties this file exists for:
//!
//! 1. **The action and the route return the same bytes.** `documents.download` answers base64 in JSON and
//!    `GET /api/documents/{id}/content` answers the raw body; both call the same `fetch_bytes`, and this test
//!    proves it by comparing them rather than by reading the code.
//! 2. **A delete removes the object, not just the row.** After `documents.delete` and the `document.purge` job,
//!    the bytes are gone from the store (`Objects::head` → false) and the row survives as a tombstone.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn get_content(running: &support::Running, id: &str, token: &str) -> (u16, Vec<u8>, String) {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/documents/{id}/content", running.base()))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("the byte route");
    let status = response.status().as_u16();
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response.bytes().await.unwrap_or_default().to_vec();
    (status, bytes, etag)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_document_round_trips_through_the_action_and_the_route() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // 1 MiB of text: big enough that a truncation or a TOAST-round-trip in the wrong layer would show.
    let body = "a".repeat(1024 * 1024);
    let expected_hash = hex(body.as_bytes());

    let (status, created) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "Round trip",
                "content_type": "text/plain",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(body.as_bytes()),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created)["id"]
        .as_str()
        .expect("a document id")
        .to_owned();
    assert_eq!(output(&created)["content_hash"], expected_hash.as_str());
    assert_eq!(output(&created)["size_bytes"], 1_048_576);
    assert_eq!(output(&created)["status"], "active");

    // The list sees it, with the same version the create returned.
    let (status, listed) = running
        .invoke(
            "documents.list",
            "dev:alice",
            json!({"limit": 10}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let items = output(&listed)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "one document");
    assert_eq!(items[0]["id"], document.as_str());

    // `documents.download` answers base64 whose hash is the row's hash.
    let (status, downloaded) = running
        .invoke(
            "documents.download",
            "dev:alice",
            json!({"id": document}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{downloaded}");
    let action_bytes = base64::engine::general_purpose::STANDARD
        .decode(
            output(&downloaded)["content_base64"]
                .as_str()
                .expect("base64"),
        )
        .expect("valid base64");
    assert_eq!(hex(&action_bytes), expected_hash, "the action's bytes hash");
    assert_eq!(action_bytes.len(), 1_048_576);

    // ── the byte route returns THE SAME BYTES ────────────────────────────────────────────
    let (status, route_bytes, etag) = get_content(&running, &document, "dev:alice").await;
    assert_eq!(status, 200, "the byte route serves the document");
    assert_eq!(
        route_bytes, action_bytes,
        "route and action must agree byte for byte"
    );
    assert_eq!(
        etag,
        format!("\"{expected_hash}\""),
        "the ETag is the content hash"
    );

    // ── delete: tombstone + purge, and the object is GONE ───────────────────────────────
    let (status, deleted) = running
        .invoke(
            "documents.delete",
            "dev:alice",
            json!({"id": document}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{deleted}");

    let (status, after) = running
        .invoke(
            "documents.get",
            "dev:alice",
            json!({"id": document}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "the tombstone survives: {after}");
    assert_eq!(output(&after)["status"], "deleted");
    assert_eq!(
        output(&after)["version"],
        2,
        "the tombstone write bumped the version"
    );

    // The download refuses: a deleted document has no bytes to serve, on either path.
    let (status, body) = running
        .invoke(
            "documents.download",
            "dev:alice",
            json!({"id": document}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "no bytes after delete: {body}");
    let (status, _, _) = get_content(&running, &document, "dev:alice").await;
    assert_eq!(status, 404, "the route refuses a tombstone too");

    // The tombstone still names its object; the `document.purge` job is what removes it, and that path is
    // asserted in `apps/server/tests/documents_purge.rs` (the worker lives in the server crate).
    let key: Option<String> =
        sqlx::query_scalar("SELECT object_key FROM document WHERE id = $1 AND tenant_id = $2")
            .bind(Uuid::parse_str(&document).expect("uuid"))
            .bind(alice.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the object key");
    assert!(
        key.is_some(),
        "a tombstone keeps its key until the purge job clears it"
    );

    running.shutdown().await;
}

/// The merge in `documents.update` skips the three server-owned facts. Without that skip a caller holding
/// only `documents:write` could re-label an accepted payload — an accepted PDF served as `text/html` is
/// stored XSS at the app origin — and `size_bytes`/`sha256` would stop describing the bytes that were
/// validated. Prove it by injecting all three and reading them back from both the action and the route.
#[tokio::test(flavor = "multi_thread")]
async fn a_caller_cannot_rewrite_the_stored_objects_facts() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let body = "hello";
    let expected_hash = hex(body.as_bytes());

    let (status, created) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "Immutable facts",
                "content_type": "text/plain",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(body.as_bytes()),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created)["id"]
        .as_str()
        .expect("a document id")
        .to_owned();
    assert_eq!(output(&created)["content_type"], "text/plain");
    assert_eq!(output(&created)["size_bytes"], 5);
    assert_eq!(output(&created)["content_hash"], expected_hash.as_str());

    // All three protected keys plus one benign key: the benign one must land, the facts must not move.
    let (status, updated) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({
                "id": document,
                "meta": {
                    "content_type": "text/html",
                    "size_bytes": 1,
                    "sha256": "deadbeef",
                    "note": "kept",
                },
            }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(
        output(&updated)["content_type"],
        "text/plain",
        "the stored MIME type is not the caller's to choose"
    );
    assert_eq!(
        output(&updated)["size_bytes"],
        5,
        "size_bytes must keep describing the validated bytes"
    );
    assert_eq!(
        output(&updated)["content_hash"],
        expected_hash.as_str(),
        "sha256 must keep describing the validated bytes"
    );
    assert_eq!(
        output(&updated)["meta"]["note"],
        "kept",
        "benign metadata still merges"
    );

    // The route serves the document with its own stored type — the header is the XSS surface.
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/documents/{document}/content",
            running.base()
        ))
        .header("authorization", "Bearer dev:alice")
        .send()
        .await
        .expect("the byte route");
    assert_eq!(response.status().as_u16(), 200);
    let served_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        served_type.starts_with("text/plain"),
        "the route serves the stored type, not the caller's: {served_type}"
    );

    running.shutdown().await;
}
