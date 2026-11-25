//! T19F (#683) — the T9 board-sitting and T10 cadence bindings.
//!
//! A board subject decision is taken in a sitting whose seats write `via='meeting'` decisions
//! through T19's `close_item_tx`; a cadence row materialises exactly one meeting. The direct
//! `architecture.board_decide` path is unchanged.
//!
//! `cargo test -p conveyor-api --test meetings_binding`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

/// A board type whose cadence slot resolves as an agenda item.
fn cadence_policy() -> Value {
    json!({
        "name": "PI planning",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": [{"kind": "cadence"}],
        "required_participation": [],
        "evidence_minimum": 0
    })
}

/// Open and activate a board carrying `meeting_type_key`.
async fn board(f: &meetings::Fixture, key: &str) -> Uuid {
    let body = meetings::created(
        &f.running,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": key,
            "name": format!("{key} board"),
            "meeting_type_key": key,
        }),
        None,
    )
    .await;
    let body_id = meetings::uuid_of(&body, "id");
    meetings::created(
        &f.running,
        "architecture.board_decide",
        json!({"body_id": body_id, "decision": "approved", "subject_kind": "board"}),
        None,
    )
    .await;
    body_id
}

/// One delivery contract on the fixture's project.
async fn contract(f: &meetings::Fixture, title: &str) -> Uuid {
    let contract = meetings::created(
        &f.running,
        "architecture.contracts_request",
        json!({
            "project_id": f.project_id,
            "kind": "delivery_contract",
            "phase_code": "f",
            "title": title,
        }),
        None,
    )
    .await;
    meetings::uuid_of(&contract, "id")
}

/// Open the body's sitting over the subject.
async fn sit(f: &meetings::Fixture, body_id: Uuid, subject_id: Uuid) -> Value {
    meetings::created(
        &f.running,
        "architecture.board_sit_open",
        json!({
            "body_id": body_id,
            "subject_kind": "contract",
            "subject_id": subject_id,
            "list_id": f.list_id,
            "evidence": [{"kind": "document", "ref": "readme.md"}],
        }),
        None,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn board_sitting_uses_close_item() {
    let f = meetings::fixture().await;
    meetings::schedule(&f, "sitting_board", Some(meetings::policy()), None).await;
    let body_id = board(&f, "sitting_board").await;
    let subject = contract(&f, "Sitting contract").await;
    let sitting = sit(&f, body_id, subject).await;
    let meeting_id = sitting["meeting_id"]
        .as_str()
        .expect("meeting id")
        .to_owned();
    let approval_id = meetings::uuid_of(&sitting, "approval_id");

    // The seat is a distinct human from the sitting's requester (P4's self-approval rule).
    let reviewer = support::reviewer_id(&f.pg).await;
    meetings::invite(&f, &meeting_id, "app_user", reviewer, "chair", None).await;
    meetings::respond(&f, &meeting_id, "app_user", reviewer, "accepted").await;
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let item_id = detail["agenda"][0]["id"]
        .as_str()
        .expect("agenda item")
        .to_owned();
    meetings::position(
        &f,
        &meeting_id,
        &item_id,
        "app_user",
        reviewer,
        "approve",
        "web",
    )
    .await;

    let result = meetings::ok(
        &f.running,
        "architecture.board_sit_decide",
        json!({
            "body_id": body_id,
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
        }),
        None,
    )
    .await;
    assert_eq!(result["approval_status"], "approved", "{result}");
    assert_eq!(result["decisions_written"], 1, "{result}");
    assert_eq!(result["approval_id"], approval_id.to_string(), "{result}");

    let (surface, decision): (String, String) = sqlx::query_as(
        "SELECT via ->> 'surface', decision FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the seat's decision row");
    assert_eq!(surface, "meeting");
    assert_eq!(decision, "approve");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_without_a_type_refuses_a_sitting() {
    let f = meetings::fixture().await;
    let body_id = board(&f, "no_type_board").await;
    let subject = contract(&f, "No-type contract").await;
    let (status, refused) = meetings::call(
        &f.running,
        "architecture.board_sit_open",
        json!({
            "body_id": body_id,
            "subject_kind": "contract",
            "subject_id": subject,
            "list_id": f.list_id,
        }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "meeting_type_policy_required", "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_quorum_miss_defers_inquorate() {
    let f = meetings::fixture().await;
    let department = meetings::department(&f.pg, f.seeded.tenant_id, "Architecture").await;
    let policy = json!({
        "name": "Board",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": [],
        "required_participation": {"departments": [{"id": department, "minimum": 1}]},
        "evidence_minimum": 0
    });
    meetings::schedule(&f, "inquorate_board", Some(policy), None).await;
    let body_id = board(&f, "inquorate_board").await;
    let subject = contract(&f, "Inquorate contract").await;
    let sitting = sit(&f, body_id, subject).await;
    let meeting_id = sitting["meeting_id"]
        .as_str()
        .expect("meeting id")
        .to_owned();

    // The seat is admitted but holds no seat in the required department.
    let reviewer = support::reviewer_id(&f.pg).await;
    meetings::invite(&f, &meeting_id, "app_user", reviewer, "member", None).await;
    meetings::respond(&f, &meeting_id, "app_user", reviewer, "accepted").await;
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let item_id = detail["agenda"][0]["id"]
        .as_str()
        .expect("agenda item")
        .to_owned();
    meetings::position(
        &f,
        &meeting_id,
        &item_id,
        "app_user",
        reviewer,
        "approve",
        "web",
    )
    .await;

    let result = meetings::ok(
        &f.running,
        "architecture.board_sit_decide",
        json!({
            "body_id": body_id,
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
        }),
        None,
    )
    .await;
    assert_eq!(result["decisions_written"], 0, "{result}");
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    assert_eq!(detail["agenda"][0]["outcome"], "deferred", "{detail}");
    assert_eq!(detail["agenda"][0]["defer_reason"], "inquorate", "{detail}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_direct_board_decide_path_still_works() {
    let f = meetings::fixture().await;
    let body = meetings::created(
        &f.running,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": "direct_board",
            "name": "Direct Board",
        }),
        None,
    )
    .await;
    let body_id = meetings::uuid_of(&body, "id");
    let decision = meetings::created(
        &f.running,
        "architecture.board_decide",
        json!({"body_id": body_id, "decision": "approved", "subject_kind": "board"}),
        None,
    )
    .await;
    assert!(decision["approval_id"].is_string(), "{decision}");
    let status: String =
        sqlx::query_scalar("SELECT status FROM governance_body WHERE tenant_id = $1 AND id = $2")
            .bind(f.seeded.tenant_id)
            .bind(body_id)
            .fetch_one(&f.pg.super_pool)
            .await
            .expect("the body status");
    assert_eq!(status, "active");
}

#[tokio::test(flavor = "multi_thread")]
async fn cadence_materialise_is_idempotent() {
    let f = meetings::fixture().await;
    meetings::schedule(&f, "pi_planning", Some(cadence_policy()), None).await;
    let cadence = meetings::cadence_slot(&f.pg, f.seeded.tenant_id).await;
    sqlx::query("UPDATE programme_cadence SET meeting_type_key = 'pi_planning' WHERE id = $1")
        .bind(cadence)
        .execute(&f.pg.super_pool)
        .await
        .expect("bind the cadence type");

    let first = meetings::created(
        &f.running,
        "cadences.materialise",
        json!({"id": cadence, "list_id": f.list_id}),
        None,
    )
    .await;
    assert_eq!(first["meeting_type"], "pi_planning", "{first}");
    assert_eq!(first["cadence_slot"], cadence.to_string(), "{first}");
    let meeting_id = first["id"].as_str().expect("meeting id").to_owned();
    let second = meetings::created(
        &f.running,
        "cadences.materialise",
        json!({"id": cadence, "list_id": f.list_id}),
        None,
    )
    .await;
    assert_eq!(second["id"], meeting_id, "{second}");

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM meeting
          WHERE tenant_id = $1 AND cadence_slot = $2 AND status <> 'cancelled'",
    )
    .bind(f.seeded.tenant_id)
    .bind(cadence)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count the cadence meetings");
    assert_eq!(count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cadence_without_a_type_is_refused() {
    let f = meetings::fixture().await;
    let cadence = meetings::cadence_slot(&f.pg, f.seeded.tenant_id).await;
    let (status, refused) = meetings::call(
        &f.running,
        "cadences.materialise",
        json!({"id": cadence, "list_id": f.list_id}),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "cadence_type_missing", "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cadence_item_resolves() {
    let f = meetings::fixture().await;
    meetings::schedule(&f, "pi_planning", Some(cadence_policy()), None).await;
    let cadence = meetings::cadence_slot(&f.pg, f.seeded.tenant_id).await;
    sqlx::query("UPDATE programme_cadence SET meeting_type_key = 'pi_planning' WHERE id = $1")
        .bind(cadence)
        .execute(&f.pg.super_pool)
        .await
        .expect("bind the cadence type");
    let meeting = meetings::created(
        &f.running,
        "cadences.materialise",
        json!({"id": cadence, "list_id": f.list_id}),
        None,
    )
    .await;
    let meeting_id = meeting["id"].as_str().expect("meeting id").to_owned();
    meetings::open(&f, &meeting_id).await;
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let resolved = detail["agenda"]
        .as_array()
        .expect("agenda")
        .iter()
        .find(|item| item["resolved_kind"] == "cadence")
        .expect("the cadence item");
    assert_eq!(resolved["resolved_id"], cadence.to_string(), "{detail}");
}
