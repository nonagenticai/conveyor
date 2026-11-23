//! WP17c — one free-text question, offered on every surface, answered once.
//!
//! The question is a run-scoped approval carrying `meta.requires_answer`; one dispatcher tick
//! offers it to the human web inboxes and the tenant's Telegram binding in the same words, and
//! the offer teaches the `answer <id> <text>` grammar the inbound channels parse. The answer
//! arrives over MCP `tasks/update` under the owner's published delegation and becomes the one
//! `rule` decision; a second answer through the Telegram command grammar loses the race (`409`)
//! and writes nothing. The close sweep then expires every remaining offer and, idempotently,
//! gives each losing recipient exactly one closure notice.
//!
//! The two worker ticks are driven through the same `worker::run_kind` entry point the worker
//! binary uses, so the test is deterministic: no child process and no polling.

#[path = "../../../crates/actions/tests/support/mod.rs"]
mod support;

use std::collections::BTreeSet;

use conveyor_actions::channel::command::{self, Parsed};
use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::{ChannelAuthority, Role};
use conveyor_server::worker::run_kind;
use serde_json::{Value, json};
use uuid::Uuid;

/// The extension identifier (SEP-2663) a task-answering client declares.
const TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";
const CHAT_ID: i64 = 42;
const BOT_TOKEN_SECRET: &str = "telegram-bot-token";
const INCOMING_SECRET: &str = "telegram-incoming";
const QUESTION: &str = "Which release channel should the agent use?";
const ANSWER: &str = "Use the canary channel";

fn declaring_capabilities() -> Value {
    json!({ "extensions": { TASKS_EXTENSION: {} } })
}

/// The one status query every step re-reads: the first decision must remain the only one.
async fn approval_status(pg: &support::Pg, tenant_id: Uuid, approval: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(approval)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the approval exists")
}

/// The decisions on a gate: `(decision, note, decided_by_kind, via)`.
async fn decisions(
    pg: &support::Pg,
    tenant_id: Uuid,
    approval: Uuid,
) -> Vec<(String, String, String, Value)> {
    sqlx::query_as(
        "SELECT decision, note, decided_by_kind, via
           FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2
          ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .bind(approval)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the decisions")
}

#[tokio::test]
async fn a_question_offers_web_and_telegram_and_the_first_answer_wins() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` builds its board from the catalogue's standard template.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("seed the catalogue");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;

    // ── fixture: the Telegram route and its provisioned service identity ─────────────────
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    let binding = conveyor_store::channel::upsert_binding_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::channel::NewChannelBinding {
            channel_kind: "telegram".to_owned(),
            route_key: "default".to_owned(),
            // The two secret fields name sealed values. Nothing in this test sends or resolves
            // them: the dispatch tick only enqueues the send, and the losing answer uses the
            // shared grammar with the channel authority the inbound route would build.
            config: json!({
                "chat_id": CHAT_ID.to_string(),
                "bot_token_secret": BOT_TOKEN_SECRET,
                "incoming_secret": INCOMING_SECRET,
            }),
            active: true,
        },
    )
    .await
    .expect("bind the telegram route");
    let telegram =
        conveyor_store::channel::ensure_service_account_tx(&mut tx, seeded.tenant, "telegram")
            .await
            .expect("provision the telegram identity");
    tx.commit().await.expect("commit the binding");

    // ── a run-scoped question that demands an answer ─────────────────────────────────────
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "WP17c question route", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = project["output"]["id"]
        .as_str()
        .expect("a project id")
        .to_owned();

    let mut mcp =
        support::Mcp::connect_with_capabilities(&running, "dev:alice", declaring_capabilities())
            .await;
    let task = mcp
        .call("projects_execute", json!({"project_id": project_id}), None)
        .await;
    let task_id = task["taskId"].as_str().expect("a task id").to_owned();

    // The owner's published decision the MCP answer is delegated under.
    let delegation = support::delegation_ref(&running, "dev:alice").await;

    let (status, gate) = running
        .invoke(
            "approvals.request",
            support::REVIEWER,
            json!({
                "run_id": task_id,
                "project_id": project_id,
                "gate": "agent_question",
                "kind": "question",
                "description": QUESTION,
                "requires_answer": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{gate}");
    let approval = gate["output"]["id"]
        .as_str()
        .expect("a question gate id")
        .to_owned();
    let approval_id = Uuid::parse_str(&approval).expect("a uuid");
    // The gate is the run's, and it is a question that demands an answer.
    let (run_id, meta): (Option<Uuid>, Value) =
        sqlx::query_as("SELECT run_id, meta FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(approval_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the question gate");
    assert_eq!(
        run_id,
        Some(Uuid::parse_str(&task_id).expect("the task id is the run id"))
    );
    assert_eq!(meta["requires_answer"], json!(true), "{meta}");
    assert_eq!(meta["description"], json!(QUESTION), "{meta}");

    // ── a/b. one tick offers the same question to a web inbox and Telegram ───────────────
    let config = support::config(&pg);
    run_kind(
        &pg.runtime,
        &config,
        None,
        seeded.tenant,
        "interaction.dispatch",
        &json!({}),
    )
    .await
    .expect("the dispatch tick");

    let offers: Vec<(String, String, Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT channel_kind, recipient_ref, message_id, binding_id
           FROM interaction_dispatch
          WHERE tenant_id = $1 AND approval_id = $2
          ORDER BY channel_kind, recipient_ref",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the dispatch rows");
    assert!(
        offers.len() >= 2,
        "the question reaches at least the web inbox and the binding: {offers:?}"
    );

    let recipients: BTreeSet<(String, String)> = offers
        .iter()
        .map(|(kind, recipient, _, _)| (kind.clone(), recipient.clone()))
        .collect();
    let web_inbox = ("web".to_owned(), seeded.user_id.to_string());
    let telegram_binding = ("telegram".to_owned(), format!("binding:{}", binding.id));
    assert!(
        recipients.contains(&web_inbox),
        "a human web inbox holds the question: {recipients:?}"
    );
    assert!(
        recipients.contains(&telegram_binding),
        "the telegram binding holds the same question: {recipients:?}"
    );

    // The same words on every surface, with the dispatch row and its delivery witnessing each.
    let mut bodies = BTreeSet::new();
    for (channel_kind, recipient, message_id, binding_id) in &offers {
        let (kind, body, meta): (String, String, Value) =
            sqlx::query_as("SELECT kind, body, meta FROM message WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant_id)
                .bind(*message_id)
                .fetch_one(&pg.super_pool)
                .await
                .expect("the offer message");
        assert_eq!(kind, "interaction-requested", "{body}");
        assert_eq!(meta["approval_id"], json!(approval_id), "{meta}");
        assert!(
            body.contains(QUESTION),
            "the offer carries the question: {body}"
        );
        // WP17b: a question must teach the answer command, never a bare approve.
        assert!(
            body.contains(&format!("answer {approval_id} <text>")),
            "the offer names the answer grammar: {body}"
        );
        assert!(
            !body.contains("approve "),
            "a requires-answer gate does not invite a bare approve: {body}"
        );
        bodies.insert(body);

        let (state, delivery_channel, delivery_recipient): (String, String, String) =
            sqlx::query_as(
                "SELECT state, channel_kind, recipient_ref FROM delivery
              WHERE tenant_id = $1 AND message_id = $2",
            )
            .bind(seeded.tenant_id)
            .bind(*message_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the offer delivery");
        assert_eq!(state, "pending", "no send has drained this offer");
        assert_eq!(&delivery_channel, channel_kind);
        assert_eq!(&delivery_recipient, recipient);
        if channel_kind == "telegram" {
            assert_eq!(
                binding_id,
                &Some(binding.id),
                "the telegram offer names the binding it came through"
            );
        }
    }
    assert_eq!(
        bodies.len(),
        1,
        "every surface reads the same question: {bodies:?}"
    );

    // ── c. the answer over MCP under the delegation is the one rule decision ─────────────
    let ack = mcp
        .raw(
            "tasks/update",
            json!({
                "taskId": task_id,
                "inputResponses": {
                    approval.clone(): {"action": "accept", "content": {"answer": ANSWER}}
                },
                "_meta": {"delegation_ref": delegation.clone()},
            }),
        )
        .await;
    assert!(ack.get("result").is_some(), "{ack}");

    let rows = decisions(&pg, seeded.tenant_id, approval_id).await;
    assert_eq!(rows.len(), 1, "an answer is exactly one decision: {rows:?}");
    assert_eq!(rows[0].0, "approve", "an answer is the approval");
    assert_eq!(rows[0].1, ANSWER, "the answer is the decision's note");
    assert_eq!(
        rows[0].2, "rule",
        "a delegated answer is a rule, never the caller"
    );
    assert_eq!(
        rows[0].3["delegation_ref"],
        json!(delegation),
        "the published owner decision is in via: {}",
        rows[0].3
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, approval_id).await,
        "approved"
    );

    // ── d. a second answer through Telegram loses: 409, no second row ────────────────────
    let Parsed::Command { action, input } =
        command::parse(&format!("answer {approval_id} use the stable channel"))
    else {
        panic!("the shared grammar must parse `answer <id> <text>`");
    };
    assert_eq!(action, "approvals.decide");
    let overrides =
        conveyor_store::rbac::channel_policy_for(&pg.runtime, seeded.tenant, "telegram")
            .await
            .expect("the telegram grant overrides");
    let second = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        InvokeRequest {
            credential_alias: None,
            action,
            input,
            precondition: None,
            idempotency_key: Some(format!("channel:{}:telegram:2", binding.id)),
            approval_id: None,
            actor_id: telegram,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Channel(ChannelAuthority::new(
                "telegram",
                Some("tg:99".to_owned()),
                overrides,
            )),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await;
    let error = second.expect_err("the first decision wins; the second is refused");
    assert_eq!(error.status, 409, "{error:?}");
    assert!(
        error.detail.contains("only a pending gate can change"),
        "the refusal names the already-approved gate, not a different guard: {error:?}"
    );
    assert_eq!(
        decisions(&pg, seeded.tenant_id, approval_id).await.len(),
        1,
        "a refused answer writes no decision"
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, approval_id).await,
        "approved",
        "the gate stays approved"
    );

    // ── e. close expires every losing offer and emits one notice per recipient ──────────
    run_kind(
        &pg.runtime,
        &config,
        None,
        seeded.tenant,
        "interaction.close",
        &json!({}),
    )
    .await
    .expect("the close tick");

    let offer_deliveries: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT d.state, d.last_error
           FROM delivery d
           JOIN message m ON m.tenant_id = d.tenant_id AND m.id = d.message_id
          WHERE d.tenant_id = $1 AND m.kind = 'interaction-requested'
          ORDER BY d.channel_kind, d.recipient_ref",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the offer deliveries");
    assert_eq!(
        offer_deliveries.len(),
        offers.len(),
        "every offer has its delivery"
    );
    for (state, error) in &offer_deliveries {
        assert_eq!(state, "expired", "a losing offer is expired");
        assert!(
            error
                .as_deref()
                .is_some_and(|error| error.starts_with("superseded: resolved via ")),
            "the expiry names the resolving channel: {error:?}"
        );
    }
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM delivery d
           JOIN message m ON m.id = d.message_id
          WHERE d.tenant_id = $1 AND m.kind = 'interaction-requested'
            AND d.state IN ('pending', 'sent')",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("live offers");
    assert_eq!(live, 0, "no offer is left live after the answer");

    let notices: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT m.channel_kind, d.recipient_ref, count(*)::bigint
           FROM message m
           JOIN delivery d ON d.tenant_id = m.tenant_id AND d.message_id = m.id
          WHERE m.tenant_id = $1 AND m.kind = 'interaction-closed'
            AND m.correlation_id = $2
          GROUP BY m.channel_kind, d.recipient_ref
          ORDER BY m.channel_kind, d.recipient_ref",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the closure notices");
    assert_eq!(
        notices.len(),
        offers.len(),
        "one closure notice per losing recipient"
    );
    assert!(
        notices.iter().all(|(_, _, count)| *count == 1),
        "no recipient is notified twice: {notices:?}"
    );
    let notified: BTreeSet<(String, String)> = notices
        .iter()
        .map(|(channel, recipient, _)| (channel.clone(), recipient.clone()))
        .collect();
    assert_eq!(notified, recipients, "every losing offer is closed");

    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM message
          WHERE tenant_id = $1 AND kind = 'interaction-closed' AND correlation_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the closure bodies");
    assert!(
        bodies
            .iter()
            .all(|body| body.contains("was approved via")
                && body.contains("no answer is needed here")),
        "{bodies:?}"
    );

    let closed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM interaction_dispatch
          WHERE tenant_id = $1 AND approval_id = $2 AND closed_notified_at IS NOT NULL",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("closed dispatches");
    assert_eq!(
        closed,
        offers.len() as i64,
        "every dispatch is stamped as notified"
    );

    // A second close tick finds nothing open: the closure is idempotent.
    run_kind(
        &pg.runtime,
        &config,
        None,
        seeded.tenant,
        "interaction.close",
        &json!({}),
    )
    .await
    .expect("the second close tick");
    let again: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM message
          WHERE tenant_id = $1 AND kind = 'interaction-closed' AND correlation_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the notice count after a replay");
    assert_eq!(
        again,
        offers.len() as i64,
        "a replayed close tick emits no second notice"
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, approval_id).await,
        "approved"
    );
}
