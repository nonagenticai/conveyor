//! S04 P1/P2/P3: one tenant-aware resolution model for compiled and approved imported actions.
//!
//! `widgets.list` is not compiled into this binary. It is imported from an OpenAPI document,
//! approved, discovered, invoked over HTTP and MCP, resolved again after a restart, revoked and
//! refreshed. The stub echoes the path, the query and the bearer, so the bindings and the tenant
//! credential are asserted on what the upstream received — not on what the engine says it sent.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};

const WIDGETS_V1: &[u8] = include_bytes!("fixtures/external/widgets_v1.json");
const WIDGETS_V2: &[u8] = include_bytes!("fixtures/external/widgets_v2.json");

/// Point the approved `widgets.list` row at the probe contract: `GET /probe/{id}`, `id` in the
/// path, `q` in the query, no body, and the tenant credential `widgets-token`.
async fn point_at_probe(pg: &support::Pg, tenant_id: uuid::Uuid) {
    let spec = json!({
        "method": "GET",
        "path": "/probe/{id}",
        "input": {
            "type": "object",
            "properties": {"id": {"type": "string"}, "q": {"type": "string"}},
            "required": ["id"],
            "additionalProperties": false
        },
        "output": {"type": "object"},
        "required": ["id"],
        "schema_hash": "probe",
        "runtime": {"auth": "widgets-token"},
        "bindings": {"path": ["id"], "query": ["q"], "body": []}
    });
    sqlx::query(
        "UPDATE action_registry SET spec = $2::jsonb \
          WHERE tenant_id = $1 AND name = 'widgets.list'",
    )
    .bind(tenant_id)
    .bind(spec.to_string())
    .execute(&pg.super_pool)
    .await
    .expect("point the approved row at the probe");
}

fn structured_content(result: &Value) -> Value {
    result
        .get("structuredContent")
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_imported_action_resolves_through_every_surface() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let stub = support::Stub::start().await;
    // One process-global URL for the service this binary imports; every database below shares it.
    support::set_env("CONVEYOR_EXTERNAL_WIDGETS_URL", &stub.base);

    // 1. Import → approve through the production path.
    let outcome = conveyor_actions::ingest::ingest_production(
        &pg.owner,
        seeded.tenant,
        "widgets",
        WIDGETS_V1,
        false,
    )
    .await
    .expect("ingest widgets");
    assert_eq!(outcome.approved, vec![("widgets.list".to_owned(), 1)]);
    point_at_probe(&pg, seeded.tenant_id).await;

    // 2. The boot rule accepts the imported row (P1.1).
    conveyor_actions::resolve::assert_boot_registry(&pg.owner)
        .await
        .expect("boot accepts an approved imported action");

    let running = support::Running::start(&pg).await;
    let (status, body) = running
        .invoke(
            "secrets.create",
            "dev:alice",
            json!({"name": "widgets-token", "value": "s3cret"}),
            None,
            None,
            None,
        )
        .await;
    assert!((200..300).contains(&status), "seal the credential: {body}");

    // 3. Discovery lists it over HTTP ...
    let manifest: Value = reqwest::Client::new()
        .get(format!("{}/api/actions", running.base()))
        .bearer_auth("dev:alice")
        .send()
        .await
        .expect("manifest")
        .json()
        .await
        .expect("manifest json");
    let names: Vec<&str> = manifest["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(
        names.contains(&"widgets.list"),
        "the manifest lists the imported action: {names:?}"
    );

    // ... and over MCP.
    let mut mcp = support::Mcp::connect(&running, "dev:alice").await;
    let tools = mcp.list_tools().await;
    let tool_names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        tool_names.contains(&"widgets_list"),
        "the MCP list carries it: {tool_names:?}"
    );

    // 4. HTTP invocation: the path is substituted and encoded, the query appended, the bearer opened
    //    from the tenant's sealed secret (P3.1).
    let input = json!({"id": "a b", "q": "x y"});
    let (status, body) = running
        .invoke("widgets.list", "dev:alice", input.clone(), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let output = body["output"].clone();
    assert_eq!(
        output["id"], "a b",
        "the path segment round-trips: {output}"
    );
    assert_eq!(
        output["query"]["q"], "x y",
        "the query round-trips: {output}"
    );
    assert_eq!(
        output["authorization"], "Bearer s3cret",
        "the tenant credential reached the upstream: {output}"
    );

    // 5. MCP resolves the same definition and returns the same output (P1.2).
    let result = mcp.call("widgets_list", input.clone(), None).await;
    assert_eq!(
        structured_content(&result)["output"],
        output,
        "HTTP and MCP diverge: {result}"
    );

    // 6. Restart: a new server over the same database still resolves it (P1.1/P2.1).
    running.shutdown().await;
    let running = support::Running::start(&pg).await;
    let (status, body) = running
        .invoke("widgets.list", "dev:alice", input.clone(), None, None, None)
        .await;
    assert_eq!(status, 200, "after restart: {body}");

    // 7. Revocation: versions remain, no approved row does → typed 404, and no surface lists it
    //    (P1.4).
    sqlx::query(
        "UPDATE action_registry SET approved_at = NULL \
          WHERE tenant_id = $1 AND name = 'widgets.list'",
    )
    .bind(seeded.tenant_id)
    .execute(&pg.super_pool)
    .await
    .expect("revoke the approval");
    let (status, body) = running
        .invoke("widgets.list", "dev:alice", input.clone(), None, None, None)
        .await;
    assert_eq!(status, 404, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or("")
            .contains("no version is approved"),
        "a revoked action is typed as unavailable, not unknown: {body}"
    );

    // 8. A breaking refresh keeps the approved version callable (P1.3). A second database, so the
    //    revoked row above is not reused.
    let pg2 = support::Pg::fresh().await;
    let seeded2 = support::seed(&pg2, "dev-b", "alice", "Alice").await;
    conveyor_actions::ingest::ingest_production(
        &pg2.owner,
        seeded2.tenant,
        "widgets",
        WIDGETS_V1,
        false,
    )
    .await
    .expect("the first ingest");
    let refresh = conveyor_actions::ingest::ingest_production(
        &pg2.owner,
        seeded2.tenant,
        "widgets",
        WIDGETS_V2,
        false,
    )
    .await
    .expect("the refresh");
    assert!(
        refresh.has_breaking(),
        "the refresh is breaking: {refresh:?}"
    );
    let action = conveyor_actions::resolve::get(&pg2.owner, seeded2.tenant, "widgets.list")
        .await
        .expect("resolve the approved version");
    assert_eq!(action.version, 1, "the approved version still serves");
    let mut tx = begin_scoped(&pg2.owner, seeded2.tenant)
        .await
        .expect("scope");
    let versions = conveyor_store::actions::registry_versions(&mut tx, "widgets.list")
        .await
        .expect("versions");
    tx.commit().await.expect("commit");
    assert!(
        versions
            .iter()
            .any(|row| row.spec_version == 2 && row.approved_at.is_none()),
        "the breaking version is held for approval: {versions:?}"
    );

    // 8b. The deadline covers the BODY read (P3.2): the stub sends headers at once and stalls the
    //     body for 2 s; a 200 ms action times out instead of waiting for it.
    // A raw TCP server: headers at once, the body only after 2 s. It is its own listener so the
    // shared stub keeps its fixed routes.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stalling listener");
    let stall_addr = listener.local_addr().expect("stall addr");
    let stall_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0_u8; 1024];
                let _ = socket.read(&mut buffer).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n",
                    )
                    .await;
                let _ = socket.flush().await;
                tokio::time::sleep(std::time::Duration::from_millis(2_000)).await;
                let _ = socket.write_all(b"{}").await;
            });
        }
    });
    support::set_env(
        "CONVEYOR_EXTERNAL_WIDGETS_URL",
        &format!("http://{stall_addr}"),
    );
    let stall_spec = json!({
        "method": "GET",
        "path": "/stall",
        "input": {"type": "object", "properties": {}},
        "output": {"type": "object"},
        "required": [],
        "schema_hash": "stall",
        "bindings": {"path": [], "query": [], "body": []}
    });
    sqlx::query(
        "UPDATE action_registry SET spec = $2::jsonb, timeout_ms = 200 \
          WHERE tenant_id = $1 AND name = 'widgets.list'",
    )
    .bind(seeded2.tenant_id)
    .bind(stall_spec.to_string())
    .execute(&pg2.super_pool)
    .await
    .expect("point the row at the stalling route");
    let stalled = support::Running::start(&pg2).await;
    let started = std::time::Instant::now();
    let (status, body) = stalled
        .invoke("widgets.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 504, "a stalled body is a typed timeout: {body}");
    assert_eq!(body["type"], "action-timeout", "{body}");
    assert!(
        started.elapsed() < std::time::Duration::from_millis(1_500),
        "the deadline did not wait for the body: {:?}",
        started.elapsed()
    );
    stalled.shutdown().await;
    stall_task.abort();

    // 9. Negative boot rule: an imported workflow is refused — this binary has no engine for it.
    sqlx::query(
        "INSERT INTO action_registry \
             (tenant_id, name, spec_version, kind, service, spec, schema_hash, timeout_ms, \
              idempotent, approved_at) \
         VALUES ($1, 'widgets.flow', 1, 'workflow', 'widgets', $2::jsonb, 'h', 30000, false, now())",
    )
    .bind(seeded2.tenant_id)
    .bind(json!({"method": "POST", "path": "/x", "input": {}, "output": {}}).to_string())
    .execute(&pg2.super_pool)
    .await
    .expect("insert a workflow row");
    let refused = conveyor_actions::resolve::assert_boot_registry(&pg2.owner).await;
    let refused_for_the_workflow = refused
        .as_ref()
        .is_err_and(|detail| detail.contains("imported workflow"));
    assert!(
        refused_for_the_workflow,
        "an imported workflow is refused at boot: {refused:?}"
    );

    running.shutdown().await;
}
