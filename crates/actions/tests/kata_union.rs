//! The kata union — the lane's candidate/propagation/close surface on main's kata loop
//! (SPEC_V2 §6.3, §9.3–§9.4, §12 T5; #526, D-U1…D-U4).
//!
//! Main's loop stays the runtime (its own tests cover open/get/observe/analyse/hypothesise/
//! propose/measure/adopt/revert). This file proves the union's additions: a propagation row
//! starts pending, a reverted close must name the restore, a close is one-way, and the list
//! reads the closed row.

mod support;

use serde_json::json;

#[tokio::test]
async fn a_kata_propagates_closes_and_lists() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let instrument =
        support::kata::instrument(&pg, &anchor.seeded, &[0.5], &[0.0], &[0.5], &[0.0]).await;
    let running = support::Running::start(&pg).await;
    let kata = support::kata::open(
        &running,
        &anchor,
        instrument.version_id,
        "union-propagate",
        "process:union",
        0.1,
    )
    .await;

    // A propagation row is created pending; the target carries its own later gate.
    let (status, body) = running
        .invoke(
            "kata.propagate",
            "dev:alice",
            json!({
                "kata_id": kata,
                "target_ref": {"kind": "job", "id": "job-1", "base_version": 1},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["status"], "pending", "{body}");
    assert_eq!(body["output"]["kata_id"], kata, "{body}");

    // The list reads it back.
    let (status, body) = running
        .invoke("kata.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        body["output"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["id"] == kata)),
        "the kata is listed: {body}"
    );

    // A reverted close without the restored version is refused by name.
    let (status, body) = running
        .invoke(
            "kata.close",
            "dev:alice",
            json!({"kata_id": kata, "outcome": "reverted", "reason": "regression"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["code"], "kata_revert_missing", "{body}");
    assert_eq!(body["errors"][0]["field"], "restored_ref", "{body}");

    // A kept close lands; a second close is refused.
    let (status, body) = running
        .invoke(
            "kata.close",
            "dev:alice",
            json!({"kata_id": kata, "outcome": "kept", "reason": "the hypothesis held"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "closed", "{body}");
    let (status, body) = running
        .invoke(
            "kata.close",
            "dev:alice",
            json!({"kata_id": kata, "outcome": "kept", "reason": "again"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "kata_not_open", "{body}");
}
