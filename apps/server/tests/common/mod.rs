//! Shared integration-test harness: an ephemeral Postgres 18 and per-test databases.
//!
//! CONTRACT §10 / the estate's test-DB standard:
//! - `TEST_DATABASE_URL` (a SUPERUSER URL) short-circuits; otherwise one throwaway
//!   `postgres:18` container per test binary on an OS-assigned host port.
//! - Every test gets a FRESH database on that server.
//! - A container that cannot start FAILS the test; it never skips.
//! - The compose database is never used, and `DATABASE_URL` is deliberately not consulted.
//!
//! Statics are never dropped, so the container is removed by a detached watcher: it polls
//! `kill -0 <test pid>` and runs `docker rm -f -v <id>` when the test process exits. That is
//! the `strata_rag` atexit hook without `unsafe` (this workspace forbids unsafe code).

#![allow(dead_code)]

use std::process::{Command, Stdio};
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, PgPool};
use tokio::sync::OnceCell;

/// The image a test container runs.
const IMAGE: &str = "postgres:18";

/// `scripts/db-roles.sql`, read from the workspace. It is embedded in the TEST binary only:
/// the dev passwords must not be linked into the service image, so `conveyor-store` does not
/// carry this constant.
const ROLES_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/db-roles.sql"
));

/// The runtime role's password, from `scripts/db-roles.sql` (local development only).
pub const RUNTIME_PASSWORD: &str = "conveyor_runtime_dev";

/// The schema owner's password, from `scripts/db-roles.sql`.
pub const OWNER_PASSWORD: &str = "conveyor_migrate_dev";

/// The admin URL of the shared server (database `postgres`).
struct Server {
    base: String,
}

static SERVER: OnceCell<Server> = OnceCell::const_new();

/// One database setup at a time per PROCESS. The cross-database races are handled inside
/// `scripts/db-roles.sql`: the `pg_authid` role rows and the built-in `ts_match_vq` `pg_proc`
/// row are shared by every database, and PostgreSQL advisory locks are DATABASE-LOCAL (a lock
/// held in `postgres` does not conflict with the same key in another database), so that file
/// retries the loser of an `XX000 tuple concurrently updated` instead. This mutex only
/// serialises the tasks inside one process — cargo's in-binary thread pool — and is not, and
/// never was, a cross-process guard; under nextest every test is its own process.
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

/// Start one throwaway Postgres 18 and return its admin URL.
async fn start_container() -> String {
    let image = image();
    let name = format!("conveyor-test-{}", uuid::Uuid::now_v7().simple());
    // Phase markers on stderr. libtest prints a failing test's captured output, so when
    // this path blocks the last marker names the step. The Docker path is CI-only
    // (`TEST_DATABASE_URL` bypasses it on the build host), which makes a CI-only hang
    // otherwise unreproducible and unbounded.
    eprintln!("harness: docker run {image}");
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
    eprintln!("harness: container up, id={id}");
    // Watch from the moment the container exists, so a later panic still cleans up.
    spawn_watcher(&id);

    eprintln!("harness: resolving the published port");
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
    eprintln!("harness: waiting for postgres on 127.0.0.1:{port} (90s budget)");
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

/// Remove the container when the test process exits. The id is docker-generated hex and the
/// pid is numeric, so the shell script has no interpolated user input.
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

/// Replace the userinfo and database of an admin URL.
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
pub struct TestDb {
    /// Superuser on the test database. The POSITIVE CONTROL: it bypasses RLS even with FORCE.
    pub super_pool: PgPool,
    /// `conveyor_migrate`: owns the schema, still subject to FORCE RLS.
    pub owner: PgPool,
    /// `conveyor_runtime`: NOSUPERUSER NOBYPASSRLS, the role every request runs as.
    pub runtime: PgPool,
    /// The runtime role's URL (an `AppState` uses it).
    pub runtime_url: String,
    /// The test database's name.
    pub name: String,
}

impl TestDb {
    /// Create a database, bootstrap the roles exactly as a developer does, and migrate.
    pub async fn fresh() -> TestDb {
        let _setup = DB_SETUP.lock().await;
        let server = server().await;
        let admin = PgPool::connect(&server.base)
            .await
            .expect("connect to the shared test postgres server");
        let name = format!(
            "conveyor_t_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            uuid::Uuid::now_v7().simple()
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

        // The roles are created by a superuser, never by a migration (S2-D2).
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

        TestDb {
            super_pool,
            owner,
            runtime,
            runtime_url,
            name,
        }
    }
}

/// A small pool that never connects: for routes that must not touch a database.
pub fn lazy_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("lazy pool")
}

pub mod process_fixture;
