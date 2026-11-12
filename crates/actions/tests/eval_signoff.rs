//! `eval_signoffs.*` — requesting and deciding the instrument's two pauses (S19 step 7).
//!
//! The three properties this file proves:
//! 1. a request freezes the digest of the instrument **as it is now** (not the version's stored
//!    digest) and ignores a supplied artifact for the two gating pauses;
//! 2. one gate has one pending sign-off — the second request is refused by the unique index,
//!    surfaced as `409 signoff_pending`;
//! 3. a sign-off is a human act: a member without `evals:approve` is refused by the scope, an
//!    agent principal by the allowlist, and a service principal — which the allowlist does not
//!    cover — by the handler's own `403 signoff_requires_human`.

mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn add_principal(
    pg: &support::Pg,
    tenant: TenantId,
    tenant_id: Uuid,
    login: &str,
    role: &str,
    kind: &str,
) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(format!("dev:{login}"))
    .bind(format!("{login}@dev.invalid"))
    .bind(login)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("insert principal");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(&mut *tx)
        .await
        .expect("insert membership");
    tx.commit().await.expect("commit membership");
    user_id
}

/// Create an eval and its first version; `(eval_id, version_id)`.
async fn eval_with_version(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "signoff",
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

/// Request a sign-off and return the approval id.
async fn request(
    running: &support::Running,
    token: &str,
    eval_id: &str,
    gate: &str,
    subject: Option<&str>,
) -> (u16, Value) {
    let mut body = json!({ "eval_id": eval_id, "gate": gate, "ask": "please sign" });
    if let Some(subject) = subject {
        body["subject_sha256"] = json!(subject);
    }
    running
        .invoke("eval_signoffs.request", token, body, None, None, None)
        .await
}

#[tokio::test]
async fn a_request_stores_the_gate_and_the_current_digest_and_admits_one_pending_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, version_id) = eval_with_version(&running).await;

    // A supplied artifact cannot change what a gating sign-off covers.
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_inputs", Some("deadbeef")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["gate"], "eval_inputs");
    assert_eq!(body["output"]["kind"], "eval");
    assert_eq!(body["output"]["status"], "pending");
    assert!(body["output"]["run_id"].is_null(), "{body}");
    assert_eq!(body["output"]["required_decisions"], 1);
    let inputs_approval = body["output"]["id"]
        .as_str()
        .expect("approval id")
        .to_owned();

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let digest = store::eval::version_digest_tx(
        &mut tx,
        seeded.tenant,
        Uuid::parse_str(&version_id).expect("version uuid"),
    )
    .await
    .expect("the instrument digest");
    let row: (Option<Uuid>, String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT eval_id, gate, kind, meta->>'artifact_sha256', meta->>'ask'
           FROM approval WHERE id = $1",
    )
    .bind(Uuid::parse_str(&inputs_approval).expect("approval uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("read the eval gate");
    tx.commit().await.expect("commit");
    assert_eq!(row.0, Some(Uuid::parse_str(&eval_id).expect("eval uuid")));
    assert_eq!(row.1, "eval_inputs");
    assert_eq!(row.2, "eval");
    assert_eq!(
        row.3,
        Some(digest.clone()),
        "a gating sign-off always signs the instrument as it is now"
    );
    assert_eq!(row.4.as_deref(), Some("please sign"));

    // One pending sign-off per (tenant, eval, gate): the index is the guarantee.
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_inputs", None).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "signoff_pending");

    // A different gate is a different row.
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_grading", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["gate"], "eval_grading");
    let grading_approval = body["output"]["id"].as_str().expect("id").to_owned();

    // The two recorded-but-not-gating pauses take the caller's artifact, because a cost
    // ceiling or a harness path is not part of the instrument digest.
    let (status, body) = request(
        &running,
        TOKEN,
        &eval_id,
        "eval_cost",
        Some("cost-artifact"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let cost: Option<String> =
        sqlx::query_scalar("SELECT meta->>'artifact_sha256' FROM approval WHERE id = $1")
            .bind(Uuid::parse_str(body["output"]["id"].as_str().expect("id")).expect("uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("read the cost gate");
    tx.commit().await.expect("commit");
    assert_eq!(cost.as_deref(), Some("cost-artifact"));

    // `eval_harness` without an artifact falls back to the digest, exactly like the gating two.
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_harness", None).await;
    assert_eq!(status, 200, "{body}");

    // ── decide ──
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": inputs_approval, "decision": "approve", "note": "read it" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "approved");
    assert_eq!(body["output"]["approvals"], 1);

    // One approver, one decision: a second approve by the same person is a conflict.
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": inputs_approval, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");

    // A rejection resolves its own gate immediately.
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": grading_approval, "decision": "reject", "note": "no" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "rejected");

    // A decision outside the vocabulary is a 422, not a silent approve.
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": grading_approval, "decision": "maybe" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "decision");

    running.shutdown().await;
}

#[tokio::test]
async fn only_a_human_with_the_approve_scope_may_decide() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (eval_id, _version_id) = eval_with_version(&running).await;

    let (status, body) = request(&running, TOKEN, &eval_id, "eval_inputs", None).await;
    assert_eq!(status, 200, "{body}");
    let approval_id = body["output"]["id"].as_str().expect("id").to_owned();

    // A member builds, but does not sign: `evals:approve` is Owner|Admin.
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "molly",
        "member",
        "human",
    )
    .await;
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            "dev:molly",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "action-forbidden");

    // A machine principal cannot reach the action at all (it is not in the MCP channel
    // default), even holding a role whose scopes would allow it.
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "machine",
        "admin",
        "agent",
    )
    .await;
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            "dev:machine",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 403,
        "an agent principal must never sign off: {body}"
    );

    // S06d: a service credential is the tenant's MCP channel, not a role holder, so the grant
    // gate refuses it before the handler. The handler's own `signoff_requires_human` check is
    // still the second door — the store test `a_machine_cannot_decide_an_eval_gate` proves it
    // at the shared writer, which every transport traverses.
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "svc",
        "admin",
        "service",
    )
    .await;
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            "dev:svc",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    // A machine channel is refused by the grant gate, which is the first door; the shared
    // writer is the second and does not depend on this transport.
    assert_eq!(body["type"], "action-forbidden");

    // The gate is still pending: none of the three refusals wrote a decision.
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "approved");
    assert_eq!(body["output"]["approvals"], 1);

    running.shutdown().await;
}

#[tokio::test]
async fn a_non_eval_approval_is_not_decidable_here() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // An ordinary gate — no `eval_id` — inserted through the store, exactly like
    // `approvals.request` would (but with no parent, because this test needs no project).
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let plain = store::approval::insert_approval_tx(
        &mut tx,
        seeded.tenant,
        &store::approval::NewApproval::new("manual", "manual"),
    )
    .await
    .expect("insert a plain gate");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": plain.id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found");

    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": Uuid::now_v7(), "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn a_request_needs_a_version_a_known_gate_and_a_visible_eval() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    // No version: there is no instrument to sign off.
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "unversioned",
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
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_inputs", None).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "eval_not_ready");

    // A gate outside the four is a 422.
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_vibes", None).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "gate");

    // An eval in another tenant is a 404, never a row on someone else's instrument.
    let (status, body) = request(&running, "dev:bob", &eval_id, "eval_inputs", None).await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found");

    // And bob cannot decide it either.
    let (eval_id, _version) = eval_with_version(&running).await;
    let (status, body) = request(&running, TOKEN, &eval_id, "eval_inputs", None).await;
    assert_eq!(status, 200, "{body}");
    let approval_id = body["output"]["id"].as_str().expect("id").to_owned();
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            "dev:bob",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    running.shutdown().await;
}
