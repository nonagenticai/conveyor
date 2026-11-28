//! P3.2 — a binding's effect ceiling is never raised.
//!
//! A job is bound to a release with a declared ceiling (`read < write < destructive`). The ceiling
//! is a property of the **binding**, never of the call: a caller cannot raise it by putting a field
//! in the request document, and a typo in the binding narrows to a refusal rather than widening to
//! unlimited.
//!
//! Every call here runs against a pool that points at a closed port. That is the instrument: if
//! the ceiling refuses before the backend, the answer is the typed `effect_ceiling` error; if the
//! check were late (or absent), the answer would be a database error instead. The one case that is
//! *allowed* through demonstrates exactly that contrast.

use conveyor_actions::invoke::{
    ActionError, Authority, InvokeRequest, binding_ceiling, invoke_bound,
};
use conveyor_actions::{Registry, registry};
use conveyor_auth::{Role, ScopePolicy};
use conveyor_domain::TenantId;
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

/// A write action from the built-in registry.
const WRITE_ACTION: &str = "roles.policy_set";

/// A pool that can never connect: every query fails at the socket.
fn unreachable_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://nobody:nobody@127.0.0.1:1/nothing")
        .expect("an unreachable pool is still a pool")
}

fn request(action: &str) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_string(),
        input: json!({"entries": []}),
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: Uuid::now_v7(),
        actor_kind: "user".to_string(),
        role: Role::Admin,
        policy: ScopePolicy::default(),
        authority: Authority::Internal,
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: "conveyor".to_string(),
        usage: None,
    }
}

fn tenant() -> TenantId {
    TenantId::from_uuid(Uuid::now_v7()).expect("a fresh uuid is a tenant")
}

fn registry_ref() -> &'static Registry {
    registry()
}

#[tokio::test]
async fn effect_ceiling_is_never_raised() {
    let meta = json!({"limits": {"effect_ceiling": "read"}});
    let error = invoke_bound(
        &unreachable_pool(),
        registry_ref(),
        tenant(),
        &meta,
        request(WRITE_ACTION),
    )
    .await
    .expect_err("a read ceiling must refuse a write action");
    assert_eq!(error.status, 403, "{error:?}");
    assert_eq!(error.code, "effect_ceiling", "{error:?}");
    assert!(
        error.detail.contains("write") && error.detail.contains("read"),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_same_call_with_a_write_ceiling_reaches_the_backend() {
    // Identical request, ceiling raised to the action's own effect: the gate lets it through and
    // the unreachable pool is what answers. That difference is the proof the check is a ceiling
    // and not a blanket refusal.
    let meta = json!({"limits": {"effect_ceiling": "write"}});
    let error = invoke_bound(
        &unreachable_pool(),
        registry_ref(),
        tenant(),
        &meta,
        request(WRITE_ACTION),
    )
    .await
    .expect_err("the unreachable pool must fail the call");
    assert_ne!(
        error.code, "effect_ceiling",
        "a write ceiling must not refuse a write action: {error:?}"
    );
}

#[tokio::test]
async fn a_caller_cannot_declare_its_own_ceiling() {
    // The same document field the binding uses, placed in the *input*, changes nothing: the
    // ceiling is read from the binding's meta and nowhere else.
    let meta = json!({"limits": {"effect_ceiling": "read"}});
    let mut forged = request(WRITE_ACTION);
    forged.input = json!({"entries": [], "effect_ceiling": "destructive", "limits": {"effect_ceiling": "destructive"}});
    let error = invoke_bound(&unreachable_pool(), registry_ref(), tenant(), &meta, forged)
        .await
        .expect_err("an input field must not raise the ceiling");
    assert_eq!(error.code, "effect_ceiling", "{error:?}");
}

#[test]
fn an_unknown_ceiling_is_a_refusal_never_unlimited() {
    let error = binding_ceiling(&json!({"limits": {"effect_ceiling": "root"}}))
        .expect_err("an unknown ceiling must be refused");
    assert_eq!(error.code, "bad_binding", "{error:?}");
}

#[test]
fn a_binding_without_a_ceiling_is_governed_by_the_actions_own_effect() {
    assert_eq!(
        binding_ceiling(&json!({"limits": {}})).expect("absent is not an error"),
        None
    );
    assert_eq!(
        binding_ceiling(&json!({})).expect("absent is not an error"),
        None
    );
}

/// The scale itself, so a future effect cannot be added without answering where it sits.
#[test]
fn the_effect_scale_is_read_write_destructive() {
    use conveyor_actions::spec::Effect;
    assert!(Effect::Read.rank() < Effect::Write.rank());
    assert!(Effect::Write.rank() < Effect::Destructive.rank());
    assert_eq!(Effect::parse("read"), Some(Effect::Read));
    assert_eq!(Effect::parse("nonsense"), None);
}

/// Silence the unused-import warning in builds where the error type is not named.
#[allow(dead_code)]
fn _error_type_is_public(_: ActionError) {}
