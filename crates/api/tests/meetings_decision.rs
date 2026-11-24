//! T19 (#606) D4 — the outcome arithmetic: per-seat decision rows, majority/unanimous/chair,
//! ties, observer exclusion, P4 and the approval's stricter policy.
//!
//! `cargo test -p conveyor-api --test meetings_decision`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

type DecisionRow = (String, Option<Uuid>, Option<Uuid>, String, String);

fn policy_with(required: Value, quorum: i64, rule: &str) -> Value {
    json!({
        "name": "Board",
        "decision_rule": rule,
        "quorum": quorum,
        "agenda_sources": [],
        "required_participation": required,
        "evidence_minimum": 0
    })
}

async fn seat(
    f: &meetings::Fixture,
    meeting_id: &str,
    kind: &str,
    participant: Uuid,
    department: Option<Uuid>,
    stance: &str,
) {
    meetings::invite(f, meeting_id, kind, participant, "member", department).await;
    meetings::respond(f, meeting_id, kind, participant, "accepted").await;
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let item_id = detail["agenda"][0]["id"].as_str().expect("agenda item");
    meetings::position(f, meeting_id, item_id, kind, participant, stance, "web").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn majority_records_each_seat_and_keeps_dissent() {
    let f = meetings::fixture().await;
    let departments = [
        meetings::department(&f.pg, f.seeded.tenant_id, "Architecture").await,
        meetings::department(&f.pg, f.seeded.tenant_id, "Engineering").await,
        meetings::department(&f.pg, f.seeded.tenant_id, "Operations").await,
    ];
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let owner = meetings::member(&f.pg, f.seeded.tenant_id, "dev:owner").await;
    let policy = policy_with(
        json!({"departments": departments.map(|id| json!({"id": id, "minimum": 1}))}),
        3,
        "majority",
    );
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;

    let approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;

    seat(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        Some(departments[0]),
        "approve",
    )
    .await;
    seat(
        &f,
        &meeting_id,
        "app_user",
        bob,
        Some(departments[1]),
        "approve",
    )
    .await;
    seat(
        &f,
        &meeting_id,
        "app_user",
        owner,
        Some(departments[2]),
        "reject",
    )
    .await;

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": meetings::ok(
            &f.running, "meetings.get", json!({"meeting_id": meeting_id}), None
        ).await["agenda"][0]["id"]}),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "decision", "{result}");
    assert_eq!(result["approval_status"], "approved", "{result}");
    assert_eq!(result["decisions_written"], 3, "{result}");

    let rows: Vec<DecisionRow> = sqlx::query_as(
        "SELECT decision, approver_id, job_binding_id, participant_kind,
                via->>'surface'
           FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2
          ORDER BY decision",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_all(&f.pg.super_pool)
    .await
    .expect("decision rows");
    assert_eq!(rows.len(), 3, "one row per qualifying seat");
    assert_eq!(
        rows.iter().filter(|row| row.0 == "reject").count(),
        1,
        "the dissent is kept"
    );
    assert!(
        rows.iter().all(|row| row.1.is_some() && row.2.is_none()),
        "human seats use approver_id"
    );
    assert!(
        rows.iter()
            .all(|row| row.3 == "human" && row.4 == "meeting"),
        "every row is a human seat on the meeting surface"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_meeting_never_lowers_dual_control() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval_with(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
        2,
        None,
        None,
    )
    .await;
    let item = meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let item_id = meetings::id_of(&item);
    meetings::invite(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        "member",
        None,
    )
    .await;
    meetings::respond(&f, &meeting_id, "app_user", f.seeded.user_id, "accepted").await;
    meetings::position(
        &f,
        &meeting_id,
        &item_id,
        "app_user",
        f.seeded.user_id,
        "approve",
        "web",
    )
    .await;

    let (status, refused) = meetings::call(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": item_id}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "meeting_approval_stricter", "{refused}");

    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count decisions");
    assert_eq!(decisions, 0, "a refused close writes no row");
    let (outcome,): (Option<String>,) =
        sqlx::query_as("SELECT outcome FROM meeting_agenda_item WHERE tenant_id = $1 AND id = $2")
            .bind(f.seeded.tenant_id)
            .bind(item_id.parse::<Uuid>().expect("item uuid"))
            .fetch_one(&f.pg.super_pool)
            .await
            .expect("item outcome");
    assert_eq!(outcome, None, "the item stays open for more positions");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tie_defers_and_observers_do_not_count() {
    let f = meetings::fixture().await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let owner = meetings::member(&f.pg, f.seeded.tenant_id, "dev:owner").await;
    let dave = meetings::member(&f.pg, f.seeded.tenant_id, "dev:dave").await;
    let eve = meetings::member(&f.pg, f.seeded.tenant_id, "dev:eve").await;
    let policy = policy_with(json!([]), 4, "majority");
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;

    let approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;

    seat(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        None,
        "approve",
    )
    .await;
    seat(&f, &meeting_id, "app_user", bob, None, "approve").await;
    seat(&f, &meeting_id, "app_user", owner, None, "reject").await;
    seat(&f, &meeting_id, "app_user", dave, None, "reject").await;

    // The observer approves; if the observer counted the result would be 3-2, not a tie.
    meetings::invite(&f, &meeting_id, "app_user", eve, "observer", None).await;
    meetings::respond(&f, &meeting_id, "app_user", eve, "accepted").await;
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let item_id = detail["agenda"][0]["id"].as_str().expect("agenda item");
    meetings::position(&f, &meeting_id, item_id, "app_user", eve, "approve", "web").await;

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": item_id}),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "deferred", "{result}");
    assert_eq!(result["item"]["defer_reason"], "tie", "{result}");
    assert_eq!(result["decisions_written"], 0, "{result}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_conflict_rule_excludes_the_judged_seat() {
    let f = meetings::fixture().await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let card = meetings::judged_card(
        &f.pg,
        f.seeded.tenant_id,
        &f.project_id,
        f.list_id,
        f.seeded.user_id,
    )
    .await;
    let policy = policy_with(json!([]), 2, "majority");
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval_with(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
        1,
        Some(card),
        None,
    )
    .await;
    meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;

    seat(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        None,
        "approve",
    )
    .await;
    seat(&f, &meeting_id, "app_user", bob, None, "approve").await;

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": meetings::ok(
            &f.running, "meetings.get", json!({"meeting_id": meeting_id}), None
        ).await["agenda"][0]["id"]}),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "decision", "{result}");
    assert_eq!(result["decisions_written"], 1, "{result}");

    let rows: Vec<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT approver_id FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_all(&f.pg.super_pool)
    .await
    .expect("decision rows");
    assert_eq!(rows.len(), 1, "the executor's seat is not a decision");
    assert_eq!(rows[0].0, Some(bob), "only the independent seat counts");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unanimous_reject_resolves_rejected() {
    let f = meetings::fixture().await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let policy = policy_with(json!([]), 2, "unanimous");
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    seat(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        None,
        "approve",
    )
    .await;
    seat(&f, &meeting_id, "app_user", bob, None, "reject").await;

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": meetings::ok(
            &f.running, "meetings.get", json!({"meeting_id": meeting_id}), None
        ).await["agenda"][0]["id"]}),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "decision", "{result}");
    assert_eq!(result["approval_status"], "rejected", "{result}");
    assert_eq!(result["decisions_written"], 2, "{result}");
}
