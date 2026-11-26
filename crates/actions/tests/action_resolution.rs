//! S04 P1/P2 (issue #3): an imported, approved action resolves through the **same** model as a
//! compiled one — boot, HTTP and MCP discovery, invocation, a process restart, a held breaking
//! refresh and revocation.
//!
//! The fixture service (`widgets`) names an operation the binary does not compile, so every
//! assertion below runs through the imported path: `assert_db_registry` at boot, `precheck` at
//! invocation, the HTTP manifest and MCP `tools/list` at discovery, and the tenant's approval as
//! the on/off switch.

mod support;

use conveyor_actions::ingest;
use conveyor_actions::resolve::{ResolveError, resolve_row};
use conveyor_actions::{ActionKind, Effect, HandlerRef};
use conveyor_store::actions::RegistryRow;
use serde_json::{Value, json};
use uuid::Uuid;

const WIDGETS_V1: &[u8] = include_bytes!("fixtures/external/widgets_v1.json");
const WIDGETS_V2: &[u8] = include_bytes!("fixtures/external/widgets_v2.json");

/// `service_url` reads `CONVEYOR_EXTERNAL_<SERVICE>_URL`, so an imported `widgets` action needs
/// this key pointing at the stub.
const WIDGETS_URL_ENV: &str = "CONVEYOR_EXTERNAL_WIDGETS_URL";

fn stored_row(kind: &str, method: &str, path: &str, input: Value) -> RegistryRow {
    RegistryRow {
        id: Uuid::now_v7(),
        name: "widgets.list".to_owned(),
        spec_version: 3,
        kind: kind.to_owned(),
        service: "widgets".to_owned(),
        spec: json!({
            "method": method,
            "path": path,
            "input": input,
            "output": {"type": "object"},
            "schema_hash": "h",
        }),
        schema_hash: "h".to_owned(),
        timeout_ms: 5_000,
        idempotent: true,
        approved_at: Some(chrono::Utc::now()),
        approved_by: None,
    }
}

#[test]
fn a_stored_row_becomes_an_action_spec_and_a_bad_row_is_refused_typed() {
    let spec = resolve_row(&stored_row(
        "external",
        "post",
        "/widgets/search",
        json!({"type": "object", "properties": {"query": {"type": "string"}}}),
    ))
    .expect("an approved row resolves");
    assert_eq!(spec.name.as_ref(), "widgets.list");
    assert_eq!(spec.version, 3);
    assert_eq!(spec.kind, ActionKind::External);
    assert_eq!(spec.effect, Effect::Write, "POST writes");
    assert!(matches!(&spec.handler, HandlerRef::External { .. }));
    assert_eq!(
        spec.input_schema(),
        json!({"type": "object", "properties": {"query": {"type": "string"}}})
    );
    assert_eq!(spec.spec_hash().len(), 64, "a real contract hash");

    assert!(matches!(
        resolve_row(&stored_row("workflow", "post", "/x", json!({}))),
        Err(ResolveError::UnsupportedKind { .. })
    ));
    assert!(matches!(
        resolve_row(&stored_row("external", "TRACE", "/x", json!({}))),
        Err(ResolveError::BadMethod { .. })
    ));
    assert!(matches!(
        resolve_row(&stored_row("external", "post", "//evil", json!({}))),
        Err(ResolveError::BadPath { .. })
    ));
    let mut missing_input = stored_row("external", "post", "/x", json!({}));
    missing_input
        .spec
        .as_object_mut()
        .expect("spec object")
        .remove("input");
    assert!(matches!(
        resolve_row(&missing_input),
        Err(ResolveError::BadSpec { field: "input", .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_imported_action_boots_lists_and_invokes_over_http_and_mcp() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let first = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V1, false)
        .await
        .expect("first ingest");
    assert_eq!(first.approved, vec![("widgets.list".to_owned(), 1)]);

    let stub = support::Stub::start().await;
    support::set_env(WIDGETS_URL_ENV, &stub.base);

    // P1.1: the boot gate accepts an approved imported action. `serve` calls this before bind;
    // before S04 it was `RunError::Refused` ("the database names action … which this binary
    // does not compile").
    conveyor_actions::resolve::assert_served_registry(
        &pg.owner,
        conveyor_actions::registry::registry(),
    )
    .await
    .expect("the boot gate accepts an approved imported action");

    let running = support::Running::start(&pg).await;

    // P1.1: discovery over HTTP lists it.
    let (status, manifest) = running
        .http(
            reqwest::Method::GET,
            "/api/actions",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{manifest}");
    let http_names: Vec<&str> = manifest["actions"]
        .as_array()
        .expect("a manifest array")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert!(
        http_names.contains(&"widgets.list"),
        "the imported action is absent from the HTTP manifest"
    );

    // P1.2: the MCP tool list carries the same action under its MCP name.
    let mut mcp = support::Mcp::connect(&running, "dev:alice").await;
    let tools = mcp.list_tools().await;
    let tool_names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("a tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        tool_names.contains(&"widgets_list"),
        "the imported action is absent from MCP tools/list: {tools}"
    );

    // P1.2/P2.1: the HTTP call resolves the stored spec and reaches the upstream. The stub's
    // fallback answers with the path it was asked for, which is the witness that the stored
    // operation — not a compiled one — was called.
    let (status, body) = running
        .invoke(
            "widgets.list",
            "dev:alice",
            json!({"query": "bolts"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"]["path"], "/widgets/search",
        "the imported action never reached its stored upstream path: {body}"
    );

    // P1.2: the same action over MCP resolves the same definition and returns the same output.
    let result = mcp
        .call("widgets_list", json!({"query": "bolts"}), None)
        .await;
    let structured = result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| panic!("no structured content: {result}"));
    assert_eq!(
        structured["output"]["path"], "/widgets/search",
        "the MCP call never reached the stored upstream path: {structured}"
    );
    // `structuredContent` is the invocation body (the same shape HTTP returns); the transport's
    // own fields are not part of the action's output.
    assert_eq!(
        structured["output"], body["output"],
        "HTTP and MCP disagree about the same imported action: {structured} vs {body}"
    );

    // P2.1: a restarted process re-resolves the row and invokes again with no boot refusal.
    running.shutdown().await;
    let running = support::Running::start(&pg).await;
    let (status, body) = running
        .invoke(
            "widgets.list",
            "dev:alice",
            json!({"query": "bolts"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "after restart: {body}");

    // P1.4: a revoked approval makes the action absent and the refusal typed.
    sqlx::query("UPDATE action_registry SET approved_at = NULL WHERE name = 'widgets.list'")
        .execute(&pg.owner)
        .await
        .expect("revoke the approval");
    let (status, body) = running
        .invoke(
            "widgets.list",
            "dev:alice",
            json!({"query": "bolts"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "a revoked action must be absent: {body}");
    assert_eq!(body["type"], "action-not-found", "{body}");
    let (status, manifest) = running
        .http(
            reqwest::Method::GET,
            "/api/actions",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{manifest}");
    assert!(
        !manifest["actions"]
            .as_array()
            .expect("a manifest array")
            .iter()
            .any(|entry| entry["name"] == "widgets.list"),
        "a revoked action must leave the manifest"
    );

    // The gate is not a rubber stamp: an approved row this build cannot execute is still a
    // refusal before bind (a workflow row with no compiled handler).
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO action_registry
             (name, spec_version, kind, service, spec, schema_hash, timeout_ms, idempotent, approved_at)
         VALUES ('widgets.bad', 1, 'workflow', 'widgets', '{}'::jsonb, 'h', 5000, false, now())",
    )
    .execute(&mut *tx)
    .await
    .expect("insert an unresolvable approved row");
    tx.commit().await.expect("commit");
    let refusal = conveyor_actions::resolve::assert_served_registry(
        &pg.owner,
        conveyor_actions::registry::registry(),
    )
    .await;
    assert!(
        matches!(
            refusal,
            Err(conveyor_actions::resolve::BootRefusal::Unresolvable { .. })
        ),
        "the boot gate must refuse a row it cannot execute: {refusal:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_breaking_refresh_is_held_and_the_approved_version_still_invokes() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let first = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V1, false)
        .await
        .expect("first ingest");
    assert_eq!(first.approved, vec![("widgets.list".to_owned(), 1)]);

    // P1.3: the removal of `query` is breaking. The refresh is written UNAPPROVED and the
    // v1 definition keeps serving.
    let second = ingest::ingest_production(&pg.owner, seeded.tenant, "widgets", WIDGETS_V2, false)
        .await
        .expect("refresh");
    assert!(
        second.has_breaking(),
        "the removal must be breaking: {second:?}"
    );
    assert!(second.approved.is_empty(), "v2 must not be approved");

    let stub = support::Stub::start().await;
    support::set_env(WIDGETS_URL_ENV, &stub.base);
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "widgets.list",
            "dev:alice",
            json!({"query": "bolts"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 200,
        "the approved v1 must keep serving while v2 is held: {body}"
    );
    assert_eq!(
        body["output"]["path"], "/widgets/search",
        "the approved v1 did not reach its upstream: {body}"
    );
}
