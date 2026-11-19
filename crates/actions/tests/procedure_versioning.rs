//! T16 P3 — a re-ingest is versioned: the next draft version appears, and the published version
//! keeps every reference it was released with.
//!
//! Re-scoped 2026-11-19: the versioned knowledge is the strata pack the source names (owner
//! decision — no PKO tables, #413), not local ontology rows.

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::Mutex;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

/// The inference and strata destinations are process-global; these tests hold the lock throughout.
static ENV_LOCK: Mutex<()> = Mutex::const_new(());

async fn fixture() -> (support::Pg, support::Running) {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn field<'a>(body: &'a Value, name: &str) -> &'a str {
    output(body)[name]
        .as_str()
        .unwrap_or_else(|| panic!("{name} is absent from {body}"))
}

async fn stub_inference() -> (String, Arc<Mutex<String>>) {
    let draft: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let state = draft.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move || {
            let state = state.clone();
            async move {
                let content = state.lock().await.clone();
                axum::Json(json!({
                    "choices": [{"message": {"role": "assistant", "content": content}}]
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the inference stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, draft)
}

fn draft_with(action_text: &str) -> String {
    json!({
        "domain": "(define (domain receiving))",
        "init": [],
        "steps": [{
            "action": "procedures.list",
            "action_text": action_text,
            "preconditions": [],
            "effects": {"add": ["done"], "delete": []},
        }],
        "requested_tools": [],
    })
    .to_string()
}

fn sop_input(process_key: &str, source_key: &str, store_id: Option<&str>) -> Value {
    let mut input = json!({
        "title": "Synthetic receiving SOP",
        "process_key": process_key,
        "source_key": source_key,
        "source": {"kind": "text", "text": "Synthetic SOP body."},
    });
    if let Some(store_id) = store_id {
        input["strata_store_id"] = json!(store_id);
    }
    input
}

async fn create_secret(running: &support::Running) {
    let (status, body) = running
        .invoke(
            "secrets.create",
            TOKEN,
            json!({"name": "strata_ingest.token", "value": "test-token-not-a-credential"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

/// Review, approve the gate and publish the derived process (and then its judge).
async fn publish_derived(
    running: &support::Running,
    process_id: &str,
    procedure_id: &str,
    asset_id: &str,
) {
    let (status, body) = running
        .invoke(
            "procedures.review_source",
            REVIEWER,
            json!({"asset_id": asset_id, "decision": "approve", "note": "reviewed"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    // The judge must be published before the executable: every step names the SOP it is judged
    // against, and publish refuses a step whose judge is still a draft.
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": procedure_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, requested) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({"gate": "processes.publish", "kind": "publication", "target": process_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{requested}");
    let approval = field(&requested, "id").to_owned();
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");
    let (status, body) = running
        .invoke_approved(
            "processes.publish",
            TOKEN,
            json!({"process_id": process_id}),
            None,
            uuid::Uuid::parse_str(&approval).expect("approval id"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// One process version's stored facts.
#[derive(Debug)]
struct VersionRow {
    id: String,
    version: i64,
    status: String,
    source_asset_id: Option<String>,
    knowledge_pack_ref: String,
    action_text: Option<String>,
}

async fn versions(pg: &support::Pg, key: &str) -> Vec<VersionRow> {
    let rows: Vec<(uuid::Uuid, i64, String, Option<uuid::Uuid>, String)> = sqlx::query_as(
        "SELECT id, version, status, source_asset_id, knowledge_pack_ref FROM process \
          WHERE key = $1 ORDER BY version",
    )
    .bind(key)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the process versions");
    let mut out = Vec::new();
    for (id, version, status, source, pack) in rows {
        let action_text: Option<String> = sqlx::query_scalar(
            "SELECT action_text FROM process_step WHERE process_id = $1 ORDER BY position LIMIT 1",
        )
        .bind(id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the step's action text");
        out.push(VersionRow {
            id: id.to_string(),
            version,
            status,
            source_asset_id: source.map(|source| source.to_string()),
            knowledge_pack_ref: pack,
            action_text,
        });
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reingest_creates_a_new_draft_version() {
    let _env = ENV_LOCK.lock().await;
    let (pg, running) = fixture().await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content
        .lock()
        .await
        .clone_from(&draft_with("verify_stock"));

    let (status, first) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-versioned", "receiving-sop-v1", None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{first}");
    publish_derived(
        &running,
        field(&first, "process_id"),
        field(&first, "procedure_id"),
        field(&first, "source_asset_id"),
    )
    .await;

    // The changed source: a second ingest of the same process key.
    draft_content
        .lock()
        .await
        .clone_from(&draft_with("verify_stock_again"));
    let (status, second) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-versioned", "receiving-sop-v2", None),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second}");

    let rows = versions(&pg, "receiving-versioned").await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].version, 1);
    assert_eq!(rows[0].status, "published");
    assert_eq!(
        rows[0].action_text.as_deref(),
        Some("verify_stock"),
        "the published version still returns its original steps"
    );
    assert_eq!(rows[1].version, 2);
    assert_eq!(rows[1].status, "draft");
    assert_eq!(rows[1].action_text.as_deref(), Some("verify_stock_again"));
    assert_ne!(rows[0].id, rows[1].id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_published_procedure_keeps_its_source_and_pack() {
    let _env = ENV_LOCK.lock().await;
    let (pg, running) = fixture().await;
    let stub = support::Stub::start().await;
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);
    create_secret(&running).await;
    let (base, draft_content) = stub_inference().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &base);
    draft_content
        .lock()
        .await
        .clone_from(&draft_with("verify_stock"));

    let store_one = "0199c3c6-0000-7000-8000-000000000001";
    let (status, first) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-packed", "receiving-packed-v1", Some(store_one)),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{first}");
    publish_derived(
        &running,
        field(&first, "process_id"),
        field(&first, "procedure_id"),
        field(&first, "source_asset_id"),
    )
    .await;

    // A new source version, and a different strata store, produce new references.
    let store_two = "0199c3c6-0000-7000-8000-000000000002";
    let (status, second) = running
        .invoke(
            "procedures.ingest_sop",
            TOKEN,
            sop_input("receiving-packed", "receiving-packed-v2", Some(store_two)),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second}");

    let rows = versions(&pg, "receiving-packed").await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    // The published row is untouched: its original source and its original pack.
    assert_eq!(
        rows[0].source_asset_id.as_deref(),
        Some(field(&first, "source_asset_id"))
    );
    assert_eq!(
        rows[0].knowledge_pack_ref,
        field(&first, "knowledge_pack_ref")
    );
    assert!(rows[0].knowledge_pack_ref.starts_with(store_one));
    // The new draft names the new ones.
    assert_eq!(
        rows[1].source_asset_id.as_deref(),
        Some(field(&second, "source_asset_id"))
    );
    assert_eq!(
        rows[1].knowledge_pack_ref,
        field(&second, "knowledge_pack_ref")
    );
    assert!(rows[1].knowledge_pack_ref.starts_with(store_two));
    assert_ne!(rows[0].knowledge_pack_ref, rows[1].knowledge_pack_ref);
}
