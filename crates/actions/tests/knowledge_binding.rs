//! S15 (v1-s15-1) — the proof the gate named and no shipped test carried: **a capability bound
//! to a published pack returns a hit carrying `pack` and `layer`**.
//!
//! The conveyor owns the binding; strata owns retrieval. This test pins the conveyor's half
//! against a stub strata: the pack resolves to its leaf, the query names exactly that leaf and
//! the capability code as metadata, and the upstream hit is returned unchanged — so a hit that
//! carries `pack` and `layer` reaches the caller instead of being flattened. The live half (the
//! cluster's real strata_rag with a conveyor token) stays the ignored `strata_live` target.
//!
//! `cargo test -p conveyor-actions --test knowledge_binding`.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pack_bound_capability_returns_a_hit_carrying_pack_and_layer() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the capability catalogue seed");
    let running = support::Running::start(&pg).await;

    // The tenant's own upstream token: `knowledge.query` refuses without it, before any call.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "strata_rag.token",
        b"stub-token",
        "tenant",
    )
    .await
    .expect("seed the tenant's strata token");
    tx.commit().await.expect("commit");

    // A stub strata: the pack's one leaf, and a query answer whose hit carries pack+layer.
    let leaf = Uuid::now_v7();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Value::Null));
    let app = axum::Router::new()
        .route(
            "/api/platform/packs/{key}/{version}/stores",
            axum::routing::get({
                let leaf = leaf.to_string();
                move || async move { axum::Json(json!({"stores": [{"id": leaf, "kind": "leaf"}]})) }
            }),
        )
        .route(
            "/api/query",
            axum::routing::post({
                let seen = seen.clone();
                move |axum::Json(body): axum::Json<Value>| {
                    let seen = seen.clone();
                    async move {
                        *seen.lock().expect("stub lock") = body;
                        axum::Json(json!({"results": [{
                            "chunk_id": "chunk-1",
                            "document_id": "doc-1",
                            "layer": "domain",
                            "pack": "design@1",
                            "text": "the answer",
                        }]}))
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    support::set_env("CONVEYOR_STRATA_RAG_URL", &base);

    // One seeded capability, bound to the pack.
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let capability = output(&body)["items"][0]["id"]
        .as_str()
        .expect("a seeded capability")
        .to_owned();
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": capability,
                "pack_key": "design",
                "pack_version": 1,
                "required": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "knowledge.query",
            TOKEN,
            json!({"capability_id": capability, "query": "what is the answer"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let out = output(&body);
    assert_eq!(out["packs"], json!(["design@1"]));
    let hit = &out["hits"][0];
    assert_eq!(hit["pack"], "design@1", "the hit keeps its pack");
    assert_eq!(hit["layer"], "domain", "the hit keeps its layer");

    // The upstream was asked for exactly the pack's leaf, filtered by the capability code.
    let request = seen.lock().expect("stub lock").clone();
    assert_eq!(request["leaf_ids"], json!([leaf]));
    assert_eq!(request["metadata"]["capability"], out["capability_code"]);
}
