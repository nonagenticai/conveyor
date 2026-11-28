//! Audit joins the caller's transaction; append-only by GRANT; POST replay is verbatim.
//!
//! `cargo test -p conveyor-store --test audit_idempotency -- --nocapture`.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_store::audit::{self, AuditEntry, MAX_DETAILS_BYTES};
use conveyor_store::begin_scoped;
use conveyor_store::idempotency::{self, Lookup};
use serde_json::json;
use uuid::Uuid;

async fn a_tenant(db: &common::TestDb) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('audit-a', 'Audit A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

fn entry(request_id: &str) -> AuditEntry {
    AuditEntry {
        actor_id: None,
        actor_kind: "service".into(),
        action: "cards.move".into(),
        status: "ok".into(),
        resource_type: Some("card".into()),
        resource_id: Some(Uuid::now_v7()),
        request_id: Some(request_id.into()),
        details: json!({"from": "active", "to": "done"}),
    }
}

#[tokio::test]
async fn audit_record_rolls_back_with_the_callers_transaction_and_records_the_request_id() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;

    // A rolled-back mutation takes its audit row with it.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    audit::record(&mut tx, tenant, &entry("rolled-back"))
        .await
        .expect("record inside the transaction");
    tx.rollback().await.expect("rollback");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_event")
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    assert_eq!(
        count, 0,
        "the audit row must not outlive a rolled-back write"
    );

    // A committed transaction persists it, with X-Request-Id.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let id = audit::record(&mut tx, tenant, &entry("req-9"))
        .await
        .expect("record");
    tx.commit().await.expect("commit");
    let (stored_id, request_id): (Uuid, Option<String>) =
        sqlx::query_as("SELECT id, request_id FROM audit_event")
            .fetch_one(&db.super_pool)
            .await
            .expect("read the row");
    println!("audit row {stored_id} request_id={request_id:?}");
    assert_eq!(stored_id, id);
    assert_eq!(request_id.as_deref(), Some("req-9"));
}

#[tokio::test]
async fn append_only_is_a_grant_not_a_convention() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    audit::record(&mut tx, tenant, &entry("append-only"))
        .await
        .expect("record");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let update = sqlx::query("UPDATE audit_event SET status = 'error'")
        .execute(&mut *tx)
        .await;
    let error = update.expect_err("the runtime role must not UPDATE audit_event");
    let code = match &error {
        sqlx::Error::Database(database) => database.code().map(|c| c.into_owned()),
        other => panic!("expected a database error, got {other:?}"),
    };
    println!("audit UPDATE refused: {code:?} {error}");
    assert_eq!(code.as_deref(), Some("42501"));
    tx.rollback().await.expect("rollback");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let delete = sqlx::query("DELETE FROM audit_event")
        .execute(&mut *tx)
        .await;
    assert!(
        delete.is_err(),
        "the runtime role must not DELETE audit_event"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn audit_refuses_secrets_oversized_details_and_bad_vocabulary() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");

    let mut secret = entry("x");
    secret.details = json!({"nested": {"password": "hunter2"}});
    assert!(matches!(
        audit::record(&mut tx, tenant, &secret).await,
        Err(audit::AuditError::ForbiddenDetail { what: "key", .. })
    ));

    let mut big = entry("x");
    big.details = json!({"blob": "x".repeat(MAX_DETAILS_BYTES)});
    assert!(matches!(
        audit::record(&mut tx, tenant, &big).await,
        Err(audit::AuditError::DetailsTooLarge { .. })
    ));

    let mut bad_action = entry("x");
    bad_action.action = "Cards.move".into();
    assert!(matches!(
        audit::record(&mut tx, tenant, &bad_action).await,
        Err(audit::AuditError::InvalidAction(_))
    ));

    let mut bad_status = entry("x");
    bad_status.status = "maybe".into();
    assert!(matches!(
        audit::record(&mut tx, tenant, &bad_status).await,
        Err(audit::AuditError::InvalidStatus(_))
    ));

    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn idempotent_replay_returns_the_original_status_and_body() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let scope = "cards.create";
    let key = "client-key-1";
    let body = json!({"title": "Hello"});
    let hash = idempotency::request_hash(&body);

    // First request: a miss, then the response is stored in the same transaction.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert_eq!(
        idempotency::lookup(&mut tx, tenant, scope, key, &hash)
            .await
            .expect("lookup"),
        Lookup::Miss
    );
    let created = idempotency::save(
        &mut tx,
        tenant,
        scope,
        key,
        &hash,
        201,
        &json!({"id": "card-1", "title": "Hello"}),
    )
    .await
    .expect("save");
    assert!(created);
    tx.commit().await.expect("commit");

    // The replay returns the original status and body verbatim.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let lookup = idempotency::lookup(&mut tx, tenant, scope, key, &hash)
        .await
        .expect("lookup");
    tx.commit().await.expect("commit");
    println!("replay: {lookup:?}");
    assert_eq!(
        lookup,
        Lookup::Replay {
            status_code: 201,
            response: json!({"id": "card-1", "title": "Hello"}),
        }
    );

    // The same key with a different body is a conflict, not a replay.
    let other = idempotency::request_hash(&json!({"title": "Different"}));
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert_eq!(
        idempotency::lookup(&mut tx, tenant, scope, key, &other)
            .await
            .expect("lookup"),
        Lookup::ConflictHash
    );
    assert!(
        !idempotency::save(&mut tx, tenant, scope, key, &other, 200, &json!({}))
            .await
            .expect("conflicting save"),
        "a conflicting hash must not overwrite the stored response"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn an_in_flight_key_is_not_a_replay_and_an_expired_key_is_a_miss() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let scope = "cards.create";
    let hash = idempotency::request_hash(&json!({"title": "x"}));

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert!(
        idempotency::begin(
            &mut tx,
            tenant,
            scope,
            "k",
            &hash,
            chrono::Duration::hours(idempotency::TTL_HOURS)
        )
        .await
        .expect("begin")
    );
    assert!(
        !idempotency::begin(
            &mut tx,
            tenant,
            scope,
            "k",
            &hash,
            chrono::Duration::hours(idempotency::TTL_HOURS)
        )
        .await
        .expect("second begin"),
        "a second in-flight request for the same key is not admitted"
    );
    assert!(
        matches!(
            idempotency::lookup(&mut tx, tenant, scope, "k", &hash)
                .await
                .expect("lookup"),
            Lookup::InFlight { .. }
        ),
        "an unsettled row is in flight, and carries the moment it stops blocking"
    );
    tx.commit().await.expect("commit");

    sqlx::query("UPDATE idempotency_key SET expires_at = now() - interval '1 hour'")
        .execute(&db.super_pool)
        .await
        .expect("expire the key");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert_eq!(
        idempotency::lookup(&mut tx, tenant, scope, "k", &hash)
            .await
            .expect("lookup"),
        Lookup::Miss,
        "an expired key is a miss even before the sweep deletes it"
    );
    // `Miss` must also mean "you may run": `begin` reclaims the expired row. Without this the
    // key was permanently stuck between its TTL and the sweep — `lookup` said Miss while the
    // INSERT's conflict said "taken", and the caller answered a 409 it could never clear.
    assert!(
        idempotency::begin(
            &mut tx,
            tenant,
            scope,
            "k",
            &hash,
            chrono::Duration::hours(idempotency::TTL_HOURS)
        )
        .await
        .expect("reclaim the expired key"),
        "an expired key is reclaimable; a live one is not"
    );
    assert!(
        idempotency::save(
            &mut tx,
            tenant,
            scope,
            "k",
            &hash,
            200,
            &json!({"ok": true})
        )
        .await
        .expect("save the reclaimed key"),
        "the reclaimed row accepts the retry's response"
    );
    assert_eq!(
        idempotency::lookup(&mut tx, tenant, scope, "k", &hash)
            .await
            .expect("lookup after reclaim"),
        Lookup::Replay {
            status_code: 200,
            response: json!({"ok": true}),
        },
        "the retry is stored and replays like any first request"
    );
    tx.commit().await.expect("commit");
}

/// An in-flight claim lives as long as the CALL that holds it, never as long as the replay window.
///
/// conveyor#900: a deploy SIGKILLed the worker mid-`inference.chat`; because the in-flight row
/// carried the 24-hour contract TTL, every retry of that case answered `409 … still running` and
/// the producer wrote a terminal `no_attempt`. The claim must instead stop blocking once the
/// engine's own deadline has passed, because after that no legitimate holder can still be running.
#[test]
fn an_in_flight_ttl_is_the_calls_deadline_plus_a_margin() {
    assert_eq!(
        idempotency::in_flight_ttl(300_000).num_seconds(),
        300 + idempotency::IN_FLIGHT_MARGIN_SECS,
        "inference.chat declares a 300 s deadline; its claim may not outlive it by more than the \
         bookkeeping margin"
    );
    assert_eq!(
        idempotency::in_flight_ttl(0).num_seconds(),
        idempotency::IN_FLIGHT_MARGIN_SECS,
        "no call gets a claim that expires before it could finish"
    );
    assert!(
        idempotency::in_flight_ttl(300_000) < chrono::Duration::hours(idempotency::TTL_HOURS),
        "an in-flight claim is never the replay TTL"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_in_flight_claim_is_reclaimable_once_its_own_ttl_passes() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let scope = "inference.chat";
    let hash = idempotency::request_hash(&json!({"model": "local-llm"}));
    // Long enough that the assertion below cannot lose a race with the database: the point is the
    // TTL's BOUND, not its millisecond precision. A 20 ms window failed on CI once (#909) because
    // the second transaction's round trip outlasted it.
    let live = chrono::Duration::milliseconds(750);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert!(
        idempotency::begin(&mut tx, tenant, scope, "k", &hash, live)
            .await
            .expect("begin")
    );
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert!(
        !idempotency::begin(&mut tx, tenant, scope, "k", &hash, live)
            .await
            .expect("a live claim still refuses a replay"),
        "a claim inside its own window still blocks"
    );
    tx.rollback().await.expect("rollback");

    tokio::time::sleep(std::time::Duration::from_millis(2_000)).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    assert!(
        idempotency::begin(&mut tx, tenant, scope, "k", &hash, live)
            .await
            .expect("reclaim the dead holder's claim"),
        "once the call's own deadline has passed, the claim is reclaimable — a killed holder must \
         not refuse retries for the 24-hour replay TTL"
    );
    tx.commit().await.expect("commit");
}
