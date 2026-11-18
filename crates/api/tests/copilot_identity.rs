//! S16 W7 — the copilot's ledger row names the recipe it actually executed.
//!
//! The copilot compiles `copilot_system@1` from the tenant catalogue, but used to record the
//! literal `copilot`/`1` and the hash of the whole request. These tests prove the row now carries
//! the compiled recipe's key, version and sha256 — the same identity the prompt audit compiles —
//! and that a missing recipe refuses before the model call with the typed readiness error.

mod copilot;

use conveyor_store::begin_scoped;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_copilot_records_the_compiled_recipe_identity() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;
    model.text("hello");

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "hi").await;
    assert_eq!(status, 201, "{body}");

    // The audit's compile path with no run facts. `copilot_system@1` is one literal section, so
    // this is byte-identical to what the copilot compiled with the conversation's facts — which
    // is what makes the two callers share one identity.
    let compiled =
        conveyor_actions::prompt::compile_for_audit(&pg.runtime, seeded.tenant, "copilot_system@1")
            .await
            .expect("the seeded copilot recipe compiles");

    let (prompt_ref, prompt_version, prompt_sha256): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT prompt_ref, prompt_version, prompt_sha256
           FROM inference_call
          WHERE tenant_id = $1 AND caller_kind = 'copilot'
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the copilot's ledger row");

    assert_eq!(prompt_ref.as_deref(), Some("copilot_system"));
    assert_eq!(prompt_version.as_deref(), Some("1"));
    assert_eq!(
        prompt_sha256.as_deref(),
        Some(compiled.sha256.as_str()),
        "the ledger carries the compiled text's hash; the audit compiles the same identity"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_recipe_refuses_before_the_model_call_with_a_typed_409() {
    let pg = copilot::support::Pg::fresh().await;
    let seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    sqlx::query("DELETE FROM prompt WHERE key = 'copilot_system'")
        .execute(&mut *tx)
        .await
        .expect("remove the tenant's recipe");
    tx.commit().await.expect("commit");

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "hi").await;

    assert_eq!(
        status, 409,
        "a readiness refusal is typed, not a 500: {body}"
    );
    assert_eq!(body["type"], "capability_not_ready", "{body}");
    assert_eq!(model.hits(), 0, "the refusal precedes every model call");
}

/// P1 (W7's "every usage-capturing action"): a tool the copilot runs is a real invocation.
///
/// The copilot surface includes model-capturing actions, so a tool call can be a paid model call.
/// It must carry the turn's context into `inference_call` rather than vanish from the ledger.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copilot_tool_that_is_a_model_call_still_reaches_the_ledger() {
    let pg = copilot::support::Pg::fresh().await;
    let _seeded = copilot::support::seed(&pg, "dev-a", "alice", "Alice").await;
    copilot::catalogue(&pg).await;
    let model = copilot::Model::start().await;
    let running = copilot::boot(&pg, &model).await;

    // Round 1 asks for the tool; the tool itself calls the model (the stub answers it); round 2
    // finishes the turn.
    model.tool_call(
        "completions_create",
        json!({
            "model": "alias-model",
            "messages": [{"role": "user", "content": "a call inside a tool"}]
        }),
    );
    model.text("the tool answered");
    model.text("done");

    let conversation = copilot::conversation(&running, "dev:alice").await;
    let (status, body) = copilot::turn(&running, "dev:alice", conversation, "use the tool").await;
    assert_eq!(status, 201, "{body}");

    let rows: Vec<(String, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT caller_kind, conversation_turn_id
           FROM inference_call
          WHERE tenant_id = $1
          ORDER BY created_at",
    )
    .bind(_seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the ledger rows");
    assert_eq!(
        rows.len(),
        3,
        "round 1, the tool's own model call, and round 2: {rows:?}"
    );
    assert!(
        rows.iter().all(|(kind, _)| kind == "copilot"),
        "every call in the turn names the copilot: {rows:?}"
    );
    assert!(
        rows.iter().all(|(_, turn)| turn.is_some()),
        "every row joins the conversation turn: {rows:?}"
    );
}
