//! S10 §7 (e): a scope-absent tool is omitted from BOTH surfaces and a call to it is refused.
//!
//! The first claim proved here is the **omission**: `/api/copilot/actions` and `/mcp` `tools/list`
//! are one projection of one `ActionSpec` set, so a low-scope principal sees the same (smaller) set
//! on both.
//!
//! The second is the shipped S04-D6 rule, which S10 §5.2 and §7(e) were corrected to state
//! (2026-11-21, v1-s10-1): for a **human** principal the tool is absent from `tools/list`, and a
//! call to its known name is `403 action-forbidden` — the same refusal HTTP returns. The omission
//! is the guarantee, not indistinguishability at `tools/call`. (An **agent** principal is
//! different: S06b's allowlist answers an out-of-list name `-32602`,
//! `crates/api/tests/machine_login.rs`.) A name outside the registry is `-32602` here too.

mod copilot;

use std::collections::BTreeSet;

use serde_json::{Value, json};

fn names(body: &Value) -> BTreeSet<String> {
    body["actions"]
        .as_array()
        .expect("an actions array")
        .iter()
        .map(|action| action["name"].as_str().expect("a name").to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_low_scope_principal_omits_the_same_tools_on_both_surfaces() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::member(&pg, seeded.tenant_id, "dave", "viewer").await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    let (status, owner) = running
        .http(
            reqwest::Method::GET,
            "/api/copilot/actions",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{owner}");
    let (status, viewer) = running
        .http(
            reqwest::Method::GET,
            "/api/copilot/actions",
            Some("dev:dave"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{viewer}");

    let owner_names = names(&owner);
    let viewer_names = names(&viewer);
    assert!(
        viewer_names.len() < owner_names.len(),
        "a viewer must see fewer actions than an owner: {} vs {}",
        viewer_names.len(),
        owner_names.len()
    );
    assert!(owner_names.contains("projects.create"));
    assert!(
        !viewer_names.contains("projects.create"),
        "projects.create needs members:write"
    );
    // The hints ride on every entry, defaulted where nothing was declared.
    let move_action = owner["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .find(|action| action["name"] == "cards.move")
        .expect("cards.move is in the surface");
    assert_eq!(move_action["agent"]["proposal_required"], true);
    assert_eq!(move_action["agent"]["tier"], "core");

    // `/mcp` answers the same set for the same principal — one projection, two renderings.
    let mut mcp = copilot::support::Mcp::connect(&running, "dev:dave").await;
    let tools = mcp.list_tools().await;
    let mcp_names: BTreeSet<String> = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| {
            tool["_meta"]["operation"]
                .as_str()
                .expect("_meta.operation")
                .to_owned()
        })
        .collect();
    assert_eq!(
        mcp_names, viewer_names,
        "the copilot surface and /mcp tools/list must be the same set"
    );

    // A name outside the registry is a JSON-RPC -32602 on both counts of "absent".
    let refused = mcp
        .raw(
            "tools/call",
            json!({"name": "cards_teleport", "arguments": {}}),
        )
        .await;
    assert!(
        refused.get("error").is_some(),
        "an unknown tool must be a JSON-RPC error: {refused}"
    );

    // A KNOWN name outside this principal's set is the shipped S04-D6 refusal, not -32602:
    // the HTTP invocation route answers `403 action-forbidden` ...
    let (status, forbidden) = running
        .invoke(
            "projects.create",
            "dev:dave",
            json!({"title": "a viewer cannot create this"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{forbidden}");
    assert_eq!(forbidden["type"], "action-forbidden", "{forbidden}");

    // ... and `/mcp` returns the same problem as an `isError` tool result, not a protocol
    // error: the name resolves, and `invoke` refuses it exactly as it refuses the HTTP call.
    let forbidden = mcp
        .call(
            "projects_create",
            json!({"title": "a viewer cannot create this"}),
            None,
        )
        .await;
    assert!(
        forbidden.get("error").is_none(),
        "a known-but-forbidden tool is a tool result, not -32602: {forbidden}"
    );
    assert_eq!(forbidden["isError"], true, "{forbidden}");
    assert_eq!(
        forbidden["structuredContent"]["type"], "action-forbidden",
        "{forbidden}"
    );
    assert_eq!(forbidden["structuredContent"]["status"], 403, "{forbidden}");
}
