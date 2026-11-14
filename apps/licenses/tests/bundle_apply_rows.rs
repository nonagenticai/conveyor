//! The bundle-ensure ROW WRITES against a real Postgres 18.
//!
//! Adoption's compare-and-set (including dependency promotion), the subject create-or-get and its
//! two race/refusal shapes, the licence insert's full column set, the lost-insert re-read, and the
//! migration-029 attribution rule.

mod common;

use chrono::{DateTime, Utc};
use conveyor_licenses::db;
use conveyor_licenses::db::bundle_apply_rows::{
    CreateLicenceError, CreateSubjectError, LostRaceOutcome, NewLicence, adopt_license_into_bundle,
    attribute_subject_to_tenant, create_license, create_or_get_subject, resolve_lost_insert,
};
use serde_json::json;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const OTHER_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
const MACHINE_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const PLAN_PRO: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0006);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
const BUNDLE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0008);
const SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0009);
const SVC2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000a);
const SVC3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000b);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000c);
const LICENCE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000d);
const LICENCE3: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000e);
const LICENCE4: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000f);
const LICENCE5: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0010);
const UNKNOWN_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_00ff);

/// Two org tenants, one machine tenant, one app with two plans, and two bundles.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-a', 'Owner A')")
        .execute(&db.pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, kind, byo_mode)
         VALUES ($1, 'tenant-a', now(), 'owner-a', 'customer', 'byo_required'),
                ($2, 'tenant-b', now(), 'owner-a', 'customer', 'byo_required'),
                ($3, 'machine-svc', now(), 'owner-a', 'service', 'platform_only')",
    )
    .bind(TENANT)
    .bind(OTHER_TENANT)
    .bind(MACHINE_TENANT)
    .execute(&db.pool)
    .await
    .expect("tenants");
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, 'demo', 'Demo', now())")
        .bind(APP)
        .execute(&db.pool)
        .await
        .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, token_budget_monthly, quota_monthly, created_at)
         VALUES ($1, $3, 'basic', 'Basic', 1000, 100, now()),
                ($2, $3, 'pro', 'Pro', 2000, 200, now())",
    )
    .bind(PLAN)
    .bind(PLAN_PRO)
    .bind(APP)
    .execute(&db.pool)
    .await
    .expect("plans");
    sqlx::query(
        "INSERT INTO bundle (id, key, name, created_at)
         VALUES ($1, 'bundle-a', 'Bundle A', now()), ($2, 'bundle-b', 'Bundle B', now())",
    )
    .bind(BUNDLE)
    .bind(BUNDLE2)
    .execute(&db.pool)
    .await
    .expect("bundles");
    db
}

async fn subject(db: &common::TestDb, id: Uuid, keycloak_sub: &str) {
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'service', now())",
    )
    .bind(id)
    .bind(keycloak_sub)
    .execute(&db.pool)
    .await
    .expect("subject");
}

struct Licence {
    id: Uuid,
    subject: Uuid,
    tenant: Uuid,
    plan: Uuid,
    bundle: Option<Uuid>,
    dependency: bool,
    status: &'static str,
}

fn licence(id: Uuid, subject: Uuid) -> Licence {
    Licence {
        id,
        subject,
        tenant: TENANT,
        plan: PLAN,
        bundle: None,
        dependency: false,
        status: "active",
    }
}

async fn insert_licence(db: &common::TestDb, licence: Licence) {
    sqlx::query(
        "INSERT INTO license (
             id, subject_id, app_id, plan_id, status, starts_at, tenant_id, bundle_id,
             is_dependency)
         VALUES ($1, $2, $3, $4, $5, now(), $6, $7, $8)",
    )
    .bind(licence.id)
    .bind(licence.subject)
    .bind(APP)
    .bind(licence.plan)
    .bind(licence.status)
    .bind(licence.tenant)
    .bind(licence.bundle)
    .bind(licence.dependency)
    .execute(&db.pool)
    .await
    .expect("license");
}

#[derive(Debug, sqlx::FromRow)]
struct LicenceRow {
    subject_id: Uuid,
    app_id: Uuid,
    plan_id: Uuid,
    tenant_id: Uuid,
    bundle_id: Option<Uuid>,
    is_dependency: bool,
    token_budget_override: Option<i32>,
    quota_override: Option<i32>,
    metadata: Option<serde_json::Value>,
    status: String,
    starts_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn licence_row(db: &common::TestDb, id: Uuid) -> LicenceRow {
    sqlx::query_as(
        "SELECT subject_id, app_id, plan_id, tenant_id, bundle_id, is_dependency,
                token_budget_override, quota_override, metadata, status, starts_at, updated_at
           FROM license WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db.pool)
    .await
    .expect("licence row")
}

async fn subject_tenant(db: &common::TestDb, id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT tenant_id FROM subject WHERE id = $1")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("subject tenant")
}

#[tokio::test]
async fn adoption_is_a_compare_and_set_and_promotes_a_dependency_licence() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:one").await;
    subject(&db, SVC2, "svc:tenant-a:two").await;
    subject(&db, SVC3, "svc:tenant-a:three").await;
    insert_licence(&db, licence(LICENCE, SVC)).await;
    insert_licence(
        &db,
        Licence {
            dependency: true,
            ..licence(LICENCE2, SVC2)
        },
    )
    .await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE2),
            ..licence(LICENCE3, SVC3)
        },
    )
    .await;
    sqlx::query(
        "UPDATE license SET updated_at = now() - interval '2 days' WHERE id IN ($1, $2, $3)",
    )
    .bind(LICENCE)
    .bind(LICENCE2)
    .bind(LICENCE3)
    .execute(&db.pool)
    .await
    .expect("age the rows");

    let before = licence_row(&db, LICENCE).await;
    assert!(
        adopt_license_into_bundle(&db.pool, LICENCE, BUNDLE)
            .await
            .expect("adopt"),
        "an unbundled licence is adopted"
    );
    let adopted = licence_row(&db, LICENCE).await;
    assert_eq!(adopted.bundle_id, Some(BUNDLE));
    assert!(!adopted.is_dependency);
    assert!(
        adopted.updated_at > before.updated_at,
        "updated_at advances"
    );

    let before_dependency = licence_row(&db, LICENCE2).await;
    assert!(
        adopt_license_into_bundle(&db.pool, LICENCE2, BUNDLE)
            .await
            .expect("promote"),
        "an unbundled dependency licence is adopted"
    );
    let promoted = licence_row(&db, LICENCE2).await;
    assert_eq!(promoted.bundle_id, Some(BUNDLE));
    assert!(
        !promoted.is_dependency,
        "adoption PROMOTES a dependency licence"
    );
    assert!(promoted.updated_at > before_dependency.updated_at);

    let before_other = licence_row(&db, LICENCE3).await;
    assert!(
        !adopt_license_into_bundle(&db.pool, LICENCE3, BUNDLE)
            .await
            .expect("compare-and-set"),
        "a licence another bundle already owns is never re-pointed"
    );
    let unchanged = licence_row(&db, LICENCE3).await;
    assert_eq!(unchanged.bundle_id, Some(BUNDLE2));
    assert_eq!(unchanged.updated_at, before_other.updated_at);

    assert!(
        !adopt_license_into_bundle(&db.pool, LICENCE, BUNDLE)
            .await
            .expect("re-adopt"),
        "adoption is idempotent"
    );
}

#[tokio::test]
async fn a_subject_is_created_once_and_a_duplicate_returns_the_winner() {
    let db = seeded().await;
    let first = create_or_get_subject(&db.pool, "svc:tenant-a:fresh", "service")
        .await
        .expect("first");
    let second = create_or_get_subject(&db.pool, "svc:tenant-a:fresh", "service")
        .await
        .expect("second");
    assert_eq!(first, second);

    // Two genuinely concurrent provisions: the loser's unique violation is absorbed.
    let (left, right) = tokio::join!(
        create_or_get_subject(&db.pool, "svc:tenant-a:race", "service"),
        create_or_get_subject(&db.pool, "svc:tenant-a:race", "service"),
    );
    assert_eq!(left.expect("left"), right.expect("right"));
    let raced: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subject WHERE keycloak_sub = 'svc:tenant-a:race'")
            .fetch_one(&db.pool)
            .await
            .expect("count");
    assert_eq!(raced, 1, "exactly one subject row exists for the racer");

    // A pre-existing row is returned unchanged.
    subject(&db, SVC, "svc:tenant-a:existing").await;
    assert_eq!(
        create_or_get_subject(&db.pool, "svc:tenant-a:existing", "service")
            .await
            .expect("existing"),
        SVC
    );
}

#[tokio::test]
async fn a_tenant_id_is_refused_as_a_new_subject_unless_the_tenant_already_trades() {
    let db = seeded().await;
    let tenant_sub = TENANT.to_string();

    match create_or_get_subject(&db.pool, &tenant_sub, "portal").await {
        Err(CreateSubjectError::Refused { message }) => {
            assert!(message.contains("already registered tenant"), "{message}");
        }
        other => panic!("expected the tenant-as-subject refusal, got {other:?}"),
    }
    let written: i64 = sqlx::query_scalar("SELECT count(*) FROM subject WHERE keycloak_sub = $1")
        .bind(&tenant_sub)
        .fetch_one(&db.pool)
        .await
        .expect("count");
    assert_eq!(written, 0, "the refusal writes nothing");

    // ⛔ The exemption is LIVE STATE, not an allowlist: a licence of ANY status exempts.
    subject(&db, SVC, "svc:tenant-a:holder").await;
    insert_licence(
        &db,
        Licence {
            status: "revoked",
            ..licence(LICENCE, SVC)
        },
    )
    .await;
    let admitted = create_or_get_subject(&db.pool, &tenant_sub, "portal")
        .await
        .expect("a tenant already trading is admitted");
    let (realm, tenant_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT keycloak_realm, tenant_id FROM subject WHERE id = $1")
            .bind(admitted)
            .fetch_one(&db.pool)
            .await
            .expect("admitted subject");
    assert_eq!(realm, "portal");
    assert_eq!(tenant_id, None, "attribution is a separate, later write");

    // ⛔ An EXISTING subject that violates is returned UNCHANGED, never refused.
    subject(&db, SVC2, &OTHER_TENANT.to_string()).await;
    assert_eq!(
        create_or_get_subject(&db.pool, &OTHER_TENANT.to_string(), "portal")
            .await
            .expect("existing violator"),
        SVC2
    );
}

#[tokio::test]
async fn the_licence_insert_lands_every_field_including_the_dependency_shape() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:dep").await;
    subject(&db, SVC2, "svc:tenant-a:sold").await;

    let dependency = create_license(
        &db.pool,
        NewLicence {
            subject_id: SVC,
            app_id: APP,
            plan_id: PLAN,
            tenant_id: TENANT,
            bundle_id: None,
            is_dependency: true,
            token_budget_override: Some(3000),
            quota_override: Some(7),
            metadata: json!({"provisioned_by": "bundle_ensure", "dependency_of": "non-agentic-ai"}),
        },
    )
    .await
    .expect("dependency licence");
    let row = licence_row(&db, dependency).await;
    assert_eq!(row.subject_id, SVC);
    assert_eq!(row.app_id, APP);
    assert_eq!(row.plan_id, PLAN);
    assert_eq!(row.tenant_id, TENANT);
    assert_eq!(row.bundle_id, None, "a dependency licence is never bundled");
    assert!(row.is_dependency);
    assert_eq!(row.token_budget_override, Some(3000));
    assert_eq!(row.quota_override, Some(7));
    assert_eq!(
        row.metadata,
        Some(json!({"provisioned_by": "bundle_ensure", "dependency_of": "non-agentic-ai"}))
    );
    assert_eq!(row.status, "active");
    assert!(row.starts_at <= Utc::now(), "starts_at defaults to now()");

    let sold = create_license(
        &db.pool,
        NewLicence {
            subject_id: SVC2,
            app_id: APP,
            plan_id: PLAN,
            tenant_id: TENANT,
            bundle_id: Some(BUNDLE),
            is_dependency: false,
            token_budget_override: None,
            quota_override: None,
            metadata: json!({"provisioned_by": "bundle_ensure", "bundle": "non-agentic-ai"}),
        },
    )
    .await
    .expect("sold licence");
    let row = licence_row(&db, sold).await;
    assert_eq!(row.bundle_id, Some(BUNDLE));
    assert!(!row.is_dependency);
    assert_eq!(row.token_budget_override, None);
    assert_eq!(row.quota_override, None);
    assert_eq!(
        row.metadata,
        Some(json!({"provisioned_by": "bundle_ensure", "bundle": "non-agentic-ai"}))
    );
}

#[tokio::test]
async fn a_lost_insert_is_lost_race_and_a_real_fault_is_database() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:winner").await;
    create_license(
        &db.pool,
        NewLicence {
            subject_id: SVC,
            app_id: APP,
            plan_id: PLAN,
            tenant_id: TENANT,
            bundle_id: Some(BUNDLE),
            is_dependency: false,
            token_budget_override: None,
            quota_override: None,
            metadata: json!({"provisioned_by": "bundle_ensure", "bundle": "non-agentic-ai"}),
        },
    )
    .await
    .expect("winner");

    let lost = create_license(
        &db.pool,
        NewLicence {
            subject_id: SVC,
            app_id: APP,
            plan_id: PLAN_PRO,
            tenant_id: TENANT,
            bundle_id: Some(BUNDLE),
            is_dependency: false,
            token_budget_override: None,
            quota_override: None,
            metadata: json!({"provisioned_by": "bundle_ensure", "bundle": "non-agentic-ai"}),
        },
    )
    .await;
    match lost {
        Err(CreateLicenceError::LostRace) => {}
        other => panic!("expected LostRace, got {other:?}"),
    }
    let winners: i64 = sqlx::query_scalar("SELECT count(*) FROM license WHERE subject_id = $1")
        .bind(SVC)
        .fetch_one(&db.pool)
        .await
        .expect("count");
    assert_eq!(winners, 1);

    // Any other database fault is NOT a lost race (here: a foreign-key violation on the plan).
    subject(&db, SVC2, "svc:tenant-a:fault").await;
    let fault = create_license(
        &db.pool,
        NewLicence {
            subject_id: SVC2,
            app_id: APP,
            plan_id: UNKNOWN_PLAN,
            tenant_id: TENANT,
            bundle_id: None,
            is_dependency: false,
            token_budget_override: None,
            quota_override: None,
            metadata: json!({"provisioned_by": "bundle_ensure"}),
        },
    )
    .await;
    match fault {
        Err(CreateLicenceError::Database(_)) => {}
        other => panic!("expected Database, got {other:?}"),
    }
}

#[tokio::test]
async fn a_lost_insert_to_a_licence_carrying_the_bundle_is_exists() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:one").await;
    subject(&db, SVC2, "svc:tenant-a:two").await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE),
            ..licence(LICENCE, SVC)
        },
    )
    .await;

    assert_eq!(
        resolve_lost_insert(&db.pool, TENANT, APP, SVC, BUNDLE, false)
            .await
            .expect("re-read"),
        LostRaceOutcome::Exists {
            licence_id: LICENCE,
            plan_id: PLAN,
        }
    );

    // A DEPENDENCY insert is satisfied by the tenant's unbundled licence too.
    insert_licence(&db, licence(LICENCE2, SVC2)).await;
    assert_eq!(
        resolve_lost_insert(&db.pool, TENANT, APP, SVC2, BUNDLE, true)
            .await
            .expect("re-read"),
        LostRaceOutcome::Exists {
            licence_id: LICENCE2,
            plan_id: PLAN,
        }
    );
}

#[tokio::test]
async fn a_lost_insert_to_a_licence_not_carrying_the_bundle_is_lost() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:one").await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE2),
            ..licence(LICENCE, SVC)
        },
    )
    .await;
    assert_eq!(
        resolve_lost_insert(&db.pool, TENANT, APP, SVC, BUNDLE, false)
            .await
            .expect("re-read"),
        LostRaceOutcome::Lost,
        "the winner sits in another bundle"
    );

    // An unbundled winner satisfies only a DEPENDENCY insert.
    subject(&db, SVC2, "svc:tenant-a:two").await;
    insert_licence(&db, licence(LICENCE2, SVC2)).await;
    assert_eq!(
        resolve_lost_insert(&db.pool, TENANT, APP, SVC2, BUNDLE, false)
            .await
            .expect("re-read"),
        LostRaceOutcome::Lost
    );
    assert_eq!(
        resolve_lost_insert(&db.pool, TENANT, APP, SVC2, BUNDLE, true)
            .await
            .expect("re-read"),
        LostRaceOutcome::Exists {
            licence_id: LICENCE2,
            plan_id: PLAN,
        }
    );
}

#[tokio::test]
async fn attribution_sets_only_a_resolvable_parent_org() {
    let db = seeded().await;
    subject(&db, SVC, "svc:tenant-a:one").await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE),
            ..licence(LICENCE, SVC)
        },
    )
    .await;
    attribute_subject_to_tenant(&db.pool, SVC, TENANT)
        .await
        .expect("attribute");
    assert_eq!(subject_tenant(&db, SVC).await, Some(TENANT));

    // `already_set`: a stated parentage is never overwritten.
    attribute_subject_to_tenant(&db.pool, SVC, OTHER_TENANT)
        .await
        .expect("second attribution");
    assert_eq!(subject_tenant(&db, SVC).await, Some(TENANT));

    // `ambiguous`: licences under two tenants leave the column NULL.
    subject(&db, SVC2, "svc:tenant-a:two").await;
    insert_licence(&db, licence(LICENCE2, SVC2)).await;
    insert_licence(
        &db,
        Licence {
            tenant: OTHER_TENANT,
            ..licence(LICENCE4, SVC2)
        },
    )
    .await;
    attribute_subject_to_tenant(&db.pool, SVC2, TENANT)
        .await
        .expect("ambiguous");
    assert_eq!(subject_tenant(&db, SVC2).await, None);

    // `tenant_is_machine`: a `kind='service'` tenant is never written as a parent.
    subject(&db, SVC3, "svc:tenant-a:three").await;
    insert_licence(
        &db,
        Licence {
            tenant: MACHINE_TENANT,
            ..licence(LICENCE5, SVC3)
        },
    )
    .await;
    attribute_subject_to_tenant(&db.pool, SVC3, MACHINE_TENANT)
        .await
        .expect("machine tenant");
    assert_eq!(subject_tenant(&db, SVC3).await, None);
}
