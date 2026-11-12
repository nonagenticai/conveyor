#![forbid(unsafe_code)]
//! The four `kind: external` fixture actions (S04 §5.1).
//!
//! The external contract lives in `tests/fixtures/external/openapi.json`; these values are
//! its compiled shape. `external_actions.rs` ingests the document and asserts that the two
//! agree (name, method, path, input/output members), so the fixture spec is not a second
//! source that could drift — a change to one without the other is a red test.
//!
//! No URL is compiled in: the stub's base is `CONVEYOR_EXTERNAL_FIXTURES_URL`, read at
//! invocation. An unconfigured service is a `503`, not a boot failure (the serving gate runs
//! without the stub).

use crate::registry::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, Confirm, Effect, HandlerRef, Scope, Tenancy,
};
use crate::views::{EchoIn, EchoOut, FailIn, FailOut, SlowIn, SlowOut, TimeoutOut};

/// The service key every fixture action names.
pub const SERVICE: &str = "fixtures";

fn echo_input() -> schemars::Schema {
    schemars::schema_for!(EchoIn)
}
fn echo_output() -> schemars::Schema {
    schemars::schema_for!(EchoOut)
}
fn echo_spec() -> ActionSpec {
    ActionSpec {
        name: "fixtures.echo",
        version: 1,
        kind: ActionKind::External,
        input: echo_input,
        output: echo_output,
        scopes: &[Scope::ActionsRead],
        idempotent: true,
        timeout_ms: 30_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/echo",
            auth: None,
            auth_optional: false,
        },
    }
}
inventory::submit! { ActionRegistration::new(echo_spec) }

fn slow_input() -> schemars::Schema {
    schemars::schema_for!(SlowIn)
}
fn slow_output() -> schemars::Schema {
    schemars::schema_for!(SlowOut)
}
fn slow_spec() -> ActionSpec {
    ActionSpec {
        name: "fixtures.slow",
        version: 1,
        kind: ActionKind::External,
        input: slow_input,
        output: slow_output,
        scopes: &[Scope::ActionsRead],
        idempotent: true,
        // Pinned at ingest: the stub sleeps 600 ms, so the caller's 200 ms deadline fires.
        timeout_ms: 200,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/slow",
            auth: None,
            auth_optional: false,
        },
    }
}
inventory::submit! { ActionRegistration::new(slow_spec) }

fn fail_input() -> schemars::Schema {
    schemars::schema_for!(FailIn)
}
fn fail_output() -> schemars::Schema {
    schemars::schema_for!(FailOut)
}
fn fail_spec() -> ActionSpec {
    ActionSpec {
        name: "fixtures.fail",
        version: 1,
        kind: ActionKind::External,
        input: fail_input,
        output: fail_output,
        scopes: &[Scope::ActionsRead],
        idempotent: false,
        timeout_ms: 30_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/fail",
            auth: None,
            auth_optional: false,
        },
    }
}
inventory::submit! { ActionRegistration::new(fail_spec) }

fn timeout_input() -> schemars::Schema {
    schemars::schema_for!(crate::views::EmptyIn)
}
fn timeout_output() -> schemars::Schema {
    schemars::schema_for!(TimeoutOut)
}
fn timeout_spec() -> ActionSpec {
    ActionSpec {
        name: "fixtures.timeout",
        version: 1,
        kind: ActionKind::External,
        input: timeout_input,
        output: timeout_output,
        scopes: &[Scope::ActionsRead],
        idempotent: false,
        timeout_ms: 30_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/timeout",
            auth: None,
            auth_optional: false,
        },
    }
}
inventory::submit! { ActionRegistration::new(timeout_spec) }

/// The fixture operations, for the ingest comparison test.
pub const NAMES: [&str; 4] = [
    "fixtures.echo",
    "fixtures.slow",
    "fixtures.fail",
    "fixtures.timeout",
];
