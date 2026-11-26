//! #744 — `lean_proof` as typed gate evidence.
//!
//! A gate that pins a specification digest accepts a certificate only when it is `proven`, on
//! that pin, with a digest that still describes its own columns. Every refusal names which check
//! failed, because §5.4's test is not that a gate can open — it is that a gate can **refuse**, and
//! say why.
//!
//! The evidence resolver is the same function `pm.route_gate` calls; these tests open the gate
//! through `approvals.request`, which needs no PM job binding and exercises the identical arm.
//!
//! `rbuild -- test -p conveyor-actions --test pm_gate_proof`.

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
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    proof::prover().await;
    proof::seal_lean4(&pg, seeded.tenant).await;
    (pg, seeded)
}

/// A delivery project the gate can hang on.
async fn project(pg: &support::Pg, seeded: &support::Seeded) -> String {
    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "projects.create",
            seeded.user_id,
            json!({"title": "proof gate", "project_type_code": "delivery"}),
            None,
        ),
    )
    .await
    .expect("projects.create");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    output(&outcome.body)["id"]
        .as_str()
        .expect("the project id")
        .to_owned()
}

/// A `approvals.request` body citing one certificate.
fn gate(project_id: &str, proof_id: &str, pin: Option<&str>) -> Value {
    let mut input = json!({
        "project_id": project_id,
        "gate": "release",
        "kind": "programme",
        "description": "the release gate",
        "require_evidence": true,
        "evidence": [{ "kind": "lean_proof", "id": proof_id }],
    });
    if let Some(pin) = pin {
        input["pinned_spec_sha256"] = json!(pin);
    }
    input
}

async fn request(
    pg: &support::Pg,
    seeded: &support::Seeded,
    input: Value,
) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
    conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("approvals.request", seeded.user_id, input, None),
    )
    .await
}

/// #746 — a gate that *names* `lean_proof` as required refuses to open without one, and opens
/// with it. `require_evidence` alone is advisory: it only refuses an empty list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_required_kind_is_refused_without_it_and_accepted_with_it() {
    let (pg, seeded) = fixture().await;
    let project_id = project(&pg, &seeded).await;
    let (proof_id, certificate) = proof::record(&pg, &seeded, "required").await;

    let mut without = gate(&project_id, &proof_id, Some(&proof::spec_sha256()));
    without["evidence"] = json!([]);
    without["required_kinds"] = json!(["lean_proof"]);
    let error = request(&pg, &seeded, without)
        .await
        .expect_err("a gate that requires a proof cannot open without one");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_missing", "{error:?}");
    assert!(
        error.detail.contains("lean_proof"),
        "the refusal names the missing kind: {error:?}"
    );

    // With the certificate named, the same gate opens and its meta cites the certificate.
    let mut with = gate(&project_id, &proof_id, Some(&proof::spec_sha256()));
    with["required_kinds"] = json!(["lean_proof"]);
    let outcome = request(&pg, &seeded, with)
        .await
        .expect("a gate that requires a proof opens when one is named");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    let approval_id = output(&outcome.body)["id"]
        .as_str()
        .expect("the approval id")
        .to_owned();
    let meta: Value = sqlx::query_scalar("SELECT meta FROM approval WHERE id = $1")
        .bind(Uuid::parse_str(&approval_id).expect("the approval id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the approval row");
    assert_eq!(meta["evidence"][0]["certificate_sha256"], certificate);
}

/// A required kind that no evidence item can satisfy is refused with its own code, not silently
/// ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_required_kind_is_refused() {
    let (pg, seeded) = fixture().await;
    let project_id = project(&pg, &seeded).await;
    let (proof_id, _) = proof::record(&pg, &seeded, "unknown-kind").await;

    let mut input = gate(&project_id, &proof_id, Some(&proof::spec_sha256()));
    input["required_kinds"] = json!(["vibes"]);
    let error = request(&pg, &seeded, input)
        .await
        .expect_err("a kind that is not typed cannot be required");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_unknown_kind", "{error:?}");
}

/// P4.1 — a valid proof on the pinned specification opens the gate, and the meta cites it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_valid_proof_opens_the_gate() {
    let (pg, seeded) = fixture().await;
    let project_id = project(&pg, &seeded).await;
    let (proof_id, certificate) = proof::record(&pg, &seeded, "valid").await;

    let outcome = request(
        &pg,
        &seeded,
        gate(&project_id, &proof_id, Some(&proof::spec_sha256())),
    )
    .await
    .expect("a proven certificate opens the gate");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    let approval_id = output(&outcome.body)["id"]
        .as_str()
        .expect("the approval id")
        .to_owned();
    let meta: Value = sqlx::query_scalar("SELECT meta FROM approval WHERE id = $1")
        .bind(Uuid::parse_str(&approval_id).expect("the approval id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("the approval row");

    assert_eq!(meta["pinned_spec_sha256"], proof::spec_sha256());
    let evidence = meta["evidence"].as_array().expect("the resolved evidence");
    assert_eq!(evidence.len(), 1, "{meta}");
    assert_eq!(evidence[0]["kind"], "lean_proof");
    assert_eq!(evidence[0]["id"], proof_id);
    assert_eq!(evidence[0]["certificate_sha256"], certificate);
    assert_eq!(evidence[0]["spec_sha256"], proof::spec_sha256());
}

/// P4.2 — every bad certificate is refused with its own code, and the gate never opens.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_bad_proof_is_refused() {
    let (pg, seeded) = fixture().await;
    let project_id = project(&pg, &seeded).await;
    let (proof_id, _) = proof::record(&pg, &seeded, "refused").await;

    // A stale pin: the certificate answers a different specification.
    let stale = "b".repeat(64);
    let error = request(&pg, &seeded, gate(&project_id, &proof_id, Some(&stale)))
        .await
        .expect_err("a proof on another specification does not evidence this gate");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_proof_wrong_spec", "{error:?}");

    // No pin at all: a proof of an unnamed specification evidences nothing.
    let error = request(&pg, &seeded, gate(&project_id, &proof_id, None))
        .await
        .expect_err("a gate without a pin cannot accept a certificate");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_proof_unpinned", "{error:?}");

    // A row rewritten after the prover answered.
    sqlx::query("UPDATE proof SET certificate_sha256 = $1 WHERE id = $2")
        .bind("c".repeat(64))
        .bind(Uuid::parse_str(&proof_id).expect("the proof id"))
        .execute(&pg.super_pool)
        .await
        .expect("tamper the certificate digest");
    let error = request(
        &pg,
        &seeded,
        gate(&project_id, &proof_id, Some(&proof::spec_sha256())),
    )
    .await
    .expect_err("a tampered certificate does not open a gate");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_proof_digest", "{error:?}");

    // An honest row, but the prover said it was not proven.
    let refuted_source = proof::source(&proof::facts("unproven"));
    proof::prover().await.refute(&refuted_source);
    let (unproven_id, _) = proof::record(&pg, &seeded, "unproven").await;
    let error = request(
        &pg,
        &seeded,
        gate(&project_id, &unproven_id, Some(&proof::spec_sha256())),
    )
    .await
    .expect_err("an unproven certificate does not open a gate");
    assert_eq!(error.status, 422);
    assert_eq!(error.code, "gate_evidence_proof_unproven", "{error:?}");

    // Another tenant's certificate is invisible under RLS: the same id in tenant B's gate is
    // simply not found, never a certificate it cannot see.
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    // The catalogue seed walks the tenants that exist, so a tenant created after it needs it
    // re-run — otherwise `projects.create` refuses the project type in that tenant.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed for the second tenant");
    let other_project = project(&pg, &other).await;
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        other.tenant,
        proof::request(
            "approvals.request",
            other.user_id,
            gate(&other_project, &proof_id, Some(&proof::spec_sha256())),
            None,
        ),
    )
    .await
    .expect_err("a cross-tenant certificate is not evidence");
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "gate_evidence_not_found", "{error:?}");

    // And none of those opened a gate.
    let gates: i64 = sqlx::query_scalar("SELECT count(*) FROM approval WHERE project_id = $1")
        .bind(Uuid::parse_str(&project_id).expect("the project id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the gates");
    assert_eq!(gates, 0, "a refused gate writes no approval row");
}
