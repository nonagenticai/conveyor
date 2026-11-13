//! `conveyor snapshot`: deterministic bytes and the public-redaction contract.
//!
//! The same database and the same `--generated-at` must produce byte-identical files; a secret
//! value/key id and an email pasted into a card description must not reach the public files.
//!
//! `cargo test -p conveyor-server --test snapshot_deterministic`.

mod common;

use std::path::Path;

use conveyor_server::snapshot::{SnapshotArgs, run_with_pool};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("read the snapshot directory")
        .map(|entry| {
            let entry = entry.expect("directory entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            (
                name,
                std::fs::read(entry.path()).expect("read a snapshot file"),
            )
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[tokio::test]
async fn snapshot_is_byte_identical_and_redacts_secrets_and_pii() {
    let db = common::TestDb::fresh().await;

    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('snap', 'Snapshot tenant') RETURNING id",
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
        "INSERT INTO project (tenant_id, key, name, programme_id) VALUES ($1, 'SNAP', 'Snapshot project', $2) RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'task', 'Task') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("card type");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position) VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Ready', 0, 'active') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("list");

    // A credential-shaped string and an email pasted into a public field: neither may be
    // published. `ghp_` followed by 30 characters matches the token pattern. The card body is
    // not selected at all; the title is, so the redaction pass must catch it there.
    let token = format!("ghp_{}", "A".repeat(30));
    let title = format!("contact alice@example.com · token {token}");
    sqlx::query(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description, position, assignee_id)
         VALUES ($1, $2, $3, $4, $5, 'private body that is never selected', 0, $6)",
    )
    .bind(tenant)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .bind(&title)
    .bind(user)
    .execute(&db.super_pool)
    .await
    .expect("card");

    // The secret table stores ciphertext; the snapshot may publish the NAME and must publish
    // neither the ciphertext nor the key id.
    sqlx::query(
        "INSERT INTO secret (tenant_id, name, ciphertext, key_id, scope)
         VALUES ($1, 'deploy-token', $2, 'key-id-marker', 'tenant')",
    )
    .bind(tenant)
    .bind(vec![0u8, 1, 2])
    .execute(&db.super_pool)
    .await
    .expect("secret");

    let dir = std::env::temp_dir().join(format!("conveyor-snapshot-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    let issues_path = dir.join("issues.json");
    std::fs::write(
        &issues_path,
        serde_json::to_string(&json!({"issues": [{
            "number": 900,
            "title": "Feedback on the organisation page",
            "state": "OPEN",
            "labels": [{"name": "feedback"}],
            "url": "https://example.test/issues/900"
        }]}))
        .expect("issues json"),
    )
    .expect("write issues");

    let args = SnapshotArgs {
        tenant: "snap".to_owned(),
        out: dir.clone(),
        generated_at: Some("2026-11-12T00:00:00Z".to_owned()),
        issues: issues_path,
        feedback_url: "https://example.test/feedback".to_owned(),
        database_url: None,
        git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
    };
    let first = run_with_pool(&db.runtime, args.clone())
        .await
        .expect("first snapshot");
    let first_files = files_in(&dir);
    let second = run_with_pool(&db.runtime, args)
        .await
        .expect("second snapshot");
    let second_files = files_in(&dir);

    assert_eq!(
        first.manifest_sha256, second.manifest_sha256,
        "a fixed --generated-at must produce an identical manifest"
    );
    assert_eq!(
        first_files, second_files,
        "every file must be byte-identical"
    );

    let work = std::fs::read_to_string(dir.join("work.json")).expect("work.json");
    assert!(!work.contains("alice@example.com"), "emails are masked");
    assert!(work.contains("***@redacted"), "the email is visibly masked");
    assert!(!work.contains(&token), "a token-shaped string is redacted");
    assert!(
        work.contains("[redacted-token]"),
        "the token is visibly redacted"
    );
    assert!(
        !work.contains("private body that is never selected"),
        "card bodies are not selected at all"
    );
    assert!(work.contains("Alice"), "the display name stays public");

    let secrets = std::fs::read_to_string(dir.join("secrets.json")).expect("secrets.json");
    assert!(
        !secrets.contains("deploy-token"),
        "secret names are not public"
    );
    assert!(
        !secrets.contains("key-id-marker"),
        "the key id is not public"
    );
    assert!(!secrets.contains("AAEC"), "the ciphertext is not public");
    assert!(secrets.contains("\"count\": 1"), "the count is public");

    let feedback: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("feedback.json")).expect("feedback.json"),
    )
    .expect("feedback json");
    assert_eq!(feedback["issues"][0]["number"], 900);
    assert_eq!(
        feedback["issues"].as_array().expect("feedback array").len(),
        1
    );

    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("manifest.json")).expect("manifest.json"),
    )
    .expect("manifest json");
    assert_eq!(manifest["schema"], "design-snapshot/v1");
    assert_eq!(manifest["tenant"], "snap");
    assert_eq!(manifest["feedback_url"], "https://example.test/feedback");
    let files = manifest["files"].as_array().expect("file list");
    assert!(files.iter().any(|file| file["path"] == "programme.json"));
    assert!(files.iter().any(|file| file["path"] == "snapshot.js"));
    for file in files {
        let name = file["path"].as_str().expect("file name");
        let bytes = std::fs::read(dir.join(name)).expect("read the listed file");
        assert_eq!(
            file["sha256"].as_str().expect("file hash"),
            sha256_hex(&bytes),
            "{name} must match its manifest hash"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// T0 P5.1: the seeded organisation spine is published, with the design site's keys.
#[tokio::test]
async fn organisation_is_populated_for_a_seeded_tenant() {
    let db = common::TestDb::fresh().await;
    sqlx::query("INSERT INTO tenant (slug, name) VALUES ('nonagentic', 'Nonagentic')")
        .execute(&db.super_pool)
        .await
        .expect("tenant");
    let counts =
        conveyor_store::seed_organisation::seed_organisation(&db.super_pool, Some("nonagentic"))
            .await
            .expect("seed the organisation");
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].1.releases_published, 5);

    let dir = std::env::temp_dir().join(format!("conveyor-organisation-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    let issues_path = dir.join("issues.json");
    std::fs::write(&issues_path, r#"{"issues": []}"#).expect("write issues");
    let args = SnapshotArgs {
        tenant: "nonagentic".to_owned(),
        out: dir.clone(),
        generated_at: Some("2026-11-13T00:00:00Z".to_owned()),
        issues: issues_path,
        feedback_url: "https://example.test/feedback".to_owned(),
        database_url: None,
        git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
    };
    conveyor_server::snapshot::run_with_pool(&db.runtime, args)
        .await
        .expect("snapshot");

    let raw = std::fs::read_to_string(dir.join("organisation.json")).expect("organisation.json");
    let organisation: Value = serde_json::from_str(&raw).expect("organisation json");
    assert_eq!(organisation["schema"], "design-snapshot/v1");
    assert_eq!(
        organisation["industries"].as_array().map(Vec::len),
        Some(1),
        "the software industry"
    );
    assert_eq!(
        organisation["departments"].as_array().map(Vec::len),
        Some(5),
        "the five departments"
    );
    assert_eq!(
        organisation["jobs"].as_array().map(Vec::len),
        Some(22),
        "five described jobs plus the §4.6 drafts"
    );
    assert_eq!(
        organisation["releases"].as_array().map(Vec::len),
        Some(5),
        "one compiled release per described job"
    );
    let backend = organisation["jobs"]
        .as_array()
        .expect("jobs")
        .iter()
        .find(|job| job["key"] == "backend_engineer")
        .expect("the backend row");
    assert!(
        backend["charter"]
            .as_str()
            .is_some_and(|charter| charter.contains("Implement bounded backend changes")),
        "the release anatomy is flattened onto the job row: {backend}"
    );
    assert_eq!(backend["department_names"][0], "delivery");
    assert_eq!(backend["status"], "active");
    assert!(
        !raw.contains("alice@") && !raw.contains("@example."),
        "no email address reaches the public payload"
    );
    assert!(
        !raw.contains("ghp_"),
        "no credential-shaped value reaches the public payload"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
