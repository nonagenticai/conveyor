#![forbid(unsafe_code)]
//! The snapshot refuses an issue registry it cannot prove belongs to its tenant.
//!
//! `scripts/fetch-issues.sh` writes `{"schema":"issue-registry/v1","provenance":{"tenant":…,
//! "source":…},"issues":[…]}`. A registry fetched for another tenant, or one that declares no
//! provenance at all, must never be injected into this tenant's programme; the accepted
//! registry's provenance carries the content hash of its `issues` array.
//!
//! `cargo test -p conveyor-pack --test issue_registry`.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};

use conveyor_pack::snapshot::{SnapshotArgs, SnapshotError, run_with_pool};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const SOURCE: &str = "github:nonagenticai/conveyor";

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn args(out: &Path, issues: PathBuf, tenant: &str) -> SnapshotArgs {
    SnapshotArgs {
        tenant: tenant.to_owned(),
        out: out.to_path_buf(),
        outputs_dir: None,
        generated_at: Some("2026-11-23T00:00:00Z".to_owned()),
        issues,
        conformance: None,
        feedback_url: "https://example.test/feedback".to_owned(),
        database_url: None,
        git_sha: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        manifest_only: false,
    }
}

fn write_registry(path: &Path, tenant: &str, issues: &[Value]) {
    std::fs::write(
        path,
        serde_json::to_string(&json!({
            "schema": "issue-registry/v1",
            "provenance": {"tenant": tenant, "source": SOURCE},
            "issues": issues,
        }))
        .expect("registry json"),
    )
    .expect("write the registry");
}

/// The wrapper `snapshot.js` as JSON: `window.CONVEYOR_SNAPSHOT = {…};`.
fn wrapper(out: &Path) -> Value {
    let text = std::fs::read_to_string(out.join("snapshot.js")).expect("snapshot.js");
    let body = text
        .strip_prefix("window.CONVEYOR_SNAPSHOT = ")
        .and_then(|rest| rest.strip_suffix(";\n"))
        .expect("the wrapper's exact shape");
    serde_json::from_str(body).expect("the wrapper is JSON")
}

#[tokio::test]
async fn issue_registry_must_name_this_tenant_and_is_hashed() {
    let db = common::TestDb::fresh().await;
    sqlx::query("INSERT INTO tenant (slug, name) VALUES ('nonagentic', 'Nonagentic')")
        .execute(&db.super_pool)
        .await
        .expect("tenant");

    let out = std::env::temp_dir().join(format!("conveyor-issue-registry-{}", Uuid::now_v7()));
    std::fs::create_dir_all(&out).expect("snapshot dir");
    let issues_path = out.join("issues.json");

    // (a) Another tenant's registry is refused, and the message names both tenants.
    write_registry(&issues_path, "other", &[]);
    let error = run_with_pool(&db.runtime, args(&out, issues_path.clone(), "nonagentic"))
        .await
        .expect_err("a registry fetched for another tenant must be refused");
    assert!(
        matches!(&error, SnapshotError::IssuesTenantMismatch { declared, requested }
            if declared == "other" && requested == "nonagentic"),
        "the refusal is a tenant mismatch: {error}"
    );
    let message = error.to_string();
    assert!(
        message.contains("other") && message.contains("nonagentic"),
        "the refusal names both tenants: {message}"
    );

    // (b) A registry with no provenance is refused.
    std::fs::write(&issues_path, r#"{"issues": []}"#).expect("write issues");
    let error = run_with_pool(&db.runtime, args(&out, issues_path.clone(), "nonagentic"))
        .await
        .expect_err("a registry with no provenance must be refused");
    assert!(
        matches!(error, SnapshotError::IssuesProvenanceMissing { .. }),
        "the refusal is a missing provenance: {error}"
    );
    assert!(
        error.to_string().contains("issues.json"),
        "the refusal names the registry: {error}"
    );

    // (c) The matching registry is accepted, and the published provenance names the tenant,
    // the source and the hash of the very `issues` array the programme carries.
    let issues = json!([{
        "number": 901,
        "title": "one",
        "state": "OPEN",
        "labels": [],
        "url": "https://example.test/issues/901"
    }]);
    write_registry(
        &issues_path,
        "nonagentic",
        issues.as_array().expect("issues array"),
    );
    run_with_pool(&db.runtime, args(&out, issues_path, "nonagentic"))
        .await
        .expect("a matching registry is accepted");
    let provenance = &wrapper(&out)["programme"]["issue_registry"]["provenance"];
    assert_eq!(provenance["tenant"], "nonagentic");
    assert_eq!(provenance["source"], SOURCE);
    let expected = sha256_hex(
        serde_json::to_string(&issues)
            .expect("serialize the issues array")
            .as_bytes(),
    );
    assert_eq!(provenance["sha256"], expected.as_str());

    let _ = std::fs::remove_dir_all(&out);
}
