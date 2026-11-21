//! T9 (#81) D4 — a delivery project's design gate resolves only against a live contract.
//!
//! `rbuild -- test -p conveyor-api --test togaf_delivery_design_gate`.

#[path = "architecture_support/mod.rs"]
mod arch;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

async fn route(f: &arch::Fixture, project: Uuid) -> (u16, Value) {
    arch::call(
        &f.running,
        "pm.route_gate",
        json!({
            "job_binding_id": f.binding,
            "project_id": project,
            "gate": "design",
            "kind": "design_gate",
            "evidence": [],
            // SPEC_V2 §5.4: an evidence-less gate needs a recorded waiver reason; this suite is
            // about the contract half of the design gate.
            "evidence_waiver": "the live delivery contract is the design gate's evidence",
        }),
        None,
    )
    .await
}

#[tokio::test]
async fn a_delivery_project_without_a_contract_cannot_pass_design() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Delivery").await;

    let (status, refused) = route(&f, project).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_contract_missing",
        "{refused}"
    );
    assert_eq!(
        arch::approval_count(&f.pg, project, "design").await,
        0,
        "the refused design gate wrote an approval row"
    );
}

#[tokio::test]
async fn a_superseded_contract_refuses_design() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Delivery").await;
    let contract = arch::live_contract(&f, &project.to_string(), "delivery_contract", "f").await;
    arch::ok(
        &f.running,
        "architecture.contracts_supersede",
        json!({ "id": contract }),
        Some("\"2\""),
    )
    .await;

    let (status, refused) = route(&f, project).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(
        refused["type"], "architecture_contract_not_live",
        "{refused}"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("superseded"),
        "the refusal does not name the status: {refused}"
    );
    assert_eq!(
        arch::approval_count(&f.pg, project, "design").await,
        0,
        "the refused design gate wrote an approval row"
    );
}

#[tokio::test]
async fn a_live_contract_opens_the_design_gate() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Delivery").await;
    let contract = arch::live_contract(&f, &project.to_string(), "delivery_contract", "f").await;
    // Phase G is the design gate for every delivery project: the live contract alone is not
    // enough, its compliance review is the second half of the gate.
    arch::created_as(
        &f.running,
        support::REVIEWER,
        "architecture.reviews_record",
        json!({
            "project_id": project.to_string(),
            "contract_id": contract,
            "level": "compliant",
            "delegation_ref": f.delegation,
            "findings": [{ "finding": "the design is compliant" }],
            "evidence": { "ref": "design gate" },
        }),
        None,
    )
    .await;

    let (status, out) = route(&f, project).await;
    assert_eq!(status, 201, "{out}");
    assert!(out["approval_id"].is_string(), "{out}");
    assert_eq!(arch::approval_count(&f.pg, project, "design").await, 1);
}
