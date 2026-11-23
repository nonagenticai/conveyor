//! `cards.move` enforces the card type's Definition of Done (finding `v1-p54-1`).
//!
//! SPEC_V2 §5.4: the Definition of Done lives on `card_type.definition` and is checked before a
//! card enters a `done` list; an approval may override it with a reason. The gate is the product's
//! one approval primitive: an approved, card-scoped `card_done` gate carrying typed evidence or
//! the explicit `evidence_waiver` reason. A type that declares no acceptance has no gate.
//!
//! `cargo test -p conveyor-actions --test card_done_gate`.

mod support;

use conveyor_actions::{
    ActionError, Authority, InvokeOutcome, InvokeRequest, Precondition, invoke, registry,
};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

const APP_KEY: &str = "conveyor-test";

fn request_with(
    actor: Uuid,
    action: &str,
    input: Value,
    precondition: Option<Precondition>,
) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some(format!("rid-dod-{}", Uuid::now_v7())),
        trace_id: Some("trace-dod".to_owned()),
        depth: 0,
        app_key: APP_KEY.to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

fn request(actor: Uuid, action: &str, input: Value) -> InvokeRequest {
    request_with(actor, action, input, None)
}

async fn call(
    pg: &support::Pg,
    seeded: &support::Seeded,
    request: InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    invoke(&pg.runtime, registry(), seeded.tenant, request).await
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    (pg, seeded)
}

/// The `feature` card type's Definition of Done, as the move reads it.
async fn set_dod(pg: &support::Pg, seeded: &support::Seeded, definition: Value) {
    sqlx::query("UPDATE card_type SET definition = $1 WHERE tenant_id = $2 AND code = 'feature'")
        .bind(definition)
        .bind(seeded.tenant.as_uuid())
        .execute(&pg.super_pool)
        .await
        .expect("set the feature Definition of Done");
}

/// A delivery project, its board, and the `active` and `done` lanes.
async fn board_and_lanes(pg: &support::Pg, seeded: &support::Seeded) -> (String, String, String) {
    let created = call(
        pg,
        seeded,
        request(
            seeded.user_id,
            "projects.create",
            json!({
                "title": format!("DoD {}", Uuid::now_v7().simple()),
                "project_type_code": "delivery"
            }),
        ),
    )
    .await
    .expect("create the project");
    assert_eq!(created.status, 201, "{}", created.body);
    let project = text(output(&created.body), "id");
    let view = call(
        pg,
        seeded,
        request(seeded.user_id, "projects.get", json!({ "id": project })),
    )
    .await
    .expect("read the project");
    assert_eq!(view.status, 200, "{}", view.body);
    let board_id = text(&output(&view.body)["boards"][0], "id");
    let board = call(
        pg,
        seeded,
        request(seeded.user_id, "boards.get", json!({ "id": board_id })),
    )
    .await
    .expect("read the board");
    assert_eq!(board.status, 200, "{}", board.body);
    let lists = output(&board.body)["lists"]
        .as_array()
        .expect("the board's lanes");
    let lane = |kind: &str| {
        lists
            .iter()
            .find(|row| row["list"]["kind"] == kind)
            .map(|row| text(&row["list"], "id"))
            .unwrap_or_else(|| panic!("no {kind} lane in {}", board.body))
    };
    (board_id, lane("active"), lane("done"))
}

async fn create_card(
    pg: &support::Pg,
    seeded: &support::Seeded,
    board: &str,
    lane: &str,
) -> (String, i64) {
    let created = call(
        pg,
        seeded,
        request(
            seeded.user_id,
            "cards.create",
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("DoD card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature"
            }),
        ),
    )
    .await
    .expect("create the card");
    assert_eq!(created.status, 201, "{}", created.body);
    let card = text(output(&created.body), "id");
    let version = output(&created.body)["version"].as_i64().unwrap_or(1);
    (card, version)
}

/// Open the card's `card_done` gate, optionally with a waiver reason.
async fn request_gate(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
    waiver: Option<&str>,
    require_evidence: bool,
) -> String {
    let mut input = json!({
        "card_id": card,
        "gate": "card_done",
        "kind": "card_dod",
        "require_evidence": require_evidence
    });
    if let Some(reason) = waiver {
        input["evidence_waiver_reason"] = json!(reason);
    }
    let requested = call(
        pg,
        seeded,
        request(seeded.user_id, "approvals.request", input),
    )
    .await
    .expect("open the Definition-of-Done gate");
    assert_eq!(requested.status, 200, "{}", requested.body);
    text(output(&requested.body), "id")
}

async fn approve(pg: &support::Pg, seeded: &support::Seeded, gate: &str) {
    let reviewer = support::reviewer_id(pg).await;
    let decided = call(
        pg,
        seeded,
        request(
            reviewer,
            "approvals.decide",
            json!({
                "approval_id": gate,
                "decision": "approve",
                "note": "the Definition of Done is met"
            }),
        ),
    )
    .await
    .expect("decide the gate");
    assert_eq!(decided.status, 200, "{}", decided.body);
}

async fn move_to_done(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
    version: i64,
    done: &str,
) -> Result<InvokeOutcome, ActionError> {
    call(
        pg,
        seeded,
        request_with(
            seeded.user_id,
            "cards.move",
            json!({ "id": card, "list_id": done }),
            Some(Precondition::Version(version)),
        ),
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_card_with_a_definition_cannot_enter_done_without_its_gate() {
    let (pg, seeded) = fixture().await;
    set_dod(
        &pg,
        &seeded,
        json!({ "acceptance": ["DONE: the work is finished"] }),
    )
    .await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card(&pg, &seeded, &board, &active).await;

    let refused = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect_err("a card type with a Definition of Done must not enter done unproven");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "card_done_criteria_unmet");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_gate_with_a_waiver_permits_the_move() {
    let (pg, seeded) = fixture().await;
    set_dod(
        &pg,
        &seeded,
        json!({ "acceptance": ["DONE: the work is finished"] }),
    )
    .await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card(&pg, &seeded, &board, &active).await;
    let gate = request_gate(
        &pg,
        &seeded,
        &card,
        Some("the acceptance was verified by inspection"),
        true,
    )
    .await;
    approve(&pg, &seeded, &gate).await;

    let moved = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect("an approved waiver permits the move");
    assert_eq!(moved.status, 200, "{}", moved.body);
    assert_eq!(output(&moved.body)["list_id"], done);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_gate_with_no_evidence_or_waiver_is_refused() {
    let (pg, seeded) = fixture().await;
    set_dod(
        &pg,
        &seeded,
        json!({ "acceptance": ["DONE: the work is finished"] }),
    )
    .await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card(&pg, &seeded, &board, &active).await;
    let gate = request_gate(&pg, &seeded, &card, None, false).await;
    approve(&pg, &seeded, &gate).await;

    let refused = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect_err("an empty Definition-of-Done gate proves nothing");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "card_done_evidence_missing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_type_with_no_definition_moves_without_a_gate() {
    let (pg, seeded) = fixture().await;
    set_dod(&pg, &seeded, json!({})).await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card(&pg, &seeded, &board, &active).await;

    let moved = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect("a type with no acceptance has no gate to pass");
    assert_eq!(moved.status, 200, "{}", moved.body);
}
