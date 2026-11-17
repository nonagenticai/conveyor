//! #245 — the authoring standard's five acceptance scenarios over real HTTP and real Postgres.
//!
//! The standard carries a template per category; a text output may not become public without its
//! required headings, whichever action sets the public bit (`documents.publish`, or a
//! `documents.compose`/`documents.update` that asks for `publishable: true`). An unpublish and an
//! already-published row are never re-checked, so the outputs that predate the templates stay
//! public. An admin can return a requested draft with a note; the request clears and the order
//! lands in `meta.rework` in one UPDATE. The preview is the public page's own renderer.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn etag(value: &Value) -> String {
    format!(
        "\"{}\"",
        value["version"].as_i64().expect("a document version")
    )
}

/// The vision template's four required headings, in the standard's order.
const COMPLETE_VISION: &str = "# Vision\n\n## Purpose\n\nWhy the programme exists.\n\n## Outcomes\n\nThe outcomes it is judged on.\n\n## Boundary\n\nWhat is in and out.\n\n## Who it serves\n\nThe stakeholders.\n";
/// Missing Outcomes, Boundary and Who it serves.
const INCOMPLETE_VISION: &str = "# Vision\n\n## Purpose\n\nWhy.\n";

async fn compose(running: &support::Running, token: &str, input: Value) -> (u16, Value) {
    running
        .invoke("documents.compose", token, input, None, None, None)
        .await
}

/// A member of the seeded tenant: the author whose draft an admin returns.
async fn add_member(pg: &support::Pg, seeded: &support::Seeded) -> String {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:member', 'member@dev.invalid', 'Member')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the member");
    // `membership` is RLS-scoped: write it inside the tenant's transaction, as the seed does.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(seeded.tenant_id)
    .bind(id)
    .execute(&mut *tx)
    .await
    .expect("insert the member's membership");
    tx.commit().await.expect("commit the member");
    "dev:member".to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_incomplete_output_cannot_be_published() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, created) = compose(
        &running,
        "dev:alice",
        json!({"title": "Vision", "body": INCOMPLETE_VISION, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    let id = document["id"].as_str().expect("id").to_owned();
    let version = etag(document);

    let (status, refused) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id, "publish": true}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    let text = refused.to_string();
    assert!(text.contains("output_incomplete"), "{refused}");
    assert!(text.contains("Outcomes"), "{refused}");
    assert!(text.contains("Boundary"), "{refused}");
    assert!(text.contains("Who it serves"), "{refused}");

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
    assert_eq!(
        output(&got)["publishable"],
        false,
        "a refused publish leaves the row private"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_complete_output_publishes_and_unpublish_never_blocks() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, created) = compose(
        &running,
        "dev:alice",
        json!({"title": "Vision", "body": COMPLETE_VISION, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    let id = document["id"].as_str().expect("id").to_owned();
    let v1 = etag(document);

    let (status, published) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id, "publish": true}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(output(&published)["publishable"], true);

    // An already-published row is never re-checked: the admin may change the body, and the row
    // stays public whatever the new body's headings.
    let (status, updated) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": id, "body": "# Vision\n\nA note."}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["publishable"], true);
    let v2 = etag(output(&updated));

    // Unpublish is free: no heading check in that direction.
    let (status, unpublished) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id, "publish": false}),
            Some(&v2),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{unpublished}");
    assert_eq!(output(&unpublished)["publishable"], false);
}

/// The CLI's bulk path composes straight to public; it must meet the same gate.
#[tokio::test(flavor = "multi_thread")]
async fn an_output_composed_straight_to_public_is_checked() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, refused) = compose(
        &running,
        "dev:alice",
        json!({
            "title": "Vision",
            "body": INCOMPLETE_VISION,
            "category": "vision",
            "publishable": true
        }),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused.to_string().contains("output_incomplete"),
        "{refused}"
    );

    let (status, created) = compose(
        &running,
        "dev:alice",
        json!({
            "title": "Vision",
            "body": COMPLETE_VISION,
            "category": "vision",
            "publishable": true
        }),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(output(&created)["publishable"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_return_clears_the_request_and_records_the_note() {
    let (pg, running, seeded) = fixture().await;
    let member = add_member(&pg, &seeded).await;
    let (status, created) = compose(
        &running,
        &member,
        json!({"title": "Vision", "body": INCOMPLETE_VISION, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    let id = document["id"].as_str().expect("id").to_owned();
    let version = etag(document);

    let (status, requested) = running
        .invoke(
            "documents.request_publish",
            &member,
            json!({"id": id}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{requested}");
    assert!(output(&requested)["publish_requested_at"].is_string());

    let (status, returned) = running
        .invoke(
            "documents.return",
            "dev:alice",
            json!({"id": id, "note": "Add the missing sections and resubmit."}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{returned}");
    let row = output(&returned);
    assert_eq!(row["publish_requested_at"], Value::Null);
    assert_eq!(row["version"], 1, "a return is not a content edit");
    assert_eq!(
        row["meta"]["rework"]["note"],
        "Add the missing sections and resubmit."
    );
    let missing = row["meta"]["rework"]["missing"]
        .as_array()
        .expect("the missing sections");
    assert!(
        missing.iter().any(|value| value == "Boundary"),
        "{returned}"
    );
    assert!(
        missing.iter().any(|value| value == "Outcomes"),
        "{returned}"
    );

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND topic = 'document:' || $2 AND type = 'document.returned'",
    )
    .bind(seeded.tenant_id)
    .bind(&id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the return events");
    assert_eq!(events, 1, "exactly one document.returned event");

    // A second return has no open request to return.
    let (status, refused) = running
        .invoke(
            "documents.return",
            "dev:alice",
            json!({"id": id, "note": "Again."}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(refused.to_string().contains("not_requested"), "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_published_draft_cannot_be_returned() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, created) = compose(
        &running,
        "dev:alice",
        json!({"title": "Vision", "body": COMPLETE_VISION, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    let id = document["id"].as_str().expect("id").to_owned();
    let version = etag(document);
    let (status, _) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id, "publish": true}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);

    let (status, refused) = running
        .invoke(
            "documents.return",
            "dev:alice",
            json!({"id": id, "note": "No."}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused.to_string().contains("published_locked"),
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_non_admin_cannot_return_a_draft() {
    let (pg, running, seeded) = fixture().await;
    let member = add_member(&pg, &seeded).await;
    let (status, created) = compose(
        &running,
        &member,
        json!({"title": "Vision", "body": INCOMPLETE_VISION, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    let id = document["id"].as_str().expect("id").to_owned();
    let version = etag(document);
    let (status, _) = running
        .invoke(
            "documents.request_publish",
            &member,
            json!({"id": id}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);

    let (status, refused) = running
        .invoke(
            "documents.return",
            &member,
            json!({"id": id, "note": "I will review my own work."}),
            Some(&version),
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{refused}");
    assert!(
        refused.to_string().contains("publish_forbidden"),
        "{refused}"
    );

    let (status, got) = running
        .invoke(
            "documents.get",
            &member,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert!(
        output(&got)["publish_requested_at"].is_string(),
        "a refused return leaves the request open"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_preview_neutralises_author_markup() {
    let (_pg, running, _seeded) = fixture().await;
    let body = "# Vision\n\n## Purpose\n\n<script>alert(1)</script> <img src=x onerror=alert(1)> [x](javascript:alert(1))\n";
    let (status, created) = compose(
        &running,
        "dev:alice",
        json!({"title": "Vision", "body": body, "category": "vision"}),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    let id = output(&created)["id"].as_str().expect("id").to_owned();

    let (status, preview) = running
        .invoke(
            "documents.preview",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{preview}");
    let rendered = output(&preview);
    let html = rendered["html"].as_str().expect("html");
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    // The author's own text may spell the scheme; what must not exist is an executable URL.
    assert!(!html.contains("href=\"javascript:"), "{html}");
    assert!(!html.contains("src=\"javascript:"), "{html}");
    assert_eq!(rendered["category"], "vision");
    let missing = rendered["missing"].as_array().expect("missing");
    assert!(missing.iter().any(|value| value == "Boundary"), "{preview}");
    assert!(missing.iter().any(|value| value == "Outcomes"), "{preview}");
}
