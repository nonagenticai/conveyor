//! `cards.move` enforces the card type's Definition of Done (finding `v1-p54-1`).
//!
//! SPEC_V2 §5.4: the Definition of Done lives on `card_type.definition` and is checked before a
//! card enters a `done` list; an approval may override it with a reason. The gate is the product's
//! one approval primitive: an approved, card-scoped `card_done` gate carrying typed evidence or
//! the explicit `evidence_waiver` reason. A type that declares no acceptance has no gate.
//!
//! SPEC_V2 §5.8 (D69, #935) adds the work-package gates around the same `card_done` gate: a run
//! starts only from an INVEST-ready spec (`runs.create`), and a work package enters done only
//! when every parsed scenario has evidence naming it with a run or step of that card.
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
    create_card_of_type(pg, seeded, board, lane, "feature").await
}

async fn create_card_of_type(
    pg: &support::Pg,
    seeded: &support::Seeded,
    board: &str,
    lane: &str,
    card_type: &str,
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
                "card_type_code": card_type
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

// ── SPEC_V2 §5.8 (D69): the work-package gates ─────────────────────────────────────────────

/// The two-scenario feature every work-package test authors. The scenario keys are
/// `the-first-part` and `the-second-part` — the stable ids evidence must name.
const FEATURE: &str = r#"Feature: the work package

  @verify=V1
  Scenario: the first part
    Given a work package
    When the first part is executed
    Then the first scenario passes

  @verify=V2
  Scenario: the second part
    Given a work package
    When the second part is executed
    Then the second scenario passes
"#;

/// The six INVEST criteria, each met with evidence — an INVEST-ready definition of ready.
fn ready_dor() -> Value {
    json!({
        "independent": {"met": true, "evidence": "no blocked-by link"},
        "negotiable": {"met": true, "evidence": "outcome fixed; approach settled at delivery"},
        "valuable": {"met": true, "evidence": "the story's so-that names the benefit"},
        "estimable": {"met": true, "evidence": "baseline and every file are named"},
        "small": {"met": true, "evidence": "one work package, one run"},
        "testable": {"met": true, "evidence": "every scenario carries @verify"},
    })
}

/// A structurally valid definition of ready with one criterion unmet: `cards.spec_put` stores
/// it, and the run-start gate is where the unmet criterion refuses.
fn unready_dor() -> Value {
    let mut dor = ready_dor();
    dor["small"] = json!({"met": false, "evidence": "two runs are planned"});
    dor
}

/// A `delivery` card type and the matrix row that lets it move `active → done`.
///
/// The dev catalogue seeds only `feature`/`bug`; both §5.8's scope predicate and the lane matrix
/// key on the card type's code, so the fixture owns both. The cache clear is the board fixture's
/// rule for a catalogue row written after a possible earlier read in this process.
async fn seed_delivery_card_type(pg: &support::Pg, seeded: &support::Seeded) {
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'delivery', 'Delivery')
         ON CONFLICT (tenant_id, code) DO NOTHING",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("the delivery card type");
    sqlx::query(
        "UPDATE catalogue_entry
            SET meta = jsonb_set(meta, '{matrix,delivery}',
                                 '{\"intake\": [\"active\"], \"active\": [\"done\"]}'::jsonb)
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = 'delivery'",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("the delivery matrix");
    conveyor_actions::board::matrix::clear_cache_for(seeded.tenant);
}

/// Author the card's work-package spec through the real writer, `cards.spec_put`.
async fn put_spec(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
    dor: Value,
) -> Result<InvokeOutcome, ActionError> {
    call(
        pg,
        seeded,
        request(
            seeded.user_id,
            "cards.spec_put",
            json!({
                "card_id": card,
                "story_persona": "a delivery team",
                "story_action": "the work package to carry its acceptance",
                "story_benefit": "done is judged per scenario, not by a checkbox",
                "feature": FEATURE,
                "dor": dor,
            }),
        ),
    )
    .await
}

/// Start the card's run through `runs.create`.
async fn create_run(
    pg: &support::Pg,
    seeded: &support::Seeded,
    card: &str,
) -> Result<InvokeOutcome, ActionError> {
    call(
        pg,
        seeded,
        request(seeded.user_id, "runs.create", json!({ "card_id": card })),
    )
    .await
}

/// Record an approved `card_done` gate whose `meta` is exactly `meta`.
///
/// `approvals.request` can only store typed `{kind, id}` evidence, but §5.8's scenario evidence
/// is `{scenario, run_id|step_id}`; the fixture therefore writes the decided row the gate reads.
async fn approve_gate(pg: &support::Pg, seeded: &support::Seeded, card: &str, meta: Value) {
    let card_id = Uuid::parse_str(card).expect("the card id");
    sqlx::query(
        "INSERT INTO approval (tenant_id, card_id, gate, kind, status, meta)
         VALUES ($1, $2, 'card_done', 'card_dod', 'approved', $3)",
    )
    .bind(seeded.tenant_id)
    .bind(card_id)
    .bind(meta)
    .execute(&pg.super_pool)
    .await
    .expect("the approved card_done gate");
}

/// One `run_step` on the run, returned as the id scenario evidence can cite.
async fn record_run_step(pg: &support::Pg, seeded: &support::Seeded, run: &str) -> String {
    let run_id = Uuid::parse_str(run).expect("the run id");
    let step: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step (tenant_id, run_id, position, action)
         VALUES ($1, $2, 1, 'agent.run') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(run_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the run's step");
    step.to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_on_a_work_package_with_no_spec_is_refused() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, _done) = board_and_lanes(&pg, &seeded).await;
    let (card, _version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;

    let refused = create_run(&pg, &seeded, &card)
        .await
        .expect_err("a work package with no spec is not ready to start");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "work_package_dor_unmet");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_on_a_work_package_with_an_unmet_invest_criterion_is_refused() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, _done) = board_and_lanes(&pg, &seeded).await;
    let (card, _version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;

    // The spec is stored — readiness is the run-start gate's refusal, not the writer's.
    let stored = put_spec(&pg, &seeded, &card, unready_dor())
        .await
        .expect("a structurally valid spec is stored");
    assert_eq!(stored.status, 200, "{}", stored.body);

    let refused = create_run(&pg, &seeded, &card)
        .await
        .expect_err("an unmet INVEST criterion refuses the start");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "work_package_dor_unmet");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_on_a_ready_work_package_is_created() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, _done) = board_and_lanes(&pg, &seeded).await;
    let (card, _version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;

    let stored = put_spec(&pg, &seeded, &card, ready_dor())
        .await
        .expect("an INVEST-ready spec is stored");
    assert_eq!(stored.status, 200, "{}", stored.body);

    let created = create_run(&pg, &seeded, &card)
        .await
        .expect("a ready work package starts");
    assert_eq!(created.status, 201, "{}", created.body);
    assert!(
        !text(output(&created.body), "id").is_empty(),
        "the run has an id: {}",
        created.body
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_work_package_with_one_uncovered_scenario_cannot_enter_done() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;
    put_spec(&pg, &seeded, &card, ready_dor())
        .await
        .expect("a ready spec is stored");
    let run = create_run(&pg, &seeded, &card)
        .await
        .expect("a ready work package starts");
    assert_eq!(run.status, 201, "{}", run.body);
    let run_id = text(output(&run.body), "id");

    // Only the first scenario is evidenced; the second is uncovered.
    approve_gate(
        &pg,
        &seeded,
        &card,
        json!({"evidence": [{"scenario": "the-first-part", "run_id": run_id}]}),
    )
    .await;

    let refused = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect_err("every scenario needs its own evidence before done");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "work_package_evidence_unmet");
    assert!(
        refused.detail.contains("the-second-part"),
        "the refusal names the uncovered scenario: {}",
        refused.detail
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_work_package_with_every_scenario_evidenced_enters_done() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;
    put_spec(&pg, &seeded, &card, ready_dor())
        .await
        .expect("a ready spec is stored");
    let run = create_run(&pg, &seeded, &card)
        .await
        .expect("a ready work package starts");
    assert_eq!(run.status, 201, "{}", run.body);
    let run_id = text(output(&run.body), "id");
    let step_id = record_run_step(&pg, &seeded, &run_id).await;

    // The first scenario cites the run, the second a step of that same run, and the second key
    // arrives in `scenario_id`: both accepted spellings and both ownership paths.
    approve_gate(
        &pg,
        &seeded,
        &card,
        json!({"evidence": [
            {"scenario": "the-first-part", "run_id": run_id},
            {"scenario_id": "the-second-part", "step_id": step_id},
        ]}),
    )
    .await;

    let moved = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect("every scenario is evidenced by this card's run");
    assert_eq!(moved.status, 200, "{}", moved.body);
    assert_eq!(output(&moved.body)["list_id"], done);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_non_delivery_card_is_not_a_work_package() {
    let (pg, seeded) = fixture().await;
    // The delivery card type is absent from this tenant: a `feature` card cannot be one.
    set_dod(
        &pg,
        &seeded,
        json!({ "acceptance": ["DONE: the work is finished"] }),
    )
    .await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card(&pg, &seeded, &board, &active).await;

    // Non-scenario evidence satisfies the type-level floor; no spec is owed.
    approve_gate(
        &pg,
        &seeded,
        &card,
        json!({"evidence": [{"kind": "governed_asset", "id": "an-approved-asset"}]}),
    )
    .await;

    let moved = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect("a feature card keeps the type-level DoD only");
    assert_eq!(moved.status, 200, "{}", moved.body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waiver_does_not_excuse_a_missing_spec() {
    let (pg, seeded) = fixture().await;
    seed_delivery_card_type(&pg, &seeded).await;
    let (board, active, done) = board_and_lanes(&pg, &seeded).await;
    let (card, version) = create_card_of_type(&pg, &seeded, &board, &active, "delivery").await;
    approve_gate(
        &pg,
        &seeded,
        &card,
        json!({"evidence_waiver": "the acceptance was verified by inspection"}),
    )
    .await;

    let refused = move_to_done(&pg, &seeded, &card, version, &done)
        .await
        .expect_err("a waiver excuses evidence, never the spec");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.code, "work_package_evidence_unmet");
    assert!(
        refused.detail.contains("no spec"),
        "the refusal names the missing spec: {}",
        refused.detail
    );
}
