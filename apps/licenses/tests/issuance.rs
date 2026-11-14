//! Behaviour of the issuance-authorization core (`db::issuance`).
//!
//! Every case runs against a real Postgres 18 created by `common` and the embedded baseline, so
//! the reference's owner/grant/issuer predicates are exercised against the captured schema.
//! `User` values are built directly: the middleware is the only production attach point for
//! `owner_key`, and these tests are the "outside that path" case the module documents.
//!
//! ⚠️ Requires `pub mod issuance;` in `src/db/mod.rs` (the parent wires the module).

mod common;

use axum::http::StatusCode;
use conveyor_licenses::auth::User;
use conveyor_licenses::db;
use conveyor_licenses::db::issuance::{
    LicenceRow, app_issuers, authorize_issuance, authorize_licence_revoke, bundle_owner_may_revoke,
    licences_for_token_hash_in_tenant, require_known_owner,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1001);
const FOREIGN_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1002);
const GRANTLESS_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1003);
const APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1004);
const OTHER_APP: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1005);
const PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1006);
const OTHER_PLAN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1007);
const SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1008);
const OTHER_SUBJECT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1009);
const BUNDLE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1010);
const LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1011);
const BUNDLE_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1012);
const FOREIGN_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1013);
const DEPENDENCY_LICENSE: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1014);
const TOKEN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1015);
const FOREIGN_TOKEN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1016);
const ABSENT_TENANT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1099);

const RAW: &str = "raw-licence-token";

fn user(owner: Option<&str>) -> User {
    User {
        sub: "sub-1".to_owned(),
        azp: Some("client-a".to_owned()),
        owner_key: owner.map(str::to_owned),
        ..User::default()
    }
}

fn cross_owner_user(owner: Option<&str>) -> User {
    User {
        cross_owner: true,
        ..user(owner)
    }
}

/// owner-a owns TENANT and GRANTLESS_TENANT; owner-b owns FOREIGN_TENANT.
///
/// - `APP`        issued by owner-a, owner-b, owner-d.
/// - `OTHER_APP`  issued by owner-b ONLY — so owner-a (TENANT's owner) never issues it, which is
///   exactly the shape the bundle-owner widening exists for.
/// - owner-b holds an ACTIVE grant on TENANT (a grantee that may mint `APP`).
/// - owner-c holds a revoked grant and issues neither app.
/// - owner-d issues `APP` but its grant is revoked (issuer without tenant access).
async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");

    for owner in ["owner-a", "owner-b", "owner-c", "owner-d"] {
        sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ($1, $1)")
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("tenant_owner");
    }

    for (id, name, owner) in [
        (TENANT, "tenant-a", "owner-a"),
        (FOREIGN_TENANT, "tenant-b", "owner-b"),
        (GRANTLESS_TENANT, "tenant-c", "owner-a"),
    ] {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
             VALUES ($1, $2, now(), $3, 'local_allowed')",
        )
        .bind(id)
        .bind(name)
        .bind(owner)
        .execute(&db.pool)
        .await
        .expect("tenant");
    }

    for (id, key) in [(APP, "demo"), (OTHER_APP, "other")] {
        sqlx::query("INSERT INTO app (id, key, name, created_at) VALUES ($1, $2, $2, now())")
            .bind(id)
            .bind(key)
            .execute(&db.pool)
            .await
            .expect("app");
    }
    for (id, app) in [(PLAN, APP), (OTHER_PLAN, OTHER_APP)] {
        sqlx::query(
            "INSERT INTO plan (id, app_id, code, name, created_at)
             VALUES ($1, $2, 'basic', 'Basic', now())",
        )
        .bind(id)
        .bind(app)
        .execute(&db.pool)
        .await
        .expect("plan");
    }

    for (app_id, owner) in [
        (APP, "owner-a"),
        (APP, "owner-b"),
        (APP, "owner-d"),
        (OTHER_APP, "owner-b"),
    ] {
        sqlx::query("INSERT INTO app_issuer (app_id, owner_key) VALUES ($1, $2)")
            .bind(app_id)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("app_issuer");
    }

    // owner-b: active grant. owner-c / owner-d: revoked grants.
    for (owner, revoked) in [("owner-b", false), ("owner-c", true), ("owner-d", true)] {
        sqlx::query(
            "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by)
             VALUES ($1, $2, 'owner-a')",
        )
        .bind(TENANT)
        .bind(owner)
        .execute(&db.pool)
        .await
        .expect("tenant_app_grant");
        if revoked {
            sqlx::query(
                "UPDATE tenant_app_grant SET revoked_at = now()
                  WHERE tenant_id = $1 AND grantee_owner_key = $2",
            )
            .bind(TENANT)
            .bind(owner)
            .execute(&db.pool)
            .await
            .expect("revoke grant");
        }
    }

    for (id, sub) in [
        (SUBJECT, "0195f48b-d000-7000-8000-000000001001"),
        (OTHER_SUBJECT, "0195f48b-d000-7000-8000-000000001002"),
    ] {
        sqlx::query("INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at) VALUES ($1, $2, 'portal', now())")
            .bind(id)
            .bind(sub)
            .execute(&db.pool)
            .await
            .expect("subject");
    }

    sqlx::query(
        "INSERT INTO bundle (id, key, name, created_at) VALUES ($1, 'bundle-a', 'Bundle A', now())",
    )
    .bind(BUNDLE)
    .execute(&db.pool)
    .await
    .expect("bundle");

    for (id, subject, app, plan, tenant, bundle, dependency) in [
        (LICENSE, SUBJECT, APP, PLAN, TENANT, None, false),
        (
            BUNDLE_LICENSE,
            SUBJECT,
            OTHER_APP,
            OTHER_PLAN,
            TENANT,
            Some(BUNDLE),
            false,
        ),
        (
            FOREIGN_LICENSE,
            OTHER_SUBJECT,
            APP,
            PLAN,
            FOREIGN_TENANT,
            None,
            false,
        ),
        (
            DEPENDENCY_LICENSE,
            OTHER_SUBJECT,
            OTHER_APP,
            OTHER_PLAN,
            TENANT,
            None,
            true,
        ),
    ] {
        sqlx::query(
            "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id,
                                  bundle_id, is_dependency)
             VALUES ($1, $2, $3, $4, 'active', now(), $5, $6, $7)",
        )
        .bind(id)
        .bind(subject)
        .bind(app)
        .bind(plan)
        .bind(tenant)
        .bind(bundle)
        .bind(dependency)
        .execute(&db.pool)
        .await
        .expect("license");
    }

    // The same raw token on a licence of another tenant: it must never enter TENANT's list.
    for (id, licence) in [(TOKEN, LICENSE), (FOREIGN_TOKEN, FOREIGN_LICENSE)] {
        sqlx::query(
            "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
             VALUES ($1, $2, $3, 'api', now())",
        )
        .bind(id)
        .bind(licence)
        .bind(db::tokens::hash_token(RAW))
        .execute(&db.pool)
        .await
        .expect("token");
    }

    db
}

async fn load_licence(pool: &PgPool, id: Uuid) -> LicenceRow {
    sqlx::query_as::<_, LicenceRow>(
        "SELECT id, tenant_id, app_id, subject_id, status, is_dependency, bundle_id
           FROM license WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("licence row")
}

// ── app_issuers ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn app_issuers_lists_every_owner_key_declared_for_the_app() {
    let db = seeded().await;
    assert_eq!(
        app_issuers(&db.pool, APP).await.expect("issuers"),
        vec![
            "owner-a".to_owned(),
            "owner-b".to_owned(),
            "owner-d".to_owned()
        ]
    );
    assert_eq!(
        app_issuers(&db.pool, OTHER_APP).await.expect("issuers"),
        vec!["owner-b".to_owned()]
    );
}

// ── require_known_owner ─────────────────────────────────────────────────────

#[test]
fn a_known_owner_passes_and_an_unknown_api_client_is_refused() {
    assert_eq!(
        require_known_owner(&user(Some("owner-a"))).expect("known owner"),
        "owner-a"
    );

    let refusal = require_known_owner(&user(None)).expect_err("unknown client");
    assert_eq!(refusal.status, StatusCode::FORBIDDEN);
    assert_eq!(
        refusal.body,
        json!({"detail": {
            "error": "unknown_api_client",
            "message": "this client is not registered as an owning app (api_client); tenant and licence writes are refused",
        }})
    );
}

// ── authorize_issuance ──────────────────────────────────────────────────────

#[tokio::test]
async fn the_issuing_owner_may_mint_on_its_own_tenant() {
    let db = seeded().await;
    authorize_issuance(
        &db.pool,
        &user(Some("owner-a")),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect("the owner issues the app and owns the tenant");
}

#[tokio::test]
async fn a_non_issuer_is_refused_with_app_not_issuable() {
    let db = seeded().await;
    let refusal = authorize_issuance(
        &db.pool,
        &user(Some("owner-c")),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect_err("owner-c issues neither app");

    assert_eq!(refusal.status, StatusCode::FORBIDDEN);
    assert_eq!(
        refusal.body["detail"]["error"],
        "app_not_issuable_by_caller"
    );
    assert_eq!(
        refusal.body["detail"]["message"],
        "owner 'owner-c' may not issue this app's licences"
    );
}

#[tokio::test]
async fn an_issuer_without_access_to_the_tenant_is_refused_tenant_not_accessible() {
    let db = seeded().await;
    // owner-b issues APP but owns neither GRANTLESS_TENANT nor holds a grant on it.
    let refusal = authorize_issuance(
        &db.pool,
        &user(Some("owner-b")),
        APP,
        Some(GRANTLESS_TENANT),
        "license_issue",
    )
    .await
    .expect_err("issued, but the tenant is not accessible");

    assert_eq!(refusal.status, StatusCode::FORBIDDEN);
    assert_eq!(refusal.body["detail"]["error"], "tenant_not_accessible");
    assert_eq!(
        refusal.body["detail"]["message"],
        "the tenant is neither owned by nor granted to the calling app"
    );
}

#[tokio::test]
async fn a_grantee_that_issues_the_app_may_mint() {
    let db = seeded().await;
    authorize_issuance(
        &db.pool,
        &user(Some("owner-b")),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect("an ACTIVE grant admits an issuer that is not the tenant owner");
}

#[tokio::test]
async fn a_revoked_grant_does_not_admit_the_issuer() {
    let db = seeded().await;
    let refusal = authorize_issuance(
        &db.pool,
        &user(Some("owner-d")),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect_err("owner-d's grant on TENANT is revoked");

    assert_eq!(refusal.body["detail"]["error"], "tenant_not_accessible");
}

#[tokio::test]
async fn an_absent_tenant_is_left_to_the_routes_own_refusal() {
    let db = seeded().await;
    // No row for the id: the reference's `accessible` is true and the route answers its 404.
    authorize_issuance(
        &db.pool,
        &user(Some("owner-a")),
        APP,
        Some(ABSENT_TENANT),
        "license_issue",
    )
    .await
    .expect("an unknown tenant is not this authorizer's refusal");

    // No tenant context at all: the same.
    authorize_issuance(&db.pool, &user(Some("owner-a")), APP, None, "license_issue")
        .await
        .expect("a missing tenant is not this authorizer's refusal");
}

#[tokio::test]
async fn cross_owner_bypasses_the_issuer_and_tenant_refusals() {
    let db = seeded().await;
    authorize_issuance(
        &db.pool,
        &cross_owner_user(Some("owner-c")),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect("cross_owner bypasses both locks, audited");

    // ...but never the known-owner gate: an unknown client is not break-glass.
    let refusal = authorize_issuance(
        &db.pool,
        &cross_owner_user(None),
        APP,
        Some(TENANT),
        "license_issue",
    )
    .await
    .expect_err("an unknown client is refused before the bypass");
    assert_eq!(refusal.body["detail"]["error"], "unknown_api_client");
}

// ── bundle_owner_may_revoke ─────────────────────────────────────────────────

#[tokio::test]
async fn the_tenant_owner_may_revoke_a_bundle_minted_licence_it_does_not_issue() {
    let db = seeded().await;
    let licence = load_licence(&db.pool, BUNDLE_LICENSE).await;
    assert!(
        bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-a")),
            &[licence],
            Some(TENANT),
            false
        )
        .await
        .expect("decided"),
        "owner-a owns TENANT and the row was minted by its bundle"
    );
}

#[tokio::test]
async fn one_non_bundle_licence_refuses_the_whole_bundle_widening() {
    let db = seeded().await;
    let rows = vec![
        load_licence(&db.pool, BUNDLE_LICENSE).await,
        load_licence(&db.pool, LICENSE).await,
    ];
    assert!(
        !bundle_owner_may_revoke(&db.pool, &user(Some("owner-a")), &rows, Some(TENANT), false)
            .await
            .expect("decided"),
        "one row with bundle_id NULL refuses the set"
    );
}

#[tokio::test]
async fn a_grantee_may_not_use_the_bundle_widening() {
    let db = seeded().await;
    let licence = load_licence(&db.pool, BUNDLE_LICENSE).await;
    assert!(
        !bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-b")),
            &[licence],
            Some(TENANT),
            false
        )
        .await
        .expect("decided"),
        "owner-b holds a grant, not ownership, on TENANT"
    );
}

#[tokio::test]
async fn a_licence_from_another_tenant_refuses_the_whole_set() {
    let db = seeded().await;
    let licence = load_licence(&db.pool, BUNDLE_LICENSE).await;
    assert!(
        !bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-b")),
            &[licence],
            Some(FOREIGN_TENANT),
            false
        )
        .await
        .expect("decided"),
        "the row belongs to TENANT, not to the tenant id the call names"
    );
}

#[tokio::test]
async fn the_bundle_widening_counts_an_archived_tenant() {
    let db = seeded().await;
    sqlx::query("UPDATE tenants SET archived_at = now() WHERE id = $1")
        .bind(TENANT)
        .execute(&db.pool)
        .await
        .expect("archive");
    let licence = load_licence(&db.pool, BUNDLE_LICENSE).await;
    assert!(
        bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-a")),
            &[licence],
            Some(TENANT),
            false
        )
        .await
        .expect("decided"),
        "offboarding an archived tenant is the widening's use case"
    );
}

#[tokio::test]
async fn a_dependency_licence_is_admitted_only_when_dependencies_are_allowed() {
    let db = seeded().await;
    let licence = load_licence(&db.pool, DEPENDENCY_LICENSE).await;
    assert!(
        !bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-a")),
            std::slice::from_ref(&licence),
            Some(TENANT),
            false,
        )
        .await
        .expect("decided"),
        "a dependency row has bundle_id NULL and is not bundle-minted"
    );
    assert!(
        bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-a")),
            std::slice::from_ref(&licence),
            Some(TENANT),
            true,
        )
        .await
        .expect("decided"),
        "allow_dependency admits it, and the tenant owner owns the row's tenant"
    );
}

#[tokio::test]
async fn an_unknown_owner_or_an_empty_set_cannot_use_the_bundle_widening() {
    let db = seeded().await;
    let licence = load_licence(&db.pool, BUNDLE_LICENSE).await;
    assert!(
        !bundle_owner_may_revoke(
            &db.pool,
            &user(None),
            std::slice::from_ref(&licence),
            Some(TENANT),
            false,
        )
        .await
        .expect("decided")
    );
    assert!(
        !bundle_owner_may_revoke(&db.pool, &user(Some("owner-a")), &[], Some(TENANT), false)
            .await
            .expect("decided")
    );
    assert!(
        !bundle_owner_may_revoke(
            &db.pool,
            &user(Some("owner-a")),
            std::slice::from_ref(&licence),
            None,
            false
        )
        .await
        .expect("decided")
    );
}

// ── authorize_licence_revoke ────────────────────────────────────────────────

#[tokio::test]
async fn the_issuer_path_admits_a_revoke() {
    let db = seeded().await;
    let row = load_licence(&db.pool, LICENSE).await;
    authorize_licence_revoke(
        &db.pool,
        &user(Some("owner-a")),
        &[row],
        Some(TENANT),
        "license_revoke",
        false,
    )
    .await
    .expect("owner-a issues APP and owns TENANT");
}

#[tokio::test]
async fn a_bundle_owner_may_revoke_an_app_it_does_not_issue() {
    let db = seeded().await;
    let row = load_licence(&db.pool, BUNDLE_LICENSE).await;
    authorize_licence_revoke(
        &db.pool,
        &user(Some("owner-a")),
        &[row],
        Some(TENANT),
        "license_revoke",
        false,
    )
    .await
    .expect("owner-a owns TENANT and its bundle minted the OTHER_APP row");
}

#[tokio::test]
async fn a_mixed_set_is_refused_when_a_later_app_fails() {
    let db = seeded().await;
    // APP (owner-a is an issuer) is walked before OTHER_APP (it is not, and the dependency row is
    // not bundle-minted), so the FIRST app is admitted and the SECOND refuses the whole call.
    let rows = vec![
        load_licence(&db.pool, LICENSE).await,
        load_licence(&db.pool, DEPENDENCY_LICENSE).await,
    ];
    let refusal = authorize_licence_revoke(
        &db.pool,
        &user(Some("owner-a")),
        &rows,
        Some(TENANT),
        "license_revoke",
        false,
    )
    .await
    .expect_err("the dependency row is refused without allow_dependency");
    assert_eq!(refusal.status, StatusCode::FORBIDDEN);
    assert_eq!(
        refusal.body["detail"]["error"],
        "app_not_issuable_by_caller"
    );

    authorize_licence_revoke(
        &db.pool,
        &user(Some("owner-a")),
        &rows,
        Some(TENANT),
        "license_revoke",
        true,
    )
    .await
    .expect("allow_dependency admits the second app through the owner widening");
}

#[tokio::test]
async fn cross_owner_may_revoke_across_apps() {
    let db = seeded().await;
    let rows = vec![
        load_licence(&db.pool, LICENSE).await,
        load_licence(&db.pool, BUNDLE_LICENSE).await,
    ];
    authorize_licence_revoke(
        &db.pool,
        &cross_owner_user(Some("owner-c")),
        &rows,
        Some(TENANT),
        "license_revoke",
        false,
    )
    .await
    .expect("cross_owner bypasses the per-app issuers");
}

#[tokio::test]
async fn an_unknown_api_client_is_refused_before_the_per_app_loop() {
    let db = seeded().await;
    let row = load_licence(&db.pool, LICENSE).await;
    let refusal = authorize_licence_revoke(
        &db.pool,
        &user(None),
        &[row],
        Some(TENANT),
        "license_revoke",
        false,
    )
    .await
    .expect_err("unknown client");
    assert_eq!(refusal.body["detail"]["error"], "unknown_api_client");
}

// ── licences_for_token_hash_in_tenant ───────────────────────────────────────

#[tokio::test]
async fn every_tenant_licence_carrying_the_token_hash_is_returned() {
    let db = seeded().await;
    // A twin: a second licence of the SAME tenant, a different app, whose own token carries the
    // same raw token. `revoke_token_for_tenant` would write both, so both must be authorized.
    let twin_license = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1017);
    let twin_token = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1018);
    // `uq_license_active_tenant_app_subject` makes a second stored-active licence for
    // (TENANT, OTHER_APP, SUBJECT) impossible, so the twin needs its own subject.
    let twin_subject = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_1016);
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(twin_subject)
    .bind("0195f48b-d000-7000-8000-000000001016")
    .execute(&db.pool)
    .await
    .expect("twin subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5)",
    )
    .bind(twin_license)
    .bind(twin_subject)
    .bind(OTHER_APP)
    .bind(OTHER_PLAN)
    .bind(TENANT)
    .execute(&db.pool)
    .await
    .expect("twin licence");
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at)
         VALUES ($1, $2, $3, 'api', now())",
    )
    .bind(twin_token)
    .bind(twin_license)
    .bind(db::tokens::hash_token(RAW))
    .execute(&db.pool)
    .await
    .expect("twin token");

    let rows = licences_for_token_hash_in_tenant(&db.pool, RAW, &TENANT.to_string())
        .await
        .expect("licences");
    let mut ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    ids.sort();
    let mut expected = vec![LICENSE, twin_license];
    expected.sort();
    assert_eq!(ids, expected);
    assert!(rows.iter().all(|row| row.tenant_id == TENANT));
    // FOREIGN_LICENSE carries the same raw token on another tenant and must not appear.
    assert!(rows.iter().all(|row| row.id != FOREIGN_LICENSE));
}

// `ux_token_license_hash (license_id, token_hash)` makes "two tokens with one hash on ONE
// licence" impossible in this schema; the multi-record case the revoke message describes comes
// from one hash on several licences, which the test above covers.

#[tokio::test]
async fn an_unknown_hash_returns_no_rows() {
    let db = seeded().await;
    let rows = licences_for_token_hash_in_tenant(&db.pool, "never-issued", &TENANT.to_string())
        .await
        .expect("licences");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn a_non_uuid_tenant_returns_no_rows() {
    let db = seeded().await;
    let rows = licences_for_token_hash_in_tenant(&db.pool, RAW, "n8n-dev")
        .await
        .expect("a non-UUID tenant is an empty list, not a fault");
    assert!(rows.is_empty());
}
