//! #282 — the destructive gate is the engine's, not a handler's memory.
//!
//! Four claims, and the fourth is the one that keeps the other three honest over time:
//!
//! * an approved approval for **another object** cannot authorise this call;
//! * a destructive call that names **no** approval is refused before any backend exists;
//! * every destructive action either declares its target or is named in [`UNGATED`] — so a new
//!   one cannot ship ungated, and the ones still to be adopted are a visible list rather than
//!   silence (#283 empties it);
//! * a target is a **pure function of the input**: two calls with the same input authorise the
//!   same object, so an approval cannot be satisfied by something that drifts.

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_store as store;

/// A request as an admin caller (the only role that holds `ops:write`).
fn request(action: &str, input: Value, approval_id: Option<Uuid>) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id,
        actor_id: Uuid::now_v7(),
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-destructive-gate".to_owned()),
        trace_id: Some("trace-destructive-gate".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// An APPROVED gate for `gate` whose `meta.target` is `target`.
async fn approved(pg: &support::Pg, tenant: TenantId, gate: &str, target: &str) -> Uuid {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the approval insert");
    let new = store::approval::NewApproval {
        run_id: None,
        card_id: None,
        project_id: None,
        eval_id: None,
        gate: gate.to_owned(),
        kind: "gate".to_owned(),
        required_decisions: 1,
        requested_by: None,
        expires_at: None,
        callback_url: None,
        correlation_id: None,
        meta: json!({"target": target}),
    };
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the gate");
    sqlx::query("UPDATE approval SET status = 'approved' WHERE id = $1")
        .bind(approval.id)
        .execute(&mut *tx)
        .await
        .expect("approve the gate");
    tx.commit().await.expect("commit the gate");
    approval.id
}

/// Every destructive action at `main` that still declares no approval target.
///
/// **This list is the burn-down, not a permit.** #282 gates the four actions that declare a
/// target; #283 declares one for each name below and deletes the entry. A destructive action
/// that is *not* here and declares no target fails [`every_destructive_action_is_gated_or_listed`],
/// which is what stops a new one being written without a decision about its gate.
/// **Empty, and it stays empty.** #282 started it with 31 names, #285 shrank it by the six the
/// console invokes, and #283 declared a target for every one of the rest. The test below still
/// iterates the registry: a destructive action that declares no target and is not on this list
/// fails, which is what stops a new one being written without a decision about its gate.
const UNGATED: &[&str] = &[];

/// Every action that declares a target today, with an input each target accepts.
///
/// The inputs name ids that need not exist: a target is computed from the input, and the gate
/// runs before the object is read — which is exactly the property a dry run relies on. `ops`
/// rows name a host that need not exist for the same reason.
///
/// #285 added the six the console invokes (`cards.archive`, `custom_fields.clear_value`, the
/// eval family, `trace_links.delete`); #283 adds the rest and empties [`UNGATED`].
fn declaring_targets() -> Vec<(&'static str, Value)> {
    let id = Uuid::nil().to_string();
    let other = Uuid::max().to_string();
    vec![
        ("cards.archive", json!({ "id": id })),
        (
            "custom_fields.clear_value",
            json!({ "card_id": id, "custom_field_id": other }),
        ),
        ("evals.retire", json!({ "id": id })),
        (
            "eval_cases.delete",
            json!({ "version_id": other, "id": id }),
        ),
        (
            "eval_metrics.delete",
            json!({ "version_id": other, "id": id }),
        ),
        ("trace_links.delete", json!({ "id": id })),
        // ── #283: the rest of the registry, each naming its own object ──────────────────────
        ("benefits.delete", json!({ "id": id })),
        ("benefits.unlink", json!({ "id": id })),
        ("cadences.delete", json!({ "id": id })),
        (
            "channels.unbind",
            json!({"tenant_id": id, "channel_kind": "telegram", "route_key": "chat:1"}),
        ),
        ("comments.delete", json!({ "id": id })),
        ("custom_fields.delete", json!({ "id": id })),
        ("documents.delete", json!({ "id": id })),
        ("labels.delete", json!({ "id": id })),
        ("model_elements.delete", json!({ "id": id })),
        ("model_relationships.delete", json!({ "id": id })),
        ("model_views.delete", json!({ "id": id })),
        ("models.delete", json!({ "id": id })),
        (
            "pm.close",
            json!({"job_binding_id": id, "project_id": other, "closure": "closing"}),
        ),
        ("project_dependencies.delete", json!({ "id": id })),
        ("projects.close", json!({ "id": id })),
        ("registers.delete", json!({ "id": id })),
        ("repositories.delete", json!({ "id": id })),
        (
            "repositories.unlink",
            json!({"project_id": id, "repository_id": other}),
        ),
        ("secrets.delete", json!({ "id": id })),
        (
            "secrets.rotate",
            json!({ "id": id, "value": "not-a-real-secret" }),
        ),
        ("stakeholders.delete", json!({ "id": id })),
        ("stakeholders.profile_delete", json!({ "id": id })),
        ("task_lists.delete", json!({ "id": id })),
        ("tasks.delete", json!({ "id": id })),
        ("tranches.delete", json!({ "id": id })),
        ("view_connections.delete", json!({ "id": id })),
        ("view_objects.delete", json!({ "id": id })),
        ("wbs.delete", json!({ "id": id })),
        ("webhooks.delete", json!({ "id": id })),
        (
            "ops.restart",
            json!({"namespace": "conveyor", "deployment": "conveyor"}),
        ),
        (
            "ops.scale",
            json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3}),
        ),
        (
            "ops.rollback",
            json!({"namespace": "conveyor", "deployment": "conveyor"}),
        ),
        (
            "ops.host_restart_unit",
            json!({"host": "mini2", "unit": "com.nonagentic.conveyor-probe"}),
        ),
        // T7 (#78): the incident-scoped mitigation and the fourth write verb.
        (
            "ops.mitigate",
            json!({
                "incident_id": id,
                "action": "ops.scale",
                "input": {"namespace": "conveyor", "deployment": "conveyor", "replicas": 3},
            }),
        ),
        (
            "ops.flag_flip",
            json!({"namespace": "conveyor", "deployment": "conveyor",
                   "flag": "FEATURE_X", "value": "on"}),
        ),
    ]
}

/// P1.3 — the set of destructive actions with no declared target is exactly [`UNGATED`].
///
/// Both directions matter: a name in the list that has since declared a target is a stale entry
/// (the burn-down did not move), and a destructive action absent from the list with no target is
/// a new ungated destructive action (the defect this test exists to prevent).
#[test]
fn every_destructive_action_is_gated_or_listed() {
    let mut ungated: Vec<&str> = Vec::new();
    let mut gated: Vec<&str> = Vec::new();
    for spec in registry().iter() {
        if spec.effect != conveyor_actions::Effect::Destructive {
            continue;
        }
        // The gate runs in `invoke_internal`'s transaction. An external handler is dispatched by
        // `invoke_external`, which opens no transaction of its own — so an external destructive
        // action would escape the gate entirely. Every destructive action is internal today;
        // this keeps a future one from being the first silently ungated action in the registry.
        assert_eq!(
            spec.kind,
            conveyor_actions::ActionKind::Internal,
            "{} is destructive and not internal: the gate cannot cover it",
            spec.name
        );
        if spec.approval_target.is_some() {
            gated.push(spec.name.as_ref());
        } else {
            ungated.push(spec.name.as_ref());
        }
    }
    ungated.sort_unstable();
    let mut expected = UNGATED.to_vec();
    expected.sort_unstable();
    assert_eq!(
        ungated, expected,
        "a destructive action declares no approval target and is not on the burn-down list, \
         or the list names one that has since declared its target"
    );
    // Exactly the actions the engine can gate today are declared, and nothing else is: a stray
    // declaration in the burn-down would silently change behaviour before #283 lands its tests.
    gated.sort_unstable();
    let mut declared: Vec<&str> = declaring_targets()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    declared.sort_unstable();
    assert_eq!(gated, declared, "the declared-target set moved");
}

/// P1.1 — a destructive call that names no approval is refused before any backend is touched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_destructive_action_refuses_without_an_approval() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // An address that answers nothing: a gate that let the call through would surface as a
    // transport failure instead of a 403, so this also proves no client was built.
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    for (action, input) in declaring_targets() {
        let error = invoke(
            &pg.runtime,
            registry(),
            seeded.tenant,
            request(action, input, None),
        )
        .await
        .expect_err("a destructive action with no approval must be refused");
        assert_eq!(error.status, 403, "{action}: {}", error.detail);
        assert_eq!(error.code, "approval_required", "{action}");
    }
}

/// P1.2 — an approved approval for another object authorises nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_for_another_target_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    // Approved, and approved for a DIFFERENT deployment: the gate must compare targets, not
    // merely find a row that says `approved`.
    let approval_id = approved(
        &pg,
        seeded.tenant,
        "ops.scale",
        "conveyor/deployment/somewhere-else/scale/3",
    )
    .await;
    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "ops.scale",
            json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3}),
            Some(approval_id),
        ),
    )
    .await
    .expect_err("an approval for another deployment must not authorise this one");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "approval_wrong_target");
}

/// P2.1 — the drift guard #283 exists for: NO destructive action is ungated, and the burn-down
/// list that tracked them is empty rather than merely short.
#[test]
fn every_destructive_action_declares_a_target() {
    let mut undecided: Vec<&str> = Vec::new();
    for spec in registry().iter() {
        if spec.effect != conveyor_actions::Effect::Destructive {
            continue;
        }
        if spec.approval_target.is_none() {
            undecided.push(spec.name.as_ref());
        }
    }
    assert!(
        undecided.is_empty(),
        "these destructive actions declare no approval target: {undecided:?}"
    );
    assert!(
        UNGATED.is_empty(),
        "the burn-down list must be empty once every destructive action is gated"
    );
}

/// P1.1 — every destructive action produces a non-empty target for its documented sample input.
#[test]
fn every_destructive_action_produces_a_target() {
    for (action, input) in declaring_targets() {
        let spec = registry()
            .get(action)
            .expect("the sample action is registered");
        let compute = spec
            .approval_target
            .unwrap_or_else(|| panic!("{action} declares no approval target"));
        let target = compute(&input).unwrap_or_else(|detail| {
            panic!("{action}: the sample input produced no target: {detail}")
        });
        assert!(
            !target.trim().is_empty(),
            "{action} produced an empty target"
        );
    }
}

/// P1.2 — the target binds the OBJECT, not the action: an approved gate whose target is another
/// object's is refused for every destructive action in the registry, not only the four ops verbs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_target_binds_the_object_not_the_action() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    for (action, input) in declaring_targets() {
        let spec = registry()
            .get(action)
            .expect("the sample action is registered");
        let compute = spec.approval_target.expect("a declared target");
        let target = compute(&input).expect("the sample input yields a target");
        // Approved, opened for the right action, naming a DIFFERENT object.
        let approval_id =
            approved(&pg, seeded.tenant, action, &format!("{target}-elsewhere")).await;
        let error = invoke(
            &pg.runtime,
            registry(),
            seeded.tenant,
            request(action, input, Some(approval_id)),
        )
        .await
        .expect_err("an approval for another object must be refused");
        assert_eq!(error.status, 403, "{action}: {}", error.detail);
        assert_eq!(error.code, "approval_wrong_target", "{action}");
    }
}

/// P1.4 — the target is a pure function of the input, so it can never drift between the approval
/// and the call it authorises.
#[test]
fn the_target_is_deterministic() {
    for (action, input) in declaring_targets() {
        let spec = registry().get(action).expect("the action is registered");
        let compute = spec.approval_target.expect("a declared target");
        let first = compute(&input).expect("the sample input yields a target");
        let second = compute(&input).expect("the same input yields a target");
        assert_eq!(first, second, "{action} produced two targets for one input");
        assert!(!first.is_empty(), "{action} produced an empty target");
    }
}
