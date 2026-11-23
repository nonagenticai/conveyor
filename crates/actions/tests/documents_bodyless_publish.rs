//! v1-outputs-1 — the public pack renders only published rows with a text body.
//!
//! `list_published_outputs_tx` filters `body IS NOT NULL` (crates/store/src/document.rs), so a
//! bytes-backed row published through `documents.publish` would be public and absent from the
//! pack. The acceptance allows rendering or refusal; the action refuses the transition, and this
//! test pins that refusal so a published row can never be invisible.
//!
//! The row is written directly: it is the legacy shape (bytes, no body) the guard exists for, and
//! no action creates it in one call.

mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn a_published_document_without_a_text_body_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // A byte-backed row: an object key and a hash, no body — the shape the pack cannot render.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO document (tenant_id, kind, title, object_key, content_hash)
         VALUES ($1, 'document', 'A ruling stored only as bytes', $2, $3)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!(
        "tenants/{}/{}/object",
        seeded.tenant_id,
        Uuid::now_v7()
    ))
    .bind("a".repeat(64))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert a body-less, byte-backed document");

    let (status, refused) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id.to_string(), "publish": true}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused.to_string().contains("text_body_missing"),
        "the refusal names the missing text body: {refused}"
    );

    let published_at_is_null: bool =
        sqlx::query_scalar("SELECT published_at IS NULL FROM document WHERE id = $1")
            .bind(id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the row's publication bit");
    assert!(
        published_at_is_null,
        "a refused publish never makes the row public"
    );
}
