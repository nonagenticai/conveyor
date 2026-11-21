//! `conveyor snapshot --outputs-dir`: the public snapshot carries only published output metadata,
//! and the publish path renders a published body to static HTML with hostile markup neutralised.
//!
//! `cargo test -p conveyor-server --test snapshot_outputs`.

mod common;

use std::collections::HashSet;
use std::path::Path;

use conveyor_server::snapshot::{SnapshotArgs, run_with_pool};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn read_value(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read file")).expect("parse json")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn only_published_outputs_are_public_and_rendered_safely() {
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
    let publishable_only = Uuid::now_v7();
    let hostile = "before <script>alert(1)</script> <img src=x onerror=alert(1)> [x](javascript:alert(1)) after";
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, published_at, meta)
         VALUES ($1, $2, $3, 'document', 'Published note', 'active', $4, true, now(), '{}'::jsonb)",
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
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, meta)
         VALUES ($1, $2, $3, 'document', 'Publishable but never published', 'active', 'metadata only', true, '{}'::jsonb)",
    )
    .bind(publishable_only)
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("publishable-only output");

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
            conformance: None,
            feedback_url: "https://example.test/feedback".to_owned(),
            database_url: None,
            git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
            manifest_only: false,
        },
    )
    .await
    .expect("snapshot");
    assert_eq!(report.outputs, 1, "exactly one published output");

    let artefacts = read_value(&data.join("artefacts.json"));
    let documents = artefacts["documents"].as_array().expect("documents");
    assert_eq!(documents.len(), 1, "only the published output is public");
    assert!(
        documents
            .iter()
            .all(|document| document["title"] != "Publishable but never published"),
        "publishable alone is not publication"
    );
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
    assert!(
        !snapshot.contains("Publishable but never published"),
        "a publishable row that was never published stays private"
    );

    let index = std::fs::read_to_string(docs.join("outputs.js")).expect("outputs.js");
    assert!(index.starts_with("window.CONVEYOR_OUTPUTS = {"));
    assert!(index.contains("\"schema\":\"design-outputs/v1\""));
    assert!(index.contains("Published note"));
    assert!(!index.contains("Private note"));
    assert!(
        !index.contains("Publishable but never published"),
        "the outputs index carries only rows that were published"
    );

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

/// v1-pack-6: a token or email in a published title, body, attachment name, project name,
/// author or slot is redacted in the page, the index and `programme.csv` exactly as the JSON
/// record redacts it; an output of a project outside the programme has no page and no index row.
///
/// v1-pack-3/5: every published file — output pages, `outputs.js` and the issue registry — is
/// hashed in `manifest.json`, so a mismatched registry cannot be relabelled as the tenant's.
#[tokio::test]
async fn pages_csv_and_manifest_share_the_json_redaction_and_programme_filter() {
    let db = common::TestDb::fresh().await;
    let token = format!("ghp_{}", "C".repeat(30));

    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('publication', 'Publication tenant') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (email, display_name) VALUES ('bob@example.com', 'bob@example.com') RETURNING id",
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
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'publication programme') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("programme");
    let bound: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id) VALUES ($1, 'PUB', $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(format!("Public {token}"))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");
    let fixture: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status) VALUES ($1, 'FIX', 'Scratch fixture', 'draft') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("fixture");

    let shown = Uuid::now_v7();
    let hidden = Uuid::now_v7();
    let attachment = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, published_at, category, unit, created_by, meta)
         VALUES ($1, $2, $3, 'document', $4, 'active', $5, true, now(), 'evidence', $6, $7, '{}'::jsonb)",
    )
    .bind(shown)
    .bind(tenant)
    .bind(bound)
    .bind(format!("Contact bob@example.com · {token}"))
    .bind(format!("# Note\n\npasted {token} here\n"))
    .bind(format!("PUB {token}"))
    .bind(user)
    .execute(&db.super_pool)
    .await
    .expect("published output");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, kind, title, status, body, publishable, published_at, meta)
         VALUES ($1, $2, $3, 'document', 'Fixture output', 'active', 'not public', true, now(), '{}'::jsonb)",
    )
    .bind(hidden)
    .bind(tenant)
    .bind(fixture)
    .execute(&db.super_pool)
    .await
    .expect("fixture output");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, project_id, parent_document_id, kind, title, status, object_key, meta)
         VALUES ($1, $2, $3, $4, 'attachment', $5, 'active', 'attachments/one', '{\"content_type\": \"text/plain\", \"size_bytes\": 4}'::jsonb)",
    )
    .bind(attachment)
    .bind(tenant)
    .bind(bound)
    .bind(shown)
    .bind(format!("{token}.txt"))
    .execute(&db.super_pool)
    .await
    .expect("attachment");

    let root = std::env::temp_dir().join(format!("conveyor-publication-{}", Uuid::now_v7()));
    let data = root.join("data");
    let docs = root.join("docs");
    std::fs::create_dir_all(&data).expect("data dir");
    std::fs::create_dir_all(&docs).expect("docs dir");
    let issues = data.join("issues.json");
    std::fs::write(
        &issues,
        "{\"issues\": [{\"number\": 700, \"title\": \"A public issue\", \"state\": \"OPEN\", \"labels\": []}]}\n",
    )
    .expect("issues");
    let report = run_with_pool(
        &db.runtime,
        SnapshotArgs {
            tenant: "publication".to_owned(),
            out: data.clone(),
            outputs_dir: Some(docs.clone()),
            generated_at: None,
            issues,
            conformance: None,
            feedback_url: "https://example.test/feedback".to_owned(),
            database_url: None,
            git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
            manifest_only: false,
        },
    )
    .await
    .expect("snapshot");
    assert_eq!(report.outputs, 1, "exactly one programme output");

    // The page carries the JSON record's masking, and the attachment name is free text too.
    let page = std::fs::read_to_string(docs.join(format!("output-{shown}.html")))
        .expect("the published page");
    assert!(!page.contains(&token), "the page redacts the token: {page}");
    assert!(
        !page.contains("bob@example.com"),
        "the page redacts the email: {page}"
    );
    assert!(page.contains("[redacted-token]"), "{page}");
    assert!(page.contains("***@redacted"), "{page}");
    assert!(
        page.contains("[redacted-token].txt"),
        "the attachment name is redacted: {page}"
    );

    // The index, and the programme boundary: the fixture's output has neither page nor row.
    let index = std::fs::read_to_string(docs.join("outputs.js")).expect("outputs.js");
    assert!(
        !index.contains(&token),
        "the index redacts the token: {index}"
    );
    assert!(
        !index.contains("bob@example.com"),
        "the index redacts the email: {index}"
    );
    assert!(index.contains("[redacted-token]"), "{index}");
    assert!(index.contains("Public [redacted-token]"), "{index}");
    assert!(!index.contains("Fixture output"), "a fixture is not public");
    assert!(
        !docs.join(format!("output-{hidden}.html")).exists(),
        "a fixture's output has no page"
    );

    // The live download's CSV redacts the same fields and carries only the programme.
    let view = conveyor_pack::live::site_view(
        &db.runtime,
        conveyor_pack::live::SiteArgs {
            tenant: conveyor_domain::TenantId::from_uuid(tenant).expect("tenant id"),
            tenant_slug: "publication",
            issues: json!({"issues": []}),
            rework: None,
            feedback_url: "https://example.test/feedback",
            git_sha: "0123456789abcdef0123456789abcdef01234567",
        },
    )
    .await
    .expect("the live view");
    assert!(
        !view.programme_csv.contains(&token),
        "{}",
        view.programme_csv
    );
    assert!(
        !view.programme_csv.contains("bob@example.com"),
        "{}",
        view.programme_csv
    );
    assert!(
        view.programme_csv.contains("[redacted-token]"),
        "{}",
        view.programme_csv
    );
    assert!(
        !view.programme_csv.contains("Scratch fixture"),
        "a fixture project is not in the programme: {}",
        view.programme_csv
    );
    assert!(view.pages.contains_key(&shown));
    assert!(!view.pages.contains_key(&hidden));

    // The manifest hashes every published file and names the registry's provenance.
    let manifest = read_value(&data.join("manifest.json"));
    let files = manifest["files"].as_array().expect("file list");
    let mut listed = HashSet::new();
    for file in files {
        let path = file["path"].as_str().expect("file name");
        listed.insert(path.to_owned());
        let target = data.join(path);
        let bytes = std::fs::read(&target).unwrap_or_else(|error| {
            panic!(
                "{path} is listed but unreadable at {}: {error}",
                target.display()
            )
        });
        assert_eq!(
            file["sha256"].as_str().expect("file hash"),
            sha256_hex(&bytes),
            "{path} must match its manifest hash"
        );
        assert_eq!(
            file["bytes"].as_u64().expect("file size") as usize,
            bytes.len(),
            "{path} must match its manifest size"
        );
    }
    let output_path = format!("../docs/output-{shown}.html");
    for path in [output_path.as_str(), "../docs/outputs.js", "issues.json"] {
        assert!(listed.contains(path), "{path} is published but unlisted");
    }
    assert!(
        report.files.iter().any(|file| file.path == output_path),
        "the report names the output pages it wrote"
    );
    for entry in std::fs::read_dir(&data).expect("data dir") {
        let name = entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name != "manifest.json" {
            assert!(listed.contains(&name), "{name} is written but unlisted");
        }
    }
    for entry in std::fs::read_dir(&docs).expect("docs dir") {
        let name = entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .into_owned();
        assert!(
            listed.contains(&format!("../docs/{name}")),
            "{name} is written but unlisted"
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}
