//! One hung handler must not stop the other jobs (#421).
//!
//! A real `conveyor worker` child process, a real HTTP stub whose FIRST `/licenses/active`
//! request hangs for a minute, and two `licence.sync` jobs. The tick used to await each handler
//! inline, so once the first job's call blocked, the poll loop stopped claiming and the second
//! job never ran — measured 2026-11-19 with a failed run's coach run holding the only worker.
//! The worker now spawns each claimed job (bounded by `MAX_IN_FLIGHT_JOBS`), so the second job
//! completes while the first is still in flight.
//!
//! The stub makes the test order-agnostic: whichever job reaches the stub first hangs, and the
//! assertion is that exactly one job succeeds while the other is still `running`.

mod common;

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use conveyor_domain::TenantId;
use tokio::net::TcpListener;
use uuid::Uuid;

/// A child worker that is killed on drop, so a failed assertion cannot leave a poller running
/// against the test database.
struct WorkerChild {
    child: Child,
}

impl WorkerChild {
    fn spawn(db: &common::TestDb, base: &str) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_conveyor"))
            .arg("worker")
            .env("APP_ENV", "test")
            .env("DATABASE_URL", &db.runtime_url)
            .env("DATABASE_LISTEN_URL", &db.runtime_url)
            .env("DATABASE_MIGRATE_URL", &db.runtime_url)
            .env("LICENCES_BASE_URL", base)
            .env("APP_KEY", "conveyor")
            .env("JOB_LEASE_SECS", "2")
            .env("RUST_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the conveyor worker binary");
        Self { child }
    }
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A stub whose FIRST `/licenses/active` request hangs for a minute; later requests answer
/// immediately. The hung request keeps its handler in flight for the whole test window.
async fn slow_first_stub() -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let app = Router::new().route(
        "/licenses/active",
        get(move || {
            let calls = counter.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                println!("stub received licence.sync request #{}", n + 1);
                if n == 0 {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
                (
                    StatusCode::OK,
                    r#"{"active":true,"status":"active","expires_at":null}"#,
                )
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("stub server");
    });
    (format!("http://{addr}"), calls)
}

async fn a_tenant(db: &common::TestDb) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('conc-a', 'Concurrency A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn status_of(db: &common::TestDb, id: Uuid) -> String {
    let (status,): (String,) = sqlx::query_as("SELECT status FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the job row");
    status
}

async fn wait_for<F, Fut>(what: &str, timeout: Duration, mut predicate: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if predicate().await {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out after {timeout:?} waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn enqueue(db: &common::TestDb, tenant: TenantId, key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, 'licence.sync', $2, '{}', 5)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .fetch_one(&db.super_pool)
    .await
    .expect("enqueue the job")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_handler_does_not_starve_the_next_job() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let (base, calls) = slow_first_stub().await;

    let first = enqueue(&db, tenant, "conc-first").await;
    let second = enqueue(&db, tenant, "conc-second").await;

    let _worker = WorkerChild::spawn(&db, &base);

    // One job reaches the stub first and hangs; the other must finish while it is still in flight.
    wait_for(
        "one job to succeed while the other is still running",
        Duration::from_secs(20),
        || async {
            let a = status_of(&db, first).await;
            let b = status_of(&db, second).await;
            (a == "succeeded" && b == "running") || (b == "succeeded" && a == "running")
        },
    )
    .await;

    let (a, b) = (status_of(&db, first).await, status_of(&db, second).await);
    println!(
        "first={a} second={b} stub_calls={}",
        calls.load(Ordering::SeqCst)
    );
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "the second handler must have reached the stub"
    );
    assert_eq!(
        [a.as_str(), b.as_str()]
            .iter()
            .filter(|s| **s == "succeeded")
            .count(),
        1,
        "exactly one job completes while the other is hung"
    );
}
