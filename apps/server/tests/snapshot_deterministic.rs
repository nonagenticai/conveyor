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

    // The definition-set metadata (#333): the public page shows a published document's category,
    // unit and published state; a draft's existence must stay invisible.
    sqlx::query(
        "INSERT INTO document (tenant_id, project_id, kind, title, body, publishable, category, unit, status)
         VALUES ($1, $2, 'document', 'Snapshot definition artefact', 'a published body', true,
                 'vision', 'P0', 'active')",
    )
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("published document");
    sqlx::query(
        "INSERT INTO document (tenant_id, project_id, kind, title, body, publishable, category, unit)
         VALUES ($1, $2, 'document', 'Snapshot private draft', 'a draft body', false,
                 'vision', 'P0')",
    )
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("draft document");

    // The MSP records (#305): the public window serves them, and the same redaction must apply.
    let msp_text = format!("snapshot benefit · alice@example.com · token {token}");
    let benefit: Uuid = sqlx::query_scalar(
        "INSERT INTO benefit_profile (tenant_id, programme_id, reference, description, category, status)
         VALUES ($1, $2, 'BEN-SNAP', $3, 'efficiency', 'draft') RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .bind(&msp_text)
    .fetch_one(&db.super_pool)
    .await
    .expect("benefit");
    sqlx::query(
        "INSERT INTO benefit_link
            (tenant_id, programme_id, output_ref, capability_ref, operational_outcome,
             benefit_id, strategic_objective, kind)
         VALUES ($1, $2, 'T1 plan spine', 'WBS', 'the plan is managed', $3,
                 'faster delivery', 'benefit')",
    )
    .bind(tenant)
    .bind(programme)
    .bind(benefit)
    .execute(&db.super_pool)
    .await
    .expect("benefit link");
    sqlx::query(
        "INSERT INTO stakeholder_profile (tenant_id, programme_id, name, concerns)
         VALUES ($1, $2, 'Sponsoring group', 'delivery confidence')",
    )
    .bind(tenant)
    .bind(programme)
    .execute(&db.super_pool)
    .await
    .expect("stakeholder profile");
    let tranche: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 0, 'planned') RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("tranche");
    sqlx::query(
        "INSERT INTO programme_tranche_review (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, 'continue', '{\"baselines\": [\"B0\"]}'::jsonb, $3)",
    )
    .bind(tenant)
    .bind(tranche)
    .bind(user)
    .execute(&db.super_pool)
    .await
    .expect("tranche review");
    sqlx::query(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period)
         VALUES ($1, $2, 'pi', 'PI-2027.1')",
    )
    .bind(tenant)
    .bind(programme)
    .execute(&db.super_pool)
    .await
    .expect("cadence");
    sqlx::query(
        "INSERT INTO wbs_node (tenant_id, project_id, kind, title)
         VALUES ($1, $2, 'deliverable', 'Snapshot deliverable')",
    )
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("wbs node");
    sqlx::query(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title)
         VALUES ($1, $2, 'risk', 'Snapshot risk')",
    )
    .bind(tenant)
    .bind(project)
    .execute(&db.super_pool)
    .await
    .expect("register item");
    let other: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, 'SNAP2', 'Snapshot project two', $2) RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("second project");
    sqlx::query(
        "INSERT INTO project_dependency (tenant_id, programme_id, from_project, to_project, kind)
         VALUES ($1, $2, $3, $4, 'blocks')",
    )
    .bind(tenant)
    .bind(programme)
    .bind(project)
    .bind(other)
    .execute(&db.super_pool)
    .await
    .expect("dependency");

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
        outputs_dir: None,
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

    let msp = std::fs::read_to_string(dir.join("msp.json")).expect("msp.json");
    assert!(msp.contains("BEN-SNAP"), "the benefit profile is public");
    assert!(
        msp.contains("Sponsoring group"),
        "the stakeholder profile is public"
    );
    assert!(msp.contains("PI-2027.1"), "the cadence row is public");
    assert!(
        msp.contains("Snapshot deliverable"),
        "the WBS node is public"
    );
    assert!(msp.contains("Snapshot risk"), "the register item is public");
    assert!(msp.contains("blocks"), "the typed dependency is public");
    assert!(!msp.contains("alice@example.com"), "MSP emails are masked");
    assert!(
        msp.contains("***@redacted"),
        "the MSP email is visibly masked"
    );
    assert!(
        !msp.contains(&token),
        "an MSP token-shaped string is redacted"
    );
    let snapshot_js = std::fs::read_to_string(dir.join("snapshot.js")).expect("snapshot.js");
    assert!(
        snapshot_js.contains("\"msp\""),
        "the wrapper carries the msp section"
    );
    assert!(
        !snapshot_js.contains(&token),
        "the wrapper redacts the MSP token"
    );

    let artefacts = std::fs::read_to_string(dir.join("artefacts.json")).expect("artefacts.json");
    assert!(
        artefacts.contains("Snapshot definition artefact"),
        "the published document is public"
    );
    assert!(
        artefacts.contains("\"category\": \"vision\""),
        "the public document carries its category: {artefacts}"
    );
    assert!(
        artefacts.contains("\"unit\": \"P0\""),
        "the public document carries its unit"
    );
    assert!(
        artefacts.contains("\"publishable\": true"),
        "the public page can tell a published row"
    );
    assert!(
        !artefacts.contains("Snapshot private draft"),
        "a draft never reaches the public artefacts"
    );

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
        outputs_dir: None,
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

/// The live window and the CLI write the same bytes from the same rows.
///
/// This is the parity gate that lets the API serve `snapshot.js`/`outputs.js`/pages from
/// Postgres instead of the generated files: one library, one renderer, two consumers.
#[tokio::test]
async fn the_live_view_and_the_cli_write_the_same_bytes() {
    let db = common::TestDb::fresh().await;

    let tenant_uuid: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('snap-live', 'Live snapshot') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let tenant = conveyor_domain::TenantId::from_uuid(tenant_uuid).expect("tenant id");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'live programme') RETURNING id",
    )
    .bind(tenant_uuid)
    .fetch_one(&db.super_pool)
    .await
    .expect("programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, 'LIVE', 'Live project', $2) RETURNING id",
    )
    .bind(tenant_uuid)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");

    let body = "# Live output\n\nOne row, two consumers.\n";
    let document_id = Uuid::now_v7();
    let mut tx = conveyor_store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped write");
    conveyor_store::document::create_document_tx(
        &mut tx,
        tenant,
        &conveyor_store::document::NewDocument {
            id: document_id,
            project_id: Some(project),
            kind: "document".to_owned(),
            title: "Live output".to_owned(),
            status: "active".to_owned(),
            object_key: None,
            content_hash: Some(sha256_hex(body.as_bytes())),
            card_id: None,
            parent_document_id: None,
            body: Some(body.to_owned()),
            external_ref: None,
            meta: json!({"category": "evidence"}),
            publishable: true,
            category: Some("evidence".to_owned()),
            unit: None,
            created_by: None,
            updated_by: None,
        },
    )
    .await
    .expect("compose the live output");
    tx.commit().await.expect("commit");

    let dir = std::env::temp_dir().join(format!("conveyor-live-parity-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    let issues_path = dir.join("issues.json");
    std::fs::write(&issues_path, r#"{"issues": []}"#).expect("write issues");
    let issues: Value = serde_json::from_str(r#"{"issues": []}"#).expect("issues json");
    let feedback = "https://example.test/feedback";
    let git_sha = "0123456789abcdef0123456789abcdef01234567";

    let view = conveyor_pack::live::site_view(
        &db.runtime,
        conveyor_pack::live::SiteArgs {
            tenant,
            tenant_slug: "snap-live",
            issues,
            feedback_url: feedback,
            git_sha,
        },
    )
    .await
    .expect("the live view");

    conveyor_server::snapshot::run_with_pool(
        &db.runtime,
        SnapshotArgs {
            tenant: "snap-live".to_owned(),
            out: dir.clone(),
            outputs_dir: Some(dir.join("docs")),
            generated_at: Some(view.generated_at.clone()),
            issues: issues_path,
            feedback_url: feedback.to_owned(),
            database_url: None,
            git_sha: Some(git_sha.to_owned()),
        },
    )
    .await
    .expect("the CLI snapshot");

    assert_eq!(
        std::fs::read(dir.join("snapshot.js")).expect("snapshot.js"),
        view.snapshot_js.as_bytes(),
        "the CLI and the live route write one snapshot"
    );
    assert_eq!(
        std::fs::read(dir.join("docs/outputs.js")).expect("outputs.js"),
        view.outputs_js.as_bytes(),
        "the CLI and the live route write one output index"
    );
    assert_eq!(
        std::fs::read(dir.join(format!("docs/output-{document_id}.html"))).expect("output page"),
        view.pages
            .get(&document_id)
            .expect("the live page")
            .as_bytes(),
        "the CLI and the live route render one page"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
