//! S07 §5.5: `quality.score` queues a rubric job in the caller's transaction and the worker
//! appends the evaluation — twice for the same immutable revision, with the same score — and
//! never writes the scored asset.
//!
//! The worker is a real `conveyor worker` child process (a job handler must be driven where
//! `CARGO_BIN_EXE_conveyor` exists); the action surface is the real router bound in-process
//! to the harness's database.
//!
//! The governed asset and its revision are seeded with superuser SQL: S07's
//! `requirements.*` surface is another wave's file, and what is under test here is the
//! queue → worker → `quality_evaluation` path, not the asset lifecycle.

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
            // Long enough that the fast handler can never lose its lease and re-append.
            .env("JOB_LEASE_SECS", "10")
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
        let response = reqwest::Client::new()
            .post(format!("{}/api/actions/{action}/invocations", self.base()))
            .header("authorization", format!("Bearer {token}"))
            .json(&json!({"input": input}))
            .send()
            .await
            .expect("invoke the action");
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
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
    #[allow(dead_code)]
    user: Uuid,
}

async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('dev-a', 'Quality') RETURNING id",
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

async fn seed_asset(
    db: &common::TestDb,
    tenant: Uuid,
    key: &str,
    title: &str,
    body: &str,
    acceptance: &Value,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, body, acceptance)
         VALUES ($1, 'requirement', $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant)
    .bind(key)
    .bind(title)
    .bind(body)
    .bind(acceptance)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the governed asset")
}

/// Insert a revision with the S07 §5.3 snapshot shape (and its real SHA-256) as superuser.
///
/// A fixture: the eight arguments are the eight columns of the document it writes, and
/// bundling them into a struct would only move the list one line down.
#[allow(clippy::too_many_arguments)]
async fn seed_revision(
    db: &common::TestDb,
    tenant: Uuid,
    entity_type: &str,
    entity_id: Uuid,
    key: &str,
    title: &str,
    body: &str,
    acceptance: &Value,
) -> Uuid {
    let content = if entity_type == "governed_asset" {
        json!({
            "id": entity_id,
            "kind": "requirement",
            "key": key,
            "title": title,
            "body": body,
            "status": "draft",
            "version": 1,
            "project_id": null,
            "owner_user_id": null,
            "phase_code": null,
            "acceptance": acceptance,
            "links": {},
            "lens": {},
            "meta": {},
        })
    } else {
        json!({ "title": title, "body": body })
    };
    let hash = conveyor_domain::compare::content_hash(&content);
    sqlx::query_scalar(
        "INSERT INTO revision (tenant_id, entity_type, entity_id, seq, label, content,
                               content_hash, status)
         VALUES ($1, $2, $3, 1, 'created', $4, $5, 'draft')
         RETURNING id",
    )
    .bind(tenant)
    .bind(entity_type)
    .bind(entity_id)
    .bind(&content)
    .bind(hash)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the revision")
}

async fn seed_trace(db: &common::TestDb, tenant: Uuid, from_id: Uuid, to_type: &str, to_id: Uuid) {
    sqlx::query(
        "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id)
         VALUES ($1, 'governed_asset', $2, $3, $4)",
    )
    .bind(tenant)
    .bind(from_id)
    .bind(to_type)
    .bind(to_id)
    .execute(&db.super_pool)
    .await
    .expect("seed the trace link");
}

/// Every score of one revision, oldest first.
async fn evaluation_scores(db: &common::TestDb, revision_id: Uuid) -> Vec<f64> {
    sqlx::query_scalar(
        "SELECT score::float8 FROM quality_evaluation
          WHERE entity_type = 'revision' AND entity_id = $1
          ORDER BY created_at, id",
    )
    .bind(revision_id)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the evaluation history")
}

/// Every `quality.score` job of one revision, oldest first, as `(status, dedupe_key)`.
async fn revision_jobs(db: &common::TestDb, revision_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT status, dedupe_key FROM job
          WHERE kind = 'quality.score' AND dedupe_key = $1
          ORDER BY created_at, id",
    )
    .bind(format!("revision:{revision_id}"))
    .fetch_all(&db.super_pool)
    .await
    .expect("read the quality.score jobs")
}

/// The scored asset's concurrency-visible state: `(version, status, updated_at)`.
async fn asset_state(db: &common::TestDb, id: Uuid) -> (i64, String, String) {
    sqlx::query_as("SELECT version, status, updated_at::text FROM governed_asset WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the governed asset")
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

/// A revision with a good title/body, a Gherkin acceptance entry and two trace links (one to
/// a card) scores higher than a vague, untraceable one; re-scoring the same immutable
/// revision appends the SAME score; and neither call writes the asset.
#[tokio::test(flavor = "multi_thread")]
async fn a_traceable_revision_outranks_a_vague_one_and_rescoring_appends_the_same_score() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let served = Served::start(&db).await;

    let good_title = "Retries are capped at three attempts";
    let good_body = "Every mutating request is idempotent.";
    let good_acceptance = json!([{
        "id": "a1",
        "given": "a card in the active list",
        "when": "it is moved to done",
        "then": "the board reflects the move"
    }]);
    let good_asset = seed_asset(
        &db,
        fixture.tenant_id,
        "retries",
        good_title,
        good_body,
        &good_acceptance,
    )
    .await;
    let good_revision = seed_revision(
        &db,
        fixture.tenant_id,
        "governed_asset",
        good_asset,
        "retries",
        good_title,
        good_body,
        &good_acceptance,
    )
    .await;
    // Two edges out of the asset, one of them to a card: traceability's top tier needs both.
    seed_trace(&db, fixture.tenant_id, good_asset, "card", Uuid::now_v7()).await;
    seed_trace(
        &db,
        fixture.tenant_id,
        good_asset,
        "governed_asset",
        Uuid::now_v7(),
    )
    .await;

    let vague_asset = seed_asset(
        &db,
        fixture.tenant_id,
        "vague",
        "Do the thing",
        "A and b and c and d.",
        &json!([]),
    )
    .await;
    let vague_revision = seed_revision(
        &db,
        fixture.tenant_id,
        "governed_asset",
        vague_asset,
        "vague",
        "Do the thing",
        "A and b and c and d.",
        &json!([]),
    )
    .await;

    let good_before = asset_state(&db, good_asset).await;
    let vague_before = asset_state(&db, vague_asset).await;

    // ── the action only enqueues: with no worker running, nothing can be scored ───────────
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": good_revision.to_string()}),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let out = output(&body);
    assert_eq!(out["revision_id"], good_revision.to_string());
    assert_eq!(out["queued"], true);
    let first_job = id_of(out, "job_id");
    let job_state: Option<String> = sqlx::query_scalar("SELECT status FROM job WHERE id = $1")
        .bind(Uuid::parse_str(&first_job).expect("the job id is a uuid"))
        .fetch_optional(&db.super_pool)
        .await
        .expect("read the enqueued job");
    assert_eq!(job_state.as_deref(), Some("pending"), "the job is queued");
    assert!(
        evaluation_scores(&db, good_revision).await.is_empty(),
        "the action writes no evaluation; the worker appends it after commit"
    );
    assert_eq!(
        revision_jobs(&db, good_revision).await,
        vec![("pending".to_owned(), format!("revision:{good_revision}"))],
        "the action queued exactly one job, named by the revision"
    );
    assert_eq!(
        asset_state(&db, good_asset).await,
        good_before,
        "the action touches no asset row"
    );

    // ── a second invoke while the job is live is a dedupe hit: 200, no new row ────────────
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": good_revision.to_string()}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["queued"], true);
    assert!(
        output(&body)["job_id"].is_null(),
        "a live job already covers the revision: {body}"
    );
    assert_eq!(
        revision_jobs(&db, good_revision).await.len(),
        1,
        "the live job is reused, not duplicated"
    );

    // ── the worker runs the rubric ───────────────────────────────────────────────────────
    let _worker = WorkerChild::spawn(&db);
    wait_for(
        "the first quality_evaluation row",
        Duration::from_secs(30),
        || async { evaluation_scores(&db, good_revision).await.len() == 1 },
    )
    .await;
    wait_for(
        "the first quality.score job to succeed",
        Duration::from_secs(30),
        || async {
            revision_jobs(&db, good_revision)
                .await
                .first()
                .is_some_and(|(status, _)| status == "succeeded")
        },
    )
    .await;
    let first = evaluation_scores(&db, good_revision).await;
    assert_eq!(first.len(), 1);

    // ── re-scoring the same immutable revision: same score, a second row ─────────────────
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": good_revision.to_string()}),
        )
        .await;
    assert_eq!(
        status, 201,
        "a finished job no longer dedupes, so this call creates one: {body}"
    );
    assert!(output(&body)["job_id"].is_string(), "{body}");
    wait_for(
        "the second quality_evaluation row",
        Duration::from_secs(30),
        || async { evaluation_scores(&db, good_revision).await.len() == 2 },
    )
    .await;
    let both = evaluation_scores(&db, good_revision).await;
    assert_eq!(both.len(), 2, "append-only: the history keeps both scores");
    assert_eq!(
        both[0], both[1],
        "the rubric is deterministic over an immutable revision"
    );
    assert_eq!(
        revision_jobs(&db, good_revision).await.len(),
        2,
        "the re-score ran as a separate job"
    );
    assert_eq!(
        both[0], 100.0,
        "the traceable revision earns every criterion"
    );

    // ── the vague revision scores strictly lower ────────────────────────────────────────
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": vague_revision.to_string()}),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    wait_for(
        "the vague revision's evaluation",
        Duration::from_secs(30),
        || async { evaluation_scores(&db, vague_revision).await.len() == 1 },
    )
    .await;
    let vague = evaluation_scores(&db, vague_revision).await[0];
    println!("quality scores: traceable={} vague={vague}", both[0]);
    assert_eq!(vague, 15.0, "only uniqueness scores for the vague revision");
    assert!(
        both[0] > vague,
        "a traceable revision ({}) must outrank a vague one ({vague})",
        both[0]
    );

    // ── the scored assets are unchanged: a score is never written to an asset ───────────
    assert_eq!(
        asset_state(&db, good_asset).await,
        good_before,
        "scoring never changes the asset's version, status or updated_at"
    );
    assert_eq!(
        asset_state(&db, vague_asset).await,
        vague_before,
        "scoring never changes the asset's version, status or updated_at"
    );

    // ── every evaluation is announced on its revision, naming the evaluation row ────────
    let events: Vec<(String, String, String, Value)> = sqlx::query_as(
        "SELECT topic, type, aggregate_type, payload FROM event
          WHERE type = 'quality.evaluated' ORDER BY seq",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("read the quality.evaluated events");
    assert_eq!(events.len(), 3, "one event per appended evaluation");
    for (topic, event_type, aggregate_type, payload) in &events {
        assert!(
            *topic == format!("revision:{good_revision}")
                || *topic == format!("revision:{vague_revision}"),
            "an event lands on the revision's topic: {topic}"
        );
        assert_eq!(event_type, "quality.evaluated");
        assert_eq!(aggregate_type, "quality_evaluation");
        assert_eq!(payload["rubric"], "v1", "{payload}");
        assert!(payload["revision_id"].is_string(), "{payload}");
        assert!(payload["score"].is_number(), "{payload}");
    }
    let mut announced: Vec<Uuid> =
        sqlx::query_scalar("SELECT aggregate_id FROM event WHERE type = 'quality.evaluated'")
            .fetch_all(&db.super_pool)
            .await
            .expect("read the announced ids");
    let mut evaluations: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM quality_evaluation")
        .fetch_all(&db.super_pool)
        .await
        .expect("read the evaluation ids");
    announced.sort();
    evaluations.sort();
    assert_eq!(
        announced, evaluations,
        "each event's aggregate id is its evaluation row"
    );

    // ── only a governed-asset revision can be scored ────────────────────────────────────
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": Uuid::now_v7().to_string()}),
        )
        .await;
    assert_eq!(status, 404, "an absent revision: {body}");
    assert_eq!(body["type"], "not_found", "{body}");

    let model_revision = seed_revision(
        &db,
        fixture.tenant_id,
        "model",
        Uuid::now_v7(),
        "",
        "A model revision",
        "",
        &json!([]),
    )
    .await;
    let (status, body) = served
        .invoke(
            "quality.score",
            "dev:alice",
            json!({"revision_id": model_revision.to_string()}),
        )
        .await;
    assert_eq!(status, 404, "a model revision is out of S07: {body}");
    assert_eq!(body["type"], "not_found", "{body}");
    assert_eq!(
        evaluation_scores(&db, model_revision).await.len(),
        0,
        "a refused revision is never scored"
    );
}
