//! S07 §5.4 / S06 §5.3: `project_request.timeout` expires an unresolved intake after 24 h.
//!
//! The test drives the same dispatch production uses (`worker::run_kind`), never the handler
//! directly: a schedule row that materialises a job of this kind would otherwise reach an
//! unhandled arm. One request is backdated past the window and one is left fresh in the SAME
//! tenant and pass, so the rule under test is the age and nothing else. A second run proves
//! the status guard is the idempotency witness.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::{Config, worker};
use serde_json::json;
use uuid::Uuid;

fn config() -> Config {
    Config::from_lookup(|_| None).expect("a default config")
}

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    old_id: Uuid,
    fresh_id: Uuid,
}

async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('pr-timeout', 'PR timeout') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let old_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_request (tenant_id, external_ref, external_origin, created_at)
         VALUES ($1, 'cp-old', 'control-plane', now() - interval '25 hours') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the stale request");
    let fresh_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_request (tenant_id, external_ref, external_origin)
         VALUES ($1, 'cp-fresh', 'control-plane') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the fresh request");
    Fixture {
        tenant,
        tenant_id,
        old_id,
        fresh_id,
    }
}

async fn status(db: &common::TestDb, id: Uuid) -> (String, i64) {
    sqlx::query_as("SELECT status, version FROM project_request WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the request")
}

async fn expired_events(db: &common::TestDb, tenant: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = 'project_request.expired'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the expiry events")
}

async fn sweep(db: &common::TestDb, tenant: TenantId) {
    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "project_request.timeout",
        &json!({}),
    )
    .await
    .unwrap_or_else(|error| panic!("project_request.timeout: {error:?}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_timeout_expires_a_stale_draft_once_and_never_a_fresh_one() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;

    sweep(&db, fixture.tenant).await;

    let (old_status, old_version) = status(&db, fixture.old_id).await;
    assert_eq!(
        old_status, "expired",
        "25 h of draft is past the 24 h window"
    );
    assert_eq!(old_version, 2, "the expiry bumps the visible version");
    let (fresh_status, fresh_version) = status(&db, fixture.fresh_id).await;
    assert_eq!(
        fresh_status, "draft",
        "a fresh draft is unresolved, not expired"
    );
    assert_eq!(fresh_version, 1, "a fresh draft is not touched");
    assert_eq!(
        expired_events(&db, fixture.tenant_id).await,
        1,
        "exactly one terminal event for the expired row"
    );

    // The row is kept, not deleted: its identity and payload still read.
    let (external_ref, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT external_ref, payload FROM project_request WHERE id = $1")
            .bind(fixture.old_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("the expired row survives the sweep");
    assert_eq!(external_ref, "cp-old");
    assert_eq!(payload, json!({}));

    // Idempotent: the sweep matches only `draft`, so a second run writes no row and no event.
    sweep(&db, fixture.tenant).await;
    assert_eq!(
        status(&db, fixture.old_id).await,
        ("expired".to_owned(), 2),
        "a second sweep cannot re-expire or bump the row"
    );
    assert_eq!(
        expired_events(&db, fixture.tenant_id).await,
        1,
        "the second run announces nothing"
    );
    assert_eq!(status(&db, fixture.fresh_id).await, ("draft".to_owned(), 1));
}
