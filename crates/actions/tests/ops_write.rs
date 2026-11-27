//! #171 — the gated `ops.*` writes: no approval means no call, a dry run writes nothing, an
//! approval for another target is refused, and a real write reads its result back.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::routing::get;
use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (the only role that holds `ops:write`).
fn request(action: &str, input: Value) -> InvokeRequest {
    request_gated(action, input, None)
}

/// As [`request`], naming the approved approval the call carries (#282: the id is the CALL's).
fn request_gated(action: &str, input: Value, approval_id: Option<Uuid>) -> InvokeRequest {
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
        request_id: Some("rid-ops-write".to_owned()),
        trace_id: Some("trace-ops-write".to_owned()),
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
        required_decisions: 2,
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
    for approver in [
        support::reviewer_id(pg).await,
        support::reviewer_two_id(pg).await,
    ] {
        sqlx::query(
            "INSERT INTO approval_decision (approval_id, approver_id, decision, decided_by_kind)
             VALUES ($1, $2, 'approve', 'human')
             ON CONFLICT (approval_id, approver_id) DO NOTHING",
        )
        .bind(approval.id)
        .bind(approver)
        .execute(&mut *tx)
        .await
        .expect("insert the gate decision");
    }
    tx.commit().await.expect("commit the gate");
    approval.id
}

/// P1.1 — a destructive call that names no approval is refused before any cluster call.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destructive_call_without_approval_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "ops.restart",
            json!({"namespace": "conveyor", "deployment": "conveyor"}),
        ),
    )
    .await
    .expect_err("no approval was named");

    // 403, not 503: the refusal came from the gate, BEFORE the client was ever built. A 503
    // here would mean the call reached a client with no credential — the wrong refusal.
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "approval_required");
}

/// P1.2 — a dry run returns the request it would make and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dry_run_makes_no_write() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // An address that answers nothing: a dry run that DID reach the client would surface as a
    // transport failure instead of a 200, so this proves the call never left the process.
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    // ops.scale's target is fully determined by the input; ops.restart's carries the timestamp
    // the action generates, so the deterministic dry run is scale, and the restart dry run is
    // covered by the same code path (the gate and `request_out` run before any client exists).
    let target = "conveyor/deployment/conveyor/scale/3".to_owned();
    let approval_id = approved(&pg, seeded.tenant, "ops.scale", &target).await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_gated(
            "ops.scale",
            json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3, "dry_run": true}),
            Some(approval_id),
        ),
    )
    .await
    .expect("a dry run is allowed");

    let output = &outcome.body["output"];
    assert_eq!(output["dry_run"], json!(true));
    assert_eq!(output["request"]["method"], json!("PATCH"));
    assert_eq!(
        output["request"]["path"],
        json!("/apis/apps/v1/namespaces/conveyor/deployments/conveyor")
    );
    assert_eq!(output["request"]["query"], json!(["dryRun=All"]));
    assert_eq!(output["request"]["body"]["spec"]["replicas"], json!(3));
    assert_eq!(output["checks"][0]["name"], json!("dry_run"));
    assert_eq!(output["checks"][0]["ok"], json!(true));
}

/// P2.1 — an approval opened for another target does not authorise this one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_for_another_target_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let approval_id = approved(
        &pg,
        seeded.tenant,
        "ops.scale",
        "conveyor/deployment/conveyor/scale/9",
    )
    .await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_gated(
            "ops.scale",
            json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 2, "dry_run": true}),
            Some(approval_id),
        ),
    )
    .await
    .expect_err("the approval names replicas 9, the call asks for 2");

    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "approval_wrong_target");
}

/// The Deployment document the stub API answers with.
fn deployment_document() -> Value {
    json!({
        "kind": "Deployment",
        "apiVersion": "apps/v1",
        "metadata": {"name": "conveyor", "namespace": "conveyor"},
        "spec": {"replicas": 3}
    })
}

/// The stub API: one Deployment that accepts a scale patch and reports the new replica count.
async fn stub_api() -> (String, Arc<AtomicUsize>) {
    let patches = Arc::new(AtomicUsize::new(0));
    let counter = patches.clone();
    let app = axum::Router::new().route(
        "/apis/apps/v1/namespaces/conveyor/deployments/conveyor",
        get(|| async { axum::Json(deployment_document()) }).patch({
            let counter = counter.clone();
            move |body: axum::Json<Value>| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    // The write is a merge-patch; answer the object as the API would.
                    assert_eq!(body.0["spec"]["replicas"], json!(3));
                    axum::Json(deployment_document())
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the API stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, patches)
}

/// P2.2 — a completed write reports the observed state back in `checks[]`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_write_reads_back() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, patches) = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");

    let approval_id = approved(
        &pg,
        seeded.tenant,
        "ops.scale",
        "conveyor/deployment/conveyor/scale/3",
    )
    .await;
    support::record_dry_run(
        &pg,
        seeded.tenant,
        "ops.scale",
        &json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3}),
    )
    .await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_gated(
            "ops.scale",
            json!({"namespace": "conveyor", "deployment": "conveyor", "replicas": 3}),
            Some(approval_id),
        ),
    )
    .await
    .expect("the approved write runs");

    assert_eq!(
        patches.load(Ordering::SeqCst),
        1,
        "exactly one PATCH reached the API"
    );
    let output = &outcome.body["output"];
    assert_eq!(output["dry_run"], json!(false));
    assert_eq!(output["checks"][0]["name"], json!("spec_replicas"));
    assert_eq!(output["checks"][0]["ok"], json!(true));
    assert!(
        output["checks"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("3"),
        "{}",
        output["checks"][0]["detail"]
    );
}

/// The three writes carry the destructive effect class on the copilot's surface.
#[test]
fn the_ops_writes_are_destructive_on_the_surface() {
    let registry = registry();
    for name in ["ops.restart", "ops.scale", "ops.rollback"] {
        let action = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is registered"));
        assert_eq!(
            conveyor_actions::spec::Effect::Destructive,
            action.effect,
            "{name} must be destructive"
        );
        assert!(
            action
                .scopes
                .iter()
                .any(|scope| scope.as_str() == "ops:write"),
            "{name} must require ops:write"
        );
    }
}
