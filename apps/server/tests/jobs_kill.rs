//! The kill test (S02 §7): `JOB_LEASE_SECS=2`, SIGKILL mid-handler, restart reclaims.
//!
//! A real `conveyor worker` child process, a real HTTP stub that holds the first request,
//! `Child::kill()` (SIGKILL on Unix), then a second worker process. Nothing is cleaned up on
//! the first worker's behalf: the row is left `running` with a live-then-expired lease,
//! exactly as a SIGKILL leaves it.
//!
//! Asserted, in order:
//! 1. after the kill the row is still `running` with `attempts = 1` and the same token;
//! 2. after the lease expires the restarted worker reclaims it: `attempts` becomes 2;
//! 3. the stale token writes nothing — `finish_succeeded` and `fail` both change 0 rows;
//! 4. the reclaimed attempt completes (`status = 'succeeded'`) and writes `app_licence`,
//!    which proves the handler itself ran, not just the lease dance.

mod common;

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use conveyor_domain::TenantId;
use conveyor_store::jobs;
use tokio::net::TcpListener;
use uuid::Uuid;

/// A child worker that is killed on drop, so a failed assertion cannot leave a poller
/// running against the test database.
struct WorkerChild {
    child: Child,
    pid: u32,
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
        let pid = child.id();
        println!("spawned worker pid={pid} base={base}");
        Self { child, pid }
    }

    fn kill_sigkill(&mut self) {
        self.child.kill().expect("SIGKILL the worker");
        let status = self.child.wait().expect("reap the worker");
        println!("worker pid={} killed: {status:?}", self.pid);
    }
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A stub whose FIRST `/licenses/active` request hangs for a minute (so the worker is
/// killed mid-handler) and whose later requests answer immediately.
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
        "INSERT INTO tenant (slug, name) VALUES ('kill-a', 'Kill A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn state_of(db: &common::TestDb, id: Uuid) -> (String, i32, Option<Uuid>) {
    sqlx::query_as("SELECT status, attempts, lease_token FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the job row")
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
async fn a_killed_worker_is_reclaimed_and_its_stale_token_writes_nothing() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let (base, calls) = slow_first_stub().await;

    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, 'licence.sync', 'kill-test', '{}', 5)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("enqueue the job");

    // ── worker 1: claim and block inside the handler, then SIGKILL ─────────────────────
    let mut first = WorkerChild::spawn(&db, &base);
    wait_for(
        "worker 1 to call the licence stub",
        Duration::from_secs(10),
        || async { calls.load(Ordering::SeqCst) >= 1 },
    )
    .await;
    wait_for(
        "worker 1 to claim the job",
        Duration::from_secs(10),
        || async { state_of(&db, job_id).await.0 == "running" },
    )
    .await;
    let (status, attempts, stale_token) = state_of(&db, job_id).await;
    let stale_token = stale_token.expect("a claimed job carries a token");
    println!("before kill: status={status} attempts={attempts} token={stale_token:?}");
    assert_eq!(status, "running");
    assert_eq!(attempts, 1);

    first.kill_sigkill();
    // Nothing cleaned up: the row is still the killed worker's.
    let (status, attempts, token) = state_of(&db, job_id).await;
    println!("after kill: status={status} attempts={attempts} token={token:?}");
    assert_eq!(status, "running");
    assert_eq!(attempts, 1);
    assert_eq!(token, Some(stale_token));

    // ── wait past the 2 s lease, then restart ──────────────────────────────────────────
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let _second = WorkerChild::spawn(&db, &base);
    wait_for(
        "the restarted worker to finish the reclaimed job",
        Duration::from_secs(30),
        || async { state_of(&db, job_id).await.0 == "succeeded" },
    )
    .await;

    let (status, attempts, token) = state_of(&db, job_id).await;
    println!("after restart: status={status} attempts={attempts} token={token:?}");
    assert_eq!(status, "succeeded");
    assert_eq!(attempts, 2, "the reclaim incremented attempts");
    assert_eq!(token, None);

    // ── the killed worker's token can never write ──────────────────────────────────────
    let late_finish = jobs::finish_succeeded(&db.runtime, tenant, job_id, stale_token)
        .await
        .expect("late finish");
    assert!(!late_finish, "a stale token must change nothing");
    let late_fail = jobs::fail(
        &db.runtime,
        tenant,
        job_id,
        stale_token,
        "late failure",
        Duration::from_secs(1),
    )
    .await
    .expect("late fail");
    assert!(
        !late_fail.changed,
        "a stale token must not reschedule the job"
    );
    let (status, attempts, _) = state_of(&db, job_id).await;
    assert_eq!((status.as_str(), attempts), ("succeeded", 2));

    // The handler itself ran on the reclaimed attempt: the local entitlement was written.
    let licence: (String,) = sqlx::query_as(
        "SELECT status FROM app_licence WHERE tenant_id = $1 AND app_key = 'conveyor'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("the licence row written by the reclaimed attempt");
    assert_eq!(licence.0, "active");
}
