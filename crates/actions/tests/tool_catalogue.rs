//! T13 — the catalogue and the ledger reads (SPEC_V2 §11.8; issue #557).
//!
//! Every test runs against a fresh Postgres 18 and a real HTTP destination (the stub below).
//! The catalogue is read through `tools.list`; the ledger is written by a real `tools.call`
//! through a run-scoped binding, so the row the test reads back is the broker's own.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::{begin_scoped, tool};
use serde_json::{Value, json};
use uuid::Uuid;

/// The destination the coding toolkit dispatches to.
const CODING_URL: &str = "CONVEYOR_EXTERNAL_CODING_URL";

struct Stub {
    base: String,
}

async fn answer(uri: axum::http::Uri) -> axum::Json<Value> {
    axum::Json(json!({"stub": true, "path": uri.path()}))
}

impl Stub {
    async fn start() -> Stub {
        let app = axum::Router::new().fallback(axum::routing::any(answer));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the destination stub");
        let addr = listener.local_addr().expect("stub addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Stub {
            base: format!("http://{addr}"),
        }
    }
}

/// A seeded tenant, one running run, the published manifests and the action server.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    run_id: Uuid,
    running: support::Running,
    _stub: Stub,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let stub = Stub::start().await;
        support::set_env(CODING_URL, &stub.base);
        let run_id = seed_run(&pg, seeded.tenant).await;
        let running = support::Running::start(&pg).await;
        let fixture = Fixture {
            pg,
            tenant: seeded.tenant,
            run_id,
            running,
            _stub: stub,
        };
        let (status, body) = fixture.invoke("toolkits.publish", json!({})).await;
        assert_eq!(status, 200, "publish: {body}");
        fixture.bind("coding", "read").await;
        fixture
    }

    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(action, "dev:alice", input, None, None, None)
            .await
    }

    /// Bind the run to one published toolkit, as a job binding would.
    async fn bind(&self, toolkit_key: &str, ceiling: &str) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        let toolkit_id: Uuid =
            sqlx::query_scalar("SELECT id FROM toolkit WHERE tenant_id = $1 AND key = $2")
                .bind(self.tenant.as_uuid())
                .bind(toolkit_key)
                .fetch_one(&mut *tx)
                .await
                .expect("the published toolkit");
        sqlx::query(
            "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
             VALUES ($1, $2, $3, $4, true)",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.run_id)
        .bind(toolkit_id)
        .bind(ceiling)
        .execute(&mut *tx)
        .await
        .expect("insert the binding");
        tx.commit().await.expect("commit the binding");
    }

    async fn call(&self, name: &str, input: Value) -> (u16, Value) {
        self.invoke(
            "tools.call",
            json!({"run_id": self.run_id, "tool": name, "input": input}),
        )
        .await
    }
}

async fn seed_run(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("T13-{}", Uuid::now_v7().simple()))
    .bind("Tool catalogue proof")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the fixture");
    run_id
}

#[tokio::test]
async fn the_catalogue_reads_only_published_and_pages() {
    let fixture = Fixture::new().await;

    // A draft toolkit version must never reach the catalogue.
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped write");
    let coding = tool::latest_toolkit(&mut tx, fixture.tenant, "coding")
        .await
        .expect("read the toolkit")
        .expect("the toolkit is published");
    tool::set_toolkit_status(&mut tx, fixture.tenant, coding.id, "draft")
        .await
        .expect("draft the toolkit");
    tx.commit().await.expect("commit the draft");

    let (status, first) = fixture
        .invoke("tools.list", json!({"page": {"limit": 2}}))
        .await;
    assert_eq!(status, 200, "tools.list: {first}");
    let first_rows = first["output"]["items"]
        .as_array()
        .expect("the first page")
        .clone();
    assert_eq!(first_rows.len(), 2, "{first}");
    let cursor = first["output"]["next_cursor"]
        .as_str()
        .expect("more than one page of published tools");
    let (status, second) = fixture
        .invoke(
            "tools.list",
            json!({"page": {"limit": 2, "cursor": cursor}}),
        )
        .await;
    assert_eq!(status, 200, "tools.list page 2: {second}");
    let second_rows = second["output"]["items"]
        .as_array()
        .expect("the second page")
        .clone();
    assert!(!second_rows.is_empty(), "{second}");
    let mut seen = std::collections::HashSet::new();
    for row in first_rows.iter().chain(second_rows.iter()) {
        assert_ne!(
            row["toolkit"], "coding",
            "a draft toolkit is not published: {row}"
        );
        assert_ne!(row["status"], "draft", "{row}");
        let key = format!(
            "{}@{}:{}",
            row["toolkit"].as_str().unwrap_or_default(),
            row["toolkit_version"],
            row["tool"].as_str().unwrap_or_default()
        );
        assert!(seen.insert(key), "the cursor repeated a row: {row}");
    }
}

#[tokio::test]
async fn the_ledger_reads_back_one_call() {
    let fixture = Fixture::new().await;
    let (status, body) = fixture
        .call("coding.jobs_get", json!({"id": "job-1"}))
        .await;
    assert_eq!(status, 200, "tools.call: {body}");

    let (status, page) = fixture
        .invoke(
            "tool_calls.list",
            json!({"run_id": fixture.run_id, "page": {"limit": 10}}),
        )
        .await;
    assert_eq!(status, 200, "tool_calls.list: {page}");
    let rows = page["output"]["items"].as_array().expect("the ledger page");
    assert_eq!(rows.len(), 1, "one call is one ledger row: {page}");
    let row = &rows[0];
    assert_eq!(row["tool"], "coding.jobs_get", "{row}");
    assert_eq!(row["toolkit"], "coding", "{row}");
    assert_eq!(row["status"], "ok", "{row}");
    assert_eq!(row["run_id"], fixture.run_id.to_string(), "{row}");
    assert!(
        row["latency_ms"].as_i64().unwrap_or(-1) >= 0,
        "the broker records the call's latency: {row}"
    );
    assert!(
        row["tool_id"].is_string(),
        "the row pins the exact tool version: {row}"
    );
}
