//! #174 — `media.generate_image` over the estate's MOSS-Image host.
//!
//! Three properties, one per acceptance criterion on the issue: a render stores one tenant document
//! and one ledger row; a size the host cannot serve is refused before anything leaves the process;
//! and the action declares that it spends the tenant's money.
//!
//! The stub answers the engine's own `/v1/images/generations` shape (`data[0].b64_json`), because the
//! action is a passthrough — the translation it must not do is the legacy `/api/image/generate` one,
//! which belongs to w2v's provider.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_actions::agent_surface;
use conveyor_actions::integrations::objects::Objects;
use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A real 1×1 PNG: the object store's content-type check reads the magic bytes, so a stub payload
/// has to be an actual image rather than arbitrary base64.
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// Start the image host's stub: it answers one render and counts how often it was called.
async fn stub_image_host() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = axum::Router::new()
        .route(
            "/v1/images/generations",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    // Echo the size back the way the engine does, so the test sees a real shape.
                    let size = body
                        .get("size")
                        .and_then(Value::as_str)
                        .unwrap_or("512x512");
                    axum::Json(json!({
                        "created": 1_700_000_000,
                        "data": [{
                            "b64_json": PNG_1X1,
                            "mime_type": "image/png",
                            "width": 1024,
                            "height": 1024,
                            "seed": 4242,
                            "revised_prompt": "a red fox",
                            "size": size,
                        }],
                        "output_format": "png",
                        "size": size,
                    }))
                }
            }),
        )
        .route("/health", axum::routing::get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the image stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, hits)
}

fn request(user_id: Uuid, input: Value) -> InvokeRequest {
    InvokeRequest {
        action: "media.generate_image".to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-image".to_owned()),
        trace_id: Some("trace-image".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        // The ledger row is written for an ATTRIBUTABLE call: the agent run that asked for the
        // image is the caller S16 records, exactly as it does for `completions.create`.
        usage: Some(UsageContext {
            caller_kind: CallerKind::AgentRun,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("agent:image".to_owned()),
            prompt_version: None,
            correlation_id: Some("corr-image".to_owned()),
        }),
    }
}

/// P1.1 — one render produces one object, one document row and one `inference_call`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generate_image_writes_one_object() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits) = stub_image_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            json!({"prompt": "a red fox in snow", "size": "1024x1024", "seed": 4242, "steps": 12}),
        ),
    )
    .await
    .expect("the render");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "exactly one render");
    assert_eq!(
        outcome.body["output"]["data"][0]["seed"], 4242,
        "the engine's document is passed through unchanged"
    );

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        kind: String,
        title: String,
        status: String,
        object_key: Option<String>,
        content_hash: Option<String>,
        meta: Value,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, kind, title, status, object_key, content_hash, meta FROM document",
    )
    .fetch_all(&pg.super_pool)
    .await
    .expect("the document rows");
    assert_eq!(rows.len(), 1, "one render, one document");
    let row = &rows[0];
    assert_eq!(row.kind, "document");
    assert_eq!(row.title, "Generated image");
    assert_eq!(row.status, "active");
    assert_eq!(row.meta["content_type"], "image/png");

    let key = row.object_key.as_deref().expect("the row addresses bytes");
    assert_eq!(
        key,
        conveyor_actions::integrations::objects::key_for(
            seeded.tenant_id,
            row.id,
            row.content_hash.as_deref().expect("a hash")
        )
    );
    let objects = Objects::from_env().expect("the test object backend");
    let stored = objects.get(key).await.expect("the stored bytes");
    assert!(
        stored.starts_with(&[0x89, b'P', b'N', b'G']),
        "the bytes the row addresses are the PNG the host returned"
    );
    assert_eq!(
        row.meta["size_bytes"].as_i64(),
        Some(stored.len() as i64),
        "the meta records the decoded length, not the base64 length"
    );

    let expected_hash: String = Sha256::digest(&stored)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        row.content_hash.as_deref(),
        Some(expected_hash.as_str()),
        "the hash is of the decoded bytes"
    );

    assert_eq!(
        outcome.body["output"]["document"]["id"].as_str(),
        Some(row.id.to_string().as_str()),
        "the caller is given the document, not the base64"
    );

    let call: (String, Option<String>, Option<f64>) =
        sqlx::query_as("SELECT model, usage_unit, usage_units::float8 FROM inference_call")
            .fetch_one(&pg.super_pool)
            .await
            .expect("the ledger");
    assert_eq!(call.0, "image_gen", "the surface is the pricing identity");
    assert_eq!(call.1.as_deref(), Some("image"));
    assert_eq!(call.2, Some(1.0), "one render, one image");
}

/// P1.1 — an unknown field is refused by the schema, not silently dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_field_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits) = stub_image_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            json!({"prompt": "a red fox", "cfg_scale": 3.0}),
        ),
    )
    .await
    .expect_err("an unknown field must be refused");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(hits.load(Ordering::SeqCst), 0, "refused before the call");
}

/// P1.2 — a size the host cannot serve is refused before any render is spent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invalid_size_is_refused_before_the_call() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits) = stub_image_host().await;
    support::set_env("CONVEYOR_EXTERNAL_IMAGE_GEN_URL", &base);

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            json!({"prompt": "a red fox", "size": "2048x2048"}),
        ),
    )
    .await
    .expect_err("2048x2048 is not served on this host");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "a refused size must not cost a render"
    );

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM document")
        .fetch_one(&pg.super_pool)
        .await
        .expect("the documents");
    assert_eq!(rows, 0, "nothing was stored");
}

/// P1.3 — the agent surface carries the effect, so a caller can see what it costs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_image_action_is_spend() {
    let _pg = support::Pg::fresh().await;
    let spec = registry()
        .get("media.generate_image")
        .expect("the action is registered");
    let entry = agent_surface::entry(spec);
    assert_eq!(entry.effect, "spend");
    assert_eq!(entry.kind, "external");
    assert!(entry.idempotent, "a replay returns the first render");
}
