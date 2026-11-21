//! #170 P2 — the six `ops.*` reads: a caller without the scope is refused, an unconfigured
//! client refuses visibly, and every read is on the copilot's surface as `read`.

mod support;

use std::collections::BTreeMap;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as a human caller with the given role and stored scope policy.
fn request(tenant_role: Role, policy: conveyor_auth::ScopePolicy, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: "ops.pods".to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: Uuid::now_v7(),
        actor_kind: "user".to_owned(),
        role: tenant_role,
        policy,
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-ops".to_owned()),
        trace_id: Some("trace-ops".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// P2.1 — a role the scope has been withdrawn from is refused before the handler runs.
///
/// `integrations:read` is granted to every built-in role, so "a role without it" is expressed the
/// way a tenant expresses it: a stored policy override. A `403` here is proof the refusal came
/// before the handler — an unconfigured client would have answered `503` instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_caller_without_the_scope_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let mut overrides = BTreeMap::new();
    overrides.insert("integrations:read".to_owned(), false);

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            Role::Viewer,
            conveyor_auth::ScopePolicy::new(overrides),
            json!({"namespace": "conveyor"}),
        ),
    )
    .await
    .expect_err("the scope was withdrawn");

    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "action-forbidden");
}

/// P2.2 — no credential means a typed `503`, never a guessed default or an empty list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unconfigured_client_refuses() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // An address that answers nothing, and an empty token: the client refuses to be built. The
    // empty token also makes the mounted service account the fallback, which is absent here — on
    // any machine that has one, this test fails loudly rather than passing for the wrong reason.
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            Role::Admin,
            Default::default(),
            json!({"namespace": "conveyor"}),
        ),
    )
    .await
    .expect_err("there is no credential");

    assert_eq!(error.status, 503, "{}", error.detail);
    assert_eq!(error.code, "action-service-unconfigured");
}

/// P2.3 — every read is on the copilot's surface, with its effect class.
#[test]
fn the_ops_reads_are_on_the_agent_surface() {
    const READS: [&str; 6] = [
        "ops.pods",
        "ops.deployments",
        "ops.events",
        "ops.logs",
        "ops.flux_status",
        "ops.ci_status",
    ];

    let surface = conveyor_actions::agent_surface::entries(
        registry(),
        conveyor_auth::PrincipalKind::Human,
        Role::Member,
        None,
    );

    for name in READS {
        let entry = surface
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("{name} is missing from the surface"));
        assert_eq!(entry.effect, "read", "{name}");
        assert_eq!(entry.kind, "internal", "{name}");
        assert!(entry.idempotent, "{name} is a read");
        assert_eq!(entry.scopes, vec!["integrations:read".to_owned()], "{name}");
    }
}

/// The callers a read serves are the ones who can see it: a machine caller without a channel has
/// no surface at all, which is the engine's existing rule and must not be special-cased here.
#[test]
fn a_machine_caller_without_a_channel_sees_no_ops_reads() {
    let surface = conveyor_actions::agent_surface::surface(
        registry(),
        conveyor_auth::PrincipalKind::Agent,
        Role::Admin,
        None,
    );
    assert!(
        surface.iter().all(|spec| !spec.name.starts_with("ops.")),
        "an unchannelled machine caller must not reach ops.*"
    );
}

/// P3.1 — the live read, run by hand against the cluster (never in CI).
///
/// Set `OPS_API_URL`, `OPS_TOKEN` (a token minted for `conveyor-worker`) and `OPS_CA_PATH`, then:
///
/// ```text
/// cargo test -p conveyor-actions --test ops_read -- --ignored --nocapture
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a live cluster: OPS_API_URL, OPS_TOKEN and OPS_CA_PATH"]
async fn the_live_read_returns_the_pod_list() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            Role::Admin,
            Default::default(),
            json!({"namespace": "conveyor", "limit": 20}),
        ),
    )
    .await
    .expect("the cluster answers");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    let output = &outcome.body["output"];
    assert_eq!(output["namespace"], json!("conveyor"));
    assert_eq!(output["resource"], json!("pods"));
    let items = output["items"].as_array().expect("items[]");
    assert!(!items.is_empty(), "the conveyor namespace has pods");
    for item in items {
        assert!(
            item["metadata"]["namespace"] == json!("conveyor"),
            "the API's objects pass through unmodified"
        );
    }
    println!("{}", serde_json::to_string_pretty(output).expect("json"));
}
