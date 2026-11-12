//! Property (c): duplicates, MCP collisions, bad names and handler-less internal actions
//! fail closed.
//!
//! `CONVEYOR_TEST_DUPLICATE` is read once, when the registry is first built — so this file
//! is its own test binary, and the environment is set before anything touches the registry.
//! The gate proves the same thing at the process level:
//! `CONVEYOR_TEST_DUPLICATE=members.set_role cargo run -p conveyor-server -- serve` exits 1
//! before `bind`.

use conveyor_actions::registry::{Registry, validate};
use conveyor_actions::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Scope, Tenancy,
};

fn schema() -> schemars::Schema {
    schemars::schema_for!(conveyor_actions::views::EmptyIn)
}

fn spec(name: &'static str) -> ActionSpec {
    ActionSpec {
        name,
        version: 1,
        kind: ActionKind::External,
        input: schema,
        output: schema,
        scopes: &[Scope::ActionsRead],
        idempotent: false,
        timeout_ms: 30_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::Workflow,
    }
}

#[test]
fn an_injected_duplicate_makes_the_boot_gate_fail() {
    // SAFETY: this test binary is single-threaded at this point and nothing else reads the
    // environment; the value is the mechanism the gate uses.
    unsafe { std::env::set_var("CONVEYOR_TEST_DUPLICATE", "members.set_role") };
    let error = conveyor_actions::registry::assert_registry_consistent()
        .expect_err("a duplicate name must fail the boot gate");
    assert!(
        matches!(
            &error,
            conveyor_actions::RegistryError::DuplicateName(name) if name == "members.set_role"
        ),
        "{error}"
    );
}

#[test]
fn a_mcp_name_collision_is_refused() {
    // Both names are valid; their MCP forms collide (`a.b_c` and `a_b.c` both become
    // `a_b_c`), which is exactly the non-injectivity the boot gate exists to catch.
    let registry = Registry::new(vec![spec("cards.move_now"), spec("cards_move.now")]);
    let error = validate(&registry).expect_err("two actions cannot share an MCP name");
    assert!(
        matches!(
            &error,
            conveyor_actions::RegistryError::McpCollision { mcp, .. } if mcp == "cards_move_now"
        ),
        "{error}"
    );
}

#[test]
fn a_name_outside_the_action_grammar_is_refused() {
    // The MCP charset is narrower than nothing else here: every legal action name maps to
    // a legal tool name, so the first gate that can fail is the name grammar itself.
    let registry = Registry::new(vec![spec("Cards.move")]);
    let error = validate(&registry).expect_err("an invalid action name must fail");
    assert!(
        matches!(&error, conveyor_actions::RegistryError::InvalidName { .. }),
        "{error}"
    );
}

#[test]
fn an_internal_action_without_a_handler_is_refused() {
    let mut broken = spec("me.get");
    broken.kind = ActionKind::Internal;
    let registry = Registry::new(vec![broken]);
    let error = validate(&registry).expect_err("an internal action must have a handler");
    assert!(
        matches!(&error, conveyor_actions::RegistryError::Handlerless(name) if name == "me.get"),
        "{error}"
    );
}
