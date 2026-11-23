//! S21 step 5 — a generated draft is applied only by a decision bound to it (issue #590).
//!
//! `cargo test -p conveyor-actions --test draft_apply -- --nocapture`.
//!
//! Proves rework `v1-w1c-2:T2`: a generic accepted decision on the card, a decision naming
//! another draft, a mismatched prompt hash, a pending approval and a service actor's
//! `rung=human` all fail; the matching L1 and L3 decisions apply.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn create_policy(running: &support::Running) {
    let (status, body) = running
        .invoke(
            "decision_policies.create",
            TOKEN,
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "threshold": 0.8,
                "fail_policy": "closed",
                "action": "block"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

async fn create_laya_config(running: &support::Running) {
    let (status, body) = running
        .invoke(
            "laya_configs.create",
            TOKEN,
            json!({"model_ref": "jev-latest", "timeout_ms": 2500}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

/// A project, its board, an active lane and a card: returns `(project_id, card_id)`.
async fn project_and_card(running: &support::Running, title: &str) -> (String, String) {
    let (status, project) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = id_of(output(&project), "id");
    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (status, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    let lane_id = id_of(&lane["list"], "id");
    let (status, card) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board_id,
                "list_id": lane_id,
                "name": format!("Draft card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    (project_id, id_of(output(&card), "id"))
}

async fn compose_draft(
    running: &support::Running,
    project_id: &str,
    body: &str,
    meta: Value,
) -> String {
    let (status, draft) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({"title": body, "project_id": project_id, "body": body, "meta": meta}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{draft}");
    id_of(output(&draft), "id")
}

/// An accepted human `draft.apply` decision for the card, with whatever draft binding the
/// caller supplies. The apply gate is the product's current contract; the old `ladder.evaluate`
/// fixture predates the verified-laya input and no longer exercises the binding check.
async fn human_accepted(
    running: &support::Running,
    project_id: &str,
    card_id: &str,
    meta: Value,
) -> String {
    let (status, approval) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "project_id": project_id,
                "card_id": card_id,
                "gate": "gate.draft_apply",
                "kind": "decision",
                "description": "apply the generated draft"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approval_id = id_of(output(&approval), "id");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve", "note": "apply"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    let (status, decision) = running
        .invoke(
            "ladder.record",
            TOKEN,
            json!({
                "rung": "human",
                "kind": "gate.draft_apply",
                "gate": "gate.draft_apply",
                "project_id": project_id,
                "card_id": card_id,
                "answer": "approved",
                "outcome": "accepted",
                "approval_id": approval_id,
                "meta": meta
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{decision}");
    id_of(output(&decision), "id")
}

async fn apply(
    running: &support::Running,
    document: &str,
    card: &str,
    decision: &str,
) -> (u16, Value) {
    running
        .invoke(
            "ladder.apply_draft",
            TOKEN,
            json!({"document_id": document, "card_id": card, "decision_id": decision}),
            None,
            None,
            None,
        )
        .await
}

async fn document_card(running: &support::Running, document: &str) -> Option<String> {
    let (status, read) = running
        .invoke(
            "documents.get",
            TOKEN,
            json!({"id": document}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    output(&read)["card_id"].as_str().map(str::to_owned)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unbound_decision_cannot_apply_a_draft() {
    let (_pg, running, _seeded) = fixture().await;
    create_policy(&running).await;
    create_laya_config(&running).await;
    let (project_id, card_id) = project_and_card(&running, "Unbound draft").await;
    let document = compose_draft(
        &running,
        &project_id,
        "generated patch",
        json!({"generated": true}),
    )
    .await;
    // An accepted decision on the same card that never names the draft.
    let decision = human_accepted(&running, &project_id, &card_id, json!({})).await;

    let (status, refused) = apply(&running, &document, &card_id, &decision).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(output(&refused)["type"], "draft_requires_gate", "{refused}");
    assert_eq!(
        document_card(&running, &document).await,
        None,
        "nothing is attached by an unbound decision"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decision_bound_to_another_draft_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    create_policy(&running).await;
    create_laya_config(&running).await;
    let (project_id, card_id) = project_and_card(&running, "Bound draft").await;
    let first = compose_draft(
        &running,
        &project_id,
        "first draft",
        json!({"generated": true}),
    )
    .await;
    let second = compose_draft(
        &running,
        &project_id,
        "second draft",
        json!({"generated": true}),
    )
    .await;

    // Bound to the first draft: it applies there, and nowhere else.
    let bound = human_accepted(
        &running,
        &project_id,
        &card_id,
        json!({"draft_document_id": first}),
    )
    .await;
    let (status, refused) = apply(&running, &second, &card_id, &bound).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(output(&refused)["type"], "draft_requires_gate", "{refused}");
    let (status, applied) = apply(&running, &first, &card_id, &bound).await;
    assert_eq!(status, 200, "{applied}");

    // A document carrying a prompt hash refuses a decision that names another one.
    let hash = "a".repeat(64);
    let hashed = compose_draft(
        &running,
        &project_id,
        "hashed draft",
        json!({"generated": true, "prompt_sha256": hash}),
    )
    .await;
    let wrong = human_accepted(
        &running,
        &project_id,
        &card_id,
        json!({"draft_document_id": hashed, "prompt_sha256": "b".repeat(64)}),
    )
    .await;
    let (status, refused) = apply(&running, &hashed, &card_id, &wrong).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(output(&refused)["type"], "draft_requires_gate", "{refused}");
}

/// A human decision needs a person and an approval that actually resolved.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_decision_requires_a_person_and_an_approved_approval() {
    let (pg, running, seeded) = fixture().await;
    let (project_id, card_id) = project_and_card(&running, "Human gate").await;
    let document = compose_draft(
        &running,
        &project_id,
        "human draft",
        json!({"generated": true}),
    )
    .await;

    // A pending approval cannot gate a human decision.
    let (status, approval) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "project_id": project_id,
                "gate": "gate.precheck",
                "kind": "decision",
                "description": "pending"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let pending = id_of(output(&approval), "id");
    let (status, refused) = running
        .invoke(
            "ladder.record",
            TOKEN,
            json!({
                "rung": "human",
                "kind": "gate.draft_apply",
                "card_id": card_id,
                "answer": "approved",
                "outcome": "accepted",
                "approval_id": pending,
                "meta": {"draft_document_id": document}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "approval_not_accepted",
        "{refused}"
    );

    // A service actor cannot claim the human rung, even with an approved approval. The store
    // writer is the authority; the action layer's actor kind is not caller-supplied.
    let (status, approval) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "project_id": project_id,
                "gate": "gate.precheck",
                "kind": "decision",
                "description": "approved"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approved = id_of(output(&approval), "id");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approved, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");

    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let denied = conveyor_store::ladder::record_decision_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::ladder::NewDecision {
            kind: "gate.draft_apply".to_owned(),
            rung: conveyor_domain::ladder::Rung::Human,
            project_id: Some(Uuid::parse_str(&project_id).expect("uuid")),
            card_id: Some(Uuid::parse_str(&card_id).expect("uuid")),
            question: "Apply the draft?".to_owned(),
            answer: "approved".to_owned(),
            option_set_id: None,
            option_set_version: None,
            option_set_hash: None,
            option_refinement_hashes: Vec::new(),
            probabilities: json!({}),
            confidence: None,
            input_hash: None,
            policy_id: None,
            policy_version: None,
            policy_refinement_id: None,
            refinement_version: None,
            laya_config_id: None,
            laya_config_version: None,
            laya_invocation_id: None,
            laya_request_hash: None,
            laya_response_hash: None,
            actor_kind: "service".to_owned(),
            actor_id: Some(seeded.user_id),
            approval_id: Some(Uuid::parse_str(&approved).expect("uuid")),
            latency_ms: None,
            outcome: conveyor_domain::ladder::Outcome::Accepted,
            decided_at: Some(chrono::Utc::now()),
            meta: json!({"draft_document_id": document}),
            options: Vec::new(),
        },
    )
    .await
    .expect_err("a service actor cannot record a human decision");
    assert!(
        matches!(
            denied,
            conveyor_store::ladder::LadderStoreError::HumanRequiresPerson
        ),
        "{denied}"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approved_human_decision_applies_its_draft() {
    let (_pg, running, _seeded) = fixture().await;
    let (project_id, card_id) = project_and_card(&running, "Approved human gate").await;
    let document = compose_draft(
        &running,
        &project_id,
        "approved draft",
        json!({"generated": true}),
    )
    .await;

    let (status, approval) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "project_id": project_id,
                "card_id": card_id,
                "gate": "gate.draft_apply",
                "kind": "decision",
                "description": "apply the draft"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approval_id = id_of(output(&approval), "id");
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");

    let (status, decision) = running
        .invoke(
            "ladder.record",
            TOKEN,
            json!({
                "rung": "human",
                "kind": "gate.draft_apply",
                "gate": "gate.draft_apply",
                "project_id": project_id,
                "card_id": card_id,
                "answer": "approved",
                "outcome": "accepted",
                "approval_id": approval_id,
                "meta": {"draft_document_id": document}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{decision}");
    let decision_id = id_of(output(&decision), "id");

    let (status, applied) = apply(&running, &document, &card_id, &decision_id).await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(
        document_card(&running, &document).await.as_deref(),
        Some(card_id.as_str()),
        "the approved human decision applies the draft"
    );
}
