#![allow(dead_code)]
//! Shared integration-test harness: an ephemeral Postgres 18, a real HTTP server and a real
//! external stub.
//!
//! CONTRACT §10: tests start their **own** `postgres:18` container on an OS-assigned port
//! and remove it at teardown; the compose database is never used, and a container that
//! cannot start FAILS the test. `TEST_DATABASE_URL` (a SUPERUSER URL) short-circuits the
//! container.
//!
//! Every test gets a fresh database, the estate's two roles (`conveyor_migrate` owning the
//! schema, `conveyor_runtime` NOSUPERUSER NOBYPASSRLS), the embedded migrations, and the
//! dev principals `dev-a`/`alice` and `dev-b`/`bob` **with active app_licence rows** —
//! because the invocation route is licence-gated (S04 decision 9).

pub mod redis;

use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::routing::{any, post};
use conveyor_api::{AppState, Config};
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
use tokio::sync::OnceCell;
use uuid::Uuid;

const IMAGE: &str = "postgres:18";

const ROLES_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/db-roles.sql"
));

pub const RUNTIME_PASSWORD: &str = "conveyor_runtime_dev";
pub const OWNER_PASSWORD: &str = "conveyor_migrate_dev";

struct Server {
    base: String,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();
static DB_SETUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn server() -> &'static Server {
    SERVER
        .get_or_init(|| async {
            if let Ok(url) = std::env::var("TEST_DATABASE_URL")
                && !url.trim().is_empty()
            {
                return Server { base: url };
            }
            Server {
                base: start_container().await,
            }
        })
        .await
}

fn image() -> String {
    match std::env::var("TEST_PG_REGISTRY") {
        Ok(registry) if !registry.trim().is_empty() => {
            format!("{}/postgres:18", registry.trim_end_matches('/'))
        }
        _ => IMAGE.to_owned(),
    }
}

async fn start_container() -> String {
    let image = image();
    let name = format!("conveyor-actions-test-{}", Uuid::now_v7().simple());
    let output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            &name,
            "--tmpfs",
            "/var/lib/postgresql",
            "-e",
            "POSTGRES_PASSWORD=postgres",
            "-p",
            "127.0.0.1::5432",
            &image,
            "-c",
            "fsync=off",
        ])
        .output()
        .expect("no TEST_DATABASE_URL and `docker` cannot run: integration tests fail closed");
    assert!(
        output.status.success(),
        "docker run {image} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    spawn_watcher(&id);

    let port_output = Command::new("docker")
        .args(["port", &id, "5432/tcp"])
        .output()
        .expect("docker port");
    let mapping = String::from_utf8_lossy(&port_output.stdout).into_owned();
    let port = mapping
        .lines()
        .next()
        .and_then(|line| line.rsplit(':').next())
        .map(str::trim)
        .filter(|port| !port.is_empty())
        .unwrap_or_else(|| {
            remove(&name);
            panic!("no published port for {id}: {mapping:?}")
        })
        .to_owned();

    let base = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        match PgPool::connect(&base).await {
            Ok(pool) => {
                pool.close().await;
                return base;
            }
            Err(error) if std::time::Instant::now() > deadline => {
                remove(&name);
                panic!("postgres never became ready on port {port}: {error}");
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
}

fn remove(container: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", "-v", container])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn spawn_watcher(container: &str) {
    let pid = std::process::id();
    let script = format!(
        "i=0; while kill -0 {pid} 2>/dev/null && [ $i -lt 21600 ]; do sleep 1; i=$((i+1)); done; \
         docker rm -f -v {container} >/dev/null 2>&1"
    );
    let _ = Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn with_credentials(base: &str, user: &str, password: &str, database: &str) -> String {
    let after_scheme = base.split_once("://").map(|(_, rest)| rest).unwrap_or(base);
    let host_part = after_scheme
        .split_once('@')
        .map(|(_, host)| host)
        .unwrap_or(after_scheme);
    let host = host_part.split('/').next().unwrap_or(host_part);
    // ⛔ No parallel workers on test connections. Under nextest every test is its own
    // process, and PostgreSQL allocates DSM segments from the container's /dev/shm for
    // parallel query AND for parallel index builds (migrations do CREATE INDEX). Docker's
    // default shm is 64 MB; a few concurrent processes exhaust it and the server returns
    // `53100 could not resize shared memory segment ... No space left on device`
    // (measured 2026-11-11, run 42995623647, at 1049/1229 tests). The archetype now also
    // gives the container 1 GiB of shm; this is the belt to that braces, and these are
    // correctness tests — none needs a parallel plan.
    format!(
        "postgres://{user}:{password}@{host}/{database}?options=-c%20max_parallel_workers_per_gather%3D0%20-c%20max_parallel_maintenance_workers%3D0%20-c%20max_parallel_workers%3D0"
    )
}

/// A fresh migrated database with one pool per role.
pub struct Pg {
    pub super_pool: PgPool,
    pub owner: PgPool,
    pub runtime: PgPool,
    pub runtime_url: String,
    pub owner_url: String,
    pub name: String,
}

impl Pg {
    pub async fn fresh() -> Pg {
        let _setup = DB_SETUP.lock().await;
        init_s09_env();
        let server = server().await;
        let admin = PgPool::connect(&server.base)
            .await
            .expect("connect to the shared test postgres server");
        let name = format!(
            "conveyor_a_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            Uuid::now_v7().simple()
        );
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
            .execute(&admin)
            .await
            .expect("create test database");
        admin.close().await;

        let super_url = with_credentials(&server.base, "postgres", "postgres", &name);
        let super_pool = PgPool::connect(&super_url)
            .await
            .expect("connect the test database as superuser");
        sqlx::raw_sql(ROLES_SQL)
            .execute(&super_pool)
            .await
            .expect("scripts/db-roles.sql as superuser");

        let owner_url = with_credentials(&server.base, "conveyor_migrate", OWNER_PASSWORD, &name);
        let owner = PgPool::connect(&owner_url)
            .await
            .expect("connect as conveyor_migrate");
        conveyor_store::migrate(&owner)
            .await
            .expect("migrate as the schema owner");

        let runtime_url =
            with_credentials(&server.base, "conveyor_runtime", RUNTIME_PASSWORD, &name);
        let runtime = PgPool::connect(&runtime_url)
            .await
            .expect("connect as conveyor_runtime");

        Pg {
            super_pool,
            owner,
            runtime,
            runtime_url,
            owner_url,
            name,
        }
    }
}

/// A seeded dev principal.
pub struct Seeded {
    pub tenant: TenantId,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
}

/// Seed `dev-a`/`alice` (and optionally another tenant) exactly as `migrate --dev-seed`
/// does, including the active entitlement the licence gate requires.
pub async fn seed(pg: &Pg, slug: &str, login: &str, name: &str) -> Seeded {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ($1, $2)
         ON CONFLICT (slug) DO UPDATE SET name = excluded.name
         RETURNING id",
    )
    .bind(slug)
    .bind(name)
    .fetch_one(&pg.owner)
    .await
    .expect("insert tenant");
    let subject = format!("dev:{login}");
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name) VALUES ($1, $2, $3)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(&subject)
    .bind(format!("{login}@dev.invalid"))
    .bind(name)
    .fetch_one(&pg.owner)
    .await
    .expect("insert user");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("insert membership");
    sqlx::query(
        "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
         VALUES ($1, 'conveyor', 'active', now())
         ON CONFLICT (tenant_id, app_key) DO UPDATE
             SET status = 'active', checked_at = now()",
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .expect("insert licence");
    tx.commit().await.expect("commit seed");
    // SPEC D27: a resolved executor and the copilot read the tenant's prompt catalogue, so the
    // shared fixture seeds the built-ins exactly as `seed::dev` does.
    conveyor_store::seed::prompts_for_tenant(&pg.owner, tenant_id)
        .await
        .expect("seed the prompt catalogue");
    Seeded {
        tenant,
        tenant_id,
        user_id,
    }
}

/// Create and publish a checker-inspected SOP, returning its id (S13: every step names one).
///
/// `processes.publish` refuses a step with no `procedure_id`, so a process fixture must have a
/// published SOP in the same tenant to point its steps at. The `non_empty` rule accepts any
/// non-empty output, which keeps the SOP out of the way of whatever the test is actually asserting.
pub async fn publish_sop(running: &Running, key: &str) -> String {
    let (status, body) = running
        .invoke(
            "procedures.create",
            "dev:alice",
            json!({
                "key": key,
                "title": key,
                "definition_of_ready": {},
                "definition_of_done": {},
                "acceptance_criteria": ["the output is present"],
                "inspection": {"kind": "checker", "rule": "non_empty"},
                "outputs": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "procedures.create {key}: {body}");
    let id = body["output"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no procedure id in {body}"))
        .to_owned();
    let (status, body) = running
        .invoke(
            "procedures.publish",
            "dev:alice",
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "procedures.publish {key}: {body}");
    id
}

/// The config the served app is built from.
pub fn config(pg: &Pg) -> Config {
    let runtime_url = pg.runtime_url.clone();
    let owner_url = pg.owner_url.clone();
    Config::from_lookup(move |key| match key {
        "DATABASE_URL" => Some(runtime_url.clone()),
        "DATABASE_LISTEN_URL" => Some(runtime_url.clone()),
        "DATABASE_MIGRATE_URL" => Some(owner_url.clone()),
        "CONVEYOR_DEV_AUTH" => Some("1".into()),
        "BIND_ADDR" => Some("127.0.0.1:0".into()),
        "APP_KEY" => Some("conveyor".into()),
        _ => None,
    })
    .expect("test config")
}

/// [`config`] with development auth OFF and a licences service configured, so a request is
/// authenticated by licence introspection (S06b) rather than by a `dev:` bearer.
///
/// The issuer is a placeholder that is never contacted: it exists to satisfy the production
/// configuration rule (`AuthConfig::validate` refuses dev-auth-off without an issuer), and a
/// verifier is built but only ever used for a JWT-shaped token, which these tests never send.
pub fn config_with_licences(pg: &Pg, licences_base_url: &str) -> Config {
    let runtime_url = pg.runtime_url.clone();
    let owner_url = pg.owner_url.clone();
    let licences = licences_base_url.to_owned();
    Config::from_lookup(move |key| match key {
        "DATABASE_URL" | "DATABASE_LISTEN_URL" => Some(runtime_url.clone()),
        "DATABASE_MIGRATE_URL" => Some(owner_url.clone()),
        "CONVEYOR_DEV_AUTH" => Some("0".into()),
        "CONVEYOR_OIDC_ISSUER" => Some("https://issuer.invalid/realms/conveyor".into()),
        "BIND_ADDR" => Some("127.0.0.1:0".into()),
        "APP_KEY" => Some("conveyor".into()),
        "LICENCES_BASE_URL" => Some(licences.clone()),
        _ => None,
    })
    .expect("test config")
}

/// A served app.
pub struct Running {
    pub addr: SocketAddr,
    pub state: AppState,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    pub async fn start(pg: &Pg) -> Running {
        Self::start_with_config(config(pg)).await
    }

    /// [`Running::start`] from a caller-supplied config (the licence-bearer tests).
    pub async fn start_with_config(config: Config) -> Running {
        let state = AppState::connect(config).await.expect("AppState");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let app = conveyor_api::app(state.clone());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Running { addr, state, task }
    }

    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn http(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<Value>,
    ) -> (u16, Value) {
        let (status, _headers, value) = self.http_full(method, path, token, headers, body).await;
        (status, value)
    }

    /// [`Running::http`] plus the response headers (lower-cased names), for `ETag`.
    pub async fn http_full(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<Value>,
    ) -> (u16, std::collections::HashMap<String, String>, Value) {
        let client = reqwest::Client::new();
        let mut request = client
            .request(method, format!("{}{}", self.base(), path))
            .header("accept", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("http request");
        let status = response.status().as_u16();
        let mut response_headers = std::collections::HashMap::new();
        for (name, value) in response.headers() {
            if let Ok(value) = value.to_str() {
                response_headers.insert(name.as_str().to_owned(), value.to_owned());
            }
        }
        let bytes = response.bytes().await.unwrap_or_default();
        if std::env::var("CONVEYOR_TEST_DEBUG").is_ok() {
            eprintln!("HTTP {status} {path}: {}", String::from_utf8_lossy(&bytes));
        }
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, response_headers, value)
    }

    /// [`Running::invoke`] plus the response headers.
    pub async fn invoke_full(
        &self,
        action: &str,
        token: &str,
        input: Value,
        if_match: Option<&str>,
        idempotency: Option<&str>,
        request_id: Option<&str>,
    ) -> (u16, std::collections::HashMap<String, String>, Value) {
        let mut headers: Vec<(&str, &str)> = Vec::new();
        if let Some(version) = if_match {
            headers.push(("if-match", version));
        }
        if let Some(key) = idempotency {
            headers.push(("idempotency-key", key));
        }
        if let Some(id) = request_id {
            headers.push(("x-request-id", id));
        }
        let path = format!("/api/actions/{action}/invocations");
        self.http_full(
            reqwest::Method::POST,
            &path,
            Some(token),
            &headers,
            Some(json!({ "input": input })),
        )
        .await
    }

    pub async fn invoke(
        &self,
        action: &str,
        token: &str,
        input: Value,
        if_match: Option<&str>,
        idempotency: Option<&str>,
        request_id: Option<&str>,
    ) -> (u16, Value) {
        let mut headers: Vec<(&str, &str)> = Vec::new();
        if let Some(version) = if_match {
            headers.push(("if-match", version));
        }
        if let Some(key) = idempotency {
            headers.push(("idempotency-key", key));
        }
        if let Some(id) = request_id {
            headers.push(("x-request-id", id));
        }
        let path = format!("/api/actions/{action}/invocations");
        self.http(
            reqwest::Method::POST,
            &path,
            Some(token),
            &headers,
            Some(json!({ "input": input })),
        )
        .await
    }

    pub async fn shutdown(self) {
        self.task.abort();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// ── the external stub ───────────────────────────────────────────────────────────────────

/// The stub's request counter (proving a breaker trip makes no upstream call).
#[derive(Clone, Default)]
pub struct StubHits(pub Arc<AtomicUsize>);

impl StubHits {
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// A running external stub and its base URL.
pub struct Stub {
    pub base: String,
    pub hits: StubHits,
    task: tokio::task::JoinHandle<()>,
}

impl Stub {
    pub async fn start() -> Stub {
        let hits = StubHits::default();
        let app = Router::new()
            .route("/echo", post(echo))
            .route("/slow", post(slow))
            .route("/fail", post(fail))
            .route("/timeout", post(timeout))
            // `strata_ingest`'s accepted answer, so `documents.ingest`'s real path (202 with a job id) is
            // exercised rather than its refusal path.
            .route("/api/ingest/jobs", post(ingest_accepted))
            // S09's external displays: one catch-all answers every path with the same small document, so a
            // parity sample exercises the real call (timeout, breaker, JSON decode) instead of an unconfigured
            // refusal. The body names the path, which is how a test can tell the services apart.
            .fallback(any(external_answer))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub");
        let addr = listener.local_addr().expect("stub addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Stub {
            base: format!("http://{addr}"),
            hits,
            task,
        }
    }
}

/// The answer every external service's path gets from the stub.
///
/// A JSON object naming the path: an external action passes a service's document through unchanged, so the stub
/// answers the shape the conveyor expects (an object) and lets a test assert which route was called.
async fn external_answer(uri: axum::http::Uri) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "stub": true,
        "path": uri.path(),
    }))
}

/// The job id `documents.ingest`'s samples record.
pub const INGEST_JOB: &str = "0197a0c0-7000-7000-8000-00000000cafe";

/// `strata_ingest`'s accepted answer: `202` with one queued job (S09 §5.6).
///
/// The real body is `{"jobs":[{"job_id":…,"status":"queued"}]}` — deliberately **no** top-level `job_id`, which
/// the upstream only adds for a single JSON `source` body. A stub that answered the easier shape would hide a
/// client bug.
async fn ingest_accepted() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::ACCEPTED,
        axum::Json(serde_json::json!({
            "jobs": [{"job_id": INGEST_JOB, "status": "queued"}]
        })),
    )
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn echo(
    State(_hits): State<StubHits>,
    axum::Json(body): axum::Json<Value>,
) -> axum::Json<Value> {
    axum::Json(json!({
        "message": body.get("message").and_then(Value::as_str).unwrap_or(""),
        "service": "fixtures"
    }))
}

async fn slow(
    State(hits): State<StubHits>,
    axum::Json(body): axum::Json<Value>,
) -> axum::Json<Value> {
    hits.0.fetch_add(1, Ordering::SeqCst);
    let delay = body
        .get("delay_ms")
        .and_then(Value::as_u64)
        .unwrap_or(600)
        .min(5_000);
    tokio::time::sleep(Duration::from_millis(delay)).await;
    axum::Json(json!({ "waited_ms": delay }))
}

async fn fail(
    State(hits): State<StubHits>,
    axum::Json(body): axum::Json<Value>,
) -> axum::response::Response<axum::body::Body> {
    hits.0.fetch_add(1, Ordering::SeqCst);
    let status = body
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .filter(|status| (400..500).contains(status))
        .unwrap_or(422);
    axum::response::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({"error": "refused"}).to_string(),
        ))
        .expect("stub response")
}

async fn timeout(State(hits): State<StubHits>) -> axum::response::Response<axum::body::Body> {
    hits.0.fetch_add(1, Ordering::SeqCst);
    axum::response::Response::builder()
        .status(504)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json!({"status": 504}).to_string()))
        .expect("stub response")
}

// ── a minimal MCP client ───────────────────────────────────────────────────────────────

/// A JSON-RPC client for `/mcp`.
pub struct Mcp {
    client: reqwest::Client,
    base: String,
    token: String,
    session: String,
    next_id: u64,
    /// The `initialize` result, kept so a protocol test can assert what the server advertised.
    pub initialize: Value,
}

impl Mcp {
    /// `initialize` (which mints the session) and `notifications/initialized`.
    pub async fn connect(running: &Running, token: &str) -> Mcp {
        Self::connect_with_capabilities(running, token, json!({})).await
    }

    /// [`Self::connect`] with the client capabilities a protocol test declares — S06c's task
    /// tests must declare `io.modelcontextprotocol/tasks` at `initialize`, not per request:
    /// rmcp refuses a task result to a client that did not declare the extension.
    pub async fn connect_with_capabilities(
        running: &Running,
        token: &str,
        capabilities: Value,
    ) -> Mcp {
        let client = reqwest::Client::new();
        let base = running.base();
        let response = client
            .post(format!("{base}/mcp"))
            .header("authorization", format!("Bearer {token}"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": capabilities,
                    "clientInfo": {"name": "conveyor-test", "version": "1"}
                }
            }))
            .send()
            .await
            .expect("mcp initialize");
        assert!(
            response.status().is_success(),
            "initialize failed: {}",
            response.status()
        );
        let session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .expect("the server mints a session id");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let bytes = response.bytes().await.unwrap_or_default();
        // `json_response = true` on this service, but the transport may still answer the
        // handshake as one SSE frame; accept either rather than deciding from the status.
        let initialize: Value = if content_type.starts_with("text/event-stream") {
            String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
                .find(|value| value.get("id").is_some())
                .unwrap_or(Value::Null)
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        let client_request = client
            .post(format!("{base}/mcp"))
            .header("authorization", format!("Bearer {token}"))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("mcp-session-id", &session)
            .json(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .send()
            .await
            .expect("initialized notification");
        assert!(client_request.status().is_success());
        Mcp {
            client,
            base,
            token: token.to_owned(),
            session,
            next_id: 2,
            initialize,
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.request_with_key(method, params, None).await
    }

    /// The `Idempotency-Key` rides the HTTP request, exactly as it does over
    /// `POST /api/actions/...`; rmcp drops a client `_meta` member.
    async fn request_with_key(
        &mut self,
        method: &str,
        params: Value,
        idempotency: Option<&str>,
    ) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .header("mcp-session-id", &self.session);
        if let Some(key) = idempotency {
            request = request.header("idempotency-key", key);
        }
        let response = request
            .json(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .send()
            .await
            .unwrap_or_else(|error| panic!("mcp {method}: {error}"));
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let bytes = response.bytes().await.unwrap_or_default();
        assert!(
            status.is_success(),
            "mcp {method} HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
        );
        if content_type.starts_with("text/event-stream") {
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data:")
                    && let Ok(value) = serde_json::from_str::<Value>(data.trim())
                    && value.get("id").is_some()
                {
                    return value;
                }
            }
            panic!("no JSON-RPC response in the SSE stream: {text}");
        }
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("mcp {method} body is not JSON: {error}"))
    }

    /// The whole JSON-RPC envelope (a protocol error has `error`, not `result`).
    pub async fn raw(&mut self, method: &str, params: Value) -> Value {
        self.request(method, params).await
    }

    pub async fn list_tools(&mut self) -> Value {
        let response = self.request("tools/list", json!({})).await;
        response
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("tools/list failed: {response}"))
    }

    pub async fn call(&mut self, tool: &str, arguments: Value, idempotency: Option<&str>) -> Value {
        let params = json!({"name": tool, "arguments": arguments});
        let response = self
            .request_with_key("tools/call", params, idempotency)
            .await;
        response
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("tools/call {tool} failed: {response}"))
    }
}

/// The fields the parity gate strips: the transport's own, never the action's.
pub fn strip_transport(mut body: Value) -> Value {
    if let Some(object) = body.as_object_mut() {
        object.remove("request_id");
        object.remove("duration_ms");
    }
    body
}

/// The environment variable the fixtures service reads.
pub const STUB_ENV: &str = "CONVEYOR_EXTERNAL_FIXTURES_URL";

/// The test KEK: 32 zero bytes, base64. Not a secret — it exists so the sealing path runs in tests.
pub const TEST_KEK: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// Configure the two S09 services a test process needs: a KEK for sealing and the in-memory object backend.
///
/// Called from [`Pg::fresh`], i.e. before any action runs, because `conveyor_store::secret::sealer` caches the
/// first key it reads for the life of the process (a key that changed mid-test would make one request unable
/// to open what another sealed). The object backend is `object_store`'s in-memory one — the same trait and the
/// same key scheme as S3, without requiring a MinIO image this box cannot pull.
pub fn init_s09_env() {
    set_env("CONVEYOR_SECRET_KEK", TEST_KEK);
    // The webhook recorder tests deliver to a loopback server, and the deployed address policy
    // refuses loopback. Declare this process a test; the refusal itself is proved by the unit test
    // in `crates/actions/src/board/webhooks.rs`, which runs without this variable.
    set_env("CONVEYOR_WEBHOOK_ALLOW_LOOPBACK", "1");
    if std::env::var("CONVEYOR_S3_ENDPOINT").is_err() {
        set_env("CONVEYOR_S3_BACKEND", "memory");
    }
}

/// Set an environment variable for THIS test process.
///
/// Edition 2024 made `set_var` unsafe (other threads may read the environment). The test
/// binary is single-threaded until the runtime starts, and every test file sets this before
/// any invocation; the wrapper keeps the `unsafe` out of the library, which forbids it.
pub fn set_env(key: &str, value: &str) {
    // SAFETY: called before the first action is invoked and while no other thread is
    // reading the environment in this test binary. The value is process-wide by design:
    // the engine reads the same key for every action of the `fixtures` service.
    unsafe { std::env::set_var(key, value) };
}
