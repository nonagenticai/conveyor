//! S10 §7 (a): a tool call that must be approved becomes a proposal and runs nothing.
//!
//! The model asks for `cards_move` — a `write` with `proposal_required = true` — so the turn ends
//! `proposed`, the approval is created in the same transaction, and **no** `cards.move` invocation
//! exists. The proposed input keeps `_version`, which the execution path lifts to the precondition
//! that pins the observed version (CONTRACT §3).

mod copilot;

use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_call_that_must_be_approved_becomes_one_pending_approval() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    let conversation = copilot::conversation(&running, "dev:alice").await;

    let card = Uuid::now_v7();
    let list = Uuid::now_v7();
    model.tool_call(
        "cards_move",
        json!({"id": card, "list_id": list, "_version": 3}),
    );

    let (status, body) = copilot::turn(
        &running,
        "dev:alice",
        conversation,
        "move card C to the Done list",
    )
    .await;
    assert_eq!(status, 201, "{body}");

    let turn = &body["turn"];
    assert_eq!(turn["meta"]["status"], "proposed", "{body}");
    assert_eq!(turn["meta"]["model"]["action"], "completions.create");
    let approval_id = body["proposal_id"]
        .as_str()
        .expect("a proposal id on the turn");

    // The whole point: nothing ran.
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "cards.move").await,
        0,
        "a proposal must not invoke the action"
    );

    // The proposal document, read from the approval row. `expires_at IS NULL` is the owner ruling
    // (2026-11-09): approvals never expire, so a proposal must not carry a TTL that would let the
    // sweep cancel the run while a person is still deciding.
    let (kind, status, meta, requested_by, expires_is_null): (
        String,
        String,
        Value,
        Option<Uuid>,
        bool,
    ) = sqlx::query_as(
        "SELECT kind, status, meta, requested_by, expires_at IS NULL FROM approval WHERE id = $1",
    )
    .bind(Uuid::parse_str(approval_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the approval exists");

    assert!(
        expires_is_null,
        "a proposal carries no expiry: approvals never expire"
    );

    assert_eq!(kind, "copilot.action");
    assert_eq!(status, "pending");
    assert_eq!(
        requested_by,
        Some(seeded.user_id),
        "the proposer is the caller"
    );
    assert_eq!(meta["action"], "cards.move");
    assert_eq!(meta["effect"], "write");
    assert_eq!(meta["input"]["id"], card.to_string());
    assert_eq!(meta["input"]["list_id"], list.to_string());
    assert_eq!(
        meta["input"]["_version"], 3,
        "the observed version must be pinned in the proposal"
    );
    assert_eq!(
        meta["turn_id"], turn["id"],
        "uq_approval_copilot_turn keys on the turn"
    );
    assert_eq!(meta["conversation_id"], conversation.to_string());
    assert_eq!(meta["proposer"]["actor_kind"], "user");
    let decide_via = meta["decide_via"].as_array().expect("decide_via");
    for channel in ["web", "telegram", "mcp"] {
        assert!(
            decide_via.iter().any(|value| value == channel),
            "the proposal must be decidable over {channel}: {meta}"
        );
    }
    let diff = meta["diff"].as_array().expect("diff");
    assert!(
        diff.iter().any(|entry| entry["path"] == "/list_id"),
        "the diff names the field the proposal sets: {meta}"
    );

    // A second turn with the same tool call raises its own proposal — one per turn is the index's
    // rule, not a global one.
    model.tool_call(
        "cards_move",
        json!({"id": card, "list_id": list, "_version": 3}),
    );
    let (status, second) = copilot::turn(&running, "dev:alice", conversation, "again").await;
    assert_eq!(status, 201, "{second}");
    assert_ne!(
        second["proposal_id"], body["proposal_id"],
        "a new turn raises a new proposal"
    );
    assert_eq!(
        copilot::invocations(&pg, seeded.tenant_id, "cards.move").await,
        0
    );
}
