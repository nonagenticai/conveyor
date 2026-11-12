//! S06b property (a): every MCP tool names its operation.
//!
//! The `agent` imports an external MCP system's tool as `object.verb` from `_meta.operation`
//! when that key holds a valid name, and otherwise as `tool.<snake>` of the tool name
//! (`agent/crates/agent-api/src/connectors/routes/mcp_import.rs:199-211`). A tool without the
//! key therefore imports as `tool.projects_create`, and no operation in the agent's job design
//! ever matches this registry — so the key is the whole difference between a consumable
//! surface and a decorative one.
//!
//! This is asserted against **every** registered action, not a sample: an action added
//! without the key must fail here, the same way an action added without a parity sample fails
//! in `parity_http_mcp.rs`.

use conveyor_actions::mcp::tool_json;
use conveyor_actions::registry::assert_registry_consistent;

/// A name the agent's importer accepts as `object.verb`.
fn is_object_verb(raw: &str) -> bool {
    let mut parts = raw.split('.');
    let (Some(object), Some(verb), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    [object, verb].iter().all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    })
}

#[test]
fn every_tool_carries_its_operation() {
    let registry = assert_registry_consistent().expect("the compiled registry is consistent");
    let mut wrong = Vec::new();

    for spec in registry.iter() {
        let tool = tool_json(spec);
        let operation = tool["_meta"]["operation"].as_str().unwrap_or_default();
        let tool_name = tool["name"].as_str().unwrap_or_default();

        if operation != spec.name {
            wrong.push(format!(
                "{}: _meta.operation is {operation:?}, expected {:?}",
                spec.name, spec.name
            ));
            continue;
        }
        if !is_object_verb(operation) {
            wrong.push(format!(
                "{}: {operation:?} is not an object.verb the importer accepts",
                spec.name
            ));
        }
        // The importer's fallback is `tool.<snake>`; a name equal to the tool name would mean
        // the key was never read even though it is present.
        if operation == tool_name {
            wrong.push(format!(
                "{}: operation equals the tool name {tool_name:?}",
                spec.name
            ));
        }
        if tool["_meta"]["x-action-name"].as_str() != Some(spec.name) {
            wrong.push(format!("{}: x-action-name drifted", spec.name));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} of {} tools do not name their operation:\n{}",
        wrong.len(),
        registry.len(),
        wrong.join("\n")
    );
    println!(
        "mcp meta: {} tools carry a valid _meta.operation",
        registry.len()
    );
}
