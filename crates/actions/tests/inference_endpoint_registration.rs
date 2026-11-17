//! conveyor#160 P2: a tenant-admin registers the inference destination and its sealed credential;
//! a member cannot, and nothing is written by the refused call.
//!
//! The audit and the seal are asserted on the rows, not on the response: the event names the
//! destination and whether the credential moved, and the ciphertext never contains the value.

mod support;

use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_admin_registers_the_destination_and_credential_and_a_member_cannot() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // Bob is a member of Alice's tenant: `seed` makes admins, so the refusal must be a real role.
    let bob_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:bob', 'bob@dev.invalid', 'Bob') RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert bob");
    let mut tx = begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the membership write");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(alice.tenant_id)
        .bind(bob_id)
        .execute(&mut *tx)
        .await
        .expect("insert the member");
    tx.commit().await.expect("commit the membership");

    let running = support::Running::start(&pg).await;

    // Nothing registered yet.
    let (status, body) = running
        .invoke(
            "inference_endpoint.get",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["destination_url"], json!(null));
    assert_eq!(body["output"]["credential_present"], false);

    // The admin registers both halves.
    let (status, body) = running
        .invoke(
            "inference_endpoint.set",
            "dev:alice",
            json!({
                "destination_url": "https://router.example/v1",
                "credential": "tok-secret-123",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"]["destination_url"],
        "https://router.example/v1"
    );
    assert_eq!(body["output"]["credential_present"], true);

    // The row is there, and the credential is sealed — never stored in the clear.
    let destination: String = sqlx::query_scalar(
        "SELECT destination_url FROM tenant_inference_endpoint WHERE tenant_id = $1",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the endpoint");
    assert_eq!(destination, "https://router.example/v1");
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM secret WHERE tenant_id = $1 AND name = 'inference.token'",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the sealed credential");
    assert!(
        !String::from_utf8_lossy(&ciphertext).contains("tok-secret-123"),
        "the credential was stored in the clear"
    );

    // The audit names the destination and that the credential moved — never the value.
    let (event_type, payload): (String, serde_json::Value) = sqlx::query_as(
        "SELECT type, payload FROM event WHERE aggregate_id = $1 ORDER BY seq DESC LIMIT 1",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the endpoint event");
    assert_eq!(event_type, "inference_endpoint.set");
    assert_eq!(payload["destination_url"], "https://router.example/v1");
    assert_eq!(payload["credential_updated"], true);
    assert!(
        !payload.to_string().contains("tok-secret-123"),
        "the event leaked the credential"
    );

    // A member cannot register, and nothing moves.
    let (status, body) = running
        .invoke(
            "inference_endpoint.set",
            "dev:bob",
            json!({"destination_url": "https://evil.example/v1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    let destination: String = sqlx::query_scalar(
        "SELECT destination_url FROM tenant_inference_endpoint WHERE tenant_id = $1",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the endpoint after the refusal");
    assert_eq!(
        destination, "https://router.example/v1",
        "the member's write landed"
    );

    // A credential embedded in the URL is refused by validation, not parsed and used.
    let (status, body) = running
        .invoke(
            "inference_endpoint.set",
            "dev:alice",
            json!({"destination_url": "https://user:pass@router.example/v1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input");
}
