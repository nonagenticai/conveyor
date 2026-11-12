//! S06-1 — the run lifecycle over real actions, a real Postgres 18 and a real worker child.
//!
//! `cargo test -p conveyor-server --test run_retry`.
//!
//! Properties (S06 §7 c/d):
//! * `runs.create` is refused `409` while the card already has a live run, and the partial
//!   unique index is what makes that true even under a race;
//! * the worker's `run.execute` start hook moves `queued → running` exactly once and writes
//!   exactly one `run.started`;
//! * a dead driving job is reaped to `failed`/`crashed` with one `run.failed`, and the next
//!   `runs.create` for that card is `attempt = 2` while still one live run;
//! * with the andon cord pulled, `runs.create` is `409 andon-engaged` and a queued
//!   `run.execute` job stays `pending` until the cord is released.
//!
//! The action calls go through `conveyor_actions::invoke` — the same transaction, validator
//! and envelope the HTTP route uses — not through a handler directly. The worker is the real
//! `conveyor worker` binary (the pattern `jobs_kill.rs` established), so the andon pause is
//! proven on the poll loop that ships, not on a test-only tick.

mod common;

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use conveyor_actions::InvokeRequest;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// One seeded board with one card, and the tenant whose actions run against it.
struct Fixture {
    db: common::TestDb,
    tenant: TenantId,
    user_id: Uuid,
    project_id: Uuid,
    card_id: Uuid,
}

impl Fixture {
    async fn new() -> Self {
        let db = common::TestDb::fresh().await;
        // `seed::catalogue` seeds the two dev slugs, so the tenant is created with one of
        // them and everything the board surface needs (project_type, card_type, template)
        // arrives with it.
        let tenant_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tenant (slug, name) VALUES ('dev-a', 'Dev A') RETURNING id",
        )
        .fetch_one(&db.super_pool)
        .await
        .expect("insert the tenant");
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO app_user (oidc_subject, email, display_name)
             VALUES ('dev:alice', 'alice@dev.invalid', 'Alice') RETURNING id",
        )
        .fetch_one(&db.super_pool)
        .await
        .expect("insert the user");
        sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
            .bind(tenant_id)
            .bind(user_id)
            .execute(&db.super_pool)
            .await
            .expect("insert the membership");
        let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
        store::seed::catalogue(&db.owner)
            .await
            .expect("the catalogue seed");
        // The worker's maintenance rows (`card.lease_sweep`, `run.reap`, `approval.expire`,
        // the four pre-existing kinds) — the same seed `conveyor migrate` runs. A test then
        // makes one due instead of waiting for the next minute boundary.
        store::schedule::seed_defaults(&db.runtime)
            .await
            .expect("the schedule seed");

        let mut fixture = Self {
            tenant,
            user_id,
            project_id: Uuid::nil(),
            card_id: Uuid::nil(),
            db,
        };

        let (status, _, created) = fixture
            .call("projects.create", json!({"title": "Run retry"}))
            .await;
        assert_eq!(status, 201, "projects.create: {created}");
        fixture.project_id = id_of(&created["output"]["id"]);

        let (status, _, view) = fixture
            .call("projects.get", json!({"id": fixture.project_id}))
            .await;
        assert_eq!(status, 200, "projects.get: {view}");
        let board_id = id_of(&view["output"]["boards"][0]["id"]);

        let (status, _, board) = fixture.call("boards.get", json!({"id": board_id})).await;
        assert_eq!(status, 200, "boards.get: {board}");
        let list_id = board["output"]["lists"]
            .as_array()
            .expect("lists")
            .iter()
            .find(|row| row["list"]["kind"] == "active")
            .map(|row| id_of(&row["list"]["id"]))
            .expect("an active lane");

        let (status, _, card) = fixture
            .call(
                "cards.create",
                json!({
                    "board_id": board_id,
                    "list_id": list_id,
                    "name": "Retry me",
                    "card_type_code": "feature",
                }),
            )
            .await;
        assert_eq!(status, 201, "cards.create: {card}");
        fixture.card_id = id_of(&card["output"]["id"]);
        fixture
    }

    /// Invoke one action as the seeded owner and return `(status, etag, body)`.
    async fn call(&self, action: &str, input: Value) -> (u16, Option<String>, Value) {
        let request = InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            actor_id: self.user_id,
            actor_kind: "user".to_owned(),
            role: Role::Admin,
            policy: Default::default(),
            authority: conveyor_actions::Authority::Role,
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        };
        match conveyor_actions::invoke(
            &self.db.runtime,
            conveyor_actions::registry(),
            self.tenant,
            request,
        )
        .await
        {
            Ok(outcome) => (outcome.status, outcome.etag, outcome.body),
            Err(error) => (error.status, None, error.problem(None, None)),
        }
    }
}

/// A real `conveyor worker` child, killed on drop.
struct WorkerChild {
    child: Child,
}

impl WorkerChild {
    fn spawn(db: &common::TestDb) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_conveyor"))
            .arg("worker")
            .env("APP_ENV", "test")
            .env("DATABASE_URL", &db.runtime_url)
            .env("DATABASE_LISTEN_URL", &db.runtime_url)
            .env("DATABASE_MIGRATE_URL", &db.runtime_url)
            .env("APP_KEY", "conveyor")
            .env("JOB_LEASE_SECS", "2")
            .env("RUST_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the conveyor worker binary");
        println!("spawned worker pid={}", child.id());
        Self { child }
    }
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn id_of(value: &Value) -> Uuid {
    value
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("not a uuid: {value}"))
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

/// `(status, attempt, error_class, started_at_is_set)`.
async fn run_row(db: &common::TestDb, run_id: Uuid) -> (String, i32, Option<String>, bool) {
    sqlx::query_as(
        "SELECT status, attempt, error_class, started_at IS NOT NULL FROM run WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("the run row")
}

async fn job_row(db: &common::TestDb, run_id: Uuid) -> (Uuid, String, i32) {
    sqlx::query_as(
        "SELECT id, status, attempts FROM job
          WHERE kind = 'run.execute' AND dedupe_key = $1
          ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(format!("run:{run_id}"))
    .fetch_one(&db.super_pool)
    .await
    .expect("the run.execute job")
}

async fn event_count(db: &common::TestDb, run_id: Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE type = $1 AND aggregate_id = $2")
        .bind(event_type)
        .bind(run_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("count events")
}

async fn event_payload(db: &common::TestDb, run_id: Uuid, event_type: &str) -> Value {
    sqlx::query_scalar("SELECT payload FROM event WHERE type = $1 AND aggregate_id = $2")
        .bind(event_type)
        .bind(run_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the event payload")
}

/// How many `run` rows the card has, in any status.
async fn run_count(db: &common::TestDb, tenant: TenantId, card_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
        .bind(tenant.as_uuid())
        .bind(card_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the card's runs")
}

async fn job_status_by_id(db: &common::TestDb, job_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the job status")
}

/// Pending jobs of one kind — the observable product of a worker tick.
async fn pending_jobs(db: &common::TestDb, tenant: TenantId, kind: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = $2 AND status = 'pending'",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .fetch_one(&db.super_pool)
    .await
    .expect("count pending jobs")
}

/// Make one seeded schedule due now, so the next worker tick materialises its job.
async fn make_schedule_due(db: &common::TestDb, tenant: TenantId, kind: &str) {
    let rows = sqlx::query(
        "UPDATE schedule SET next_run_at = now() - interval '1 second'
          WHERE tenant_id = $1 AND kind = $2",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .execute(&db.super_pool)
    .await
    .expect("make the schedule due")
    .rows_affected();
    assert_eq!(rows, 1, "the {kind} schedule row is seeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_create_for_a_live_card_is_409_and_one_run_remains() {
    let fixture = Fixture::new().await;

    let (status, etag, body) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        etag.as_deref(),
        Some("\"1\""),
        "a creation carries its ETag"
    );
    let run_id = id_of(&body["output"]["id"]);
    assert_eq!(body["output"]["status"], "queued");
    assert_eq!(body["output"]["attempt"], 1);
    assert_eq!(
        body["output"]["card_id"],
        json!(fixture.card_id.to_string())
    );
    assert_eq!(
        body["output"]["project_id"],
        json!(fixture.project_id.to_string())
    );
    assert_eq!(event_count(&fixture.db, run_id, "run.created").await, 1);
    assert_eq!(
        event_payload(&fixture.db, run_id, "run.created").await["attempt"],
        1
    );

    let (status, _, refused) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "conflict");

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run
          WHERE tenant_id = $1 AND card_id = $2
            AND status IN ('queued', 'running', 'waiting_approval')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.card_id)
    .fetch_one(&fixture.db.super_pool)
    .await
    .expect("count live runs");
    assert_eq!(live, 1, "the partial unique index keeps one live run");

    let (_, job_status, _) = job_row(&fixture.db, run_id).await;
    assert_eq!(job_status, "pending", "create enqueues the driving job");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_worker_starts_a_queued_run_exactly_once() {
    let fixture = Fixture::new().await;
    let (status, _, body) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = id_of(&body["output"]["id"]);

    let _worker = WorkerChild::spawn(&fixture.db);
    wait_for(
        "the worker to start the run",
        Duration::from_secs(30),
        || async { run_row(&fixture.db, run_id).await.0 == "running" },
    )
    .await;
    wait_for(
        "the driving job to finish",
        Duration::from_secs(30),
        || async { job_row(&fixture.db, run_id).await.1 == "succeeded" },
    )
    .await;

    let (status, attempt, error_class, started) = run_row(&fixture.db, run_id).await;
    assert_eq!(status, "running");
    assert_eq!(attempt, 1);
    assert_eq!(error_class, None);
    assert!(started, "the store stamps started_at on the transition");

    assert_eq!(event_count(&fixture.db, run_id, "run.started").await, 1);
    let payload = event_payload(&fixture.db, run_id, "run.started").await;
    assert_eq!(payload["attempt"], 1);
    assert_eq!(payload["run_id"], json!(run_id.to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dead_driving_job_is_reaped_as_crashed_and_the_next_attempt_is_two() {
    let fixture = Fixture::new().await;
    let (status, _, body) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = id_of(&body["output"]["id"]);

    // Park the seeded reap schedule: this test triggers the one reap it wants, so a minute
    // boundary cannot sweep the run behind the assertions below.
    let parked = sqlx::query(
        "UPDATE schedule SET next_run_at = now() + interval '1 hour'
          WHERE tenant_id = $1 AND kind = 'run.reap'",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(&fixture.db.super_pool)
    .await
    .expect("park the run.reap schedule")
    .rows_affected();
    assert_eq!(parked, 1, "the run.reap schedule row is seeded");

    let worker = WorkerChild::spawn(&fixture.db);
    wait_for(
        "the worker to start the run",
        Duration::from_secs(30),
        || async { run_row(&fixture.db, run_id).await.0 == "running" },
    )
    .await;
    wait_for(
        "the valid driving job to finish",
        Duration::from_secs(30),
        || async { job_row(&fixture.db, run_id).await.1 == "succeeded" },
    )
    .await;

    // A SIGKILLed worker leaves its job `running` until the lease expires; once its attempts
    // are spent the queue dead-letters it and the run has no driver left. Rather than
    // hand-writing that state, drive the queue's own dead-letter path: a second
    // `run.execute` for the same run whose payload has no `run_id`. The dedupe key is free
    // because the first job is terminal, the LIVE worker claims it, the handler's permanent
    // failure dead-letters it (`max_attempts = 1`), and only then does the sweep close the
    // run — no direct `run_kind` call anywhere.
    let bogus_job: Uuid = sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, 'run.execute', $2, '{}'::jsonb, 1)
         RETURNING id",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(format!("run:{run_id}"))
    .fetch_one(&fixture.db.super_pool)
    .await
    .expect("insert the invalid driving job");
    wait_for(
        "the worker to dead-letter the invalid job",
        Duration::from_secs(30),
        || async { job_status_by_id(&fixture.db, bogus_job).await == "dead" },
    )
    .await;
    // A dead job alone does not move the run; only the sweep does.
    assert_eq!(run_row(&fixture.db, run_id).await.0, "running");

    // Let the seeded 60 s `run.reap` schedule fall due: the same worker materialises the
    // job, claims it, and the sweep fails the undriven run.
    make_schedule_due(&fixture.db, fixture.tenant, "run.reap").await;
    wait_for(
        "the reap tick to close the crashed run",
        Duration::from_secs(30),
        || async { event_count(&fixture.db, run_id, "run.failed").await == 1 },
    )
    .await;

    let (status, attempt, error_class, _) = run_row(&fixture.db, run_id).await;
    assert_eq!(status, "failed");
    assert_eq!(attempt, 1);
    assert_eq!(error_class.as_deref(), Some("crashed"));
    let payload = event_payload(&fixture.db, run_id, "run.failed").await;
    assert_eq!(payload["error_class"], "crashed");
    assert_eq!(payload["attempt"], 1);
    assert_eq!(event_count(&fixture.db, run_id, "run.failed").await, 1);
    assert_eq!(
        run_count(&fixture.db, fixture.tenant, fixture.card_id).await,
        1,
        "the crashed run is the card's only row"
    );

    // Stop the worker so its start hook cannot race the retry's representation.
    drop(worker);

    // The retry: a second row, attempt 2 read from that row, the crashed row untouched.
    let (status, _, retried) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 201, "{retried}");
    let retry_id = id_of(&retried["output"]["id"]);
    assert_ne!(retry_id, run_id);
    assert_eq!(
        run_count(&fixture.db, fixture.tenant, fixture.card_id).await,
        2,
        "two rows: the crashed attempt and the retry"
    );
    let (retry_status, retry_attempt, _, _) = run_row(&fixture.db, retry_id).await;
    assert_eq!(retry_status, "queued");
    assert_eq!(retry_attempt, 2, "read from the NEW run's own row");
    let (old_status, _, old_class, _) = run_row(&fixture.db, run_id).await;
    assert_eq!(old_status, "failed");
    assert_eq!(old_class.as_deref(), Some("crashed"));

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run
          WHERE tenant_id = $1 AND card_id = $2
            AND status IN ('queued', 'running', 'waiting_approval')",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.card_id)
    .fetch_one(&fixture.db.super_pool)
    .await
    .expect("count live runs");
    assert_eq!(live, 1, "two rows, never two live");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_andon_cord_refuses_creates_and_holds_a_queued_run_execute() {
    let fixture = Fixture::new().await;
    // Make the lease sweep fall due before the worker starts: while the cord is pulled its
    // job must APPEAR (proof the tick ran) and stay pending (proof the pause held). Its
    // materialisation is the positive liveness signal the old fixed sleep never had.
    make_schedule_due(&fixture.db, fixture.tenant, "card.lease_sweep").await;

    let (status, _, body) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = id_of(&body["output"]["id"]);

    // Engage the cord as the andon singleton's own row (the state is a row, not a flag).
    sqlx::query(
        "INSERT INTO andon_state (tenant_id, engaged, engaged_at, reason)
         VALUES ($1, true, now(), 'test cord')",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(&fixture.db.super_pool)
    .await
    .expect("engage the andon cord");

    let (status, _, refused) = fixture
        .call("runs.create", json!({"card_id": fixture.card_id}))
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "andon-engaged");

    let _worker = WorkerChild::spawn(&fixture.db);
    wait_for(
        "the engaged worker's tick to materialise the sweep job",
        Duration::from_secs(30),
        || async { pending_jobs(&fixture.db, fixture.tenant, "card.lease_sweep").await >= 1 },
    )
    .await;

    // The tick demonstrably ran; what holds the work is the pause, not a dead process.
    let (status, _, _, _) = run_row(&fixture.db, run_id).await;
    assert_eq!(status, "queued", "the poller skips an engaged tenant");
    assert_eq!(
        run_count(&fixture.db, fixture.tenant, fixture.card_id).await,
        1,
        "the refused create wrote no second run"
    );
    let (_, job_status, job_attempts) = job_row(&fixture.db, run_id).await;
    assert_eq!(job_status, "pending");
    assert_eq!(job_attempts, 0, "the job was never claimed while engaged");

    sqlx::query("UPDATE andon_state SET engaged = false, released_at = now() WHERE tenant_id = $1")
        .bind(fixture.tenant.as_uuid())
        .execute(&fixture.db.super_pool)
        .await
        .expect("release the andon cord");

    wait_for(
        "the released worker to start the run",
        Duration::from_secs(30),
        || async { run_row(&fixture.db, run_id).await.0 == "running" },
    )
    .await;
    let (status, attempt, _, _) = run_row(&fixture.db, run_id).await;
    assert_eq!(status, "running");
    assert_eq!(attempt, 1);
    assert_eq!(event_count(&fixture.db, run_id, "run.started").await, 1);
    // The worker fills the job row asynchronously after the run starts. Asserting
    // immediately reads "running" whenever the worker has not finished that write yet —
    // measured 2026-11-11 on the nextest switch (run 42992636306), where parallel test
    // processes changed the timing. Wait for the transition, as every other assertion in
    // this file does.
    wait_for(
        "the released worker to finish the run's job",
        Duration::from_secs(30),
        || async { job_row(&fixture.db, run_id).await.1 == "succeeded" },
    )
    .await;
    // And the non-allowed kind materialised while engaged drains on release too.
    wait_for(
        "the released worker to drain the sweep job",
        Duration::from_secs(30),
        || async { pending_jobs(&fixture.db, fixture.tenant, "card.lease_sweep").await == 0 },
    )
    .await;
}
