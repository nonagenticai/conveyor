//! T2 — the decision ladder's acceptance scenarios (issue #73) over real HTTP and real Postgres.
//!
//! Each test builds its own tenant through [`support::seed`]; the fixture is the S05/S08 shape:
//! an ephemeral Postgres 18, the two estate roles, the embedded migrations and a real server.
//! The assertions are the issue's:
//!
//! * P1.1 — an L0 rule, an L1 laya and an L3 human decision project the same columns;
//! * P2.1 — a tenant refinement changes a threshold with no code change and no redeploy;
//! * P3.1 — every decision pins the policy, refinement and laya-config versions it read, and a
//!   later change does not rewrite the earlier row.
//!
//! The superuser pool (`pg.super_pool`) is used only to prove a row's stored pins directly;
//! every behaviour under test goes through an action.

mod support;

use serde_json::{Value, json};

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
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

async fn create_policy(
    running: &support::Running,
    kind: &str,
    gate: Option<&str>,
    threshold: f64,
    fail_policy: &str,
    action: &str,
) -> (String, String) {
    let (status, headers, body) = running
        .invoke_full(
            "decision_policies.create",
            "dev:alice",
            json!({
                "kind": kind,
                "gate": gate,
                "threshold": threshold,
                "fail_policy": fail_policy,
                "action": action
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let etag = headers.get("etag").cloned().expect("an ETag on create");
    (id_of(output(&body), "id"), etag)
}

async fn create_laya_config(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "laya_configs.create",
            "dev:alice",
            json!({"model_ref": "jev-latest", "timeout_ms": 2500}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

async fn evaluate(running: &support::Running, confidence: f64) -> (u16, Value) {
    running
        .invoke(
            "ladder.evaluate",
            "dev:alice",
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "question": "Is the capability ready for the gate?",
                "answer": "pass",
                "confidence": confidence,
                "latency_ms": 17,
                "input": {"card": "card-1", "pin": "knowledge@7"}
            }),
            None,
            None,
            None,
        )
        .await
}

/// P1.1 — a rule, a laya and a human decision share one audit shape.
#[tokio::test(flavor = "multi_thread")]
async fn a_rule_a_laya_and_a_human_decision_share_one_audit_shape() {
    let (_pg, running, _seeded) = fixture().await;
    create_policy(&running, "gate.precheck", Some("capability_ready"), 0.8, "closed", "handover").await;
    create_laya_config(&running).await;

    // L0 — a deterministic rule outcome.
    let (status, rule) = running
        .invoke(
            "ladder.record",
            "dev:alice",
            json!({
                "rung": "rule",
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "answer": "pass",
                "outcome": "accepted",
                "confidence": 1.0,
                "latency_ms": 0
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{rule}");

    // L1 — an evaluated laya choice.
    let (status, laya) = evaluate(&running, 0.9).await;
    assert_eq!(status, 201, "{laya}");

    // L3 — a human decision, recorded against its approval.
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Human decision project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = id_of(output(&project), "id");
    let (status, approval) = running
        .invoke(
            "approvals.request",
            "dev:alice",
            json!({
                "project_id": project_id,
                "gate": "gate.precheck",
                "kind": "decision",
                "description": "Confirm the gate"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approval}");
    let approval_id = id_of(output(&approval), "id");
    let (status, human) = running
        .invoke(
            "ladder.record",
            "dev:alice",
            json!({
                "rung": "human",
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "project_id": project_id,
                "answer": "approve",
                "outcome": "accepted",
                "confidence": 1.0,
                "latency_ms": 0,
                "approval_id": approval_id
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{human}");

    // The one shape: the three rows carry the same columns, and only their values differ.
    let (status, list) = running
        .invoke(
            "ladder.list",
            "dev:alice",
            json!({"kind": "gate.precheck"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    let items = list["items"].as_array().expect("items");
    assert_eq!(items.len(), 3, "{list}");

    let rungs: Vec<&str> = items
        .iter()
        .map(|row| row["rung"].as_str().expect("rung"))
        .collect();
    assert!(rungs.contains(&"rule"), "{rungs:?}");
    assert!(rungs.contains(&"laya"), "{rungs:?}");
    assert!(rungs.contains(&"human"), "{rungs:?}");

    let shape: Vec<Vec<&str>> = items
        .iter()
        .map(|row| {
            let mut keys: Vec<&str> = row
                .as_object()
                .expect("a decision object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            keys
        })
        .collect();
    assert_eq!(shape[0], shape[1], "the rule and laya rows disagree on columns");
    assert_eq!(shape[1], shape[2], "the laya and human rows disagree on columns");

    for row in items {
        assert_eq!(row["kind"], "gate.precheck", "{row}");
        assert!(row["policy_id"].is_string(), "every rung names its policy: {row}");
        assert!(row["policy_version"].is_number(), "{row}");
        assert!(row["answer"].is_string(), "{row}");
        assert!(row["confidence"].is_number(), "{row}");
        assert!(row["actor_kind"].is_string(), "{row}");
        assert!(row["latency_ms"].is_number(), "{row}");
    }

    let human_row = items
        .iter()
        .find(|row| row["rung"] == "human")
        .expect("the human row");
    assert_eq!(human_row["approval_id"], approval_id, "{human_row}");
    let laya_row = items
        .iter()
        .find(|row| row["rung"] == "laya")
        .expect("the laya row");
    assert_eq!(laya_row["outcome"], "accepted", "{laya_row}");
}

/// P2.1 — a tenant refinement changes the threshold with no code change.
#[tokio::test(flavor = "multi_thread")]
async fn a_refinement_changes_a_threshold_without_a_code_change() {
    let (_pg, running, _seeded) = fixture().await;
    let (policy_id, _etag) =
        create_policy(&running, "gate.precheck", Some("capability_ready"), 0.8, "closed", "block").await;

    // The platform policy at 0.8 refuses an 0.7 answer.
    let (status, refused) = evaluate(&running, 0.7).await;
    assert_eq!(status, 201, "{refused}");
    assert_eq!(output(&refused)["outcome"], "refused", "{refused}");
    assert_eq!(output(&refused)["policy_version"], 1, "{refused}");

    // The tenant publishes a 0.5 refinement — configuration, not a build.
    let (status, published) = running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({"policy_id": policy_id, "threshold_override": 0.5}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(output(&published)["version"], 1, "{published}");

    // The identical input is now accepted, and the row pins the refinement it applied.
    let (status, accepted) = evaluate(&running, 0.7).await;
    assert_eq!(status, 201, "{accepted}");
    assert_eq!(output(&accepted)["outcome"], "accepted", "{accepted}");
    assert_eq!(output(&accepted)["refinement_version"], 1, "{accepted}");
    assert_eq!(output(&accepted)["policy_version"], 1, "{accepted}");
    assert_eq!(
        output(&accepted)["policy_refinement_id"],
        published["output"]["id"],
        "{accepted}"
    );
}

/// P3.1 — every decision pins the versions it read; a later change does not rewrite the row.
#[tokio::test(flavor = "multi_thread")]
async fn every_decision_pins_the_refinement_and_laya_config_versions() {
    let (pg, running, _seeded) = fixture().await;
    let (policy_id, etag) =
        create_policy(&running, "gate.precheck", Some("capability_ready"), 0.8, "closed", "block").await;
    running
        .invoke(
            "refinements.publish",
            "dev:alice",
            json!({"policy_id": policy_id, "threshold_override": 0.6}),
            None,
            None,
            None,
        )
        .await;

    // A laya decision with no laya config to pin is refused: the pin is mandatory, not a default.
    let (status, unpinned) = evaluate(&running, 0.9).await;
    assert_eq!(status, 422, "{unpinned}");
    assert_eq!(output(&unpinned)["type"], "pins_incomplete", "{unpinned}");

    create_laya_config(&running).await;

    let (status, first) = evaluate(&running, 0.9).await;
    assert_eq!(status, 201, "{first}");
    assert_eq!(output(&first)["policy_version"], 1, "{first}");
    assert_eq!(output(&first)["refinement_version"], 1, "{first}");
    assert_eq!(output(&first)["laya_config_version"], 1, "{first}");

    // The policy moves to version 2; the decision already recorded still pins version 1.
    let (status, updated) = running
        .invoke(
            "decision_policies.update",
            "dev:alice",
            json!({"id": policy_id, "threshold": 0.7}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["version"], 2, "{updated}");

    let (status, second) = evaluate(&running, 0.9).await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(output(&second)["policy_version"], 2, "{second}");

    // The first row's pins are unchanged — the row is reproducible, not rewritten.
    let first_id = id_of(output(&first), "id");
    let pinned: i64 = sqlx::query_scalar("SELECT policy_version FROM decision WHERE id = $1::uuid")
        .bind(&first_id)
        .fetch_one(&pg.owner)
        .await
        .expect("read the stored pin");
    assert_eq!(pinned, 1, "the recorded decision's pin moved with the policy");
}

/// P4.1/P4.2 — a generated draft is applied only through the gated action.
///
/// The compile-time half is the `compile_fail` doctest in `conveyor_domain::ladder`; this test
/// is the runtime half on the same rule: an accepted L1 decision is the only way a generated
/// draft reaches a card, and the ordinary edit path refuses the generated class outright.
#[tokio::test(flavor = "multi_thread")]
async fn a_generated_draft_cannot_transition_a_card() {
    let (_pg, running, _seeded) = fixture().await;
    create_policy(&running, "gate.precheck", Some("capability_ready"), 0.8, "closed", "block").await;
    create_laya_config(&running).await;

    // A project, its board, an active lane and a card to apply to.
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Draft project", "project_type_code": "delivery"}),
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
            "dev:alice",
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
            "dev:alice",
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
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": lane_id,
                "name": "The card the draft wants to move",
                "card_type_code": "feature"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = id_of(output(&card), "id");

    // A below-threshold decision: the gate refuses, and it cannot apply a draft.
    let (status, refused) = running
        .invoke(
            "ladder.evaluate",
            "dev:alice",
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "card_id": card_id,
                "answer": "pass",
                "confidence": 0.4,
                "latency_ms": 5
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{refused}");
    assert_eq!(output(&refused)["outcome"], "refused", "{refused}");

    // A generated draft artefact.
    let (status, draft) = running
        .invoke(
            "documents.compose",
            "dev:alice",
            json!({
                "title": "Generated patch",
                "project_id": project_id,
                "body": "the model's proposal",
                "meta": {"generated": true}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{draft}");
    let document_id = id_of(output(&draft), "id");

    // Applying with a refused decision is refused; nothing is attached.
    let (status, refused_apply) = running
        .invoke(
            "ladder.apply_draft",
            "dev:alice",
            json!({
                "document_id": document_id,
                "card_id": card_id,
                "decision_id": id_of(output(&refused), "id")
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused_apply}");

    // An accepted L1 decision is the gate.
    let (status, accepted) = running
        .invoke(
            "ladder.evaluate",
            "dev:alice",
            json!({
                "kind": "gate.precheck",
                "gate": "capability_ready",
                "card_id": card_id,
                "answer": "pass",
                "confidence": 0.9,
                "latency_ms": 5
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{accepted}");
    assert_eq!(output(&accepted)["outcome"], "accepted", "{accepted}");
    let decision_id = id_of(output(&accepted), "id");

    let (status, applied) = running
        .invoke(
            "ladder.apply_draft",
            "dev:alice",
            json!({"document_id": document_id, "card_id": card_id, "decision_id": decision_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(output(&applied)["card_id"], card_id, "{applied}");

    let (status, read_back) = running
        .invoke(
            "documents.get",
            "dev:alice",
            json!({"id": document_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read_back}");
    assert_eq!(output(&read_back)["card_id"], card_id, "{read_back}");

    // The ordinary edit path refuses the generated class, so the gate has no bypass.
    let (status, headers, second) = running
        .invoke_full(
            "documents.compose",
            "dev:alice",
            json!({
                "title": "Another generated patch",
                "project_id": project_id,
                "body": "another proposal",
                "meta": {"generated": true}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second}");
    let second_id = id_of(output(&second), "id");
    let etag = headers.get("etag").cloned().expect("an ETag on compose");
    let (status, bypass) = running
        .invoke(
            "documents.update",
            "dev:alice",
            json!({"id": second_id, "card_id": card_id}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "the generated draft was attached by an edit: {bypass}");
    assert_eq!(output(&bypass)["type"], "draft_requires_gate", "{bypass}");
}
