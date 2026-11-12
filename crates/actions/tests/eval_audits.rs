//! S19 Part C step 14 — the audit and finding actions over the wire.
//!
//! The worker's behaviour is `apps/server/tests/eval_audit.rs`'s; what is proven here is the
//! action surface: a start writes one queued audit and exactly one job, a version from another
//! eval is refused before anything is written, and a finding is decided once — a second decision
//! is a `409`, and a foreign id is a `404`.

mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// An eval with one version.
async fn draft(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "audit contracts",
                "purpose": "does start validate and decide once",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let version_id = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    (eval_id, version_id)
}

/// One open finding on a fresh audit of the draft, written through the store.
async fn seed_finding(pg: &support::Pg, tenant: TenantId, eval_id: &str, version_id: &str) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let audit = store::eval::insert_audit_tx(
        &mut tx,
        tenant,
        &store::eval::NewAudit {
            eval_id: Uuid::parse_str(eval_id).expect("eval uuid"),
            version_id: Uuid::parse_str(version_id).expect("version uuid"),
            target_run_id: None,
            requested_by: None,
        },
    )
    .await
    .expect("insert the audit");
    let finding = store::eval::insert_finding_tx(
        &mut tx,
        tenant,
        audit,
        &store::eval::NewFinding {
            section: "4",
            item: "contract_finding",
            severity: "noise",
            evidence: "prepared",
            suggestion: "prepared",
        },
    )
    .await
    .expect("insert the finding");
    tx.commit().await.expect("commit");
    finding
}

#[tokio::test]
async fn start_writes_one_queued_audit_and_one_job_and_refuses_a_foreign_version() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = draft(&running).await;
    let (other_eval, _other_version) = draft(&running).await;

    // ── a version from another eval is refused before anything is written ──
    let (status, body) = running
        .invoke(
            "eval_audits.start",
            TOKEN,
            json!({ "eval_id": other_eval, "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().contains("version_id"), "{body}");

    // ── the real start: one queued audit, one job ──
    let (status, body) = running
        .invoke(
            "eval_audits.start",
            TOKEN,
            json!({ "eval_id": eval_id, "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "queued");
    let audit_id =
        Uuid::parse_str(body["output"]["id"].as_str().expect("audit id")).expect("audit uuid");

    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let (kinds,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'eval.audit.run'")
            .bind(seeded.tenant.as_uuid())
            .fetch_one(&mut *tx)
            .await
            .expect("count jobs");
    tx.commit().await.expect("commit");
    assert_eq!(kinds, 1, "exactly one audit job is enqueued");

    // ── get and list read the same row ──
    let (status, body) = running
        .invoke(
            "eval_audits.get",
            TOKEN,
            json!({ "id": audit_id.to_string() }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["audit"]["status"], "queued");
    assert!(
        body["output"]["findings"]
            .as_array()
            .expect("findings")
            .is_empty()
    );

    let (status, body) = running
        .invoke(
            "eval_audits.list",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().expect("items").len(), 1);

    let (status, body) = running
        .invoke("eval_findings.list", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let _ = body;
}

#[tokio::test]
async fn a_finding_is_decided_once_and_a_foreign_id_is_a_404() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = draft(&running).await;
    let finding = seed_finding(&pg, seeded.tenant, &eval_id, &version_id).await;

    let (status, body) = running
        .invoke(
            "eval_findings.decide",
            TOKEN,
            json!({
                "finding_id": finding.to_string(),
                "decision": "accepted",
                "note": "a real change to make",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "accepted");
    assert!(body["output"]["decided_by"].is_string(), "{body}");

    // A second decision is refused: the finding is no longer open.
    let (status, body) = running
        .invoke(
            "eval_findings.decide",
            TOKEN,
            json!({ "finding_id": finding.to_string(), "decision": "dismissed" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "not_open", "{body}");

    // A foreign id never names a row.
    let (status, body) = running
        .invoke(
            "eval_findings.decide",
            TOKEN,
            json!({ "finding_id": Uuid::now_v7().to_string(), "decision": "accepted" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let _ = body;
}
