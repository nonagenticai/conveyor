//! v1-t2-1 — `documents.compose` derives the generated class from the request's model-call
//! attribution and refuses to attach a generated draft to a card (§7.1, §7.4).
//!
//! `documents.update` already refuses to attach a generated draft after the fact. These tests
//! prove the creation path cannot do it in one call, that a model draft without the caller's
//! meta flag is still recognised and marked, and that a person's authored output still attaches.

mod support;

use conveyor_actions::{Authority, InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

/// The one seeded tenant, a project with an active lane, and its card.
struct Fixture {
    pg: support::Pg,
    seeded: support::Seeded,
    card_id: Uuid,
}

async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let tenant_id = seeded.tenant_id;
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'PG', 'Compose gate')
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 0, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the active lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the card type");
    let card_id: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'The card', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the card");
    Fixture {
        pg,
        seeded,
        card_id,
    }
}

/// A direct invocation of `documents.compose`; `usage` is the request's model-call attribution.
fn request(seeded: &support::Seeded, input: Value, usage: Option<UsageContext>) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: "documents.compose".to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some("rid-documents-compose-gate".to_owned()),
        trace_id: Some("trace-documents-compose-gate".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage,
    }
}

/// A copilot turn's attribution: a model call with a conversation turn and a compiled prompt.
fn model_usage() -> UsageContext {
    UsageContext {
        caller_kind: CallerKind::Copilot,
        run_id: None,
        run_step_id: None,
        conversation_turn_id: Some(Uuid::now_v7()),
        eval_run_id: None,
        prompt_ref: Some("copilot:draft".to_owned()),
        prompt_version: Some("3".to_owned()),
        prompt_sha256: Some("a".repeat(64)),
        correlation_id: Some("corr-documents-compose-gate".to_owned()),
    }
}

async fn document_count(fx: &Fixture) -> i64 {
    let mut tx = begin_scoped(&fx.pg.owner, fx.seeded.tenant)
        .await
        .expect("scope the count");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM document WHERE tenant_id = $1")
        .bind(fx.seeded.tenant_id)
        .fetch_one(&mut *tx)
        .await
        .expect("count the documents");
    tx.commit().await.expect("commit the count");
    count
}

async fn document_meta(fx: &Fixture, id: Uuid) -> Value {
    let mut tx = begin_scoped(&fx.pg.owner, fx.seeded.tenant)
        .await
        .expect("scope the meta read");
    let meta: Value =
        sqlx::query_scalar("SELECT meta FROM document WHERE tenant_id = $1 AND id = $2")
            .bind(fx.seeded.tenant_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the document meta");
    tx.commit().await.expect("commit the meta read");
    meta
}

fn uuid_of(body: &Value, field: &str) -> Uuid {
    body["output"][field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {body}"))
        .parse()
        .expect("a uuid")
}

/// 1. A model-attributed compose with a `card_id` is refused, and no document row is written —
///    even though the caller supplied no `meta.generated` flag.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_context_compose_cannot_attach_a_card() {
    let fx = fixture().await;
    let before = document_count(&fx).await;
    let error = invoke(
        &fx.pg.runtime,
        registry(),
        fx.seeded.tenant,
        request(
            &fx.seeded,
            json!({
                "title": "Model patch",
                "body": "the model's proposal",
                "card_id": fx.card_id
            }),
            Some(model_usage()),
        ),
    )
    .await
    .expect_err("a model-attributed compose must not attach a card");
    assert_eq!(error.status, 409, "{error:?}");
    assert_eq!(error.code, "draft_requires_gate", "{error:?}");
    assert_eq!(before, 0, "the fixture creates no document");
    assert_eq!(
        document_count(&fx).await,
        before,
        "a refused compose writes no document row"
    );
}

/// 2. A model draft with no `meta.generated` flag and no card is accepted, and the server stamps
///    the generated class and its attribution facts from the request.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_draft_without_the_flag_is_recognised() {
    let fx = fixture().await;
    let outcome = invoke(
        &fx.pg.runtime,
        registry(),
        fx.seeded.tenant,
        request(
            &fx.seeded,
            json!({"title": "Model draft", "body": "the model's proposal"}),
            Some(model_usage()),
        ),
    )
    .await
    .expect("a model draft without a card is accepted");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let meta = document_meta(&fx, uuid_of(&outcome.body, "id")).await;
    assert_eq!(meta["generated"], json!(true), "{meta}");
    assert_eq!(
        meta["generated_by"]["caller_kind"],
        json!("copilot"),
        "{meta}"
    );
    assert_eq!(
        meta["generated_by"]["prompt_ref"],
        json!("copilot:draft"),
        "{meta}"
    );
}

/// 3. A declared `generated: true` with a `card_id` is refused on the same gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_generated_draft_cannot_attach_a_card() {
    let fx = fixture().await;
    let error = invoke(
        &fx.pg.runtime,
        registry(),
        fx.seeded.tenant,
        request(
            &fx.seeded,
            json!({
                "title": "Declared patch",
                "body": "a proposal",
                "card_id": fx.card_id,
                "meta": {"generated": true}
            }),
            None,
        ),
    )
    .await
    .expect_err("a declared generated draft must not attach a card");
    assert_eq!(error.status, 409, "{error:?}");
    assert_eq!(error.code, "draft_requires_gate", "{error:?}");
    assert_eq!(
        document_count(&fx).await,
        0,
        "a refused compose writes no document row"
    );
}

/// 4. A person's output still attaches directly: no attribution and no flag is authored work.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_authored_output_may_attach() {
    let fx = fixture().await;
    let outcome = invoke(
        &fx.pg.runtime,
        registry(),
        fx.seeded.tenant,
        request(
            &fx.seeded,
            json!({
                "title": "Authored note",
                "body": "written by a person",
                "card_id": fx.card_id
            }),
            None,
        ),
    )
    .await
    .expect("a person's output may attach");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    assert_eq!(
        outcome.body["output"]["card_id"],
        json!(fx.card_id.to_string()),
        "{}",
        outcome.body
    );
    let meta = document_meta(&fx, uuid_of(&outcome.body, "id")).await;
    assert!(
        meta.get("generated").is_none(),
        "authored work carries no generated marker: {meta}"
    );
}
