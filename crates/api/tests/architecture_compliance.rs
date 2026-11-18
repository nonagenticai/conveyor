//! T9 (#81) D4 — one compliance level per delivery contract, and a waiver with an owner and a
//! reassessment date.
//!
//! `rbuild -- test -p conveyor-api --test architecture_compliance`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;
#[path = "architecture_support/mod.rs"]
mod arch;

use serde_json::json;

#[tokio::test]
async fn one_level_and_a_dated_waiver() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Delivery").await;
    let project_id = project.to_string();
    let contract = arch::live_contract(
        &f.running,
        &project_id,
        "delivery_contract",
        "f",
    )
    .await;

    let review = arch::created(
        &f.running,
        "architecture.reviews_record",
        json!({
            "project_id": project_id,
            "contract_id": contract,
            "level": "compliant",
            "findings": [{ "finding": "one deviation, waived below" }],
            "evidence": [{ "ref": "run:1" }],
        }),
        None,
    )
    .await;
    let review_id = arch::id_of(&review);
    assert_eq!(review["level"], "compliant", "{review}");

    let (status, refused) = arch::call(
        &f.running,
        "architecture.reviews_record",
        json!({
            "project_id": project_id,
            "contract_id": contract,
            "level": "non_conformant",
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "a second level on the same contract: {refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");

    let (status, refused) = arch::call(
        &f.running,
        "architecture.reviews_waive",
        json!({
            "review_id": review_id,
            "reason": "accepted for this delivery",
            "cost_impact": "none",
            "risk_impact": "low",
        }),
        None,
    )
    .await;
    assert_eq!(status, 422, "a waiver with no owner and no date: {refused}");
    assert_eq!(
        refused["errors"][0]["code"], "owner_and_date_required",
        "{refused}"
    );

    let owner = support::reviewer_id(&f.pg).await;
    let waiver = arch::created(
        &f.running,
        "architecture.reviews_waive",
        json!({
            "review_id": review_id,
            "owner_user_id": owner,
            "reason": "accepted for this delivery",
            "cost_impact": "none",
            "risk_impact": "low",
            "reassessment_at": "2027-01-31T00:00:00Z",
        }),
        None,
    )
    .await;
    assert_eq!(waiver["owner_user_id"], owner.to_string(), "{waiver}");
    assert_eq!(waiver["status"], "active", "{waiver}");
}
