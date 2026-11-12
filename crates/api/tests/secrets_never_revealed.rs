//! S09 §7 (c): a secret is write-only, sealed at rest, and unreachable across tenants.
//!
//! "Never revealed" is asserted three ways, because a convention is not a guarantee:
//!
//! 1. **No response document carries the value** — create, get, list and rotate are all checked for the plaintext
//!    and for the `value`/`ciphertext` keys.
//! 2. **No action in the served manifest could reveal one** — the registry is searched for a reveal-shaped name
//!    and for an output schema with a `value` property under `secrets.*`.
//! 3. **The row itself does not contain the plaintext** — read as the superuser, `ciphertext` is bytes that differ
//!    from the value and `key_id` names the generation. A test that only read the API would pass against a store
//!    that wrote plaintext and simply never echoed it.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn assert_no_secret(where_: &str, body: &Value) {
    let text = body.to_string();
    assert!(
        !text.contains("s3cr3t-value"),
        "{where_} leaked the plaintext: {text}"
    );
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(map) => {
                for (key, value) in map {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "value" | "ciphertext" | "plaintext" | "secret"
                        ),
                        "{where_} has a {key:?} field: {text}"
                    );
                    stack.push(value);
                }
            }
            Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_is_written_read_rotated_and_never_returned() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    let (status, created) = running
        .invoke(
            "secrets.create",
            "dev:alice",
            json!({"name": "webhook-signing-key", "value": "s3cr3t-value"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_no_secret("secrets.create", &created);
    let secret = output(&created)["id"].as_str().expect("an id").to_owned();
    assert_eq!(output(&created)["version"], 1);
    assert_eq!(output(&created)["key_id"], "kek:v1");

    // Read back: metadata only.
    let (status, got) = running
        .invoke(
            "secrets.get",
            "dev:alice",
            json!({"id": secret}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_no_secret("secrets.get", &got);

    let (status, listed) = running
        .invoke("secrets.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_no_secret("secrets.list", &listed);
    assert_eq!(output(&listed)["items"].as_array().expect("items").len(), 1);

    // Rotate: the version moves, the value does not appear.
    let (status, rotated) = running
        .invoke(
            "secrets.rotate",
            "dev:alice",
            json!({"id": secret, "value": "s3cr3t-value-2"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{rotated}");
    assert_no_secret("secrets.rotate", &rotated);
    assert_eq!(
        output(&rotated)["version"],
        2,
        "a rotation bumps the version"
    );

    // A second tenant cannot read it at all.
    let (status, body) = running
        .invoke(
            "secrets.get",
            "dev:bob",
            json!({"id": secret}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant's secret is a 404: {body}");
    let _ = bob;

    // ── the row does not hold the plaintext ─────────────────────────────────────────────
    let ciphertext: Vec<u8> =
        sqlx::query_scalar("SELECT ciphertext FROM secret WHERE id = $1 AND tenant_id = $2")
            .bind(uuid::Uuid::parse_str(&secret).expect("uuid"))
            .bind(alice.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the sealed row");
    assert!(
        !ciphertext
            .windows(12)
            .any(|window| window == b"s3cr3t-value"),
        "the stored bytes contain the plaintext"
    );
    assert!(
        ciphertext.len() > 24 + 16,
        "a sealed value is nonce + ciphertext + tag"
    );

    // ── no action could reveal one ──────────────────────────────────────────────────────
    let registry = conveyor_actions::registry::registry();
    for spec in registry.iter() {
        let name = spec.name;
        assert!(
            !(name.starts_with("secrets.") && (name.contains("reveal") || name.contains("read"))),
            "the registry has a reveal-shaped action: {name}"
        );
        if name.starts_with("secrets.") {
            // Property NAMES, not substrings: `key_id`'s description mentions the ciphertext, and a check on
            // the serialised text would flag prose. What matters is that no field could hold a value.
            let schema = serde_json::to_value((spec.output)()).expect("schema");
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for property in properties.keys() {
                    assert!(
                        !matches!(
                            property.as_str(),
                            "value" | "ciphertext" | "plaintext" | "secret"
                        ),
                        "{name} has a {property:?} property in its output: {schema}"
                    );
                }
            }
        }
    }

    // A stale precondition must refuse the destructive act: rotating then deleting with the OLD
    // ETag must not destroy the rotated value. (Version 2 is current after the rotation above.)
    let (status, body) = running
        .invoke(
            "secrets.delete",
            "dev:alice",
            json!({"id": secret}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "a stale If-Match must not delete: {body}");
    let (status, _) = running
        .invoke(
            "secrets.get",
            "dev:alice",
            json!({"id": secret}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "the refused delete left the secret in place");

    // Delete, then it is gone.
    let (status, _) = running
        .invoke(
            "secrets.delete",
            "dev:alice",
            json!({"id": secret}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204);
    let (status, _) = running
        .invoke(
            "secrets.get",
            "dev:alice",
            json!({"id": secret}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "a deleted secret is gone");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_duplicate_name_is_a_conflict_not_a_second_secret() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, _) = running
        .invoke(
            "secrets.create",
            "dev:alice",
            json!({"name": "duplicate", "value": "one"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201);

    let (status, body) = running
        .invoke(
            "secrets.create",
            "dev:alice",
            json!({"name": "duplicate", "value": "two"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "a name is one secret: {body}");

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM secret")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count secrets");
    assert_eq!(count, 1, "the refused create wrote nothing");

    running.shutdown().await;
}
