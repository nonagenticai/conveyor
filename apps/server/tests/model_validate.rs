//! S08 Part B: `model.validate` — the action queues a job in its own transaction and the
//! **worker** writes the run, so a full graph check never holds the model's row lock.
//!
//! `cargo test -p conveyor-server --test model_validate -- --nocapture`.
//!
//! The model, its elements and its deliberately-dangling relationship are seeded with
//! superuser SQL: the action surface is another test's file, and what is under test here is
//! the queue → worker → `model_validation_run` path plus the `model.validated` event.

mod common;

use conveyor_api::Config;
use conveyor_domain::TenantId;
use conveyor_server::worker;
use serde_json::json;
use uuid::Uuid;

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

async fn seed_tenant(db: &common::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil tenant")
}

async fn seed_model(db: &common::TestDb, tenant: TenantId, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO model (tenant_id, language, language_version, title)
         VALUES ($1, 'archimate', '3.2', $2) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(title)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a model")
}

async fn seed_element(
    db: &common::TestDb,
    tenant: TenantId,
    model: Uuid,
    kind: &str,
    name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO model_element (tenant_id, model_id, kind, name)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(model)
    .bind(kind)
    .bind(name)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed an element")
}

async fn seed_relationship(
    db: &common::TestDb,
    tenant: TenantId,
    model: Uuid,
    kind: &str,
    source: Uuid,
    target: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO model_relationship (tenant_id, model_id, kind, source_id, target_id)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(model)
    .bind(kind)
    .bind(source)
    .bind(target)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a relationship")
}

/// The newest run for a model: `(status, ruleset, findings jsonb, content_hash)`.
async fn latest_run(
    db: &common::TestDb,
    tenant: TenantId,
    model: Uuid,
) -> Option<(String, String, serde_json::Value, String)> {
    sqlx::query_as(
        "SELECT status, ruleset, findings, content_hash FROM model_validation_run
          WHERE tenant_id = $1 AND model_id = $2
          ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(model)
    .fetch_optional(&db.super_pool)
    .await
    .expect("read the run")
}

async fn validated_events(db: &common::TestDb, tenant: TenantId) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'model.validated'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("count the events")
}

#[tokio::test]
async fn a_dangling_relationship_is_a_finding_naming_its_id() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db, "mval-dangling").await;
    let model = seed_model(&db, tenant, "Dangling").await;
    let source = seed_element(&db, tenant, model, "application_component", "API").await;
    let missing = Uuid::now_v7();
    let relationship = seed_relationship(&db, tenant, model, "serving", source, missing).await;

    // Nothing is written before the worker runs: the action queues, it does not check.
    assert!(latest_run(&db, tenant, model).await.is_none());

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "model.validate",
        &json!({ "model_id": model }),
    )
    .await
    .expect("the job succeeds even when the model has findings");

    let (status, ruleset, findings, content_hash) = latest_run(&db, tenant, model)
        .await
        .expect("the worker wrote a run");
    assert_eq!(status, "findings");
    assert_eq!(ruleset, "conveyor-archimate-3.2-v1");
    assert_eq!(content_hash.len(), 64);
    let findings = findings.as_array().expect("findings is an array");
    let dangling = findings
        .iter()
        .find(|finding| finding["code"] == "dangling_relationship")
        .unwrap_or_else(|| panic!("no dangling_relationship finding in {findings:?}"));
    assert_eq!(
        dangling["relationship_id"],
        serde_json::json!(relationship),
        "the finding must name the relationship"
    );
    assert_eq!(validated_events(&db, tenant).await, 1);
}

#[tokio::test]
async fn a_clean_model_is_ok() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db, "mval-clean").await;
    let model = seed_model(&db, tenant, "Clean").await;
    let service = seed_element(&db, tenant, model, "application_service", "Serve").await;
    let component = seed_element(&db, tenant, model, "application_component", "API").await;
    // `serving` is unrestricted; both ends are in profile and present.
    seed_relationship(&db, tenant, model, "serving", component, service).await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "model.validate",
        &json!({ "model_id": model }),
    )
    .await
    .expect("the job succeeds");

    let (status, _, findings, _) = latest_run(&db, tenant, model)
        .await
        .expect("the worker wrote a run");
    assert_eq!(status, "ok");
    assert_eq!(findings, json!([]));
    assert_eq!(validated_events(&db, tenant).await, 1);
}

#[tokio::test]
async fn an_out_of_profile_kind_is_a_finding_not_a_crash() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db, "mval-profile").await;
    let model = seed_model(&db, tenant, "Odd").await;
    // Deliberately outside `conveyor-archimate-3.2-v1`: the write path refuses this, so it
    // can only arrive through a superuser fixture or an XMI import.
    let element = seed_element(&db, tenant, model, "business_interaction", "Chat").await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "model.validate",
        &json!({ "model_id": model }),
    )
    .await
    .expect("the job succeeds");

    let (status, _, findings, _) = latest_run(&db, tenant, model)
        .await
        .expect("the worker wrote a run");
    assert_eq!(status, "findings");
    let out_of_profile = findings
        .as_array()
        .expect("array")
        .iter()
        .find(|finding| finding["code"] == "out_of_profile_kind")
        .unwrap_or_else(|| panic!("no out_of_profile_kind finding in {findings:?}"));
    assert_eq!(out_of_profile["element_id"], serde_json::json!(element));
}

#[tokio::test]
async fn a_missing_model_is_a_permanent_failure() {
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db, "mval-missing").await;
    let outcome = worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "model.validate",
        &json!({ "model_id": Uuid::now_v7() }),
    )
    .await;
    let failure = outcome.expect_err("a missing model cannot be validated");
    assert!(
        failure
            .to_string()
            .contains("does not exist in this tenant"),
        "the refusal names the missing model: {failure}"
    );
    assert!(
        latest_run(&db, tenant, Uuid::nil()).await.is_none(),
        "a failed job writes no run"
    );
}
