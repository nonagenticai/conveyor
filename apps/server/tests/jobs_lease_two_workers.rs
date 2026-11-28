//! The two-worker lease proof for issue #2 P2 (#526 U5b).
//!
//! Two real `conveyor worker` child processes poll the same tenant while a real HTTP stub holds
//! one `licence.sync` handler longer than `JOB_LEASE_SECS`. The claim sets a 2 s lease and the
//! holder renews it at a third of the lease; the proof is that the row is never reclaimed while
//! the handler is in flight, so the handler runs exactly once.
//!
//! - **P2.1** the handler runs once: the stub call counter is 1 and the row keeps `attempts = 1`.
//! - **P2.2** the finish is `succeeded` with `attempts = 1`, and the lease is cleared and stays
//!   cleared.
//! - **P2.3** while the lease is alive the second worker's claim returns nothing.
//!
//! P2.3 is proved by the claim's own write, not by a log line: every claim increments `attempts`,
//! so a second claim would move it to 2 and start a second handler whose request would reach the
//! stub. The assertions read the row and the stub counter.
//!
//! `worker_concurrency.rs` proves a different property (a hung handler must not starve the next
//! job); this binary is the lease/renewal proof issue #2 P2 names.

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

/// The lease both workers run with; the handler outlives it several times over.
const LEASE_SECS: u64 = 2;

/// How long the first `/licenses/active` request hangs: four leases, so a lease that is not
/// renewed is reclaimable for three full poll ticks of the second worker.
const HANG: Duration = Duration::from_secs(8);

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
            .env("JOB_LEASE_SECS", LEASE_SECS.to_string())
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

/// A stub whose FIRST `/licenses/active` request hangs for `hang`; later requests answer
/// immediately. The hang keeps the handler in flight across several lease periods.
async fn slow_first_stub(hang: Duration) -> (String, Arc<AtomicUsize>) {
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
                    tokio::time::sleep(hang).await;
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
        "INSERT INTO tenant (slug, name) VALUES ('lease-two', 'Lease two workers') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn enqueue(db: &common::TestDb, tenant: TenantId) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, 'licence.sync', 'lease-two-workers', '{}', 5)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("enqueue the job")
}

async fn row_state(db: &common::TestDb, id: Uuid) -> (String, i32) {
    sqlx::query_as("SELECT status, attempts FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the job row")
}

async fn lease_is_live(db: &common::TestDb, id: Uuid) -> bool {
    let (live,): (bool,) = sqlx::query_as("SELECT lease_until > now() FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the lease");
    live
}

async fn lease_is_clear(db: &common::TestDb, id: Uuid) -> bool {
    let (clear,): (bool,) =
        sqlx::query_as("SELECT lease_until IS NULL AND lease_token IS NULL FROM job WHERE id = $1")
            .bind(id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the lease pair");
    clear
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

#[tokio::test(flavor = "multi_thread")]
async fn a_long_handler_is_renewed_and_runs_once_under_two_workers() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let (base, calls) = slow_first_stub(HANG).await;
    let job = enqueue(&db, tenant).await;

    let _first = WorkerChild::spawn(&db, &base);
    let _second = WorkerChild::spawn(&db, &base);

    wait_for("the job to be claimed", Duration::from_secs(15), || async {
        row_state(&db, job).await.0 == "running"
    })
    .await;

    let (status, attempts) = row_state(&db, job).await;
    assert_eq!(status, "running");
    assert_eq!(attempts, 1, "the first claim is attempt 1");

    // Outlive the 2 s lease by two full lease periods while the handler is still in flight. An
    // unrenewed lease is reclaimable at t=2 s; the second worker ticks every second, so it
    // would claim the row and a second handler would reach the stub.
    tokio::time::sleep(Duration::from_secs(2 * LEASE_SECS)).await;

    let (status, attempts) = row_state(&db, job).await;
    assert_eq!(status, "running", "the handler is still in flight");
    assert_eq!(attempts, 1, "P2.1/P2.3: the row was never reclaimed");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "P2.3: only one handler reached the stub"
    );
    assert!(
        lease_is_live(&db, job).await,
        "the holder renewed the lease past its original expiry"
    );

    wait_for("the handler to finish", Duration::from_secs(30), || async {
        row_state(&db, job).await.0 == "succeeded"
    })
    .await;

    let (status, attempts) = row_state(&db, job).await;
    assert_eq!(status, "succeeded");
    assert_eq!(attempts, 1, "P2.2: exactly one attempt");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "P2.1: the handler ran exactly once"
    );
    assert!(
        lease_is_clear(&db, job).await,
        "P2.2: the finish clears the lease"
    );

    // A renewal task that outlived the finish must not resurrect the lease.
    tokio::time::sleep(Duration::from_secs(LEASE_SECS)).await;
    assert!(
        lease_is_clear(&db, job).await,
        "P2.2: no renewal after the finish"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
