//! S06 §5.5 / §7(d): the andon cord refuses new runs, pauses the worker, drains on release,
//! and is visible on the scrape as `conveyor_andon_engaged{tenant_id}` 1 → 0.
//!
//! The worker is a real `conveyor worker` child process (a job handler must be driven where
//! `CARGO_BIN_EXE_conveyor` exists); the HTTP surface is the real router bound in-process to
//! the harness's database, which is also what `/metrics` is scraped from.

mod common;

use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use conveyor_api::{AppState, Config};
use serde_json::{Value, json};
use uuid::Uuid;

/// A child worker that is killed on drop.
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
        Self { child }
    }
}

impl Drop for WorkerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// ⛔ EVERY request this test makes to the in-process app is bounded.
///
/// Without a timeout a blocked app task turns the whole CI job into a silent hang:
/// measured 2026-11-10 on run 42771580396, `cargo test` sat >30 minutes inside this
/// one test binary (`ps` showed it at 0.1% CPU with its Postgres container healthy),
/// the job was killed at its 90-minute ceiling, and **GitHub stores no log blob for a
/// cancelled job** — so the failure left no evidence at all. A bounded request panics
/// with the action name instead, which is the only way this class is diagnosable.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A single poll inside `wait_for` must also be bounded: the deadline is only checked
/// BETWEEN polls, so a predicate whose query blocks would otherwise defeat it.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("a bounded HTTP client")
}

/// The served app on an ephemeral port, with the harness's database.
struct Served {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Served {
    async fn start(db: &common::TestDb) -> Served {
        let runtime = db.runtime_url.clone();
        let config = Config::from_lookup({
            let runtime = runtime.clone();
            move |key| match key {
                "DATABASE_URL" | "DATABASE_LISTEN_URL" | "DATABASE_MIGRATE_URL" => {
                    Some(runtime.clone())
                }
                "CONVEYOR_DEV_AUTH" => Some("1".into()),
                "BIND_ADDR" => Some("127.0.0.1:0".into()),
                "APP_KEY" => Some("conveyor".into()),
                _ => None,
            }
        })
        .expect("test config");
        let state = AppState::connect(config).await.expect("AppState");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind an ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let app = conveyor_api::app(state);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Served { addr, task }
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    async fn invoke(&self, action: &str, token: &str, input: Value) -> (u16, Value) {
        let response = client()
            .post(format!("{}/api/actions/{action}/invocations", self.base()))
            .header("authorization", format!("Bearer {token}"))
            .json(&json!({"input": input}))
            .send()
            .await
            .unwrap_or_else(|e| {
                panic!("invoke `{action}` did not answer within {REQUEST_TIMEOUT:?}: {e}")
            });
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn metrics(&self) -> String {
        client()
            .get(format!("{}/metrics", self.base()))
            .send()
            .await
            .unwrap_or_else(|e| {
                panic!("scrape /metrics did not answer within {REQUEST_TIMEOUT:?}: {e}")
            })
            .text()
            .await
            .expect("the scrape is text")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone, Debug)]
struct Fixture {
    tenant_id: Uuid,
    user: Uuid,
}

async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('dev-a', 'Andon') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name) VALUES ('dev:alice', 'Alice')
         RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed app_user");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant_id)
        .bind(user)
        .execute(&db.super_pool)
        .await
        .expect("seed membership");
    sqlx::query(
        "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
         VALUES ($1, 'conveyor', 'active', now())",
    )
    .bind(tenant_id)
    .execute(&db.super_pool)
    .await
    .expect("seed licence");
    Fixture { tenant_id, user }
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn make_project(served: &Served, title: &str) -> String {
    let (status, body) = served
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

async fn make_card(served: &Served, project_id: &str) -> String {
    let (status, project) = served
        .invoke("projects.get", "dev:alice", json!({"id": project_id}))
        .await;
    assert_eq!(status, 200, "{project}");
    let board_id = id_of(&output(&project)["boards"][0], "id");
    let (status, board) = served
        .invoke("boards.get", "dev:alice", json!({"id": board_id}))
        .await;
    assert_eq!(status, 200, "{board}");
    let lanes = output(&board)["lists"].as_array().expect("lists");
    let active = lanes
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .and_then(|row| row["list"]["id"].as_str())
        .expect("an active lane")
        .to_owned();
    let (status, card) = served
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": active,
                "name": "Andon work",
                "card_type_code": "feature",
            }),
        )
        .await;
    assert_eq!(status, 201, "{card}");
    id_of(output(&card), "id")
}

async fn job_status(db: &common::TestDb, run_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM job WHERE kind = 'run.execute' AND dedupe_key = $1")
        .bind(format!("run:{run_id}"))
        .fetch_optional(&db.super_pool)
        .await
        .expect("read the run.execute job")
}

/// The `delivery.send` probe's state: it is an allowed kind, so the paused worker claims it.
async fn probe_status(db: &common::TestDb) -> Option<String> {
    sqlx::query_scalar(
        "SELECT status FROM job WHERE kind = 'delivery.send' AND dedupe_key = 'andon-probe'",
    )
    .fetch_optional(&db.super_pool)
    .await
    .expect("read the probe job")
}

async fn job_attempts(db: &common::TestDb, run_id: &str) -> i32 {
    sqlx::query_scalar("SELECT attempts FROM job WHERE kind = 'run.execute' AND dedupe_key = $1")
        .bind(format!("run:{run_id}"))
        .fetch_one(&db.super_pool)
        .await
        .expect("read the run.execute attempts")
}

async fn run_status(db: &common::TestDb, run_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM run WHERE id = $1")
        .bind(Uuid::parse_str(run_id).expect("a uuid"))
        .fetch_one(&db.super_pool)
        .await
        .expect("read the run")
}

async fn wait_for<F, Fut>(what: &str, timeout: Duration, mut predicate: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout(CHECK_TIMEOUT, predicate()).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(_) => panic!("one {what} check did not finish within {CHECK_TIMEOUT:?}"),
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out after {timeout:?} waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The value of `metric{tenant_id="<id>"}` in a Prometheus exposition, if the sample exists.
fn gauge_value(text: &str, metric: &str, tenant_id: &str) -> Option<f64> {
    let label = format!("tenant_id=\"{tenant_id}\"");
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(metric) else {
            continue;
        };
        // `conveyor_andon_engaged` is a prefix of `_at_seconds`; a real sample continues with
        // the label set.
        if !rest.starts_with('{') || !rest.contains(&label) {
            continue;
        }
        let (_, value) = rest.rsplit_once(' ')?;
        return value.trim().parse().ok();
    }
    None
}

/// The whole scenario finishes in ~4 s on m1 (measured 2026-11-10). This is a ceiling,
/// not a budget.
///
/// The per-request and per-poll bounds below name *which* await blocked, but anything
/// outside them — the pool connect, a migration advisory lock, a child process that
/// never signals — would still hang the job silently, and a job cancelled at its
/// ceiling keeps NO log blob. This outer bound is what makes the failure always
/// debuggable.
const SCENARIO_TIMEOUT: Duration = Duration::from_secs(120);

#[tokio::test(flavor = "multi_thread")]
async fn the_cord_refuses_new_runs_pauses_the_worker_drains_on_release_and_moves_the_gauge() {
    tokio::time::timeout(SCENARIO_TIMEOUT, andon_scenario())
        .await
        .expect("the andon scenario did not finish within SCENARIO_TIMEOUT — something is blocked");
}

async fn andon_scenario() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    conveyor_store::seed::catalogue(&db.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let served = Served::start(&db).await;

    let project_id = make_project(&served, "Andon").await;
    let card_id = make_card(&served, &project_id).await;

    // ── a queued run.execute exists BEFORE the cord is pulled ────────────────────────────
    let (status, created) = served
        .invoke("runs.create", "dev:alice", json!({"card_id": card_id}))
        .await;
    assert_eq!(status, 201, "{created}");
    let run_id = id_of(output(&created), "id");
    assert_eq!(job_status(&db, &run_id).await.as_deref(), Some("pending"));

    // ── engage ──────────────────────────────────────────────────────────────────────────
    let (status, body) = served
        .invoke("andon.engage", "dev:alice", json!({"reason": "drill"}))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["engaged"], true);
    assert_eq!(output(&body)["reason"], "drill");
    assert_eq!(output(&body)["engaged_by"], fixture.user.to_string());

    // A second engage is a 409 that carries the current state (CONTRACT §3).
    let (status, body) = served
        .invoke("andon.engage", "dev:alice", json!({"reason": "again"}))
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict", "{body}");
    assert_eq!(body["current"]["engaged"], true, "{body}");

    // `runs.create` is refused while the cord is pulled.
    let (status, body) = served
        .invoke("runs.create", "dev:alice", json!({"card_id": card_id}))
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "andon-engaged", "{body}");

    // ── the worker is paused for this tenant ────────────────────────────────────────────
    //
    // The negative proof is a PROCESSED SIBLING, not a long enough sleep: `delivery.send` is
    // one of the two kinds the worker may claim while the cord is pulled, so its row leaving
    // `pending` proves a tick ran for this tenant. The `run.execute` row must still be
    // untouched at that instant — pending AND never claimed (`attempts = 0`).
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload)
         VALUES ($1, 'delivery.send', 'andon-probe', '{}'::jsonb)",
    )
    .bind(fixture.tenant_id)
    .execute(&db.super_pool)
    .await
    .expect("seed the allowed-kind probe job");

    let _worker = WorkerChild::spawn(&db);
    wait_for(
        "the worker to claim the allowed probe job while the cord is pulled",
        Duration::from_secs(10),
        || async { probe_status(&db).await.as_deref() != Some("pending") },
    )
    .await;
    assert_eq!(
        job_status(&db, &run_id).await.as_deref(),
        Some("pending"),
        "an engaged tenant's run.execute must stay pending"
    );
    assert_eq!(
        job_attempts(&db, &run_id).await,
        0,
        "the paused worker must never have claimed the run.execute job"
    );
    assert_eq!(run_status(&db, &run_id).await, "queued");

    // ── the scrape reads the cord at scrape time ────────────────────────────────────────
    let tenant = fixture.tenant_id.to_string();
    let text = served.metrics().await;
    assert_eq!(
        gauge_value(&text, "conveyor_andon_engaged", &tenant),
        Some(1.0),
        "the scrape must report the pulled cord:\n{text}"
    );
    // The age gauge must derive from `engaged_at`, not a constant. The first scrape can
    // legitimately read 0 (the cord was pulled moments ago), so poll the scrape — bounded —
    // until it reaches a whole second; the released assertion below then proves it returns
    // to 0.
    wait_for(
        "the engaged-at gauge to report a positive age",
        Duration::from_secs(5),
        || async {
            let scrape = served.metrics().await;
            gauge_value(&scrape, "conveyor_andon_engaged_at_seconds", &tenant)
                .is_some_and(|seconds| seconds >= 1.0)
        },
    )
    .await;

    // ── release: the cord is down and the queue drains within a tick ────────────────────
    let (status, body) = served.invoke("andon.release", "dev:alice", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["engaged"], false);
    assert!(output(&body)["released_at"].is_string(), "{body}");

    // A second release changes nothing and announces nothing.
    let (status, body) = served.invoke("andon.release", "dev:alice", json!({})).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["current"]["engaged"], false, "{body}");

    wait_for(
        "the queued run.execute to drain after the release",
        Duration::from_secs(10),
        || async { job_status(&db, &run_id).await.as_deref() == Some("succeeded") },
    )
    .await;
    assert_eq!(
        run_status(&db, &run_id).await,
        "running",
        "the drained handler started the run"
    );

    let text = served.metrics().await;
    assert_eq!(
        gauge_value(&text, "conveyor_andon_engaged", &tenant),
        Some(0.0),
        "the scrape must report the released cord:\n{text}"
    );
    assert_eq!(
        gauge_value(&text, "conveyor_andon_engaged_at_seconds", &tenant),
        Some(0.0),
        "a released cord reports age 0"
    );
}
