//! Shared integration-test harness: an ephemeral Postgres 18 and a fresh database per test.
//!
//! The estate's test-DB standard, as `apps/licenses/tests/common` established it:
//! - `TEST_DATABASE_URL` (a superuser URL) short-circuits; otherwise one throwaway `postgres:18`
//!   container per test binary on an OS-assigned host port.
//! - Every test gets a FRESH database on that server.
//! - A container that cannot start FAILS the test; it never skips.
//! - **Every database this module creates is dropped.** The repo measured 137 leaked databases
//!   from one binary before `Drop` guards became the rule; 2.1 GB of them is what a green gate
//!   looks like from inside.

#![allow(dead_code)]

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};
use tokio::sync::OnceCell;

const IMAGE: &str = "postgres:18";

/// `pid + SystemTime::as_nanos()` is not unique on its own — cargo runs tests on parallel threads
/// and the kernel clock ticks coarsely, so two can read the same nanosecond.
static NEXT_DB: AtomicU64 = AtomicU64::new(0);

struct Server {
    base: String,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();

/// One database per test, dropped when the test ends.
pub struct TestDb {
    pub pool: PgPool,
    pub url: String,
    pub name: String,
    pub server_base: String,
}

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
    let name = format!(
        "res-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
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

/// Remove the container when the test process exits.
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

fn with_database(base: &str, name: &str) -> String {
    let (head, _) = base.rsplit_once('/').expect("a database path");
    format!("{head}/{name}")
}

/// A fresh, empty database on the shared test server.
pub async fn fresh() -> TestDb {
    let server = server().await;
    let admin = PgPool::connect(&server.base)
        .await
        .expect("connect to the shared test postgres server");
    let name = format!(
        "res_t_{}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        NEXT_DB.fetch_add(1, Ordering::Relaxed)
    );
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
        .execute(&admin)
        .await
        .expect("create test database");
    admin.close().await;

    let mut url = with_database(&server.base, &name);
    // No parallel workers: the container's /dev/shm is small and the baseline builds indexes.
    let separator = if url.contains('?') { '&' } else { '?' };
    url.push(separator);
    url.push_str(
        "options=-c%20max_parallel_workers_per_gather%3D0%20-c%20max_parallel_maintenance_workers%3D0%20-c%20max_parallel_workers%3D0",
    );
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect the test database");

    TestDb {
        pool,
        url,
        name,
        server_base: server.base.clone(),
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let base = self.server_base.clone();
        let name = self.name.clone();
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                if let Ok(admin) = PgPool::connect(&base).await {
                    let _ = sqlx::query(AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
                    )))
                    .execute(&admin)
                    .await;
                    admin.close().await;
                }
            });
        })
        .join();
    }
}

// ── the licence-introspection stub ────────────────────────────────────────────────────────────
//
// The media service now verifies every credential at `LICENSE_INTROSPECT_URL`. Tests point it at a
// process-local axum server that answers by TOKEN PREFIX, so a test names the credential shape it
// wants and nothing is mocked inside the binary under test:
//
//   licence-<uuid>   an ACTIVE licence naming that tenant, no subject (the licence path)
//   vouch-…          an ACTIVE licence whose subject may vouch (and whose OWN tenant differs, so
//                    the "the vouch never re-labels" assertion has something to catch)
//   stranger-…       an ACTIVE licence whose subject is NOT on the allowlist
//   anything else    inactive (the negative control every refusal is paired with)

pub const VOUCH_SUBJECT: &str = "0195f48b-d000-7000-8000-0000000000c3";
pub const STRANGER_SUBJECT: &str = "0195f48b-d000-7000-8000-0000000000d4";
pub const VOUCH_TOKEN: &str = "vouch-test-token";
/// The vouch token's licence names this tenant — deliberately NOT the tenant a test names.
pub const LICENCE_OWN_TENANT: &str = "00000000-0000-7000-8000-0000000000b2";

pub fn vouch_auth() -> String {
    format!("Bearer {VOUCH_TOKEN}")
}

/// A config whose gate is satisfied by the stub, for the suites that exercise non-auth behaviour.
pub fn media_config() -> conveyor_research::media::Config {
    conveyor_research::media::Config {
        bind_host: "127.0.0.1".to_string(),
        bind_port: 0,
        enable_docs: false,
        service_name: "media-discovery".to_string(),
        version: "0.1.0".to_string(),
        database_url: None,
        max_connections: 4,
        license_introspect_url: Some(introspect_url().to_string()),
        onbehalf_subjects: vec![VOUCH_SUBJECT.to_string()],
        license_introspect_timeout: Duration::from_secs(5),
    }
}

/// The stub's URL, started once per test binary on first use.
pub fn introspect_url() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("stub runtime");
            runtime.block_on(async move {
                let app = axum::Router::new()
                    .route("/license/introspect", axum::routing::post(stub_introspect));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("stub bind");
                let address = listener.local_addr().expect("stub address");
                ready_tx
                    .send(format!("http://{address}/license/introspect"))
                    .expect("stub ready channel");
                axum::serve(listener, app).await.expect("stub serve");
            });
        });
        ready_rx.recv().expect("the stub starts")
    })
}

async fn stub_introspect(
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let token = body
        .get("token")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let payload = if let Some(tenant) = token.strip_prefix("licence-") {
        serde_json::json!({
            "active": true, "tenant_id": tenant, "subject_id": null, "license_id": "stub"
        })
    } else if token.starts_with("vouch-") {
        serde_json::json!({
            "active": true, "tenant_id": LICENCE_OWN_TENANT, "subject_id": VOUCH_SUBJECT,
            "license_id": "stub"
        })
    } else if token.starts_with("stranger-") {
        serde_json::json!({
            "active": true, "tenant_id": LICENCE_OWN_TENANT, "subject_id": STRANGER_SUBJECT,
            "license_id": "stub"
        })
    } else {
        serde_json::json!({ "active": false })
    };
    axum::Json(payload)
}
