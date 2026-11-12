//! S09 §7 (b): the upload limits are refusals with stable problem documents, not truncations.
//!
//! Three refusals, each proved at its own boundary:
//!
//! - **10 MiB exactly is accepted; 10 MiB + 1 is `413`.** The cap is a byte count on the decoded payload, so
//!   the test supplies both sides rather than trusting the arithmetic.
//! - **A content type off the allowlist is `415`** — `application/octet-stream` is the one a caller reaches for
//!   when it does not know, and accepting it would make every later sniff meaningless.
//! - **Bytes that contradict their declared type are `415`** — an HTML file claiming `application/pdf`, which is
//!   the stored-XSS shape the plan names as trap 6.
//!
//! ⚠️ The transport envelope matters here too: a 10 MiB payload travels as ~14 MB of base64, so the router's body
//! limit is 16 MiB. Before that layer existed axum refused the legal upload with its own 413 and the action's cap
//! was unreachable over HTTP — the reason this file uploads a real 10 MiB body instead of asserting the constant.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use base64::Engine as _;
use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ten_mebibyte_cap_is_a_refusal_at_the_byte() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // ── exactly at the cap: accepted ────────────────────────────────────────────────────
    let at_cap = vec![b'a'; 10 * 1024 * 1024];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&at_cap);
    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "At the cap",
                "content_type": "text/plain",
                "content_base64": encoded,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "10 MiB exactly is legal: {body}");
    assert_eq!(output(&body)["size_bytes"], 10 * 1024 * 1024);

    // ── one byte over: 413 with a problem document ─────────────────────────────────────
    let over = vec![b'a'; 10 * 1024 * 1024 + 1];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&over);
    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "Over the cap",
                "content_type": "text/plain",
                "content_base64": encoded,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 413, "one byte over is refused: {body}");
    assert!(
        body.get("type").is_some() && body.get("status").is_some(),
        "a refusal is RFC 9457 problem+json: {body}"
    );

    // Nothing was written for either refusal.
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count documents");
    assert_eq!(count, 1, "only the accepted upload left a row");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_content_type_off_the_allowlist_is_refused() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    for content_type in ["application/octet-stream", "text/html", "application/zip"] {
        let (status, body) = running
            .invoke(
                "documents.create",
                "dev:alice",
                json!({
                    "title": "Refused",
                    "content_type": content_type,
                    "content_base64": base64::engine::general_purpose::STANDARD.encode(b"hello"),
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 415, "{content_type} must be refused: {body}");
    }

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count documents");
    assert_eq!(count, 0, "a refused type writes nothing");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn bytes_that_contradict_their_declared_type_are_refused() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // An HTML document claiming to be a PDF: the magic-byte check is the only thing between this and a stored
    // XSS served from a route that sets a PDF content type.
    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "Not a PDF",
                "content_type": "application/pdf",
                "content_base64": base64::engine::general_purpose::STANDARD
                    .encode(b"<html><script>alert(1)</script></html>"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 415, "a fake PDF is refused: {body}");

    // A PNG whose signature is wrong is refused even though `image/png` is on the allowlist.
    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "Not a PNG",
                "content_type": "image/png",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(b"not a png"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 415, "a fake PNG is refused: {body}");

    // A real PNG signature is accepted, so the check is a check and not a blanket refusal.
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    png.extend_from_slice(b"rest of the file");
    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "A real PNG header",
                "content_type": "image/png",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(&png),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "a real PNG signature passes: {body}");

    running.shutdown().await;
}
