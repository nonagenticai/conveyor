//! T19 (#606) D3 — attendance, one live position per participant, quorum and `inquorate`.
//!
//! `cargo test -p conveyor-api --test meetings_attendance`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn policy_with(required: Value, quorum: i64) -> Value {
    json!({
        "name": "Board",
        "decision_rule": "majority",
        "quorum": quorum,
        "agenda_sources": [],
        "required_participation": required,
        "evidence_minimum": 0
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn one_live_position_per_participant() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let item = meetings::register_item(&f.pg, f.seeded.tenant_id, &f.project_id, "risk").await;
    let item = meetings::agenda_add(&f, &meeting_id, "register_item", item, json!([])).await;
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
    meetings::position(
        &f,
        &meeting_id,
        &item_id,
        "app_user",
        f.seeded.user_id,
        "reject",
        "mcp",
    )
    .await;

    let (total, live, stance): (i64, i64, String) = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE superseded_at IS NULL)::bigint,
                coalesce(max(stance) FILTER (WHERE superseded_at IS NULL), '')
           FROM meeting_position
          WHERE tenant_id = $1 AND agenda_item_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(item_id.parse::<uuid::Uuid>().expect("item uuid"))
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count positions");
    assert_eq!(total, 2, "both positions are kept as history");
    assert_eq!(live, 1, "one live position");
    assert_eq!(stance, "reject", "the latest stance is the live one");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_department_defers_inquorate() {
    let f = meetings::fixture().await;
    let department = meetings::department(&f.pg, f.seeded.tenant_id, "Architecture").await;
    let policy = policy_with(
        json!({"departments": [{"id": department, "minimum": 1}]}),
        1,
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
    let item = meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let item_id = meetings::id_of(&item);

    // The participant is admitted to the meeting but holds no seat in the required department.
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

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": item_id}),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "deferred", "{result}");
    assert_eq!(result["item"]["defer_reason"], "inquorate", "{result}");
    assert_eq!(result["decisions_written"], 0, "{result}");

    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count decisions");
    assert_eq!(decisions, 0, "an inquorate item writes no decision row");
    let status: String =
        sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(f.seeded.tenant_id)
            .bind(approval)
            .fetch_one(&f.pg.super_pool)
            .await
            .expect("approval status");
    assert_eq!(status, "pending", "the linked approval is untouched");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_item_position_is_immutable() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
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
    meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": item_id}),
        None,
    )
    .await;

    let (status, refused) = meetings::call(
        &f.running,
        "meetings.position",
        json!({
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
            "participant_kind": "app_user",
            "participant_id": f.seeded.user_id,
            "stance": "reject",
            "surface": "web",
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "meeting_not_permitted", "{refused}");
}
