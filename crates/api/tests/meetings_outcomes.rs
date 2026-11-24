//! T19 (#606) D5 — the closed outcome set, commitments, action cards, minutes behind the
//! chair's approval and the meeting's DoD.
//!
//! `cargo test -p conveyor-api --test meetings_outcomes`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

async fn live_meeting(f: &meetings::Fixture) -> (String, String) {
    let meeting = meetings::schedule(f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(f, &meeting_id).await;
    let item = meetings::register_item(&f.pg, f.seeded.tenant_id, &f.project_id, "risk").await;
    let item = meetings::agenda_add(f, &meeting_id, "register_item", item, json!([])).await;
    (meeting_id, meetings::id_of(&item))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_commitment_needs_owner_and_date() {
    let f = meetings::fixture().await;
    let (meeting_id, item_id) = live_meeting(&f).await;
    meetings::invite(
        &f,
        &meeting_id,
        "app_user",
        f.seeded.user_id,
        "member",
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
            "stance": "approve_with_conditions",
            "surface": "web",
            "commitment": {"what": "Ship the read model", "by": "2026-12-01"},
        }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["type"], "commitment_needs_owner_and_date",
        "{refused}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_becomes_a_card() {
    let f = meetings::fixture().await;
    let (meeting_id, item_id) = live_meeting(&f).await;

    let result = meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
            "outcome": "action",
            "action": {"title": "Publish the T19 evidence", "by": "2026-12-01"},
        }),
        None,
    )
    .await;
    assert_eq!(result["item"]["outcome"], "action", "{result}");
    let outcome_ref = result["item"]["outcome_ref"]
        .as_str()
        .expect("the linked card")
        .parse::<Uuid>()
        .expect("card uuid");

    let (code, linked): (String, String) = sqlx::query_as(
        "SELECT ct.code, c.meta->>'meeting_id'
           FROM card c JOIN card_type ct ON ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(outcome_ref)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the action card");
    assert_eq!(code, "meeting_action", "the action card's type");
    assert_eq!(linked, meeting_id, "the card is linked from the item");
}

#[tokio::test(flavor = "multi_thread")]
async fn minutes_need_the_chair_and_match_the_rows() {
    let f = meetings::fixture().await;
    let (meeting_id, item_id) = live_meeting(&f).await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    meetings::invite(&f, &meeting_id, "app_user", f.seeded.user_id, "chair", None).await;
    meetings::respond(&f, &meeting_id, "app_user", f.seeded.user_id, "accepted").await;
    meetings::invite(&f, &meeting_id, "app_user", bob, "member", None).await;
    meetings::respond(&f, &meeting_id, "app_user", bob, "accepted").await;

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let card_id = detail["meeting"]["card_id"].as_str().expect("card id");
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, required_decisions, card_id, meta)
         VALUES ($1, 'minutes', 'meeting', 'approved', 1, $2, $3) RETURNING id",
    )
    .bind(f.seeded.tenant_id)
    .bind(card_id.parse::<Uuid>().expect("card uuid"))
    .bind(json!({"meeting_id": meeting_id}))
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the chair's approval");
    sqlx::query(
        "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, participant_kind,
             decision, note, decided_by_kind, via)
         VALUES ($1, $2, $3, 'human', 'approve', '', 'human', '{\"surface\":\"web\"}'::jsonb)",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .bind(f.seeded.user_id)
    .execute(&f.pg.super_pool)
    .await
    .expect("the chair's decision");

    // Close the item as information so the forum can reach its DoD once the minutes publish.
    meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
            "outcome": "information",
        }),
        None,
    )
    .await;

    let unknown = Uuid::now_v7();
    let (status, refused) = meetings::call(
        &f.running,
        "meetings.minutes_publish",
        json!({
            "meeting_id": meeting_id,
            "approval_id": approval,
            "title": "Board minutes",
            "lines": [{"agenda_item_id": unknown, "text": "A line with no row"}],
        }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "minute_line_unmatched", "{refused}");

    let published = meetings::created(
        &f.running,
        "meetings.minutes_publish",
        json!({
            "meeting_id": meeting_id,
            "approval_id": approval,
            "title": "Board minutes",
            "lines": [{"agenda_item_id": item_id, "text": "Noted the risk register"}],
        }),
        None,
    )
    .await;
    assert_eq!(published["status"], "approved", "{published}");
    let asset: Uuid = published["asset_id"]
        .as_str()
        .expect("asset id")
        .parse()
        .expect("asset uuid");
    let (kind, body): (String, String) =
        sqlx::query_as("SELECT kind, body FROM governed_asset WHERE tenant_id = $1 AND id = $2")
            .bind(f.seeded.tenant_id)
            .bind(asset)
            .fetch_one(&f.pg.super_pool)
            .await
            .expect("the minutes asset");
    assert_eq!(kind, "minutes", "the governed asset kind");
    assert!(body.contains("Noted the risk register"), "{body}");

    let (status, refused) = meetings::call(
        &f.running,
        "meetings.minutes_publish",
        json!({
            "meeting_id": meeting_id,
            "approval_id": approval,
            "title": "Board minutes",
            "lines": [],
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "minutes_already_published", "{refused}");

    let closed = meetings::ok(
        &f.running,
        "meetings.close",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    assert_eq!(closed["status"], "closed", "{closed}");

    // The DoD is enforced: a closed meeting has every item closed and its minutes published.
    let (open, minutes): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM meeting_agenda_item
                  WHERE tenant_id = $1 AND meeting_id = $2 AND outcome IS NULL),
                (SELECT count(*) FROM governed_asset
                  WHERE tenant_id = $1 AND kind = 'minutes' AND meta->>'meeting_id' = $3)",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_id.parse::<Uuid>().expect("meeting uuid"))
    .bind(meeting_id)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the meeting's DoD rows");
    assert_eq!(open, 0, "no open item");
    assert_eq!(minutes, 1, "the minutes are published");
}

#[tokio::test(flavor = "multi_thread")]
async fn close_refuses_an_open_item() {
    let f = meetings::fixture().await;
    let (meeting_id, _item_id) = live_meeting(&f).await;

    let (status, refused) = meetings::call(
        &f.running,
        "meetings.close",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "meeting_items_open", "{refused}");
}
