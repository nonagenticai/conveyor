//! The maintenance kinds and the schedule materialiser, through the worker's own dispatch.
//!
//! `cargo test -p conveyor-server --test prune -- --nocapture`. `event.prune` runs for ONE
//! tenant and must leave every other tenant's rows untouched: that is the RLS property the
//! per-tenant design exists for (S2-D3 — there is no `BYPASSRLS` role to do otherwise).

mod common;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use conveyor_api::Config;
use conveyor_auth::licence::LicenceSyncClient;
use conveyor_domain::TenantId;
use conveyor_server::worker::{self, JobFailure};
use conveyor_store::begin_scoped;
use conveyor_store::model::{get_model_tx, publish_model_tx, write_head_revision_tx};
use conveyor_store::schedule::{self, DEFAULT_SCHEDULES};
use serde_json::json;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

async fn tenants(db: &common::TestDb) -> (TenantId, TenantId) {
    let mut ids = Vec::new();
    for (slug, name) in [("prune-a", "Prune A"), ("prune-b", "Prune B")] {
        let id: Uuid =
            sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
                .bind(slug)
                .bind(name)
                .fetch_one(&db.super_pool)
                .await
                .expect("seed a tenant");
        ids.push(TenantId::from_uuid(id).expect("non-nil"));
    }
    (ids[0], ids[1])
}

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

async fn seed_event(db: &common::TestDb, tenant: TenantId, age_days: i64) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO event (tenant_id, topic, type, aggregate_type, aggregate_id, created_at)
         VALUES ($1, 'board:0195f48b-d000-7000-8000-0000000000aa', 'card.moved', 'card', $2, now() - make_interval(days => $3))
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::now_v7())
    .bind(age_days as i32)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed an event");
    id
}

async fn seed_model(db: &common::TestDb, tenant: TenantId, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO model (tenant_id, language, language_version, title)
         VALUES ($1, 'archimate', '3.2', $2) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(title)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a model")
}

/// Insert `count` revisions (`seq` 1..=count) and return `(seq, id)` sorted by `seq`.
///
/// `RETURNING` has no guaranteed row order, so the pairs are sorted explicitly rather than
/// trusting the insert order.
async fn seed_revisions(
    db: &common::TestDb,
    tenant: TenantId,
    entity_type: &str,
    entity_id: Uuid,
    count: i32,
) -> Vec<(i32, Uuid)> {
    let mut rows: Vec<(i32, Uuid)> = sqlx::query_as(
        "INSERT INTO revision (tenant_id, entity_type, entity_id, seq, label, content,
                               content_hash, status)
         SELECT $1, $2, $3, g, 'seed', jsonb_build_object('v', g), 'h', 'draft'
           FROM generate_series(1, $4) AS g
         RETURNING seq, id",
    )
    .bind(tenant.as_uuid())
    .bind(entity_type)
    .bind(entity_id)
    .bind(count)
    .fetch_all(&db.super_pool)
    .await
    .expect("seed revisions");
    rows.sort_by_key(|(seq, _)| *seq);
    rows
}

async fn seed_validation_run(
    db: &common::TestDb,
    tenant: TenantId,
    model: Uuid,
    revision: Option<Uuid>,
    age: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO model_validation_run (tenant_id, model_id, revision_id, ruleset,
                                           ruleset_version, status, findings, content_hash,
                                           created_at)
         VALUES ($1, $2, $3, 'conveyor-archimate-3.2-v1', '1', 'ok', '[]', 'h',
                 now() - $4::interval)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(model)
    .bind(revision)
    .bind(age)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a validation run")
}

async fn revision_seqs(db: &common::TestDb, tenant: TenantId, model: Uuid) -> Vec<i32> {
    sqlx::query_scalar(
        "SELECT seq FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2
          ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .bind(model)
    .fetch_all(&db.super_pool)
    .await
    .expect("read revision seqs")
}

#[tokio::test]
async fn event_prune_deletes_only_the_callers_tenant() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;
    seed_event(&db, a, 91).await;
    seed_event(&db, a, 0).await;
    seed_event(&db, b, 91).await;
    seed_event(&db, b, 0).await;

    worker::run_kind(&db.runtime, &config(), None, a, "event.prune", &json!({}))
        .await
        .expect("event.prune for A");

    let a_old = count(
        &db.super_pool,
        &format!(
            "SELECT count(*) FROM event WHERE tenant_id = '{}' AND created_at < now() - interval '90 days'",
            a.as_uuid()
        ),
    )
    .await;
    let a_fresh = count(
        &db.super_pool,
        &format!(
            "SELECT count(*) FROM event WHERE tenant_id = '{}'",
            a.as_uuid()
        ),
    )
    .await;
    let b_old = count(
        &db.super_pool,
        &format!(
            "SELECT count(*) FROM event WHERE tenant_id = '{}' AND created_at < now() - interval '90 days'",
            b.as_uuid()
        ),
    )
    .await;
    println!("after event.prune(A): A old={a_old} A total={a_fresh} B old={b_old}");
    assert_eq!(a_old, 0, "A's 91-day-old event is pruned");
    assert_eq!(a_fresh, 1, "A's fresh event survives");
    assert_eq!(
        b_old, 1,
        "B's old event is untouched: the prune ran in A's tenant scope"
    );
}

#[tokio::test]
async fn job_prune_and_idempotency_sweep_are_per_tenant_and_respect_state() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    // A: one old terminal job, one old pending job, one old terminal job for B.
    let old_terminal: Uuid = sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status, updated_at)
         VALUES ($1, 'x.old', 'done', 'succeeded', now() - interval '31 days')
         RETURNING id",
    )
    .bind(a.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("old terminal job");
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status, updated_at)
         VALUES ($1, 'x.pending', 'queued', 'pending', now() - interval '31 days')",
    )
    .bind(a.as_uuid())
    .execute(&db.super_pool)
    .await
    .expect("old pending job");
    let b_old_terminal: Uuid = sqlx::query_scalar(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status, updated_at)
         VALUES ($1, 'x.old', 'done', 'succeeded', now() - interval '31 days')
         RETURNING id",
    )
    .bind(b.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("B's old terminal job");

    worker::run_kind(&db.runtime, &config(), None, a, "job.prune", &json!({}))
        .await
        .expect("job.prune for A");
    let gone: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE id = $1")
        .bind(old_terminal)
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job WHERE tenant_id = $1 AND status = 'pending'")
            .bind(a.as_uuid())
            .fetch_one(&db.super_pool)
            .await
            .expect("count");
    let b_survives: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE id = $1")
        .bind(b_old_terminal)
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    println!("job.prune(A): old terminal gone={gone} pending kept={pending} B kept={b_survives}");
    assert_eq!(gone, 0, "the 31-day-old terminal job is pruned");
    assert_eq!(pending, 1, "a pending job is the queue, never pruned");
    assert_eq!(b_survives, 1, "B's terminal job is untouched");

    // Idempotency: one expired key for A, one live key for A.
    sqlx::query(
        "INSERT INTO idempotency_key (tenant_id, scope, key, request_hash, status_code, response, expires_at)
         VALUES ($1, 's', 'old', 'h', 200, '{}', now() - interval '1 hour'),
                ($1, 's', 'live', 'h', 200, '{}', now() + interval '1 hour')",
    )
    .bind(a.as_uuid())
    .execute(&db.super_pool)
    .await
    .expect("seed idempotency keys");
    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        a,
        "idempotency.sweep",
        &json!({}),
    )
    .await
    .expect("idempotency.sweep");
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM idempotency_key ORDER BY key")
        .fetch_all(&db.super_pool)
        .await
        .expect("read keys");
    println!("idempotency.sweep(A) left {keys:?}");
    assert_eq!(keys, vec!["live".to_owned()]);
}

#[tokio::test]
async fn model_prune_keeps_the_newest_fifty_and_never_deletes_the_published_revision() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    // A: a model that has never published — the revision past the cut-off is prunable.
    let plain = seed_model(&db, a, "plain").await;
    let plain_revisions = seed_revisions(&db, a, "model", plain, 51).await;
    assert_eq!(plain_revisions.first().map(|(seq, _)| *seq), Some(1));

    // A: the published revision is the OLDEST and the only candidate: 0 deletes.
    let pinned_oldest = seed_model(&db, a, "pinned-oldest").await;
    let oldest = seed_revisions(&db, a, "model", pinned_oldest, 51).await;
    let oldest_published = oldest[0].1;
    sqlx::query("UPDATE model SET published_revision_id = $1 WHERE id = $2")
        .bind(oldest_published)
        .bind(pinned_oldest)
        .execute(&db.super_pool)
        .await
        .expect("pin the oldest revision");

    // A: the published revision is INSIDE the candidate window (seq 2 of 52): exactly one
    // delete (seq 1) and the published row survives. This is the case that proves the
    // exclusion — a fixture whose published revision is the newest passes even when the
    // exclusion is missing.
    let pinned_middle = seed_model(&db, a, "pinned-middle").await;
    let middle = seed_revisions(&db, a, "model", pinned_middle, 52).await;
    let middle_published = middle[1].1;
    sqlx::query("UPDATE model SET published_revision_id = $1 WHERE id = $2")
        .bind(middle_published)
        .bind(pinned_middle)
        .execute(&db.super_pool)
        .await
        .expect("pin the middle revision");

    // A: a governed asset's revisions are the governance record and are NEVER pruned.
    // `governed_asset` is the `revision.entity_type` the governance store writes
    // (`GOVERNED_ASSET_ENTITY_TYPE`); a principle is one of its kinds, not a family.
    let governed_entity = Uuid::now_v7();
    seed_revisions(&db, a, "governed_asset", governed_entity, 200).await;

    // B: a model and an old validation run that A's prunes must not touch.
    let b_model = seed_model(&db, b, "b-model").await;
    seed_revisions(&db, b, "model", b_model, 51).await;
    let b_run = seed_validation_run(&db, b, b_model, None, "91 days").await;

    let a_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM revision WHERE tenant_id = $1 AND entity_type = 'model'",
    )
    .bind(a.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("count A revisions");
    assert_eq!(a_before, 51 + 51 + 52, "fixture: three models of 51/51/52");

    worker::run_kind(&db.runtime, &config(), None, a, "model.prune", &json!({}))
        .await
        .expect("model.prune for A");

    // plain: 51 -> 50, and the newest 50 (seq 2..=51) survive.
    let plain_now = revision_seqs(&db, a, plain).await;
    assert_eq!(plain_now.len(), 50, "exactly one revision is pruned");
    assert_eq!(plain_now.first(), Some(&2), "the oldest is the one deleted");
    assert_eq!(plain_now.last(), Some(&51), "the newest 50 survive");

    // pinned-oldest: the only candidate was pinned, so nothing went.
    let oldest_now = revision_seqs(&db, a, pinned_oldest).await;
    assert_eq!(
        oldest_now.len(),
        51,
        "the published revision is never pruned"
    );
    assert!(oldest_now.contains(&1), "seq 1 is still there");
    let oldest_still: i64 = sqlx::query_scalar("SELECT count(*) FROM revision WHERE id = $1")
        .bind(oldest_published)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the pinned revision");
    assert_eq!(
        oldest_still, 1,
        "the row named by published_revision_id survives"
    );

    // pinned-middle: seq 1 is pruned, the published seq 2 survives.
    let middle_now = revision_seqs(&db, a, pinned_middle).await;
    assert_eq!(middle_now.len(), 51, "52 -> 51: exactly one delete");
    assert!(!middle_now.contains(&1), "the unpinned candidate is pruned");
    let middle_still: i64 = sqlx::query_scalar("SELECT count(*) FROM revision WHERE id = $1")
        .bind(middle_published)
        .fetch_one(&db.super_pool)
        .await
        .expect("count the published revision");
    assert_eq!(
        middle_still, 1,
        "the published row inside the window survives"
    );

    // Governed-asset revisions are untouched: 200 rows, every seq.
    let governed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM revision
          WHERE tenant_id = $1 AND entity_type = 'governed_asset' AND entity_id = $2",
    )
    .bind(a.as_uuid())
    .bind(governed_entity)
    .fetch_one(&db.super_pool)
    .await
    .expect("count governed revisions");
    assert_eq!(governed, 200, "governed-asset revisions are never pruned");

    // B is untouched, and A's second run is a no-op.
    assert_eq!(revision_seqs(&db, b, b_model).await.len(), 51);
    let b_run_still: i64 =
        sqlx::query_scalar("SELECT count(*) FROM model_validation_run WHERE id = $1")
            .bind(b_run)
            .fetch_one(&db.super_pool)
            .await
            .expect("count B's run");
    assert_eq!(b_run_still, 1);

    let deleted = conveyor_store::prune::model_revision_prune(
        &db.runtime,
        a,
        conveyor_store::prune::MODEL_REVISION_KEEP,
        conveyor_store::prune::PRUNE_BATCH,
    )
    .await
    .expect("second model.prune");
    assert_eq!(deleted, 0, "a second run finds nothing past the cut-off");

    let a_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM revision WHERE tenant_id = $1 AND entity_type = 'model'",
    )
    .bind(a.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("count A revisions");
    assert_eq!(
        a_after,
        a_before - 2,
        "exactly two model revisions were pruned"
    );
}

#[tokio::test]
async fn validation_run_prune_expires_by_age_and_a_pruned_revision_leaves_a_null_link() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    let model = seed_model(&db, a, "validate-me").await;
    let revisions = seed_revisions(&db, a, "model", model, 51).await;
    let prunable = revisions[0].1;

    let old_run = seed_validation_run(&db, a, model, Some(prunable), "91 days").await;
    let fresh_run = seed_validation_run(&db, a, model, Some(prunable), "1 hour").await;

    let b_model = seed_model(&db, b, "b-model").await;
    let b_run = seed_validation_run(&db, b, b_model, None, "91 days").await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        a,
        "validation.prune",
        &json!({}),
    )
    .await
    .expect("validation.prune for A");

    let old_left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM model_validation_run WHERE id = $1")
            .bind(old_run)
            .fetch_one(&db.super_pool)
            .await
            .expect("count the old run");
    let fresh_left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM model_validation_run WHERE id = $1")
            .bind(fresh_run)
            .fetch_one(&db.super_pool)
            .await
            .expect("count the fresh run");
    let b_left: i64 = sqlx::query_scalar("SELECT count(*) FROM model_validation_run WHERE id = $1")
        .bind(b_run)
        .fetch_one(&db.super_pool)
        .await
        .expect("count B's run");
    println!("validation.prune(A): old={old_left} fresh={fresh_left} b={b_left}");
    assert_eq!(old_left, 0, "a 91-day-old run is pruned");
    assert_eq!(fresh_left, 1, "a one-hour-old run is not");
    assert_eq!(
        b_left, 1,
        "B's old run is untouched: the prune ran in A's scope"
    );

    // Prune the revision the fresh run names. 0010's ON DELETE SET NULL must keep the run
    // readable with a NULL link instead of failing the delete (0009's NO ACTION would have
    // refused it) or taking the run with it.
    worker::run_kind(&db.runtime, &config(), None, a, "model.prune", &json!({}))
        .await
        .expect("model.prune for A");
    assert!(
        !revision_seqs(&db, a, model).await.contains(&1),
        "the referenced revision is gone"
    );
    let fresh: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT revision_id FROM model_validation_run WHERE id = $1")
            .bind(fresh_run)
            .fetch_optional(&db.super_pool)
            .await
            .expect("read the fresh run");
    assert_eq!(
        fresh,
        Some(None),
        "the run survives with revision_id IS NULL"
    );

    // B's own prune does delete B's run: it survived A's run because of the tenant scope,
    // not because the row was unprunable.
    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        b,
        "validation.prune",
        &json!({}),
    )
    .await
    .expect("validation.prune for B");
    let b_left: i64 = sqlx::query_scalar("SELECT count(*) FROM model_validation_run WHERE id = $1")
        .bind(b_run)
        .fetch_one(&db.super_pool)
        .await
        .expect("count B's run");
    assert_eq!(b_left, 0, "B's own prune removes B's old run");
}

#[tokio::test]
async fn publish_still_publishes_and_write_head_revision_writes_a_draft() {
    let db = common::TestDb::fresh().await;
    let (a, _) = tenants(&db).await;
    let model = seed_model(&db, a, "frozen").await;

    let current = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let record = get_model_tx(&mut tx, a, model)
            .await
            .expect("read model")
            .expect("exists");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(
        current.version.get(),
        1,
        "a freshly seeded model is version 1"
    );

    // publish_model_tx keeps its contract: published status, the pointer, and a supersede
    // when a later publish runs.
    let first = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let outcome = publish_model_tx(&mut tx, a, model, current.version, None)
            .await
            .expect("first publish");
        tx.commit().await.expect("commit");
        outcome
    };
    assert_eq!(first.seq, 1);
    assert_eq!(first.model.head_revision, 1);
    assert_eq!(first.model.published_revision_id, Some(first.revision_id));
    assert_eq!(first.content_hash.len(), 64, "content_hash is sha-256 hex");

    let after_first = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let record = get_model_tx(&mut tx, a, model)
            .await
            .expect("read model")
            .expect("exists");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(after_first.version.get(), 2);

    // write_head_revision_tx writes a DRAFT at the next dense seq and advances
    // `head_revision` without moving `published_revision_id` or superseding anything.
    let draft = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let outcome = write_head_revision_tx(&mut tx, a, model, after_first.version, None)
            .await
            .expect("write head revision");
        tx.commit().await.expect("commit");
        outcome
    };
    assert_eq!(draft.seq, 2, "the draft takes the next dense seq");
    assert_eq!(draft.model.head_revision, 2, "head_revision advances");
    assert_eq!(
        draft.model.published_revision_id,
        Some(first.revision_id),
        "the published pointer does not move"
    );

    let statuses: Vec<(i32, Uuid, String, String)> = sqlx::query_as(
        "SELECT seq, id, status, label FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2
          ORDER BY seq",
    )
    .bind(a.as_uuid())
    .bind(model)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the revisions");
    assert_eq!(
        statuses,
        vec![
            (
                1,
                first.revision_id,
                "published".to_owned(),
                "published".to_owned()
            ),
            (2, draft.revision_id, "draft".to_owned(), "draft".to_owned()),
        ],
        "the import draft is not a publication and the published revision is untouched"
    );

    // A later publish supersedes the first published revision and moves the pointer.
    let after_draft = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let record = get_model_tx(&mut tx, a, model)
            .await
            .expect("read model")
            .expect("exists");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(after_draft.version.get(), 3);
    let second = {
        let mut tx = begin_scoped(&db.runtime, a).await.expect("scope");
        let outcome = publish_model_tx(&mut tx, a, model, after_draft.version, None)
            .await
            .expect("second publish");
        tx.commit().await.expect("commit");
        outcome
    };
    assert_eq!(second.seq, 3);
    assert_eq!(second.model.published_revision_id, Some(second.revision_id));

    let statuses: Vec<(i32, String)> = sqlx::query_as(
        "SELECT seq, status FROM revision
          WHERE tenant_id = $1 AND entity_type = 'model' AND entity_id = $2
          ORDER BY seq",
    )
    .bind(a.as_uuid())
    .bind(model)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the revisions");
    assert_eq!(
        statuses,
        vec![
            (1, "superseded".to_owned()),
            (2, "draft".to_owned()),
            (3, "published".to_owned()),
        ],
        "publish supersedes the previous published revision; the draft is left alone"
    );
}

async fn stub(status: StatusCode, body: &'static str) -> String {
    let app = Router::new().route(
        "/licenses/active",
        get(move || async move { (status, body) }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("stub");
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn licence_sync_writes_the_local_row_and_maps_absence_to_unknown() {
    let db = common::TestDb::fresh().await;
    let (a, _) = tenants(&db).await;

    let base = stub(
        StatusCode::OK,
        r#"{"active":true,"status":"active","expires_at":"2099-01-01T00:00:00Z"}"#,
    )
    .await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let config = Config::from_lookup(|key| match key {
        "LICENCES_BASE_URL" => Some(base.clone()),
        _ => None,
    })
    .expect("config");
    worker::run_kind(
        &db.runtime,
        &config,
        Some(&client),
        a,
        "licence.sync",
        &json!({}),
    )
    .await
    .expect("licence.sync");
    let row: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, expires_at FROM app_licence WHERE tenant_id = $1")
            .bind(a.as_uuid())
            .fetch_one(&db.super_pool)
            .await
            .expect("the synced row");
    println!("licence.sync -> {row:?}");
    assert_eq!(row.0, "active");
    assert!(row.1.is_some());

    // A 404 means "no active licence": the local row becomes unknown, never active.
    let missing = stub(StatusCode::NOT_FOUND, "{}").await;
    let client = LicenceSyncClient::new(&missing).expect("client");
    worker::run_kind(
        &db.runtime,
        &config,
        Some(&client),
        a,
        "licence.sync",
        &json!({}),
    )
    .await
    .expect("licence.sync on 404");
    let status: String = sqlx::query_scalar("SELECT status FROM app_licence WHERE tenant_id = $1")
        .bind(a.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("row");
    assert_eq!(status, "unknown");
}

#[tokio::test]
async fn licence_sync_without_a_client_and_an_unknown_kind_are_permanent_failures() {
    let db = common::TestDb::fresh().await;
    let (a, _) = tenants(&db).await;

    let failure = worker::run_kind(&db.runtime, &config(), None, a, "licence.sync", &json!({}))
        .await
        .expect_err("unconfigured licence sync");
    assert!(failure.permanent, "{failure}");
    assert!(failure.message.contains("LICENCES_BASE_URL"), "{failure}");

    let failure = worker::run_kind(&db.runtime, &config(), None, a, "no.such.kind", &json!({}))
        .await
        .expect_err("an unknown kind");
    assert!(
        failure.permanent,
        "an unknown kind never retries: {failure}"
    );
    assert!(failure.message.contains("unknown job kind"), "{failure}");
}

#[tokio::test]
async fn the_worker_seeds_and_materialises_schedules_once() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    let inserted = schedule::seed_defaults(&db.runtime)
        .await
        .expect("seed defaults");
    assert_eq!(inserted, 2 * DEFAULT_SCHEDULES.len());

    sqlx::query(
        "UPDATE schedule SET next_run_at = now() - interval '1 second'
          WHERE tenant_id = $1 AND kind = 'event.prune'",
    )
    .bind(a.as_uuid())
    .execute(&db.super_pool)
    .await
    .expect("make it due");

    let now = chrono::Utc::now();
    let (left, right) = tokio::join!(
        schedule::materialise(&db.runtime, a, now),
        schedule::materialise(&db.runtime, a, now)
    );
    let left = left.expect("left");
    let right = right.expect("right");
    println!("concurrent materialise: left={left:?} right={right:?}");
    assert_eq!(left.len() + right.len(), 1, "exactly one job");
    let b_jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE tenant_id = $1")
        .bind(b.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("count B jobs");
    assert_eq!(b_jobs, 0, "B had no due schedule");
}

#[test]
fn the_default_schedule_set_is_the_maintenance_kinds() {
    let kinds: Vec<&str> = DEFAULT_SCHEDULES.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        vec![
            "event.prune",
            "job.prune",
            // The three rules §6 stated and nothing produced until 0012.
            "action_invocation.prune",
            "run.prune",
            "delivery.prune",
            // S08's retention producers (data-architecture §2.4 and §5.3).
            "model.prune",
            "validation.prune",
            "idempotency.sweep",
            "licence.sync",
            // S06's 60 s sweepers, each a schedule row rather than a timer (CONTRACT §7).
            "card.lease_sweep",
            "run.reap",
            // S19 Part B's eval driver pair (its slice's steps 8–9) and Part C's trace retention.
            "eval.run.tick",
            "eval.reap",
            "eval.prune",
            "approval.expire",
            "approval.closed.emit",
            // S06d: multicast the interaction, then close the losing recipients.
            "interaction.dispatch",
            "interaction.close",
            // S10's copilot pair: the streaming-turn reap and the proposal sweep.
            "copilot.turn.reap",
            "copilot.proposal.sweep",
            "benchmark.reap"
        ]
    );
}

/// `JobFailure` must be displayable: the worker logs it into `last_error`.
#[test]
fn job_failure_messages_are_stable() {
    let retryable = JobFailure::retryable("temporary");
    assert!(!retryable.permanent);
    assert_eq!(retryable.message, "temporary");
    assert_eq!(retryable.to_string(), "temporary");
}

// ── the three retention rules `0012` gave a producer ────────────────────────────────────────────────
//
// Each fixture is raw SQL as the SUPERUSER with an explicit age, so the test measures the RULE and not
// the clock. Every prune runs through `worker::run_kind`, i.e. the same dispatch arm production uses.

/// One tenant plus the project a `run` needs. `tenants()` above seeds only tenants.
async fn seed_project(db: &common::TestDb, tenant: TenantId, key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, project_type_code)
         VALUES ($1, $2, $2, 'delivery') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a project")
}

#[tokio::test]
async fn action_invocation_prune_keeps_the_window_and_the_other_tenant() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    let old = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO action_invocation (tenant_id, action, input_hash, outcome, created_at)
         VALUES ($1, 'cards.get', 'x', 'ok', now() - interval '31 days') RETURNING id",
    )
    .bind(a.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed an old invocation");
    let fresh = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO action_invocation (tenant_id, action, input_hash, outcome)
         VALUES ($1, 'cards.get', 'x', 'ok') RETURNING id",
    )
    .bind(a.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a fresh invocation");
    let foreign = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO action_invocation (tenant_id, action, input_hash, outcome, created_at)
         VALUES ($1, 'cards.get', 'x', 'ok', now() - interval '31 days') RETURNING id",
    )
    .bind(b.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed B's old invocation");

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        a,
        "action_invocation.prune",
        &json!({}),
    )
    .await
    .expect("action_invocation.prune for A");

    let left = |id: Uuid| {
        let pool = db.super_pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM action_invocation WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("count")
        }
    };
    assert_eq!(left(old).await, 0, "a 31-day-old invocation is pruned");
    assert_eq!(left(fresh).await, 1, "a fresh invocation stays");
    assert_eq!(
        left(foreign).await,
        1,
        "B's old invocation is untouched: the prune ran in A's scope"
    );
}

#[tokio::test]
async fn run_prune_detaches_the_approval_and_skips_a_pending_gate() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;
    let project = seed_project(&db, a, "run-prune").await;
    let b_project = seed_project(&db, b, "run-prune-b").await;

    let seed_run = |tenant: TenantId, project: Uuid, status: &'static str, age: &'static str| {
        let pool = db.super_pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO run (tenant_id, project_id, status, started_at, finished_at, \
                                  created_at, updated_at)
                 VALUES ($1, $2, $3, now() - $4::interval, now() - $4::interval,
                         now() - $4::interval, now() - $4::interval) RETURNING id",
            )
            .bind(tenant.as_uuid())
            .bind(project)
            .bind(status)
            .bind(age)
            .fetch_one(&pool)
            .await
            .expect("seed a run")
        }
    };

    let old = seed_run(a, project, "succeeded", "181 days").await;
    let fresh = seed_run(a, project, "succeeded", "1 hour").await;
    let live = seed_run(a, project, "running", "181 days").await;
    let foreign = seed_run(b, b_project, "succeeded", "181 days").await;

    // An answered gate on the old run, and a pending gate on the live one.
    let approval = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO approval (tenant_id, run_id, gate, kind, status)
         VALUES ($1, $2, 'review', 'single', 'approved') RETURNING id",
    )
    .bind(a.as_uuid())
    .bind(old)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed an answered approval");
    // A pending gate on a TERMINAL run is the anomaly the prune must refuse to resolve for us.
    let terminal_pending = seed_run(a, project, "failed", "181 days").await;
    sqlx::query(
        "INSERT INTO approval (tenant_id, run_id, gate, kind, status)
         VALUES ($1, $2, 'review', 'single', 'pending')",
    )
    .bind(a.as_uuid())
    .bind(terminal_pending)
    .execute(&db.super_pool)
    .await
    .expect("seed a pending approval on an old run");

    worker::run_kind(&db.runtime, &config(), None, a, "run.prune", &json!({}))
        .await
        .expect("run.prune for A");

    let count_run = |id: Uuid| {
        let pool = db.super_pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM run WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("count")
        }
    };
    assert_eq!(
        count_run(old).await,
        0,
        "a terminal 181-day-old run is pruned"
    );
    assert_eq!(count_run(fresh).await, 1, "a one-hour-old run stays");
    assert_eq!(count_run(live).await, 1, "a running run is never pruned");
    assert_eq!(count_run(foreign).await, 1, "B's old run is untouched");
    assert_eq!(
        count_run(terminal_pending).await,
        1,
        "a terminal run with an unanswered gate is skipped, not pruned"
    );

    // The human decision survives the run it belonged to: 0012 is ON DELETE SET NULL.
    let (linked, status): (Option<Uuid>, String) =
        sqlx::query_as("SELECT run_id, status FROM approval WHERE id = $1")
            .bind(approval)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the approval");
    assert!(linked.is_none(), "the approval is detached, not deleted");
    assert_eq!(status, "approved", "the decision itself is unchanged");
}

#[tokio::test]
async fn delivery_prune_keeps_live_states_and_the_other_tenant() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    let seed = |tenant: TenantId, state: &'static str, age: &'static str| {
        let pool = db.super_pool.clone();
        async move {
            let tenant = tenant.as_uuid();
            let message: Uuid = sqlx::query_scalar(
                "INSERT INTO message (tenant_id, channel_kind, body) VALUES ($1, 'web', 'x') \
                 RETURNING id",
            )
            .bind(tenant)
            .fetch_one(&pool)
            .await
            .expect("seed a message");
            sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO delivery (tenant_id, message_id, channel_kind, recipient_ref, state, \
                                       created_at, updated_at)
                 VALUES ($1, $2, 'web', $3, $4, now() - $5::interval, now() - $5::interval) \
                 RETURNING id",
            )
            .bind(tenant)
            .bind(message)
            .bind(format!("user-{state}-{age}-{tenant}"))
            .bind(state)
            .bind(age)
            .fetch_one(&pool)
            .await
            .expect("seed a delivery")
        }
    };

    let old_delivered = seed(a, "delivered", "31 days").await;
    let old_failed = seed(a, "failed", "31 days").await;
    let fresh = seed(a, "delivered", "1 hour").await;
    let pending = seed(a, "pending", "31 days").await;
    let sent = seed(a, "sent", "31 days").await;
    let foreign = seed(b, "delivered", "31 days").await;

    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        a,
        "delivery.prune",
        &json!({}),
    )
    .await
    .expect("delivery.prune for A");

    let count = |id: Uuid| {
        let pool = db.super_pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM delivery WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("count")
        }
    };
    assert_eq!(
        count(old_delivered).await,
        0,
        "a terminal delivery past 30 days is pruned"
    );
    assert_eq!(count(old_failed).await, 0, "failed is terminal too");
    assert_eq!(count(fresh).await, 1, "a one-hour-old delivery stays");
    assert_eq!(
        count(pending).await,
        1,
        "pending is the drainer's queue, never pruned"
    );
    assert_eq!(count(sent).await, 1, "sent is still live, never pruned");
    assert_eq!(count(foreign).await, 1, "B's old delivery is untouched");
}

#[tokio::test]
async fn eval_prune_removes_old_traces_and_keeps_fresh_ones_in_its_tenant_only() {
    let db = common::TestDb::fresh().await;
    let (a, b) = tenants(&db).await;

    let seed = |tenant: TenantId, age_days: i64, rep: i32| {
        let pool = db.super_pool.clone();
        async move {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO eval_trace (tenant_id, eval_run_id, case_id, rep, turns, sha256, bytes, created_at)
                 VALUES ($1, $2, $3, $4, '[{\"role\":\"user\",\"content\":\"hi\"}]', 'h', 4,
                         now() - make_interval(days => $5))
                 RETURNING id",
            )
            .bind(tenant.as_uuid())
            .bind(Uuid::now_v7())
            .bind(Uuid::now_v7())
            .bind(rep)
            .bind(age_days as i32)
            .fetch_one(&pool)
            .await
            .expect("seed an eval trace");
            id
        }
    };
    let a_old = seed(a, 91, 0).await;
    let a_fresh = seed(a, 1, 1).await;
    let b_old = seed(b, 91, 0).await;

    worker::run_kind(&db.runtime, &config(), None, a, "eval.prune", &json!({}))
        .await
        .expect("eval.prune for A");

    let count = |id: Uuid| {
        let pool = db.super_pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM eval_trace WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("count")
        }
    };
    assert_eq!(count(a_old).await, 0, "a 91-day-old transcript is pruned");
    assert_eq!(count(a_fresh).await, 1, "a one-day-old transcript stays");
    assert_eq!(
        count(b_old).await,
        1,
        "B's old transcript is untouched: the prune is tenant-scoped"
    );
}
