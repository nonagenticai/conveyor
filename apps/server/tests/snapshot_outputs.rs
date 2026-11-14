//! `conveyor snapshot --outputs-dir`: the public snapshot carries only published output metadata,
//! and the publish path renders a published body to static HTML with hostile markup neutralised.
//!
//! `cargo test -p conveyor-server --test snapshot_outputs`.

mod common;

use std::path::Path;

use conveyor_server::snapshot::{SnapshotArgs, run_with_pool};
use serde_json::Value;
use uuid::Uuid;

fn read_value(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read file")).expect("parse json")
}

#[tokio::test]
async fn only_publishable_outputs_are_public_and_rendered_safely() {
    let db = common::TestDb::fresh().await;

    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('out', 'Outputs tenant') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (email, display_name) VALUES ('alice@example.com', 'Alice') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("user");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant)
        .bind(user)
        .execute(&db.super_pool)
        .await
        .expect("membership");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'conveyor buildout') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id) VALUES ($1, 'S02', 'Skeleton', $2) RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");

    let published = Uuid::now_v7();
    let private = Uuid::now_v7();
    let hostile = "before <script>alert(1)</script> <img src=x onerror=alert(1)> [x](javascript:alert(1)) after";
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, meta)
         VALUES ($1, $2, $3, 'document', 'Published note', 'active', $4, true, '{}'::jsonb)",
    )
    .bind(published)
    .bind(tenant)
    .bind(project)
    .bind(hostile)
    .execute(&db.super_pool)
    .await
    .expect("published output");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, meta)
         VALUES ($1, $2, $3, 'document', 'Private note', 'active', 'do not publish me', false, '{}'::jsonb)",
    )
    .bind(private)
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("private output");

    let data = std::env::temp_dir().join(format!("conveyor-outputs-data-{}", Uuid::now_v7()));
    let docs = std::env::temp_dir().join(format!("conveyor-outputs-docs-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&docs).expect("docs dir");
    let issues = data.join("issues.json");
    std::fs::write(&issues, "{\"issues\":[]}").expect("issues");
    let report = run_with_pool(
        &db.runtime,
        SnapshotArgs {
            tenant: "out".to_owned(),
            out: data.clone(),
            outputs_dir: Some(docs.clone()),
            generated_at: Some("2026-11-13T00:00:00Z".to_owned()),
            issues,
            feedback_url: "https://example.test/feedback".to_owned(),
            database_url: None,
            git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        },
    )
    .await
    .expect("snapshot");
    assert_eq!(report.outputs, 1, "exactly one published output");

    let artefacts = read_value(&data.join("artefacts.json"));
    let documents = artefacts["documents"].as_array().expect("documents");
    assert_eq!(documents.len(), 1, "the private output is absent");
    assert_eq!(documents[0]["title"], "Published note");
    assert_eq!(documents[0]["project_id"], project.to_string());
    for forbidden in ["body", "content_hash", "meta", "status"] {
        assert!(
            documents[0].get(forbidden).is_none(),
            "{forbidden} must not be public"
        );
    }

    let snapshot = std::fs::read_to_string(data.join("snapshot.js")).expect("snapshot.js");
    assert!(snapshot.contains("Published note"));
    assert!(
        !snapshot.contains("Private note"),
        "the private output is not public"
    );
    assert!(!snapshot.contains("do not publish me"));

    let index = std::fs::read_to_string(docs.join("outputs.js")).expect("outputs.js");
    assert!(index.starts_with("window.CONVEYOR_OUTPUTS = {"));
    assert!(index.contains("\"schema\":\"design-outputs/v1\""));
    assert!(index.contains("Published note"));
    assert!(!index.contains("Private note"));

    let page = std::fs::read_to_string(docs.join(format!("output-{published}.html")))
        .expect("the published page");
    assert!(page.contains("&lt;script&gt;"), "raw HTML is text: {page}");
    assert!(!page.contains("<script"), "no executable script: {page}");
    assert!(!page.contains("<img"), "no image tag: {page}");
    assert!(page.contains("&lt;img"), "the image markup is text: {page}");
    assert!(
        !page.to_ascii_lowercase().contains("javascript:"),
        "no javascript URL"
    );
    assert!(page.contains("Published note"));
    assert!(
        !docs.join(format!("output-{private}.html")).exists(),
        "no page for an unpublished output"
    );
}
