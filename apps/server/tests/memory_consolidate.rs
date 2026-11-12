//! S15 step 10 — `memory.consolidate`: one digest, ingested once, the covered rows stamped, the
//! old run pruned, and a replay a no-op.
//!
//! The upstream is a scripted HTTP stub; the real-service proof is the ignored
//! `knowledge_live` test against the compose `strata` profile.

mod common;

use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use conveyor_domain::TenantId;
use conveyor_server::memory_consolidate;
use serde_json::{Value, json};
use uuid::Uuid;

const STORE: Uuid = Uuid::from_u128(0x5157_0000_0000_0000_0000_0000_0000_0001);

type IngestLog = Arc<Mutex<Vec<Value>>>;
type PruneLog = Arc<Mutex<Vec<(String, Option<String>)>>>;

#[derive(Clone, Default)]
struct Stub {
    ingested: IngestLog,
    pruned: PruneLog,
}

fn set_kek() {
    // SAFETY: set before the first `sealer()` call in this binary; the keystore caches it once.
    unsafe {
        std::env::set_var(
            "CONVEYOR_SECRET_KEK",
            "Y29udmV5b3ItdGVzdC1rZWstMzJieXRlcy1sb25nISE=",
        );
    }
}

fn set_env(base: &str) {
    // SAFETY: set before the first `Strata::from_env` call in this binary.
    unsafe {
        std::env::set_var("CONVEYOR_STRATA_RAG_URL", base);
    }
}

async fn stub() -> (String, Stub) {
    let state = Stub::default();
    let app = axum::Router::new()
        .route("/api/stores", get(stores).post(create_store))
        .route("/api/stores/{id}/documents", post(ingest))
        .route(
            "/api/sources/{source}/documents",
            axum::routing::delete(prune),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, state)
}

async fn stores() -> axum::Json<Value> {
    axum::Json(json!({"stores": [{
        "id": STORE, "parent_id": null, "kind": "leaf", "name": "conveyor-memory", "description": ""
    }]}))
}

async fn create_store() -> axum::Json<Value> {
    axum::Json(json!({"id": STORE, "parent_id": null, "kind": "leaf", "name": "conveyor-memory"}))
}

async fn ingest(
    State(state): State<Stub>,
    Path(_id): Path<Uuid>,
    axum::Json(body): axum::Json<Value>,
) -> axum::Json<Value> {
    state.ingested.lock().expect("lock").push(body);
    axum::Json(json!({"status": "created", "id": Uuid::now_v7()}))
}

async fn prune(
    State(state): State<Stub>,
    Path(source): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> axum::Json<Value> {
    state
        .pruned
        .lock()
        .expect("lock")
        .push((source, query.get("except_run").cloned()));
    axum::Json(json!({"deleted": 1, "kept": 1, "stores": [], "refused_stores": []}))
}

/// One tenant with one card and two hot memory rows.
async fn seed(db: &common::TestDb) -> (TenantId, Uuid) {
    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('s15-mem-job', 'S15') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('s15-mem-user') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("user");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name, owner_user_id) VALUES ($1, 'P', $2) RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .fetch_one(&db.super_pool)
    .await
    .expect("programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id, project_type_code, owner_user_id)
         VALUES ($1, 'p', 'P', $2, 'delivery', $3) RETURNING id",
    )
    .bind(tenant)
    .bind(programme)
    .bind(user)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position) VALUES ($1, $2, 'B', 0) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("list");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("card type");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'Card', 1024) RETURNING id",
    )
    .bind(tenant)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("card");
    for (kind, body) in [("fact", "found the cause"), ("attempt", "tried a fix")] {
        sqlx::query(
            "INSERT INTO work_package_memory (tenant_id, project_id, card_id, kind, source, body)
             VALUES ($1, $2, $3, $4, 'agent', $5)",
        )
        .bind(tenant)
        .bind(project)
        .bind(card)
        .bind(kind)
        .bind(body)
        .execute(&db.super_pool)
        .await
        .expect("memory row");
    }
    // The tenant's own upstream token, sealed through the store.
    let mut tx =
        conveyor_store::begin_scoped(&db.runtime, TenantId::from_uuid(tenant).expect("non-nil"))
            .await
            .expect("scope");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        TenantId::from_uuid(tenant).expect("non-nil"),
        "strata_rag.token",
        b"stub-token",
        "tenant",
    )
    .await
    .expect("seed secret");
    tx.commit().await.expect("commit");
    (TenantId::from_uuid(tenant).expect("non-nil"), card)
}

#[tokio::test]
async fn a_digest_is_ingested_once_and_an_old_run_is_pruned() {
    set_kek();
    let db = common::TestDb::fresh().await;
    let (tenant, card) = seed(&db).await;
    let (base, stub) = stub().await;
    set_env(&base);

    memory_consolidate::run(&db.runtime, tenant, &json!({"card_id": card}))
        .await
        .expect("consolidate");
    let ingested = stub.ingested.lock().expect("lock").clone();
    assert_eq!(ingested.len(), 1, "one document per consolidation");
    let document = &ingested[0];
    let text = document["text"].as_str().expect("text");
    assert!(text.starts_with(&format!("work-package: {card}")), "{text}");
    assert_eq!(
        document["source_id"],
        json!(format!("conveyor:work:{card}"))
    );
    assert!(document["source_run"].is_string());
    assert_eq!(document["metadata"]["card_id"], json!(card));

    let digest_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2 AND kind = 'digest'",
    )
    .bind(tenant.as_uuid())
    .bind(card)
    .fetch_one(&db.super_pool)
    .await
    .expect("digest rows");
    assert_eq!(digest_rows, 1);
    let unstamped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2
           AND kind <> 'digest' AND consolidated_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(card)
    .fetch_one(&db.super_pool)
    .await
    .expect("unconsolidated");
    assert_eq!(unstamped, 0, "every covered row is stamped");

    let prunes = stub.pruned.lock().expect("lock").clone();
    assert_eq!(prunes.len(), 1);
    assert_eq!(prunes[0].0, format!("conveyor:work:{card}"));
    assert_eq!(prunes[0].1.as_deref(), document["source_run"].as_str());

    // Replay: nothing un-consolidated, so no second document and no second prune.
    memory_consolidate::run(&db.runtime, tenant, &json!({"card_id": card}))
        .await
        .expect("replay");
    assert_eq!(stub.ingested.lock().expect("lock").len(), 1);
    assert_eq!(stub.pruned.lock().expect("lock").len(), 1);
}
