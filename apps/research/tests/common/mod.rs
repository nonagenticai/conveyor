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
