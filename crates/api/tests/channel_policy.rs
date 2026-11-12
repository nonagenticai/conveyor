//! S06d — the connector administration surface: kinds, bindings with sealed secret names, and
//! the tenant-admin grant matrix (including the floor that refuses an eval sign-off).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

async fn seeded_pg() -> support::Pg {
    support::Pg::fresh().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kinds_are_a_catalogue_with_schemas_and_the_bind_refuses_a_missing_secret() {
    let pg = seeded_pg().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke("channels.kinds", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let kinds = body["output"]["kinds"].as_array().expect("kinds");
    let names: Vec<&str> = kinds
        .iter()
        .filter_map(|kind| kind["kind"].as_str())
        .collect();
    for expected in ["telegram", "webhook", "web", "mcp"] {
        assert!(
            names.contains(&expected),
            "missing kind {expected}: {names:?}"
        );
    }
    let telegram = kinds
        .iter()
        .find(|kind| kind["kind"] == "telegram")
        .expect("telegram kind");
    assert!(telegram["config_schema"].is_object());
    assert_eq!(telegram["identity"], "provisioned");

    // A config that names a secret the tenant does not have is refused at bind time.
    let (status, body) = running
        .invoke(
            "channels.bind",
            "dev:alice",
            json!({
                "tenant_id": seeded.tenant_id,
                "channel_kind": "telegram",
                "route_key": "default",
                "config": {
                    "chat_id": "42",
                    "bot_token_secret": "missing-token",
                    "incoming_secret": "missing-incoming",
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("does not exist"),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_connector_projects_its_url_and_identity_and_a_test_queues_a_delivery() {
    let pg = seeded_pg().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    for (name, value) in [
        ("telegram-token", "123:token"),
        ("telegram-incoming", "s3cret"),
    ] {
        let (status, body) = running
            .invoke(
                "secrets.create",
                "dev:alice",
                json!({ "name": name, "value": value }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "channels.bind",
            "dev:alice",
            json!({
                "tenant_id": seeded.tenant_id,
                "channel_kind": "telegram",
                "route_key": "default",
                "config": {
                    "chat_id": "42",
                    "bot_token_secret": "telegram-token",
                    "incoming_secret": "telegram-incoming",
                    "api_base": "http://127.0.0.1:9"
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let binding_id = body["output"]["id"]
        .as_str()
        .expect("binding id")
        .to_owned();
    assert_eq!(
        body["output"]["inbound_url"],
        format!("/api/channels/{binding_id}/inbound")
    );
    assert_eq!(body["output"]["secrets_set"]["telegram-token"], true);
    assert_eq!(body["output"]["secrets_set"]["telegram-incoming"], true);
    assert!(
        body["output"]["config"].get("bot_token").is_none(),
        "no secret value is ever returned"
    );

    // The provisioned identity exists and is a machine service account.
    let identity = sqlx::query_as::<_, (String, String)>(
        "SELECT u.kind, m.role FROM app_user u
           JOIN membership m ON m.user_id = u.id
          WHERE m.tenant_id = $1 AND u.oidc_subject LIKE 'service:telegram:%'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the connector identity");
    assert_eq!(identity, ("service".to_owned(), "kata_executor".to_owned()));

    let (status, body) = running
        .invoke("channels.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["bindings"].as_array().map(Vec::len), Some(1));

    // `channels.test` queues a real delivery; without a worker it stays pending and the row is
    // the proof the binding was addressed.
    let (status, body) = running
        .invoke(
            "channels.test",
            "dev:alice",
            json!({ "binding_id": binding_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let delivery_id = Uuid::parse_str(body["output"]["delivery_id"].as_str().expect("delivery id"))
        .expect("uuid");
    let recipient: String = sqlx::query_scalar("SELECT recipient_ref FROM delivery WHERE id = $1")
        .bind(delivery_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the queued delivery");
    assert_eq!(recipient, format!("binding:{binding_id}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_matrix_grants_a_lifecycle_action_but_never_an_eval_signoff() {
    let pg = seeded_pg().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "channels.policy_get",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let telegram = body["output"]["channels"]
        .as_array()
        .expect("channels")
        .iter()
        .find(|channel| channel["channel_kind"] == "telegram")
        .expect("telegram card");
    let actions = telegram["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["actions"].as_array().expect("actions").clone())
        .collect::<Vec<Value>>();
    let decide = actions
        .iter()
        .find(|action| action["action"] == "approvals.decide")
        .expect("approvals.decide row");
    assert_eq!(decide["default"], true);
    assert_eq!(decide["grantable"], true);
    let signoff = actions
        .iter()
        .find(|action| action["action"] == "eval_signoffs.decide")
        .expect("eval_signoffs.decide row");
    assert_eq!(signoff["grantable"], false, "the floor is visible");
    assert_eq!(signoff["denial"], "human sign-off");

    // The floor refuses even a direct write.
    let (status, body) = running
        .invoke(
            "channels.policy_set",
            "dev:alice",
            json!({
                "channel_kind": "telegram",
                "entries": [{"action": "eval_signoffs.decide", "allowed": true}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("human sign-off"),
        "{body}"
    );

    // A lifecycle action not in the compiled default can be granted, and reset restores it.
    let (status, body) = running
        .invoke(
            "channels.policy_set",
            "dev:alice",
            json!({
                "channel_kind": "telegram",
                "entries": [{"action": "cards.create", "allowed": true}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let granted = body["output"]["channels"]
        .as_array()
        .expect("channels")
        .iter()
        .find(|channel| channel["channel_kind"] == "telegram")
        .expect("telegram card")["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["actions"].as_array().expect("actions").clone())
        .find(|action| action["action"] == "cards.create")
        .expect("cards.create row");
    assert_eq!(granted["allowed"], true);
    assert_eq!(granted["overridden"], true);

    let (status, body) = running
        .invoke(
            "channels.policy_reset",
            "dev:alice",
            json!({ "channel_kind": "telegram" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let reset = body["output"]["channels"]
        .as_array()
        .expect("channels")
        .iter()
        .find(|channel| channel["channel_kind"] == "telegram")
        .expect("telegram card")["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["actions"].as_array().expect("actions").clone())
        .find(|action| action["action"] == "cards.create")
        .expect("cards.create row");
    assert_eq!(reset["allowed"], false);
    assert_eq!(reset["overridden"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_member_cannot_read_the_matrix_and_another_tenant_sees_its_own() {
    let pg = seeded_pg().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    let (status, _) = running
        .invoke(
            "channels.policy_get",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);

    // A viewer is refused the admin surface.
    sqlx::query(
        "INSERT INTO app_user (oidc_subject, display_name) VALUES ('dev:viewer', 'Vera') ON CONFLICT DO NOTHING",
    )
    .execute(&pg.super_pool)
    .await
    .expect("viewer user");
    let viewer: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE oidc_subject = 'dev:viewer'")
            .fetch_one(&pg.super_pool)
            .await
            .expect("viewer id");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'viewer')")
        .bind(other.tenant_id)
        .bind(viewer)
        .execute(&pg.super_pool)
        .await
        .expect("viewer membership");
    let (status, body) = running
        .invoke(
            "channels.policy_get",
            "dev:viewer",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");

    // Tenant B has its own matrix and no tenant A binding leaks into it.
    let (status, body) = running
        .invoke(
            "channels.policy_get",
            "dev:bob",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let bound = body["output"]["channels"]
        .as_array()
        .expect("channels")
        .iter()
        .any(|channel| channel["bound"] == true);
    assert!(!bound, "tenant B has no bindings: {body}");
}
