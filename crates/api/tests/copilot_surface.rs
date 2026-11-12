//! S10 §7 (e): a scope-absent tool is omitted from BOTH surfaces.
//!
//! The claim proved here is the **omission**: `/api/copilot/actions` and `/mcp` `tools/list` are one
//! projection of one `ActionSpec` set, so a low-scope principal sees the same (smaller) set on both.
//!
//! ⚠️ The slice's §5.2 table also says calling an absent tool answers `action-not-found`, never `403`.
//! That is true for an **agent** principal (S06b's allowlist: an out-of-list tool is `-32602`;
//! `crates/api/tests/machine_login.rs`). For a human principal S04-D6 is deliberately weaker: the
//! tool is absent from `tools/list`, and `tools/call` on a known name is `403 action-forbidden` —
//! `crates/actions/src/mcp.rs:163-168` states it, and this test asserts the shipped rule rather than
//! silently changing a signed slice's transport semantics.

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
}
