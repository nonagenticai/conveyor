//! The bundle-ensure DECISION PLAN against a real Postgres 18.
//!
//! Every decision, every refusal reason, the dependency closure (including a cycle), the shared
//! dependency licence, the service-subject length gate, and every whole-request refusal. The
//! module is read-only: each case asserts the plan and then that NO licence/app bundle row count
//! changed (the assertion runs on the same untouched fixture).

mod common;

use chrono::Utc;
use conveyor_licenses::db;
use conveyor_licenses::db::bundle_ensure::{
    self, AppAction, AppPlan, EnsureError, EnsureErrorCode, EnsurePlan, RefuseReason,
};
use uuid::Uuid;

const KEY: &str = "non-agentic-ai";
const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
const UNKNOWN_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_00ff);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
const BUNDLE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0005);
const DEP_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0006);
const KAG_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0007);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0008);
const PLAN_PRO: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0009);
const DEP_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000a);
const KAG_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000b);
const SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000c);
const SVC2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000d);
const PERSON: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000e);
const CP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_000f);
const DEP_SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0010);
const KAG_SVC: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0011);
const LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0012);
const LICENCE2: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0013);
const DEP_LICENCE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0014);
const REVOKED_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0015);
const REVOKED_BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0016);

/// One owner, one tenant, three apps with plans, and the subjects a decision distinguishes.
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    sqlx::query(
        "INSERT INTO tenant_owner (key, display_name)
         VALUES ('owner-a', 'Owner A'), ('owner-b', 'Owner B')",
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
         VALUES ($1, 'demo', 'Demo', now()),
                ($2, 'workers', 'Workers', now()),
                ($3, 'kag', 'Kag', now()),
                ($4, 'gone', 'Gone', now())",
    )
    .bind(APP)
    .bind(DEP_APP)
    .bind(KAG_APP)
    .bind(REVOKED_APP)
    .execute(&db.pool)
    .await
    .expect("apps");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, token_budget_monthly, quota_monthly, created_at)
         VALUES ($1, $2, 'basic', 'Basic', 1000, 100, now()),
                ($3, $2, 'pro', 'Pro', 2000, 200, now()),
                ($4, $5, 'basic', 'Basic', 500, 50, now()),
                ($6, $7, 'basic', 'Basic', 500, 50, now())",
    )
    .bind(PLAN)
    .bind(APP)
    .bind(PLAN_PRO)
    .bind(DEP_PLAN)
    .bind(DEP_APP)
    .bind(KAG_PLAN)
    .bind(KAG_APP)
    .execute(&db.pool)
    .await
    .expect("plans");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'service', now()),
                ($3, $4, 'service', now()),
                ($5, '0195f48b-d000-7000-8000-000000000001', 'portal', now()),
                ($6, $7, 'cp-tenant', now()),
                ($8, $9, 'service', now()),
                ($10, $11, 'service', now())",
    )
    .bind(SVC)
    .bind(format!("svc:{TENANT}:demo"))
    .bind(SVC2)
    .bind(format!("svc:{TENANT}:demo-alt"))
    .bind(PERSON)
    .bind(CP)
    .bind(format!("cp-tenant:{TENANT}:a"))
    .bind(DEP_SVC)
    .bind(format!("svc:{TENANT}:workers"))
    .bind(KAG_SVC)
    .bind(format!("svc:{TENANT}:kag"))
    .execute(&db.pool)
    .await
    .expect("subjects");
    db
}

async fn bundle_row(db: &common::TestDb, bundle: Uuid, key: &str) {
    sqlx::query("INSERT INTO bundle (id, key, name, created_at) VALUES ($1, $2, $2, now())")
        .bind(bundle)
        .bind(key)
        .execute(&db.pool)
        .await
        .expect("bundle");
}

async fn holds(db: &common::TestDb, bundle: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_bundle (tenant_id, bundle_id, created_at) VALUES ($1, $2, now())",
    )
    .bind(TENANT)
    .bind(bundle)
    .execute(&db.pool)
    .await
    .expect("tenant_bundle");
}

async fn bundle_app(db: &common::TestDb, bundle: Uuid, app: Uuid) {
    sqlx::query("INSERT INTO bundle_app (bundle_id, app_id, created_at) VALUES ($1, $2, now())")
        .bind(bundle)
        .bind(app)
        .execute(&db.pool)
        .await
        .expect("bundle_app");
}

async fn requires(db: &common::TestDb, app: Uuid, required: Uuid) {
    sqlx::query(
        "INSERT INTO app_requires (app_id, required_app_id, caller_owner_key)
         VALUES ($1, $2, 'owner-a')",
    )
    .bind(app)
    .bind(required)
    .execute(&db.pool)
    .await
    .expect("app_requires");
}

async fn bundle_caller(db: &common::TestDb, bundle: Uuid, owner: &str, app: Uuid) {
    sqlx::query("INSERT INTO bundle_caller (bundle_id, owner_key, app_id) VALUES ($1, $2, $3)")
        .bind(bundle)
        .bind(owner)
        .bind(app)
        .execute(&db.pool)
        .await
        .expect("bundle_caller");
}

/// A licence fixture; `metered` false leaves `metering_provisioned_at` NULL.
struct Licence {
    id: Uuid,
    app: Uuid,
    plan: Uuid,
    subject: Uuid,
    bundle: Option<Uuid>,
    dependency: bool,
    metered: bool,
}

fn licence(id: Uuid, app: Uuid, plan: Uuid, subject: Uuid) -> Licence {
    Licence {
        id,
        app,
        plan,
        subject,
        bundle: None,
        dependency: false,
        metered: true,
    }
}

async fn insert_licence(db: &common::TestDb, licence: Licence) {
    sqlx::query(
        "INSERT INTO license (
             id, subject_id, app_id, plan_id, status, starts_at, tenant_id, bundle_id,
             is_dependency, metering_provisioned_at)
         VALUES ($1, $2, $3, $4, 'active', now(), $5, $6, $7, $8)",
    )
    .bind(licence.id)
    .bind(licence.subject)
    .bind(licence.app)
    .bind(licence.plan)
    .bind(TENANT)
    .bind(licence.bundle)
    .bind(licence.dependency)
    .bind(licence.metered.then(Utc::now))
    .execute(&db.pool)
    .await
    .expect("license");
}

/// The bundle under test: held, with `demo` as its only SOLD app.
async fn sold(db: &common::TestDb) {
    bundle_row(db, BUNDLE, KEY).await;
    holds(db, BUNDLE).await;
    bundle_app(db, BUNDLE, APP).await;
}

async fn plan_for(
    db: &common::TestDb,
    bundle_key: &str,
    plans: &[&str],
    quota_override: Option<i32>,
    token_budget_override: Option<i32>,
) -> Result<EnsurePlan, EnsureError> {
    let plans: Vec<String> = plans.iter().map(|code| (*code).to_owned()).collect();
    bundle_ensure::plan(
        &db.pool,
        TENANT,
        bundle_key,
        &plans,
        quota_override,
        token_budget_override,
    )
    .await
}

fn app_decision<'a>(plan: &'a EnsurePlan, app_key: &str) -> &'a AppPlan {
    plan.apps
        .iter()
        .find(|app| app.app_key == app_key)
        .unwrap_or_else(|| panic!("no decision for {app_key} in {:?}", plan.apps))
}

fn refused(result: Result<EnsurePlan, EnsureError>) -> (EnsureErrorCode, String) {
    match result {
        Err(EnsureError::Refused { code, message }) => (code, message),
        other => panic!("expected a whole-request refusal, got {other:?}"),
    }
}

fn reason(app: &AppPlan) -> RefuseReason {
    app.refuse_reason.expect("a refusal reason")
}

#[tokio::test]
async fn an_unlicensed_bundle_inserts_the_first_preferred_plan() {
    let db = seeded().await;
    bundle_row(&db, BUNDLE, KEY).await;
    bundle_app(&db, BUNDLE, APP).await;

    let plan = plan_for(&db, KEY, &["standard", "basic"], None, None)
        .await
        .expect("plan");
    assert_eq!(plan.tenant_id, TENANT);
    assert_eq!(plan.bundle_key, KEY);
    assert_eq!(plan.bundle_id, BUNDLE);
    assert!(!plan.holds, "the tenant holds no tenant_bundle row");
    assert_eq!(plan.plans, vec!["standard", "basic"]);
    assert_eq!(plan.budget_currency, None);
    assert_eq!(plan.apps.len(), 1);
    assert!(plan.dependency_grants.is_empty());

    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Insert);
    assert_eq!(app.refuse_reason, None);
    assert_eq!(app.license_id, None);
    assert_eq!(app.existing_plan_id, None);
    assert_eq!(app.subject, format!("svc:{TENANT}:demo"));
    assert_eq!(app.plan_code, "basic");
    assert_eq!(app.plan_id, Some(PLAN));
    assert_eq!(app.note, "metering: token_budget=1000, quota=100");
    assert!(!app.dependency);
}

#[tokio::test]
async fn a_tenant_level_licence_carrying_the_bundle_is_exists() {
    let db = seeded().await;
    sold(&db).await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE),
            ..licence(LICENCE, APP, PLAN, SVC)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    assert!(plan.holds);
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Exists);
    assert_eq!(app.refuse_reason, None);
    assert_eq!(app.license_id, Some(LICENCE));
    assert_eq!(app.existing_plan_id, Some(PLAN));
    assert_eq!(app.subject, format!("svc:{TENANT}:demo"));
    assert_eq!(app.note, "");
}

#[tokio::test]
async fn an_unbundled_tenant_level_licence_is_adopted_and_flags_unmetered() {
    let db = seeded().await;
    sold(&db).await;
    insert_licence(
        &db,
        Licence {
            metered: false,
            ..licence(LICENCE, APP, PLAN, SVC)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Adopt);
    assert_eq!(app.license_id, Some(LICENCE));
    assert_eq!(app.existing_plan_id, Some(PLAN));
    assert_eq!(
        app.note,
        "metering_provisioned_at NULL: left to the reconcile sweep"
    );
}

#[tokio::test]
async fn two_tenant_level_candidates_refuse_several_candidates() {
    let db = seeded().await;
    sold(&db).await;
    insert_licence(&db, licence(LICENCE, APP, PLAN, SVC)).await;
    insert_licence(&db, licence(LICENCE2, APP, PLAN, SVC2)).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(reason(app), RefuseReason::SeveralCandidates);
    assert_eq!(reason(app).as_str(), "several_candidates");
    assert_eq!(app.license_id, None);
    assert_eq!(app.subject, "");
    assert_eq!(
        app.note,
        "2 active (tenant, app) licences; resolve to one first"
    );
    assert_eq!(app.candidates.len(), 2);
    for candidate in &app.candidates {
        assert!(candidate.contains("bundle_id=None"), "{candidate}");
    }
    assert!(
        app.candidates
            .iter()
            .any(|c| c.contains(&LICENCE.to_string()))
    );
    assert!(
        app.candidates
            .iter()
            .any(|c| c.contains(&LICENCE2.to_string()))
    );
}

#[tokio::test]
async fn a_member_licence_carrying_the_bundle_refuses_member_licence() {
    let db = seeded().await;
    sold(&db).await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE),
            ..licence(LICENCE, APP, PLAN, PERSON)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(reason(app), RefuseReason::MemberLicence);
    assert_eq!(reason(app).as_str(), "member_licence");
    assert_eq!(app.license_id, Some(LICENCE));
    // ⛔ A member licence is NEVER adopted, even when it is the only one.
    assert_eq!(app.subject, "0195f48b-d000-7000-8000-000000000001");
    assert!(app.note.contains("MEMBER licence carries this bundle"));
    assert!(app.note.contains("409 ambiguous_app_licence"));
}

#[tokio::test]
async fn a_member_licence_outside_the_bundle_is_only_noted() {
    let db = seeded().await;
    sold(&db).await;
    insert_licence(&db, licence(LICENCE, APP, PLAN, PERSON)).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Insert);
    assert_eq!(app.license_id, None);
    assert!(app.candidates.is_empty());
    assert_eq!(
        app.note,
        format!(
            "metering: token_budget=1000, quota=100; member licence {LICENCE} \
             (0195f48b-d000-7000-8000-000000000001), not adopted — migrate via \
             migrate_member_licences_to_tenant.py"
        )
    );
}

#[tokio::test]
async fn a_tenant_level_licence_in_another_bundle_refuses_other_bundle() {
    let db = seeded().await;
    sold(&db).await;
    bundle_row(&db, BUNDLE2, "other-bundle").await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE2),
            ..licence(LICENCE, APP, PLAN, SVC)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(reason(app), RefuseReason::OtherBundle);
    assert_eq!(reason(app).as_str(), "other_bundle");
    assert_eq!(app.license_id, Some(LICENCE));
    assert_eq!(
        app.note,
        format!("licence already bound to another bundle ({BUNDLE2})")
    );
}

#[tokio::test]
async fn no_matching_plan_refuses_no_plan() {
    let db = seeded().await;
    sold(&db).await;

    let plan = plan_for(&db, KEY, &["premium"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(reason(app), RefuseReason::NoPlan);
    assert_eq!(reason(app).as_str(), "no_plan");
    assert_eq!(app.subject, format!("svc:{TENANT}:demo"));
    assert_eq!(
        app.note,
        "no plan 'premium' on this app (has: ['basic', 'pro'])"
    );
}

#[tokio::test]
async fn an_overlong_app_key_refuses_app_key_too_long() {
    let db = seeded().await;
    let too_long = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0017);
    let key = "a".repeat(215);
    sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $2, now())")
        .bind(too_long)
        .bind(&key)
        .execute(&db.pool)
        .await
        .expect("app");
    bundle_row(&db, BUNDLE, KEY).await;
    bundle_app(&db, BUNDLE, too_long).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, &key);
    assert_eq!(app.action, AppAction::Refuse);
    assert_eq!(reason(app), RefuseReason::AppKeyTooLong);
    assert_eq!(reason(app).as_str(), "app_key_too_long");
    assert!(
        app.note.contains("app key is 215 characters"),
        "{}",
        app.note
    );
    assert!(app.note.contains("at most 214"), "{}", app.note);
    assert_eq!(app.subject, "");
}

#[tokio::test]
async fn a_cp_tenant_licence_is_neither_member_nor_candidate() {
    let db = seeded().await;
    sold(&db).await;
    // A cp-tenant licence carrying the bundle must not turn into `member_licence`.
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE),
            ..licence(LICENCE, APP, PLAN, CP)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let app = app_decision(&plan, "demo");
    assert_eq!(app.action, AppAction::Insert);
    assert_eq!(app.note, "metering: token_budget=1000, quota=100");
}

#[tokio::test]
async fn a_licence_in_another_bundle_satisfies_a_dependency() {
    let db = seeded().await;
    sold(&db).await;
    bundle_row(&db, BUNDLE2, "other-bundle").await;
    requires(&db, APP, DEP_APP).await;
    insert_licence(
        &db,
        Licence {
            bundle: Some(BUNDLE2),
            ..licence(DEP_LICENCE, DEP_APP, DEP_PLAN, DEP_SVC)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Exists);
    assert!(dependency.dependency);
    assert_eq!(dependency.license_id, Some(DEP_LICENCE));
    assert_eq!(dependency.existing_plan_id, Some(DEP_PLAN));
    assert_eq!(
        dependency.note,
        format!("dependency satisfied by the tenant's bundle {BUNDLE2} (shared)")
    );
    assert_eq!(
        plan.dependency_grants,
        vec![("owner-a".to_owned(), "workers".to_owned())]
    );
}

#[tokio::test]
async fn a_dependency_licence_is_shared_wherever_it_sits() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;
    insert_licence(
        &db,
        Licence {
            dependency: true,
            ..licence(DEP_LICENCE, DEP_APP, DEP_PLAN, DEP_SVC)
        },
    )
    .await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Exists);
    assert!(dependency.dependency);
    assert_eq!(
        dependency.note,
        "dependency satisfied by the tenant's dependency licence (shared)"
    );
}

#[tokio::test]
async fn an_unbundled_tenant_licence_satisfies_a_dependency() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;
    insert_licence(&db, licence(DEP_LICENCE, DEP_APP, DEP_PLAN, DEP_SVC)).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Exists);
    assert_eq!(
        dependency.note,
        "dependency satisfied by the tenant's unbundled licence (shared)"
    );
}

#[tokio::test]
async fn a_missing_dependency_is_inserted_as_a_dependency() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;

    let plan = plan_for(&db, KEY, &["standard", "basic"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Insert);
    assert!(dependency.dependency);
    assert_eq!(dependency.license_id, None);
    assert_eq!(dependency.subject, format!("svc:{TENANT}:workers"));
    assert_eq!(dependency.plan_code, "basic");
    assert_eq!(dependency.plan_id, Some(DEP_PLAN));
    assert_eq!(
        dependency.note,
        "dependency; metering: token_budget=500, quota=50"
    );
}

#[tokio::test]
async fn two_dependency_candidates_refuse_several_candidates() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;
    insert_licence(&db, licence(DEP_LICENCE, DEP_APP, DEP_PLAN, DEP_SVC)).await;
    insert_licence(&db, licence(LICENCE2, DEP_APP, DEP_PLAN, SVC2)).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Refuse);
    assert_eq!(reason(dependency), RefuseReason::SeveralCandidates);
    assert_eq!(reason(dependency).as_str(), "several_candidates");
    assert!(dependency.dependency);
    assert_eq!(
        dependency.note,
        "2 active tenant-level (tenant, app) licences; resolve to one first"
    );
    assert_eq!(dependency.candidates.len(), 2);
    for candidate in &dependency.candidates {
        assert!(candidate.contains("bundle_id=None"), "{candidate}");
        assert!(!candidate.contains("subject="), "{candidate}");
    }
}

#[tokio::test]
async fn a_dependency_without_a_plan_refuses_no_plan() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;

    let plan = plan_for(&db, KEY, &["premium"], None, None)
        .await
        .expect("plan");
    let dependency = app_decision(&plan, "workers");
    assert_eq!(dependency.action, AppAction::Refuse);
    assert_eq!(reason(dependency), RefuseReason::NoPlan);
    assert_eq!(reason(dependency).as_str(), "no_plan");
    assert!(dependency.dependency);
    assert_eq!(
        dependency.note,
        "no plan 'premium' on this app (has: ['basic'])"
    );
}

#[tokio::test]
async fn the_whole_closure_is_planned_and_its_grants_are_carried() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;
    requires(&db, DEP_APP, KAG_APP).await;
    bundle_caller(&db, BUNDLE, "owner-b", APP).await;

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let keys: Vec<&str> = plan.apps.iter().map(|app| app.app_key.as_str()).collect();
    assert_eq!(keys, vec!["demo", "kag", "workers"]);
    assert!(!app_decision(&plan, "demo").dependency);
    assert!(app_decision(&plan, "workers").dependency);
    assert!(app_decision(&plan, "kag").dependency);
    assert_eq!(app_decision(&plan, "workers").action, AppAction::Insert);
    assert_eq!(app_decision(&plan, "kag").action, AppAction::Insert);
    assert_eq!(
        plan.dependency_grants,
        vec![
            ("owner-a".to_owned(), "kag".to_owned()),
            ("owner-a".to_owned(), "workers".to_owned()),
            ("owner-b".to_owned(), "demo".to_owned()),
        ]
    );
}

#[tokio::test]
async fn a_reachable_cycle_refuses_dependency_cycle() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;
    requires(&db, DEP_APP, APP).await;

    let (code, message) = refused(plan_for(&db, KEY, &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::DependencyCycle);
    assert_eq!(code.as_str(), "dependency_cycle");
    assert_eq!(message, "app_requires cycle: demo -> workers -> demo");
}

#[tokio::test]
async fn an_unknown_tenant_refuses_tenant_not_found() {
    let db = seeded().await;
    let result = bundle_ensure::plan(
        &db.pool,
        UNKNOWN_TENANT,
        KEY,
        &["basic".to_owned()],
        None,
        None,
    )
    .await;
    let (code, message) = refused(result);
    assert_eq!(code, EnsureErrorCode::TenantNotFound);
    assert_eq!(code.as_str(), "tenant_not_found");
    assert_eq!(
        message,
        format!("tenant {UNKNOWN_TENANT} is not in tenants")
    );
}

#[tokio::test]
async fn an_archived_tenant_refuses_tenant_archived() {
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");

    let (code, message) = refused(plan_for(&db, KEY, &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::TenantArchived);
    assert_eq!(code.as_str(), "tenant_archived");
    assert_eq!(message, format!("tenant {TENANT} is ARCHIVED"));
}

#[tokio::test]
async fn a_money_tenant_refuses_a_token_budget_override_but_plans_without_one() {
    let db = seeded().await;
    sold(&db).await;
    sqlx::query("UPDATE tenants SET budget_currency = 'EUR' WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("money tenant");

    let (code, message) = refused(plan_for(&db, KEY, &["basic"], None, Some(5)).await);
    assert_eq!(code, EnsureErrorCode::TokenBudgetForMoneyTenant);
    assert_eq!(code.as_str(), "token_budget_for_money_tenant");
    assert!(message.contains("budget_currency='EUR'"), "{message}");

    let plan = plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    assert_eq!(plan.budget_currency.as_deref(), Some("EUR"));
    assert_eq!(
        app_decision(&plan, "demo").note,
        "metering: token_budget=none (money tenant), quota=100"
    );
}

#[tokio::test]
async fn an_unknown_or_revoked_bundle_refuses_bundle_not_found() {
    let db = seeded().await;
    let (code, message) = refused(plan_for(&db, "nope", &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::BundleNotFound);
    assert_eq!(code.as_str(), "bundle_not_found");
    assert_eq!(message, "bundle 'nope' not found or revoked");

    bundle_row(&db, REVOKED_BUNDLE, "revoked-bundle").await;
    sqlx::query("UPDATE bundle SET revoked = true WHERE id = $1")
        .bind(REVOKED_BUNDLE)
        .execute(&db.pool)
        .await
        .expect("revoke bundle");
    let (code, message) = refused(plan_for(&db, "revoked-bundle", &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::BundleNotFound);
    assert_eq!(message, "bundle 'revoked-bundle' not found or revoked");
}

#[tokio::test]
async fn a_bundle_with_no_sellable_apps_refuses_bundle_empty() {
    let db = seeded().await;
    bundle_row(&db, BUNDLE, KEY).await;
    let (code, message) = refused(plan_for(&db, KEY, &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::BundleEmpty);
    assert_eq!(code.as_str(), "bundle_empty");
    assert_eq!(message, format!("bundle '{KEY}' has no apps"));

    // A member app that is REVOKED is not sold: the bundle is still empty.
    bundle_app(&db, BUNDLE, REVOKED_APP).await;
    sqlx::query("UPDATE app SET revoked = true WHERE id = $1")
        .bind(REVOKED_APP)
        .execute(&db.pool)
        .await
        .expect("revoke app");
    let (code, message) = refused(plan_for(&db, KEY, &["basic"], None, None).await);
    assert_eq!(code, EnsureErrorCode::BundleEmpty);
    assert_eq!(message, format!("bundle '{KEY}' has no apps"));
}

#[tokio::test]
async fn overrides_are_echoed_and_drive_the_metering_note() {
    let db = seeded().await;
    bundle_row(&db, BUNDLE, KEY).await;
    bundle_app(&db, BUNDLE, APP).await;

    let plan = plan_for(&db, KEY, &["basic"], Some(7), Some(3000))
        .await
        .expect("plan");
    assert_eq!(plan.quota_override, Some(7));
    assert_eq!(plan.token_budget_override, Some(3000));
    assert_eq!(
        app_decision(&plan, "demo").note,
        "metering: token_budget=3000 (override; plan: 1000), quota=7 (override; plan: 100)"
    );
}

#[tokio::test]
async fn the_plan_is_read_only() {
    let db = seeded().await;
    sold(&db).await;
    requires(&db, APP, DEP_APP).await;

    let before: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM license), (SELECT count(*) FROM tenant_dependency_grant)",
    )
    .fetch_one(&db.pool)
    .await
    .expect("counts");
    plan_for(&db, KEY, &["basic"], None, None)
        .await
        .expect("plan");
    let after: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM license), (SELECT count(*) FROM tenant_dependency_grant)",
    )
    .fetch_one(&db.pool)
    .await
    .expect("counts");
    assert_eq!(before, after, "the plan layer must write nothing");
}
