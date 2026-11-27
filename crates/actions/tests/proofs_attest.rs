//! #837 — `proofs.attest`: the engine derives a unit's facts from its own rows.
//!
//! The facts a unit certificate carries are the engine's, not a caller's: the development
//! charter, the recorded steps and the three gate decisions. `proofs.attest` returns them (and
//! the exact source `lean4.check` must read) and writes nothing; an incomplete record renders a
//! source the kernel refuses, never a passing certificate.
//!
//! `rbuild -- test -p conveyor-actions --test proofs_attest`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

use support::proof;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    (pg, seeded)
}

async fn certificates(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM proof WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the certificates")
}

async fn refused(
    pg: &support::Pg,
    seeded: &support::Seeded,
    input: Value,
) -> conveyor_actions::ActionError {
    conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.attest", seeded.user_id, input, None),
    )
    .await
    .expect_err("proofs.attest must be refused")
}

/// P1.1 — the facts are the rows', the source is the renderer's, and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attest_derives_without_writing() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 901, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let before = certificates(&pg, seeded.tenant_id).await;
    let attested = proof::attest(&pg, &seeded, &unit).await;
    let after = certificates(&pg, seeded.tenant_id).await;
    assert_eq!(before, after, "attest writes no certificate");

    let facts: Value = serde_json::from_str(
        attested["facts_canonical"]
            .as_str()
            .expect("the canonical facts"),
    )
    .expect("the facts are JSON");
    assert_eq!(attested["subject_type"], "unit");
    assert_eq!(facts["kind"], "unit_record");
    assert_eq!(facts["issue_ref"], unit.issue_ref);
    assert_eq!(facts["repo"], "nonagenticai/conveyor");
    assert_eq!(facts["lane"], unit.lane);
    assert_eq!(facts["pull_number"], 837);
    assert_eq!(facts["ci_verdict"], "green");
    assert_eq!(facts["gate_review"], true);
    assert_eq!(facts["gate_merge"], true);
    assert_eq!(facts["gate_deploy"], true);
    assert_eq!(
        facts["serving_digest"], facts["deploy_digest"],
        "the live check ran against the deployed digest"
    );
    assert!(
        attested["lean_source"]
            .as_str()
            .expect("the source")
            .contains("theorem unit_record_ok"),
        "the source closes the unit theorem: {}",
        attested["lean_source"]
    );
}

/// P1.3 — the same rows derive the same facts and the same source, byte for byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attest_is_deterministic() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 902, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let first = proof::attest(&pg, &seeded, &unit).await;
    let second = proof::attest(&pg, &seeded, &unit).await;
    assert_eq!(first["facts_canonical"], second["facts_canonical"]);
    assert_eq!(first["facts_sha256"], second["facts_sha256"]);
    assert_eq!(first["lean_source"], second["lean_source"]);
}

/// P1.2 — a subject that is not chartered in this tenant is refused, never defaulted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attest_refuses_unchartered() {
    let (pg, seeded) = fixture().await;
    let error = refused(
        &pg,
        &seeded,
        json!({"development_project_id": Uuid::now_v7()}),
    )
    .await;
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "proof_project_not_found");
}

/// P1.2 — a chartered project with no issue_ref cannot name the intent its unit delivers.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attest_refuses_missing_issue_ref() {
    let (pg, seeded) = fixture().await;
    let charter = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "development_projects.charter",
            seeded.user_id,
            json!({
                "title": format!("a unit with no issue {}", Uuid::now_v7().simple()),
                "repo": "nonagenticai/conveyor",
                "lane": format!("issue/837-{}", Uuid::now_v7().simple()),
                "implementer_id": seeded.user_id,
                "acceptance_command": "cargo test -p conveyor-actions --test proofs_attest",
                "proof_policy": {
                    "spec_name": "docs/SPEC_V2.md",
                    "spec_sha256": proof::spec_sha256(),
                    "required": true,
                },
            }),
            None,
        ),
    )
    .await
    .expect("the charter");
    assert_eq!(charter.status, 201, "{}", charter.body);
    let development_project_id = output(&charter.body)["id"]
        .as_str()
        .expect("a development project id")
        .to_owned();

    let error = refused(
        &pg,
        &seeded,
        json!({"development_project_id": development_project_id}),
    )
    .await;
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "proof_issue_ref_missing");
}
