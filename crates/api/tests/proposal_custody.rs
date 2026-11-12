//! S10 §5.3 + finding #11 — a proposal for a secret-bearing action never stores the value in the
//! clear, and execution replays the **sealed** input.
//!
//! `secrets.rotate` is `destructive`, so an agent's call becomes a proposal. `approval.meta` is
//! readable by every member who may list approvals, so the proposal stores a redacted projection
//! and the exact input only as ciphertext under the tenant's KEK. The test reads the row as the
//! database holds it, then approves and executes through the real path:
//!
//! * the canary value appears **nowhere** in `meta` (input, diff, or the document as text);
//! * `sealed_input` is present — the exact input survives, sealed;
//! * after approval, execution rotates the secret with the sealed value (version 1 → 2), which is
//!   only possible if the unseal path works.

mod copilot;

use conveyor_domain::Decision;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A value distinctive enough that any leak into a JSON document is unambiguous.
const CANARY: &str = "canary-SECRET-value-42";
const REDACTED: &str = "<redacted>";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_secret_rotation_proposal_never_stores_its_value_in_the_clear() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;

    // The secret to rotate, created through the store (S09 owns the create path).
    let secret = {
        let mut tx = store::begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scoped tx");
        let record = store::secret::create_secret_tx(
            &mut tx,
            seeded.tenant,
            "rotated",
            b"old-value",
            "tenant",
        )
        .await
        .expect("create secret");
        tx.commit().await.expect("commit");
        record
    };

    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;
    model.tool_call(
        "secrets_rotate",
        json!({
            "id": secret.id.to_string(),
            "value": CANARY,
            // The reserved `_version` (CONTRACT §3): the proposal pins the observed version and the
            // execution path lifts it to the If-Match precondition, exactly as `cards.move` does.
            "_version": secret.version,
        }),
    );

    let (status, body) = copilot::turn(
        &running,
        "dev:alice",
        conversation,
        "rotate the webhook secret",
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["turn"]["meta"]["status"], "proposed", "{body}");
    let approval_id: Uuid = body["proposal_id"]
        .as_str()
        .expect("a proposal id")
        .parse()
        .expect("uuid");

    // The row exactly as a member could read it: projection redacted, exact input only as ciphertext.
    let meta: Value = sqlx::query_scalar("SELECT meta FROM approval WHERE id = $1")
        .bind(approval_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the proposal row");
    assert_eq!(meta["action"], "secrets.rotate");
    assert_eq!(
        meta["input"]["value"], REDACTED,
        "the visible input must be redacted: {meta}"
    );
    let diff = meta["diff"].as_array().expect("a diff array");
    let value_entry = diff
        .iter()
        .find(|entry| entry["path"] == "/value")
        .expect("the diff carries the value field");
    assert_eq!(
        value_entry["to"], REDACTED,
        "the diff must not show the value: {meta}"
    );
    assert!(
        !meta.to_string().contains(CANARY),
        "the canary must appear nowhere in approval.meta: {meta}"
    );
    assert!(
        meta["sealed_input"].is_string() && meta["sealed_input_key_id"].is_string(),
        "the exact input survives — sealed: {meta}"
    );

    // Approve through the gate machine, then execute: only the unsealed input may run.
    let mut tx = store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    store::approval::record_decision_tx(
        &mut tx,
        seeded.tenant,
        approval_id,
        &Decision::approve(seeded.user_id),
        false,
    )
    .await
    .expect("approve");
    tx.commit().await.expect("commit");

    let outcome = conveyor_api::copilot::proposal::execute(
        &pg.runtime,
        "conveyor",
        seeded.tenant,
        approval_id,
    )
    .await
    .expect("execute");
    assert!(
        matches!(
            outcome,
            conveyor_api::copilot::proposal::Execution::Executed(_)
        ),
        "the proposal must execute from its seal: {outcome:?}"
    );

    let version: i64 =
        sqlx::query_scalar("SELECT version FROM secret WHERE id = $1 AND tenant_id = $2")
            .bind(secret.id)
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the secret row");
    assert_eq!(version, 2, "the rotation ran with the sealed value");
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "secrets.rotate").await,
        1,
        "exactly one invocation"
    );
}
