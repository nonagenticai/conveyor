//! Property (e): two identical POSTs with one `Idempotency-Key` return the same status and
//! body, and produce exactly ONE `action_invocation` and ONE mutation.

mod support;

use conveyor_store::begin_scoped;
use serde_json::json;

#[tokio::test]
async fn one_key_two_posts_replay_and_write_one_invocation_and_one_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let key = "create-engineering-once";
    let (status_1, body_1) = running
        .invoke(
            "departments.create",
            "dev:alice",
            json!({"name": "engineering"}),
            None,
            Some(key),
            Some("rid-first"),
        )
        .await;
    // CONTRACT §3: a creation is `201`, and the replay must keep `201` (not 200).
    assert_eq!(status_1, 201, "{body_1}");
    let created = body_1["output"]["id"].as_str().expect("the department id");

    let (status_2, body_2) = running
        .invoke(
            "departments.create",
            "dev:alice",
            json!({"name": "engineering"}),
            None,
            Some(key),
            Some("rid-second"),
        )
        .await;
    assert_eq!(status_2, status_1);
    assert_eq!(body_2, body_1, "the replay is the original body verbatim");
    assert_eq!(body_2["output"]["id"], created);

    // Exactly one invocation row carrying the key, and exactly one department.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (invocations,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM action_invocation
          WHERE action = 'departments.create' AND idempotency_key = $1",
    )
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .expect("count invocations");
    let (departments,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM department WHERE name = 'engineering'")
            .fetch_one(&mut *tx)
            .await
            .expect("count departments");
    tx.commit().await.expect("commit");
    assert_eq!(
        invocations, 1,
        "the replay must not write a second invocation"
    );
    assert_eq!(
        departments, 1,
        "the key and the mutation share a transaction"
    );

    // The same key with a DIFFERENT body is a client bug, refused, not silently replayed.
    let (status_3, body_3) = running
        .invoke(
            "departments.create",
            "dev:alice",
            json!({"name": "sales"}),
            None,
            Some(key),
            None,
        )
        .await;
    assert_eq!(status_3, 409, "{body_3}");
    assert_eq!(body_3["type"], "idempotency-conflict");

    running.shutdown().await;
}
