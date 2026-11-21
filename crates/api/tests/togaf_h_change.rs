//! T9 (#81) D5 — Phase H records exactly one typed outcome, and an unrecorded review refuses
//! the gate.
//!
//! `rbuild -- test -p conveyor-api --test togaf_h_change`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

const PHASES: [&str; 8] = ["preliminary", "a", "b", "c", "d", "e", "f", "g"];

#[tokio::test]
async fn a_scope_change_produces_a_change_register_row() {
    let f = arch::fixture().await;
    arch::tailor(&f.running, json!({}), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;
    let project_uuid = Uuid::parse_str(&project).expect("project id");
    let contract =
        arch::request_contract(&f.running, &project, "request_for_architecture", "h").await;
    let contract_id = arch::id_of(&contract);
    let change =
        arch::register_change(&f.pg, f.seeded.tenant_id, project_uuid, f.seeded.user_id).await;

    let review = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": contract_id,
            "level": "non_conformant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the current architecture no longer meets its purpose" }],
            "evidence": {"ref": "change register", "delegation_ref": f.delegation},
            "outcome": { "kind": "change", "change_id": change },
        }),
        None,
    )
    .await;
    assert_eq!(review["outcome"]["kind"], "change", "{review}");
    assert_eq!(review["outcome"]["change_id"], change, "{review}");

    // A systemic finding is a kata candidate and writes no change row of its own.
    let before = arch::change_rows(&f.pg, f.seeded.tenant_id).await;
    let kata_contract =
        arch::request_contract(&f.running, &project, "statement_of_intent", "h").await;
    let kata = arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": arch::id_of(&kata_contract),
            "level": "non_conformant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "systemic: the review ritual itself failed" }],
            "outcome": { "kind": "kata", "candidate": "review-ritual" },
        }),
        None,
    )
    .await;
    assert_eq!(kata["outcome"]["kind"], "kata", "{kata}");
    assert_eq!(
        arch::change_rows(&f.pg, f.seeded.tenant_id).await,
        before,
        "a kata verdict wrote a change register row"
    );
}

#[tokio::test]
async fn an_unrecorded_review_refuses_the_gate() {
    let f = arch::fixture().await;
    arch::tailor(&f.running, json!({}), json!({})).await;
    let project = arch::project(&f.running, "architecture").await;
    for phase in PHASES {
        let (status, out) = arch::advance(&f.running, &project, phase).await;
        assert_eq!(status, 200, "advance {phase}: {out}");
    }

    let (status, refused) = arch::advance(&f.running, &project, "h").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_outcome_required",
        "{refused}"
    );

    let contract =
        arch::request_contract(&f.running, &project, "request_for_architecture", "h").await;
    arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project,
            "contract_id": arch::id_of(&contract),
            "level": "compliant",
            "delegation_ref": f.delegation,
            "outcome": { "kind": "kata", "candidate": "phase-h-review" },
        }),
        None,
    )
    .await;

    let (status, out) = arch::advance(&f.running, &project, "h").await;
    assert_eq!(status, 200, "{out}");
    assert_eq!(out["closed_phase"], "h", "{out}");
    assert!(out["next_phase"].is_null(), "H is the last phase: {out}");
}
