//! S07 §5.4 / S06 §5.3 — `project_request.timeout` at the action surface.
//!
//! The worker's own target (`apps/server/tests/project_request_timeout.rs`) proves the job's
//! dispatch and its event. This target proves the state that job leaves behind is what the
//! bridge and the review screen read: `expired` is visible through
//! `project_requests.get|list`, terminal for `approve`, and not re-openable by a re-post.
//! The sweep is the store function the worker calls, run here on the same runtime role.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    (pg, running, alice)
}

/// `project_requests.create` as alice, returning the request document.
async fn create(running: &support::Running, external_ref: &str) -> Value {
    let (status, body) = running
        .invoke(
            "project_requests.create",
            "dev:alice",
            json!({
                "external_ref": external_ref,
                "external_origin": "control-plane",
                "payload": {"title": external_ref},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body).clone()
}

/// One `project_request.timeout` pass, exactly as the worker runs it.
async fn sweep(pg: &support::Pg, tenant: TenantId) -> usize {
    let mut tx = store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the sweep");
    let cutoff = Utc::now() - ChronoDuration::hours(store::project_request::TIMEOUT_HOURS);
    let expired = store::project_request::expire_stale_project_requests_tx(&mut tx, tenant, cutoff)
        .await
        .expect("expire the stale drafts");
    tx.commit().await.expect("commit the sweep");
    expired.len()
}

#[tokio::test(flavor = "multi_thread")]
async fn expiry_is_visible_terminal_and_idempotent_at_the_action_surface() {
    let (pg, running, alice) = fixture().await;
    let stale = create(&running, "cp-stale").await;
    let fresh = create(&running, "cp-fresh").await;
    let stale_id = stale["id"].as_str().expect("stale id").to_owned();
    let fresh_id = fresh["id"].as_str().expect("fresh id").to_owned();

    // Backdate one request past the 24 h window, leaving the other fresh in the same tenant.
    sqlx::query(
        "UPDATE project_request SET created_at = now() - interval '25 hours' WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&stale_id).expect("stale uuid"))
    .execute(&pg.super_pool)
    .await
    .expect("backdate the stale request");

    assert_eq!(
        sweep(&pg, alice.tenant).await,
        1,
        "only the stale draft expires"
    );

    let (status, body) = running
        .invoke(
            "project_requests.get",
            "dev:alice",
            json!({"id": stale_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["status"],
        "expired",
        "the row shows the state"
    );
    assert_eq!(output(&body)["version"], 2, "the expiry bumps the version");
    assert_eq!(
        output(&body)["external_ref"],
        "cp-stale",
        "the row is kept, never deleted"
    );

    let (_, body) = running
        .invoke(
            "project_requests.get",
            "dev:alice",
            json!({"id": fresh_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        output(&body)["status"],
        "draft",
        "a fresh draft is untouched"
    );
    assert_eq!(output(&body)["version"], 1);

    // The status filter admits the new vocabulary and returns the expired row.
    let (status, body) = running
        .invoke(
            "project_requests.list",
            "dev:alice",
            json!({"status": "expired"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], stale_id);

    // Terminal: approve is refused (409), and a re-post is the Locked 409 naming the status.
    let (status, body) = running
        .invoke(
            "project_requests.approve",
            "dev:alice",
            json!({"id": stale_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict");
    let (status, body) = running
        .invoke(
            "project_requests.create",
            "dev:alice",
            json!({"external_ref": "cp-stale", "external_origin": "control-plane"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("expired"),
        "the refusal names the status that blocked it: {body}"
    );

    // Idempotent: the sweep matches only `draft`, so a second pass writes nothing.
    assert_eq!(
        sweep(&pg, alice.tenant).await,
        0,
        "the second pass expires nothing"
    );
    let (_, body) = running
        .invoke(
            "project_requests.get",
            "dev:alice",
            json!({"id": stale_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(output(&body)["status"], "expired");
    assert_eq!(output(&body)["version"], 2, "the row was not bumped twice");
}
