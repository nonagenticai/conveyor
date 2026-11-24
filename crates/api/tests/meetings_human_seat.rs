//! T19 (#606) D6 — the human seat across surfaces: the first decision wins, the owner decides
//! in the web room and over MCP, both records say `via='meeting'`, and the ICS feed is a read.
//!
//! `cargo test -p conveyor-api --test meetings_human_seat`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn the_first_decision_across_surfaces_counts() {
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

    // The first decision is taken over the MCP surface.
    let mut mcp = support::Mcp::connect(&f.running, meetings::TOKEN).await;
    mcp.call(
        "approvals_decide",
        json!({"approval_id": approval, "decision": "approve"}),
        None,
    )
    .await;

    // The web room's later position cannot overwrite it.
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
    assert_eq!(refused["type"], "decision_already_recorded", "{refused}");

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count decisions");
    assert_eq!(
        rows, 1,
        "exactly one decision row, whichever surface was second"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_decides_on_web_and_mcp_via_meeting() {
    let f = meetings::fixture().await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let web_approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let mcp_approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let web_item = meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        web_approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let mcp_item = meetings::agenda_add(
        &f,
        &meeting_id,
        "approval",
        mcp_approval,
        json!([{"kind": "document", "ref": "readme.md"}]),
    )
    .await;
    let web_item_id = meetings::id_of(&web_item);
    let mcp_item_id = meetings::id_of(&mcp_item);
    for participant in [f.seeded.user_id, bob] {
        meetings::invite(&f, &meeting_id, "app_user", participant, "member", None).await;
        meetings::respond(&f, &meeting_id, "app_user", participant, "accepted").await;
    }

    // The owner decides the first item in the web room.
    meetings::position(
        &f,
        &meeting_id,
        &web_item_id,
        "app_user",
        f.seeded.user_id,
        "approve",
        "web",
    )
    .await;
    meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": web_item_id}),
        None,
    )
    .await;

    // The second asynchronously, through the MCP tool.
    let mut mcp = support::Mcp::connect(&f.running, meetings::TOKEN).await;
    mcp.call(
        "meetings_position",
        json!({
            "meeting_id": meeting_id,
            "agenda_item_id": mcp_item_id,
            "participant_kind": "app_user",
            "participant_id": f.seeded.user_id,
            "stance": "approve",
            "surface": "mcp",
        }),
        None,
    )
    .await;
    meetings::ok(
        &f.running,
        "meetings.close_item",
        json!({"meeting_id": meeting_id, "agenda_item_id": mcp_item_id}),
        None,
    )
    .await;

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT approval_id::text, via->>'channel'
           FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = ANY($2::uuid[])
          ORDER BY via->>'channel'",
    )
    .bind(f.seeded.tenant_id)
    .bind([web_approval, mcp_approval])
    .fetch_all(&f.pg.super_pool)
    .await
    .expect("both decisions");
    assert_eq!(rows.len(), 2, "one row per item");
    assert_eq!(
        rows,
        vec![
            (mcp_approval.to_string(), "mcp".to_owned()),
            (web_approval.to_string(), "web".to_owned()),
        ],
        "each row records the surface it was decided on, and both are meeting decisions"
    );
    let surfaces: Vec<String> = sqlx::query_scalar(
        "SELECT via->>'surface' FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = ANY($2::uuid[])",
    )
    .bind(f.seeded.tenant_id)
    .bind([web_approval, mcp_approval])
    .fetch_all(&f.pg.super_pool)
    .await
    .expect("surfaces");
    assert!(
        surfaces.iter().all(|surface| surface == "meeting"),
        "{surfaces:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ics_feed_is_a_read() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    meetings::invite(&f, &meeting_id, "app_user", f.seeded.user_id, "chair", None).await;
    meetings::respond(&f, &meeting_id, "app_user", f.seeded.user_id, "accepted").await;

    let meeting_uuid: Uuid = meeting_id.parse().expect("meeting uuid");
    let before: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM event WHERE tenant_id = $1),
                (SELECT version FROM meeting WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_uuid)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("before");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/meetings/{meeting_id}/ics",
            f.running.base()
        ))
        .bearer_auth(meetings::TOKEN)
        .send()
        .await
        .expect("the ICS feed");
    assert_eq!(response.status(), 200, "the feed answers");
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    assert!(content_type.starts_with("text/calendar"), "{content_type}");
    let body = response.text().await.expect("the calendar");
    assert!(body.contains("BEGIN:VCALENDAR"), "{body}");
    assert!(body.contains("BEGIN:VEVENT"), "{body}");
    assert!(
        body.contains(meeting["card_name"].as_str().expect("card name")),
        "{body}"
    );
    assert!(
        body.contains("ATTENDEE"),
        "the attendance is the feed: {body}"
    );

    let after: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM event WHERE tenant_id = $1),
                (SELECT version FROM meeting WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_uuid)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("after");
    assert_eq!(before, after, "the feed writes nothing");
}
