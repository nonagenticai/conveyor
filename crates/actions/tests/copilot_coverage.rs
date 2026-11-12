//! S10 §7 (g): every mutating operation resolves to an action or the declared exemption.
//!
//! Two halves, both checked against the registry rather than against a hand-kept list:
//!
//! 1. **No mutating action is unreachable.** Every `internal`/`external` action whose effect is
//!    `write` or `destructive` is in the owner's copilot surface — the surface is a projection of the
//!    registry, so an action that exists but cannot be reached is either a bug in the projection or a
//!    capability the copilot silently lost.
//! 2. **An unmapped operation fails by name.** [`unmapped`] is exercised with an injected name, so
//!    the gate is a gate: a future screen element that forgets its action fails here loudly.
//!
//! ⚠️ The slice's §5.2 points at fr's `coverage-check.ts`, which runs over fr's 34 AG-UI action
//! modules. S12 owns the conveyor's screen list; until it exists, the operation list this test can
//! check is the registry itself, and the chat transport is the one declared exemption (S10-D6).

use std::collections::BTreeSet;

use conveyor_actions::agent_surface::{TRANSPORT_EXEMPTIONS, surface, unmapped};
use conveyor_actions::registry::registry;
use conveyor_actions::{ActionKind, Effect};
use conveyor_auth::{PrincipalKind, Role};

#[test]
fn every_mutating_action_is_in_the_owners_copilot_surface() {
    let owner = surface(registry(), PrincipalKind::Human, Role::Admin, None);
    let names: BTreeSet<&str> = owner.iter().map(|spec| spec.name).collect();
    let mut missing = Vec::new();
    for spec in registry().iter() {
        let exposed_kind = matches!(spec.kind, ActionKind::Internal | ActionKind::External);
        if exposed_kind && spec.effect != Effect::Read && !names.contains(spec.name) {
            missing.push(spec.name);
        }
    }
    assert!(
        missing.is_empty(),
        "mutating actions the copilot cannot reach: {missing:?}"
    );
    // And the surface is not accidentally empty or tiny: the owner must see the bulk of the registry.
    assert!(
        names.len() > registry().len() / 2,
        "the owner's surface is {} of {} actions",
        names.len(),
        registry().len()
    );
}

#[test]
fn an_unmapped_operation_fails_by_name_and_the_transport_exemption_is_declared() {
    let owner = surface(registry(), PrincipalKind::Human, Role::Admin, None);

    let mapped = ["cards.move", "projects.close", "approvals.decide"];
    assert!(
        unmapped(&mapped, &owner).is_empty(),
        "a mapped operation must pass"
    );

    let exempt = [TRANSPORT_EXEMPTIONS[0]];
    assert!(
        unmapped(&exempt, &owner).is_empty(),
        "the chat transport is exempt by declaration, not by absence"
    );

    let injected = ["cards.teleport", "cards.move"];
    assert_eq!(
        unmapped(&injected, &owner),
        vec!["cards.teleport".to_owned()],
        "an unmapped operation must be reported by name, and only it"
    );
}

/// S10 §5.3 — the redaction rule, made enforceable **in the positive direction**.
///
/// `proposal.rs` seals the exact input and stores a redacted projection in which every `x-secret`
/// property is `<redacted>`; `secrets.rotate` is proposal-triggering today, so this rule is
/// exercised rather than deferred. For every proposal-triggering action carrying a secret, a canary
/// planted at each marked path must be absent from the projection — and the checked set must be
/// non-empty, so the gate cannot pass vacuously.
///
/// The failure this prevents is the one the old blanket ban could only postpone: a secret-bearing
/// action becoming proposal-triggering while its value is stored in member-readable `approval.meta`.
#[test]
fn a_proposal_projection_redacts_every_secret_property() {
    const CANARY: &str = "canary-SECRET-value";
    let mut checked = Vec::new();
    let mut offenders = Vec::new();
    for spec in registry().iter() {
        if !conveyor_api::copilot::proposal::required(spec) {
            continue;
        }
        let schema = spec.input_schema();
        let mut paths = Vec::new();
        secret_properties(&schema, "", &mut paths);
        if paths.is_empty() {
            continue;
        }
        checked.push(spec.name);
        let mut input = serde_json::json!({});
        for path in &paths {
            plant(&mut input, path, CANARY);
        }
        let visible = conveyor_api::copilot::proposal::redact_input(&schema, &input);
        if visible.to_string().contains(CANARY) {
            offenders.push(format!("{} ({})", spec.name, paths.join(", ")));
        }
    }
    assert!(
        !checked.is_empty(),
        "no proposal-triggering action carries a secret: this gate would be vacuous"
    );
    assert!(
        offenders.is_empty(),
        "a proposal projection leaked a secret for: {offenders:?}"
    );
}

/// Put `canary` at `path` in a fresh input object, following the walker's `a.b` / `a[]` spelling.
fn plant(value: &mut serde_json::Value, path: &str, canary: &str) {
    let (head, rest) = match path.split_once('.') {
        Some((head, rest)) => (head, Some(rest)),
        None => (path, None),
    };
    let (name, is_array) = match head.strip_suffix("[]") {
        Some(name) => (name, true),
        None => (head, false),
    };
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    let slot = value
        .as_object_mut()
        .expect("an object")
        .entry(name.to_owned())
        .or_insert_with(|| {
            if is_array {
                serde_json::json!([{}])
            } else {
                serde_json::json!({})
            }
        });
    if is_array {
        if !slot.is_array() {
            *slot = serde_json::json!([{}]);
        }
        let array = slot.as_array_mut().expect("an array");
        if array.is_empty() {
            array.push(serde_json::json!({}));
        }
        let element = array.last_mut().expect("an element");
        match rest {
            Some(rest) => plant(element, rest, canary),
            None => *element = serde_json::Value::String(canary.to_owned()),
        }
    } else {
        match rest {
            Some(rest) => plant(slot, rest, canary),
            None => *slot = serde_json::Value::String(canary.to_owned()),
        }
    }
}

/// Every `x-secret` property path in a JSON Schema — through `properties`, `items` and `$defs`.
fn secret_properties(schema: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if object.get("x-secret") == Some(&serde_json::Value::Bool(true)) && !path.is_empty() {
        out.push(path.to_owned());
    }
    if let Some(properties) = object
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, sub) in properties {
            let child = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            secret_properties(sub, &child, out);
        }
    }
    if let Some(items) = object.get("items") {
        secret_properties(items, &format!("{path}[]"), out);
    }
    for key in ["$defs", "definitions"] {
        if let Some(definitions) = object.get(key).and_then(serde_json::Value::as_object) {
            for (name, sub) in definitions {
                secret_properties(sub, &format!("{path}#{name}"), out);
            }
        }
    }
}
