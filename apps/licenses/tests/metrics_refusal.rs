//! The subject-refusal register, through the guard that refuses.
//!
//! `metrics_families.rs` asserts the register's empty rendering; this binary asserts the WIRING:
//! that the bundle's subject guard actually records a refusal (and does not record an admission).
//! It is a separate test binary on purpose — the register is process-global, and a refusal
//! recorded here must not leak into the clean-estate assertions of the other binary.

mod common;

use axum::body::Body;
use axum::http::Request;
use conveyor_licenses::db::bundle_apply_rows::{
    CreateSubjectError, create_or_get_subject_for_mint,
};
use conveyor_licenses::{config::Config, db, router, state::AppState};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        Config {
            port: 0,
            environment: "test".to_owned(),
            version: "0.0.0".to_owned(),
            trusted_issuers: Vec::new(),
            introspect_require_auth: false,
            service_actor_id: None,
        },
        conveyor_licenses::auth::JwtVerifier::new(
            Vec::new(),
            std::sync::Arc::new(conveyor_licenses::auth::UnavailableJwks),
        ),
    )
}

async fn scrape(pool: PgPool) -> String {
    let response = router::router(state(pool))
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8 exposition")
}

fn samples<'a>(text: &'a str, family: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|line| {
            line.strip_prefix(family)
                .is_some_and(|rest| rest.starts_with('{') || rest.starts_with(' '))
        })
        .collect()
}

async fn insert_tenant(pool: &PgPool, id: Uuid, name: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, $2, now(), 'owner-r', 'local_allowed')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .expect("tenant");
}

#[tokio::test]
async fn the_guard_populates_the_register_and_admissions_do_not() {
    let db = common::fresh().await;
    db::migrate(&db.pool).await.expect("migrate");
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-r', 'Owner R')")
        .execute(&db.pool)
        .await
        .expect("tenant_owner");

    // A tenant that never traded: its id is refused as a subject registration.
    let refused_tenant = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0001);
    insert_tenant(&db.pool, refused_tenant, "tenant-refusal").await;
    let refused = create_or_get_subject_for_mint(
        &db.pool,
        &refused_tenant.to_string(),
        "realm-refusal",
        false,
    )
    .await;
    assert!(
        matches!(refused, Err(CreateSubjectError::Refused { .. })),
        "a tenant id with no licences is refused as a subject: {refused:?}"
    );

    // A tenant that HAS traded is admitted (the reference's exemption) and must NOT be counted:
    // the register records prevented violations, not admissions.
    let traded_tenant = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0002);
    insert_tenant(&db.pool, traded_tenant, "tenant-traded").await;
    let app = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0003);
    let plan = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0004);
    let subject = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0005);
    let licence = Uuid::from_u128(0x2610_0000_0000_0000_0000_0000_0000_0006);
    sqlx::query(
        "INSERT INTO app (id, key, name, created_at) VALUES ($1, 'refusal', 'Refusal', now())",
    )
    .bind(app)
    .execute(&db.pool)
    .await
    .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(plan)
    .bind(app)
    .execute(&db.pool)
    .await
    .expect("plan");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, 'traded-subject', 'portal', now())",
    )
    .bind(subject)
    .execute(&db.pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5)",
    )
    .bind(licence)
    .bind(subject)
    .bind(app)
    .bind(plan)
    .bind(traded_tenant)
    .execute(&db.pool)
    .await
    .expect("license");
    let admitted = create_or_get_subject_for_mint(
        &db.pool,
        &traded_tenant.to_string(),
        "realm-refusal",
        false,
    )
    .await;
    assert!(
        admitted.is_ok(),
        "a tenant that holds a licence is admitted, not counted: {admitted:?}"
    );

    let text = scrape(db.pool.clone()).await;

    // Exactly ONE distinct refused identity, carrying the tenant name an operator must re-key.
    assert_eq!(
        samples(&text, "nonagentic_license_subject_principal_refusals"),
        vec!["nonagentic_license_subject_principal_refusals 1"]
    );
    let named = samples(&text, "nonagentic_license_subject_principal_refused");
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(
        named[0],
        format!(
            "nonagentic_license_subject_principal_refused{{keycloak_realm=\"realm-refusal\",keycloak_sub=\"{refused_tenant}\",tenant_name=\"tenant-refusal\"}} 1"
        )
    );

    // The collectors themselves are healthy; the register does not disturb them.
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_subject_collector_up"),
        vec!["nonagentic_license_tenant_subject_collector_up 1"]
    );
    assert_eq!(
        samples(&text, "nonagentic_license_expiry_collector_up"),
        vec!["nonagentic_license_expiry_collector_up 1"]
    );
}
