//! #837 — the unit's Definition of done resolves the engine-derived certificate.
//!
//! The policy the charter pinned selects the mode. Advisory reports the missing certificate in
//! the closure without blocking; required refuses the close with the same typed
//! `gate_evidence_missing` a PM gate returns, and opens only after a proven certificate over the
//! current facts and the policy's specification pin exists.
//!
//! The one judging-surface exception is tested here too: `proof_policy.required` may be toggled,
//! the specification pin never, and the implementer may never loosen the gate that judges it.
//!
//! `rbuild -- test -p conveyor-actions --test pm_gate_unit_proof`.

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
    proof::prover().await;
    proof::seal_lean4(&pg, seeded.tenant).await;
    (pg, seeded)
}

async fn close(
    pg: &support::Pg,
    seeded: &support::Seeded,
    development_project_id: &str,
) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
    conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "development_projects.close",
            seeded.user_id,
            json!({"development_project_id": development_project_id}),
            None,
        ),
    )
    .await
}

async fn toggle(
    pg: &support::Pg,
    seeded: &support::Seeded,
    actor: Uuid,
    development_project_id: &str,
    change: Value,
) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
    conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "development_projects.change_judging_surface",
            actor,
            json!({
                "development_project_id": development_project_id,
                "surface": "proof_policy",
                "change": change,
            }),
            None,
        ),
    )
    .await
}

async fn required_flag(pg: &support::Pg, tenant_id: Uuid, development_project_id: &str) -> bool {
    sqlx::query_scalar(
        "SELECT required FROM proof_policy
          WHERE tenant_id = $1 AND development_project_id = $2",
    )
    .bind(tenant_id)
    .bind(Uuid::parse_str(development_project_id).expect("a development project id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the policy row")
}

/// P3.1 — advisory mode reports the missing certificate and lets the close proceed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn advisory_reports_missing_lean_proof() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 911, false).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let outcome = close(&pg, &seeded, &unit.development_project_id)
        .await
        .expect("an advisory unit closes");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let dod = &output(&outcome.body)["closure"]["dod"];
    assert_eq!(dod["required"], false);
    assert_eq!(dod["lean_proof"], "missing");
}

/// P3.1/P3.3 — required mode refuses without the certificate and opens with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn required_unit_needs_certificate() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 912, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let refused = close(&pg, &seeded, &unit.development_project_id)
        .await
        .expect_err("a required unit cannot close without its certificate");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "gate_evidence_missing");
    assert!(
        refused.detail.contains("lean_proof"),
        "the refusal names the missing kind: {refused:?}"
    );

    let (proof_id, _) = proof::record_unit(&pg, &seeded, &unit).await;
    let closed = close(&pg, &seeded, &unit.development_project_id)
        .await
        .expect("the certified unit closes");
    assert_eq!(closed.status, 201, "{}", closed.body);
    let dod = &output(&closed.body)["closure"]["dod"];
    assert_eq!(dod["required"], true);
    assert_eq!(dod["lean_proof"], "proven");
    assert_eq!(
        dod["proof_id"], proof_id,
        "the closure names the certificate it resolved"
    );
    assert_eq!(dod["evidence"]["spec_sha256"], proof::spec_sha256());
}

/// The policy's one admissible change: `required`. The implementer may tighten it, never loosen
/// it, and the specification pin is never rewritten.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn change_judging_surface_toggles_required_but_never_repins() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 913, true).await;
    let reviewer = support::reviewer_id(&pg).await;

    let loosened = toggle(
        &pg,
        &seeded,
        seeded.user_id,
        &unit.development_project_id,
        json!({"required": false}),
    )
    .await
    .expect_err("the implementer cannot loosen its own gate");
    assert_eq!(loosened.status, 403, "{loosened:?}");
    assert_eq!(loosened.code, "judging_surface_immutable");

    let repin = toggle(
        &pg,
        &seeded,
        reviewer,
        &unit.development_project_id,
        json!({"spec_sha256": "0".repeat(64)}),
    )
    .await
    .expect_err("the specification pin is frozen at charter");
    assert_eq!(repin.status, 403, "{repin:?}");
    assert_eq!(repin.code, "judging_surface_immutable");

    // A principal other than the implementer may loosen it.
    let admitted = toggle(
        &pg,
        &seeded,
        reviewer,
        &unit.development_project_id,
        json!({"required": false}),
    )
    .await
    .expect("the reviewer may toggle the requirement");
    assert_eq!(admitted.status, 200, "{}", admitted.body);
    assert_eq!(
        required_flag(&pg, seeded.tenant_id, &unit.development_project_id).await,
        false
    );

    // The implementer may tighten it again — and still may not loosen it.
    let tightened = toggle(
        &pg,
        &seeded,
        seeded.user_id,
        &unit.development_project_id,
        json!({"required": true}),
    )
    .await
    .expect("the implementer may tighten the requirement");
    assert_eq!(tightened.status, 200, "{}", tightened.body);
    assert_eq!(
        required_flag(&pg, seeded.tenant_id, &unit.development_project_id).await,
        true
    );
    let reloosened = toggle(
        &pg,
        &seeded,
        seeded.user_id,
        &unit.development_project_id,
        json!({"required": false}),
    )
    .await
    .expect_err("the implementer still cannot loosen it");
    assert_eq!(reloosened.status, 403, "{reloosened:?}");
}
