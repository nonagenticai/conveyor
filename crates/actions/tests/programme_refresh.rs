//! `programmes.refresh` — the image's embedded programme seed applied through the action surface.
//!
//! The programme's truth is generated in the repository and compiled into the image; the tenant
//! rows only change when the seed is applied. `migrate --seed` is the operator CLI path; this
//! action is the console's. The tests prove the three properties the page relies on: the first
//! call applies the whole document, a second call is all-skipped, and the call can only touch the
//! caller's own tenant with the scope the family declares.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refresh_applies_the_embedded_seed_and_a_second_is_all_skipped() {
    let (pg, seeded, running) = fixture().await;
    let (status, body) = running
        .invoke("programmes.refresh", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["created"], 53, "{body}");
    assert_eq!(output(&body)["updated"], 0, "{body}");
    assert_eq!(output(&body)["skipped"], 0, "{body}");
    // The archived project (S06a) is skipped by design; the other 50 get their board.
    assert_eq!(
        output(&body)["boards_created"],
        51,
        "every non-archived seeded project gets its board: {body}"
    );

    let projects: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1 AND meta ? 'track'")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count the tenant's projects");
    assert_eq!(projects, 52, "the tenant owns the seeded projects");

    let (status, body) = running
        .invoke("programmes.refresh", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["created"], 0, "a second call creates nothing");
    assert_eq!(output(&body)["updated"], 0, "a second call updates nothing");
    assert_eq!(output(&body)["skipped"], 53, "{body}");
    assert_eq!(output(&body)["boards_created"], 0, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_tenant_is_untouched() {
    let (pg, _seeded, running) = fixture().await;
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let (status, body) = running
        .invoke("programmes.refresh", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(other.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the other tenant's projects");
    assert_eq!(rows, 0, "the caller's refresh never writes another tenant");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_cannot_refresh() {
    let (pg, seeded, running) = fixture().await;
    let viewer_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:viewer', 'viewer@dev.invalid', 'Viewer')
         RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the viewer");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(seeded.tenant_id)
        .bind(viewer_id)
        .execute(&mut *tx)
        .await
        .expect("the viewer membership");
    tx.commit().await.expect("commit the viewer membership");
    let (status, body) = running
        .invoke(
            "programmes.refresh",
            "dev:viewer",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
}
