//! The media job envelope the runner dispatch reads (#185).
//!
//! `apps/server/src/runner_dispatch.rs::instruction_for` builds an `Instruction` from exactly two
//! keys of the `job.payload` — `inputs` and `outputs` — and `apps/media/src/common/runner.rs`
//! uploads only a declared name. So a media action that emits neither produces a pod that cannot
//! read a file and cannot persist one, and the job still reports success.
//!
//! These tests assert on the row the dispatcher will read, not on the action's return value: the
//! `job.payload` is the contract between the two.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};

/// A 16-byte ISO-BMFF header: the `ftyp` box at bytes 4..8 is what `video/mp4` is verified by.
/// The executors never run in this file — what matters is that the store accepts footage at all.
fn tiny_mp4() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
    bytes.extend_from_slice(b"ftypisom");
    bytes.extend_from_slice(&[0x00, 0x00, 0x02, 0x00]);
    bytes
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The `job` row the dispatcher claims, newest first for that kind.
async fn latest_payload(pg: &support::Pg, tenant: conveyor_domain::TenantId, kind: &str) -> Value {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let (payload,): (Value,) =
        sqlx::query_as("SELECT payload FROM job WHERE kind = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(kind)
            .fetch_one(&mut *tx)
            .await
            .expect("the action enqueued a job");
    tx.commit().await.expect("commit");
    payload
}

#[tokio::test]
async fn render_html_declares_its_closed_output_set() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "media.render_html",
            "dev:alice",
            json!({"html": "<p>one page</p>"}),
            None,
            None,
            None,
        )
        .await;
    // An enqueue is a read of the job table plus one insert; the action is not a creation.
    assert_eq!(status, 200, "{body}");

    let payload = latest_payload(&pg, seeded.tenant, "media.render_html").await;
    let outputs = payload["outputs"]
        .as_array()
        .expect("the payload declares an output set");
    assert_eq!(outputs.len(), 1, "{payload}");
    // browser/render.rs:78 writes `{name}.{format}`; the action's default is `render.pdf`.
    assert_eq!(outputs[0]["name"], json!("render.pdf"));
    assert_eq!(outputs[0]["content_type"], json!("application/pdf"));
    assert_eq!(outputs[0]["max_bytes"], json!(256 * 1024 * 1024));
    assert_eq!(
        payload["inputs"].as_array().map(Vec::len),
        Some(0),
        "inline html needs no input document: {payload}"
    );
}

#[tokio::test]
async fn a_document_becomes_a_pinned_input() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, created) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "two seconds of nothing",
                "content_type": "video/mp4",
                "content_base64": base64(&tiny_mp4()),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document_id = created["output"]["id"].as_str().expect("document id");
    let content_hash = created["output"]["content_hash"]
        .as_str()
        .expect("the store hashed the bytes");

    let (status, body) = running
        .invoke(
            "media.probe_video",
            "dev:alice",
            json!({"input": "clip.mp4", "documents": {"clip.mp4": document_id}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let payload = latest_payload(&pg, seeded.tenant, "media.probe_video").await;
    let inputs = payload["inputs"]
        .as_array()
        .expect("the payload declares its inputs");
    assert_eq!(inputs.len(), 1, "{payload}");
    assert_eq!(inputs[0]["name"], json!("clip.mp4"));
    assert_eq!(inputs[0]["document_id"], json!(document_id));
    // The hash is the one the pod re-checks its download against, so it must be the row's own.
    assert_eq!(inputs[0]["sha256"], json!(content_hash));
    assert_eq!(inputs[0]["content_type"], json!("video/mp4"));
    assert_eq!(
        inputs[0]["bytes"],
        json!(tiny_mp4().len()),
        "the executor's size hint comes from the row"
    );
    // probe_video's answer IS the result document, so it declares no artefact.
    assert_eq!(
        payload["outputs"].as_array().map(Vec::len),
        Some(0),
        "{payload}"
    );
}

#[tokio::test]
async fn a_document_the_tenant_does_not_have_is_refused() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "media.probe_video",
            "dev:alice",
            json!({"input": "clip.mp4", "documents": {"clip.mp4": uuid::Uuid::new_v4()}}),
            None,
            None,
            None,
        )
        .await;
    // A typed refusal, not a 500 and not a job that starts only to fail its first download.
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("does not have"),
        "{body}"
    );
}

#[tokio::test]
async fn bytes_that_are_not_the_declared_type_are_refused() {
    let pg = support::Pg::fresh().await;
    let _seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "not a video",
                "content_type": "video/mp4",
                "content_base64": base64(b"<html><body>nope</body></html>"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 415, "{body}");
}
