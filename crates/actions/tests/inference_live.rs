//! One real call through the estate inference router writes exactly one `inference_call` row
//! carrying a served model — the live rehearsal the S16 plan never had (P0 finding `v1-s16-1`).
//!
//! ```sh
//! CONVEYOR_LIVE_ROUTER_TOKEN=… ~/.claude/scripts/cargo-guard.py test -p conveyor-actions --test inference_live -- --ignored --nocapture
//! ```
//!
//! The caller's licence token is read from `CONVEYOR_LIVE_ROUTER_TOKEN` at run time — no
//! checked-in file carries it — and it is never logged or printed. `CONVEYOR_LIVE_ROUTER_URL`
//! overrides the public router.

mod support;

use conveyor_actions::integrations::inference::INFERENCE_TOKEN_SECRET;
use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use conveyor_store::inference::{ListCalls, list_calls_tx};
use conveyor_store::{begin_scoped, begin_scoped_read_only};
use serde_json::json;
use uuid::Uuid;

/// The router's reserved platform model: the name its catalogue serves for the estate's own
/// weights (`apps/inference-router/src/policy.rs`, `PLATFORM_MODEL`).
const MODEL: &str = "local-llm";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a real inference router and a licence token"]
async fn one_real_call_through_the_router_writes_one_ledger_row_with_a_served_model() {
    // The licence credential exists only in the environment: hold it in memory, never print it.
    let token = match std::env::var("CONVEYOR_LIVE_ROUTER_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => panic!(
            "CONVEYOR_LIVE_ROUTER_TOKEN is not set; export the inference router's licence token \
             to run this ignored test (the value is never printed)"
        ),
    };
    let router = std::env::var("CONVEYOR_LIVE_ROUTER_URL")
        .unwrap_or_else(|_| "https://api.nonagentic.ai/inference-router".to_owned());
    // Set before any client is constructed: the action resolves its base URL from the process
    // environment, and the engine appends `/api/chat/completions`.
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);

    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    // The engine opens the tenant's sealed secret and sends it as the bearer, so the tenant is
    // configured exactly as a real BYO tenant would be.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped secret tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        INFERENCE_TOKEN_SECRET,
        token.as_bytes(),
        "tenant",
    )
    .await
    .expect("seal the router token");
    tx.commit().await.expect("commit the sealed token");

    let request = InvokeRequest {
        credential_alias: None,
        action: "completions.create".to_owned(),
        input: json!({
            "model": MODEL,
            "messages": [{"role": "user", "content": "say ok"}],
        }),
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-inference-live".to_owned()),
        trace_id: Some("trace-inference-live".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        // A usage context is what the capture seam records: without it the engine writes no row.
        usage: Some(UsageContext {
            caller_kind: CallerKind::ProcessStep,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("process:s16-live".to_owned()),
            prompt_version: Some("1:1".to_owned()),
            prompt_sha256: None,
            correlation_id: Some(Uuid::now_v7().to_string()),
        }),
    };
    let outcome = invoke(&pg.runtime, registry(), seeded.tenant, request)
        .await
        .expect("the live router call");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    let message = outcome.body["output"]["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !message.trim().is_empty(),
        "the router answered no assistant message: {}",
        outcome.body
    );

    // The scoped read: RLS confines it to the tenant, and a fresh tenant made exactly one call.
    let mut read = begin_scoped_read_only(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped ledger read");
    let calls = list_calls_tx(
        &mut read,
        seeded.tenant,
        &ListCalls {
            limit: 10,
            ..ListCalls::default()
        },
    )
    .await
    .expect("the tenant's inference ledger");
    read.commit().await.expect("close the ledger read");

    assert_eq!(
        calls.len(),
        1,
        "one real call must write exactly one inference_call row, got {}",
        calls.len()
    );
    let call = &calls[0];
    assert_eq!(call.model, MODEL, "the requested model is ledgered");
    assert_eq!(call.outcome, "ok", "a served call is ledgered as ok");
    assert!(
        call.served_model
            .as_deref()
            .is_some_and(|served| !served.trim().is_empty()),
        "the router's own served model must be recorded, not null: {:?}",
        call.served_model
    );
}
