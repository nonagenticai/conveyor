//! `ensure_bundle_grants` against a real Postgres 18: the bundle hold, the least-privilege grant
//! matrix, the union across bundled apps, the app-scoped dependency grants, and idempotency.
//!
//! Ported from `models.py`:5733 `bundle_grantees` and :5727 `ensure_bundle_grants`.

mod common;

use conveyor_licenses::db;
use conveyor_licenses::db::bundle_grants::{PLATFORM_OWNER, bundle_grantees, ensure_bundle_grants};
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const APP_OWN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
const APP_SINGLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const APP_TWIN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const APP_MULTI: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const APP_MULTI_PLATFORM: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0006);
const APP_DEMO: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
const BUNDLE_OWN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0011);
const BUNDLE_SINGLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0012);
const BUNDLE_MULTI: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0013);
const BUNDLE_MULTI_PLATFORM: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0014);
const BUNDLE_UNION: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0015);
const BUNDLE_DEPS: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0016);

/// One tenant owned by `owner-a`, the four issuer shapes the matrix distinguishes, and the bundles
/// that isolate each.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    sqlx::query(
        "INSERT INTO tenant_owner (key, display_name)
         VALUES ('owner-a', 'Owner A'), ('owner-b', 'Owner B'),
                ('owner-c', 'Owner C'), ('platform', 'Platform')",
    )
    .execute(&db.pool)
    .await
    .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key)
         VALUES ($1, 'tenant-a', now(), 'owner-a')",
    )
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("tenants");
    sqlx::query(
        "INSERT INTO app (id, key, name, created_at)
         VALUES ($1, 'own', 'Own', now()),
                ($2, 'single', 'Single', now()),
                ($3, 'twin', 'Twin', now()),
                ($4, 'multi', 'Multi', now()),
                ($5, 'multiplat', 'MultiPlatform', now()),
                ($6, 'demo', 'Demo', now())",
    )
    .bind(APP_OWN)
    .bind(APP_SINGLE)
    .bind(APP_TWIN)
    .bind(APP_MULTI)
    .bind(APP_MULTI_PLATFORM)
    .bind(APP_DEMO)
    .execute(&db.pool)
    .await
    .expect("apps");
    sqlx::query(
        "INSERT INTO app_issuer (app_id, owner_key)
         VALUES ($1, 'owner-a'),
                ($2, 'owner-b'),
                ($3, 'owner-b'),
                ($4, 'owner-b'), ($4, 'owner-c'),
                ($5, 'owner-c'), ($5, 'platform')",
    )
    .bind(APP_OWN)
    .bind(APP_SINGLE)
    .bind(APP_TWIN)
    .bind(APP_MULTI)
    .bind(APP_MULTI_PLATFORM)
    .execute(&db.pool)
    .await
    .expect("app_issuer");
    sqlx::query(
        "INSERT INTO bundle (id, key, name, created_at)
         VALUES ($1, 'b-own', 'b-own', now()),
                ($2, 'b-single', 'b-single', now()),
                ($3, 'b-multi', 'b-multi', now()),
                ($4, 'b-multiplat', 'b-multiplat', now()),
                ($5, 'b-union', 'b-union', now()),
                ($6, 'b-deps', 'b-deps', now())",
    )
    .bind(BUNDLE_OWN)
    .bind(BUNDLE_SINGLE)
    .bind(BUNDLE_MULTI)
    .bind(BUNDLE_MULTI_PLATFORM)
    .bind(BUNDLE_UNION)
    .bind(BUNDLE_DEPS)
    .execute(&db.pool)
    .await
    .expect("bundles");
    sqlx::query(
        "INSERT INTO bundle_app (bundle_id, app_id, created_at)
         VALUES ($1, $2, now()),
                ($3, $4, now()),
                ($5, $6, now()),
                ($7, $8, now()),
                ($9, $4, now()), ($9, $10, now())",
    )
    .bind(BUNDLE_OWN)
    .bind(APP_OWN)
    .bind(BUNDLE_SINGLE)
    .bind(APP_SINGLE)
    .bind(BUNDLE_MULTI)
    .bind(APP_MULTI)
    .bind(BUNDLE_MULTI_PLATFORM)
    .bind(APP_MULTI_PLATFORM)
    .bind(BUNDLE_UNION)
    .bind(APP_TWIN)
    .execute(&db.pool)
    .await
    .expect("bundle_app");
    db
}

async fn ensure(
    db: &common::TestDb,
    bundle: Uuid,
    granted_by: &str,
    hold: bool,
    dependency_grants: &[(String, String)],
) -> Vec<String> {
    ensure_bundle_grants(
        &db.pool,
        TENANT,
        bundle,
        granted_by,
        hold,
        dependency_grants,
    )
    .await
    .expect("ensure_bundle_grants")
}

async fn bundle_rows(db: &common::TestDb, bundle: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM tenant_bundle WHERE tenant_id = $1 AND bundle_id = $2")
        .bind(TENANT)
        .bind(bundle)
        .fetch_one(&db.pool)
        .await
        .expect("tenant_bundle count")
}

async fn grants(db: &common::TestDb) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT grantee_owner_key FROM tenant_app_grant
          WHERE tenant_id = $1 ORDER BY grantee_owner_key",
    )
    .bind(TENANT)
    .fetch_all(&db.pool)
    .await
    .expect("tenant_app_grant rows")
}

async fn dependency_rows(db: &common::TestDb) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT g.owner_key, a.key
           FROM tenant_dependency_grant g
           JOIN app a ON a.id = g.app_id
          WHERE g.tenant_id = $1
          ORDER BY 1, 2",
    )
    .bind(TENANT)
    .fetch_all(&db.pool)
    .await
    .expect("tenant_dependency_grant rows")
}

#[test]
fn the_grantee_matrix_is_least_privilege() {
    let issuers = |keys: &[&str]| {
        keys.iter()
            .map(|key| (*key).to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    // The tenant's own owner serves the app: nobody.
    assert!(bundle_grantees(Some("owner-a"), &issuers(&["owner-a"])).is_empty());
    // One issuer: it.
    assert_eq!(
        bundle_grantees(Some("owner-a"), &issuers(&["owner-b"])),
        issuers(&["owner-b"])
    );
    // Several issuers, none platform: no app owner either.
    assert!(bundle_grantees(Some("owner-a"), &issuers(&["owner-b", "owner-c"])).is_empty());
    // Several issuers including platform: platform only.
    assert_eq!(
        bundle_grantees(Some("owner-a"), &issuers(&["owner-c", PLATFORM_OWNER])),
        issuers(&[PLATFORM_OWNER])
    );
    // No issuer at all: nobody.
    assert!(bundle_grantees(Some("owner-a"), &issuers(&[])).is_empty());
}

#[tokio::test]
async fn the_matrix_over_the_database_grants_least_privilege() {
    let db = seeded().await;

    // The tenant's own owner is the only issuer of `own`: no grant.
    assert!(
        ensure(&db, BUNDLE_OWN, "owner-a", false, &[])
            .await
            .is_empty()
    );
    assert!(grants(&db).await.is_empty());

    // Several issuers, no platform: no grant at all (not even an app owner).
    assert!(
        ensure(&db, BUNDLE_MULTI, "owner-a", false, &[])
            .await
            .is_empty()
    );
    assert!(grants(&db).await.is_empty());

    // Several issuers including platform: platform, and only platform.
    assert_eq!(
        ensure(&db, BUNDLE_MULTI_PLATFORM, "owner-a", false, &[]).await,
        vec![PLATFORM_OWNER.to_owned()]
    );
    assert_eq!(grants(&db).await, vec![PLATFORM_OWNER.to_owned()]);

    // A single issuer: that issuer.
    assert_eq!(
        ensure(&db, BUNDLE_SINGLE, "owner-a", false, &[]).await,
        vec!["owner-b".to_owned()]
    );
    assert_eq!(
        grants(&db).await,
        vec!["owner-b".to_owned(), PLATFORM_OWNER.to_owned()]
    );
}

#[tokio::test]
async fn several_bundled_apps_union_their_grantees_without_duplicates() {
    let db = seeded().await;

    // `single` and `twin` both grant owner-b; the union is ONE grant.
    assert_eq!(
        ensure(&db, BUNDLE_UNION, "owner-a", false, &[]).await,
        vec!["owner-b".to_owned()]
    );
    assert_eq!(grants(&db).await, vec!["owner-b".to_owned()]);

    // Nothing new on an identical second call.
    assert!(
        ensure(&db, BUNDLE_UNION, "owner-a", false, &[])
            .await
            .is_empty()
    );
    assert_eq!(grants(&db).await.len(), 1);
}

#[tokio::test]
async fn the_hold_is_recorded_once_and_every_write_is_idempotent() {
    let db = seeded().await;

    // hold=false: the grant lands, the hold does not.
    assert_eq!(
        ensure(&db, BUNDLE_SINGLE, "owner-a", false, &[]).await,
        vec!["owner-b".to_owned()]
    );
    assert_eq!(bundle_rows(&db, BUNDLE_SINGLE).await, 0);
    assert_eq!(grants(&db).await, vec!["owner-b".to_owned()]);

    // hold=true: the hold lands; the existing grant is not returned again.
    assert!(
        ensure(&db, BUNDLE_SINGLE, "owner-a", true, &[])
            .await
            .is_empty()
    );
    assert_eq!(bundle_rows(&db, BUNDLE_SINGLE).await, 1);

    // A second identical call writes and returns nothing.
    assert!(
        ensure(&db, BUNDLE_SINGLE, "owner-a", true, &[])
            .await
            .is_empty()
    );
    assert_eq!(bundle_rows(&db, BUNDLE_SINGLE).await, 1);
    assert_eq!(grants(&db).await, vec!["owner-b".to_owned()]);
}

#[tokio::test]
async fn dependency_grants_are_app_scoped_written_once_and_skip_the_tenant_owner() {
    let db = seeded().await;
    let deps = vec![
        ("owner-b".to_owned(), "demo".to_owned()),
        // The tenant's own owner is never a dependency grantee.
        ("owner-a".to_owned(), "demo".to_owned()),
        // Duplicates collapse.
        ("owner-b".to_owned(), "demo".to_owned()),
        // An unknown app key writes nothing (0 rows from the INSERT ... SELECT).
        ("owner-c".to_owned(), "no-such-app".to_owned()),
    ];

    assert_eq!(
        ensure(&db, BUNDLE_DEPS, "owner-b", false, &deps).await,
        vec!["owner-b:demo".to_owned()]
    );
    assert_eq!(
        dependency_rows(&db).await,
        vec![("owner-b".to_owned(), "demo".to_owned())]
    );
    assert_eq!(bundle_rows(&db, BUNDLE_DEPS).await, 0);

    // Idempotent: no new row, nothing returned.
    assert!(
        ensure(&db, BUNDLE_DEPS, "owner-b", false, &deps)
            .await
            .is_empty()
    );
    assert_eq!(dependency_rows(&db).await.len(), 1);
}
