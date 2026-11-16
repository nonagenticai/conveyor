//! #156: provenance, the publish workflow, revisions and attachments on a document.
//!
//! `cargo test -p conveyor-api --test document_workflow`.
//!
//! The member path is exercised with a second `app_user` and a `member` membership in the same
//! tenant; the admin path with the seeded `admin`. The object store is the harness's in-memory
//! backend, so an attachment's bytes survive between the create and the read.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use base64::Engine as _;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// Add a `member` principal to an existing tenant.
async fn add_member(pg: &support::Pg, tenant: TenantId, login: &str, name: &str) {
    let subject = format!("dev:{login}");
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name) VALUES ($1, $2, $3)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(&subject)
    .bind(format!("{login}@dev.invalid"))
    .bind(name)
    .fetch_one(&pg.owner)
    .await
    .expect("insert member");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("insert membership");
    tx.commit().await.expect("commit member");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_drafts_and_requests_an_admin_publishes_and_history_restores() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "workflow", "alice", "Alice").await;
    add_member(&pg, seeded.tenant, "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    // A member composes a draft with the standard's slots; the author is recorded.
    let (status, created) = running
        .invoke(
            "documents.compose",
            "dev:bob",
            json!({
                "title": "S02 brief",
                "body": "# S02 brief\n\nThe outcome and the boundary.",
                "category": "project-brief",
                "unit": "S02",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document = output(&created);
    assert_eq!(document["publishable"], false);
    assert_eq!(document["category"], "project-brief");
    assert_eq!(document["unit"], "S02");
    assert_eq!(document["created_by_name"], "Bob");
    assert_eq!(document["publish_requested_at"], Value::Null);
    let id = document["id"].as_str().expect("id").to_owned();
    let v1 = format!("\"{}\"", document["version"].as_i64().expect("version"));

    // A member cannot publish, and cannot compose straight to public.
    let (status, refused) = running
        .invoke(
            "documents.compose",
            "dev:bob",
            json!({"title": "Straight to public", "body": "x", "publishable": true}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{refused}");
    let (status, refused) = running
        .invoke(
            "documents.publish",
            "dev:bob",
            json!({"id": id, "publish": true}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{refused}");

    // The author's request is a flag, not a version change.
    let (status, requested) = running
        .invoke(
            "documents.request_publish",
            "dev:bob",
            json!({"id": id}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{requested}");
    assert!(output(&requested)["publish_requested_at"].is_string());
    assert_eq!(output(&requested)["version"], 1);

    // The admin publishes; the request clears and the publisher is recorded.
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
    assert_eq!(output(&published)["published_by_name"], "Alice");
    assert_eq!(output(&published)["publish_requested_at"], Value::Null);
    assert_eq!(
        output(&published)["version"],
        1,
        "publish is not a content edit"
    );

    // A published row is locked to a member.
    let (status, locked) = running
        .invoke(
            "documents.update",
            "dev:bob",
            json!({"id": id, "title": "Unreviewed change"}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{locked}");

    // The admin unpublishes, and the member's next edit is version 2.
    let (status, unpublished) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": id, "publish": false}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{unpublished}");
    let (status, updated) = running
        .invoke(
            "documents.update",
            "dev:bob",
            json!({"id": id, "title": "S02 brief (revised)"}),
            Some(&v1),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    let v2 = format!(
        "\"{}\"",
        output(&updated)["version"].as_i64().expect("version")
    );
    assert_eq!(output(&updated)["updated_by_name"], "Bob");

    // History: the created version and the edit, newest first, each with its author.
    let (status, revisions) = running
        .invoke(
            "documents.revision_list",
            "dev:bob",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{revisions}");
    let rows = output(&revisions).as_array().expect("a revision array");
    assert_eq!(rows.len(), 2, "{revisions}");
    assert_eq!(rows[0]["seq"], 2);
    assert_eq!(rows[0]["label"], "updated");
    assert_eq!(rows[0]["author"], "Bob");
    assert_eq!(rows[1]["seq"], 1);
    assert_eq!(rows[1]["label"], "created");
    assert!(rows[0].get("content").is_none(), "the list omits the body");

    let (status, version) = running
        .invoke(
            "documents.revision_get",
            "dev:bob",
            json!({"id": id, "seq": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{version}");
    assert!(
        output(&version)["content"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("The outcome and the boundary"),
        "{version}"
    );

    // Restore copies v1 forward as a new draft; it does not rewrite the timeline.
    let (status, restored) = running
        .invoke(
            "documents.revision_restore",
            "dev:bob",
            json!({"id": id, "seq": 1}),
            Some(&v2),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{restored}");
    assert_eq!(output(&restored)["version"], 3);
    assert_eq!(output(&restored)["title"], "S02 brief");
    assert_eq!(output(&restored)["publishable"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_edit_of_a_legacy_row_writes_its_pre_image() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "legacy", "alice", "Alice").await;
    let id = Uuid::now_v7();
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, kind, title, status, body, content_hash, meta,
                               publishable, version)
         VALUES ($1, $2, 'document', 'Legacy note', 'active', '# Legacy', $3, '{}'::jsonb, false, 1)",
    )
    .bind(id)
    .bind(seeded.tenant.as_uuid())
    .bind("a".repeat(64))
    .execute(&mut *tx)
    .await
    .expect("insert legacy row");
    tx.commit().await.expect("commit legacy row");

    let running = support::Running::start(&pg).await;
    let (status, updated) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": id, "title": "Legacy note (edited)"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");

    let (status, revisions) = running
        .invoke(
            "documents.revision_list",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{revisions}");
    let rows = output(&revisions).as_array().expect("a revision array");
    assert_eq!(rows.len(), 2, "{revisions}");
    assert_eq!(rows[0]["seq"], 2);
    assert_eq!(rows[0]["label"], "updated");
    assert_eq!(rows[1]["seq"], 1);
    assert_eq!(rows[1]["label"], "pre-image");
    assert_eq!(
        rows[1]["author"],
        Value::Null,
        "the legacy author is unknown"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_category_backfill_crosses_tenants_under_force_rls() {
    let pg = support::Pg::fresh().await;
    let first = support::seed(&pg, "backfill-a", "alice", "Alice").await;
    let second = support::seed(&pg, "backfill-b", "bob", "Bob").await;
    for (tenant, title) in [(first.tenant, "one"), (second.tenant, "two")] {
        let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
            .await
            .expect("scoped tx");
        sqlx::query(
            "INSERT INTO document (id, tenant_id, kind, title, status, body, content_hash,
                                   meta, publishable)
             VALUES ($1, $2, 'document', $3, 'active', '# x', $4,
                     '{\"category\":\"evidence\",\"unit\":\"S02\"}'::jsonb, false)",
        )
        .bind(Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(title)
        .bind("a".repeat(64))
        .execute(&mut *tx)
        .await
        .expect("insert a legacy row");
        tx.commit().await.expect("commit the legacy row");
    }

    // The 0042 statement, with the force lifted exactly as the migration does: without this,
    // FORCE RLS makes the backfill match nothing because a migration has no tenant context.
    let mut admin = pg.owner.begin().await.expect("admin tx");
    sqlx::query("ALTER TABLE document NO FORCE ROW LEVEL SECURITY")
        .execute(&mut *admin)
        .await
        .expect("lift the force");
    let affected = sqlx::query(
        "UPDATE document
            SET category = NULLIF(btrim(meta->>'category'), ''),
                unit = NULLIF(btrim(meta->>'unit'), '')
          WHERE meta ? 'category' OR meta ? 'unit'",
    )
    .execute(&mut *admin)
    .await
    .expect("backfill")
    .rows_affected();
    sqlx::query("ALTER TABLE document FORCE ROW LEVEL SECURITY")
        .execute(&mut *admin)
        .await
        .expect("restore the force");
    admin.commit().await.expect("commit the backfill");
    assert_eq!(affected, 2, "both tenants' rows were backfilled");

    for (tenant, title) in [(first.tenant, "one"), (second.tenant, "two")] {
        let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
            .await
            .expect("scoped read");
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT category, unit FROM document WHERE title = $1")
                .bind(title)
                .fetch_optional(&mut *tx)
                .await
                .expect("read the backfilled row");
        assert_eq!(
            row,
            Some((Some("evidence".to_owned()), Some("S02".to_owned()))),
            "the promoted columns carry the meta values"
        );
        tx.rollback().await.expect("close the read");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_attachment_is_a_child_document_with_its_parents_scope() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "attach", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, created) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({"title": "Evidence note", "body": "# Evidence"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let parent = output(&created)["id"]
        .as_str()
        .expect("parent id")
        .to_owned();

    let png = base64::engine::general_purpose::STANDARD
        .encode([0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let (status, attachment) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "evidence.png",
                "content_type": "image/png",
                "content_base64": png,
                "parent_document_id": parent,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{attachment}");
    let child = output(&attachment);
    assert_eq!(child["kind"], "attachment");
    assert_eq!(child["parent_document_id"], parent);
    assert_eq!(child["publishable"], false);
    assert_eq!(child["created_by_name"], "Alice");
    let child_id = child["id"].as_str().expect("attachment id").to_owned();

    let (status, listed) = running
        .invoke(
            "documents.list",
            "dev:alice",
            json!({"parent_document_id": parent, "kind": "attachment"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(output(&listed)["items"].as_array().expect("items").len(), 1);

    // An attachment cannot parent another attachment.
    let (status, refused) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "nested.png",
                "content_type": "image/png",
                "content_base64": png,
                "parent_document_id": child_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");
}
