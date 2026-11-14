//! #70 P1: a text output is composed, read, updated under If-Match, downloaded, capped and
//! refused when it names a card that is not this tenant's.
//!
//! `cargo test -p conveyor-api --test document_outputs`.

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

#[tokio::test(flavor = "multi_thread")]
async fn a_text_output_round_trips_and_refuses_a_foreign_card() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let body = "# Note\n\nA decision recorded from the console.";

    let (status, created) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({"title": "Design note", "body": body, "publishable": true}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    assert_eq!(document["body"], body);
    assert_eq!(document["publishable"], true);
    assert_eq!(document["content_type"], "text/markdown");
    assert_eq!(document["content_hash"], hex(body.as_bytes()));
    assert_eq!(document["object_key"], Value::Null);
    let id = document["id"].as_str().expect("id").to_owned();
    let version = document["version"].as_i64().expect("version");
    let etag = format!("\"{version}\"");

    let (status, got) = running
        .invoke(
            "documents.get",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(output(&got)["body"], body);

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
    assert_eq!(items.len(), 1);
    assert!(
        items[0]["body"].is_null(),
        "a list row does not carry the body"
    );

    let (status, updated) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": id, "publishable": false}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["publishable"], false);

    // The same stale If-Match is now refused; the row is at version 2.
    let (status, stale) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": id, "title": "Renamed"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");

    let (status, downloaded) = running
        .invoke(
            "documents.download",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{downloaded}");
    assert_eq!(output(&downloaded)["content_type"], "text/markdown");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            output(&downloaded)["content_base64"]
                .as_str()
                .expect("base64"),
        )
        .expect("valid base64");
    assert_eq!(bytes, body.as_bytes());

    let too_large = "a".repeat(256 * 1024 + 1);
    let (status, refused) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({"title": "Too large", "body": too_large}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 413, "{refused}");

    let (status, missing) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({"title": "Orphan", "body": "x", "card_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{missing}");
}
