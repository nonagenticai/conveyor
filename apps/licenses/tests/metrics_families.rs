//! The `/metrics` families, against a real (throwaway) Postgres.
//!
//! The reference (`services/expiry_metrics.py`, `services/tenant_subject_invariant.py`) pinned
//! these behaviours with its own unit tests; this is the Rust port's equivalent, driven through
//! the router so the auth exemption and the handler are exercised too.
//!
//! Each test gets its own database from `common::fresh()`, migrated with the service's own
//! baseline. The seeds are deliberately small and exact: every assertion is a literal sample
//! line, so a drifting population cannot pass by looking plausible.

mod common;

use axum::body::Body;
use axum::http::Request;
use chrono::{DateTime, Duration, Utc};
use conveyor_licenses::config::Config;
use conveyor_licenses::router;
use conveyor_licenses::state::AppState;
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

use uuid::Uuid;

/// The workspace's `uuid` features do not include `v4`; a counter is enough here — every value
/// must merely be distinct, and starting at 1 keeps it clear of the sentinel (`…0000`).
fn fresh_uuid() -> Uuid {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    Uuid::from_u128(u128::from(COUNTER.fetch_add(1, Ordering::Relaxed)))
}

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

async fn migrated() -> common::TestDb {
    let db = common::fresh().await;
    conveyor_licenses::db::migrate(&db.pool)
        .await
        .expect("migrate");
    db
}

async fn scrape(app: axum::Router) -> String {
    let response = app
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answers");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; version=0.0.4; charset=utf-8")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8 exposition")
}

/// The sample lines of one family (never its HELP/TYPE headers, and never a family whose name is
/// a prefix of another's).
fn samples<'a>(text: &'a str, family: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|line| {
            line.strip_prefix(family)
                .is_some_and(|rest| rest.starts_with('{') || rest.starts_with(' '))
        })
        .collect()
}

struct Seed {
    tenant: Uuid,
    app: Uuid,
    plan: Uuid,
    subject: Uuid,
    license: Uuid,
}

async fn seed_base(pool: &PgPool) -> Seed {
    let tenant = fresh_uuid();
    let app = fresh_uuid();
    let plan = fresh_uuid();
    let subject = fresh_uuid();
    let license = fresh_uuid();
    sqlx::query("INSERT INTO tenant_owner (key, display_name) VALUES ('owner-m', 'Owner M')")
        .execute(pool)
        .await
        .expect("tenant_owner");
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
         VALUES ($1, 'tenant-metrics', now(), 'owner-m', 'local_allowed')",
    )
    .bind(tenant)
    .execute(pool)
    .await
    .expect("tenant");
    sqlx::query(
        "INSERT INTO app (id, key, name, created_at) VALUES ($1, 'metrics', 'Metrics', now())",
    )
    .bind(app)
    .execute(pool)
    .await
    .expect("app");
    sqlx::query(
        "INSERT INTO plan (id, app_id, code, name, created_at) VALUES ($1, $2, 'basic', 'Basic', now())",
    )
    .bind(plan)
    .bind(app)
    .execute(pool)
    .await
    .expect("plan");
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(subject)
    .bind(format!("sub-{subject}"))
    .execute(pool)
    .await
    .expect("subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id)
         VALUES ($1, $2, $3, $4, 'active', now(), $5)",
    )
    .bind(license)
    .bind(subject)
    .bind(app)
    .bind(plan)
    .bind(tenant)
    .execute(pool)
    .await
    .expect("license");
    Seed {
        tenant,
        app,
        plan,
        subject,
        license,
    }
}

async fn insert_license(
    pool: &PgPool,
    seed: &Seed,
    status: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Uuid {
    let id = fresh_uuid();
    // Each licence needs its OWN subject: `uq_license_active_tenant_app_subject` is unique on
    // (tenant, app, subject) for active rows, so two active licences cannot share one subject.
    let subject = fresh_uuid();
    sqlx::query(
        "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
         VALUES ($1, $2, 'portal', now())",
    )
    .bind(subject)
    .bind(format!("licence-subject-{subject}"))
    .execute(pool)
    .await
    .expect("licence subject");
    sqlx::query(
        "INSERT INTO license (id, subject_id, app_id, plan_id, status, starts_at, tenant_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, now(), $6, $7)",
    )
    .bind(id)
    .bind(subject)
    .bind(seed.app)
    .bind(seed.plan)
    .bind(status)
    .bind(seed.tenant)
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("license");
    id
}

async fn insert_token(
    pool: &PgPool,
    license: Uuid,
    created_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    revoked: bool,
) -> Uuid {
    let id = fresh_uuid();
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at, expires_at, revoked)
         VALUES ($1, $2, $3, 'api', $4, $5, $6)",
    )
    .bind(id)
    .bind(license)
    .bind(format!("hash-{id}"))
    .bind(created_at)
    .bind(expires_at)
    .bind(revoked)
    .execute(pool)
    .await
    .expect("token");
    id
}

/// A live token whose whole lifetime is `lifetime`, centred on now.
async fn token_with_lifetime(pool: &PgPool, license: Uuid, lifetime: Duration) -> Uuid {
    let now = Utc::now();
    insert_token(
        pool,
        license,
        Some(now - lifetime / 2),
        Some(now + lifetime / 2),
        false,
    )
    .await
}

// ── P1: the lifetime classes, at every boundary the reference pins ────────────────────────────

#[tokio::test]
async fn lifetime_class_boundaries() {
    let db = migrated().await;
    // The schema declares created_at NOT NULL; the reference's own suite seeds a NULL row
    // (`NULL::timestamptz` → standard) and the classifier must keep handling it.
    sqlx::query("ALTER TABLE token ALTER COLUMN created_at DROP NOT NULL")
        .execute(&db.pool)
        .await
        .expect("drop not null");
    let seed = seed_base(&db.pool).await;

    let minutes = |n: i64| Duration::minutes(n);
    let seconds = |n: i64| Duration::seconds(n);
    let mut cases: Vec<(Uuid, &str)> = Vec::new();
    for (lifetime, class) in [
        (minutes(1), "short"),
        (minutes(60), "short"),
        (minutes(65), "short"),
        (minutes(65) + seconds(1), "lt7d"),
        (minutes(66), "lt7d"),
        (minutes(24 * 60), "lt7d"),
        (Duration::days(7) + Duration::milliseconds(400), "lt7d"),
        (Duration::days(7), "lt7d"),
        (Duration::days(7) + minutes(4) + seconds(59), "lt7d"),
        (Duration::days(7) + minutes(5), "lt30d"),
        (Duration::days(30) + Duration::milliseconds(16), "lt30d"),
        (Duration::days(30), "lt30d"),
        (Duration::days(30) + minutes(4) + seconds(59), "lt30d"),
        (Duration::days(30) + minutes(5), "lt60d"),
        (Duration::days(60), "lt60d"),
        (Duration::days(60) + minutes(4) + seconds(59), "lt60d"),
        (Duration::days(60) + minutes(5), "standard"),
        (Duration::days(365), "standard"),
    ] {
        cases.push((
            token_with_lifetime(&db.pool, seed.license, lifetime).await,
            class,
        ));
    }
    // NULL created_at → standard (an ambiguity resolves to the alerted class).
    cases.push((
        insert_token(
            &db.pool,
            seed.license,
            None,
            Some(Utc::now() + Duration::days(10)),
            false,
        )
        .await,
        "standard",
    ));
    // Zero lifetime and a lifetime born backwards → standard.
    let now = Utc::now();
    cases.push((
        insert_token(&db.pool, seed.license, Some(now), Some(now), false).await,
        "standard",
    ));
    cases.push((
        insert_token(
            &db.pool,
            seed.license,
            Some(now),
            Some(now - Duration::days(1)),
            false,
        )
        .await,
        "standard",
    ));

    let text = scrape(router::router(state(db.pool.clone()))).await;
    let token_lines = samples(&text, "nonagentic_license_token_expiry_timestamp_seconds");
    for (token_id, class) in &cases {
        let hit = token_lines
            .iter()
            .find(|line| line.contains(&format!("token_id=\"{token_id}\"")))
            .unwrap_or_else(|| panic!("no sample for token {token_id}:\n{text}"));
        assert!(
            hit.contains(&format!("lifetime_class=\"{class}\"")),
            "token {token_id}: expected {class}, got {hit}"
        );
    }
    assert!(
        token_lines.len() >= cases.len(),
        "every seeded token is exported: {} samples for {} cases",
        token_lines.len(),
        cases.len()
    );
}

// ── P1: the exact sample sets and their values ─────────────────────────────────────────────────

#[tokio::test]
async fn expiry_families() {
    let db = migrated().await;
    let seed = seed_base(&db.pool).await;
    let now = Utc::now();

    // Licences: the base licence is active with NO expiry (coverage false); one active WITH an
    // expiry; one revoked and past expiry; one non-revoked and past expiry.
    let expiring_license =
        insert_license(&db.pool, &seed, "active", Some(now + Duration::days(30))).await;
    insert_license(&db.pool, &seed, "revoked", Some(now - Duration::days(1))).await;
    insert_license(&db.pool, &seed, "expired", Some(now - Duration::days(2))).await;

    // Tokens: standard (live), lt30d (live), short (expired), revoked (excluded from the series),
    // no expiry (coverage only), and a standard token already expired (the alerted series).
    let standard = insert_token(
        &db.pool,
        seed.license,
        Some(now - Duration::days(60)),
        Some(now + Duration::days(30)),
        false,
    )
    .await;
    let lt30d = insert_token(
        &db.pool,
        seed.license,
        Some(now - Duration::days(5)),
        Some(now + Duration::days(5)),
        false,
    )
    .await;
    let short_expired = insert_token(
        &db.pool,
        seed.license,
        Some(now - Duration::minutes(120)),
        Some(now - Duration::minutes(60)),
        false,
    )
    .await;
    insert_token(
        &db.pool,
        seed.license,
        Some(now - Duration::days(60)),
        Some(now + Duration::days(10)),
        true,
    )
    .await;
    insert_token(&db.pool, seed.license, Some(now), None, false).await;
    insert_token(
        &db.pool,
        seed.license,
        Some(now - Duration::days(100)),
        Some(now - Duration::days(1)),
        false,
    )
    .await;

    let text = scrape(router::router(state(db.pool.clone()))).await;

    // The collector is up.
    assert_eq!(
        samples(&text, "nonagentic_license_expiry_collector_up"),
        vec!["nonagentic_license_expiry_collector_up 1"]
    );

    // The licence expiry family names the one active licence with an expiry.
    let licence_lines = samples(&text, "nonagentic_license_expiry_timestamp_seconds");
    assert_eq!(licence_lines.len(), 1, "{licence_lines:?}");
    assert!(licence_lines[0].contains(&format!("license_id=\"{expiring_license}\"")));
    assert!(licence_lines[0].contains("status=\"active\""));

    // Token series: the four non-revoked tokens with an expiry (expired ones stay until reaped).
    let token_lines = samples(&text, "nonagentic_license_token_expiry_timestamp_seconds");
    assert_eq!(token_lines.len(), 4, "{token_lines:?}");
    assert!(
        token_lines
            .iter()
            .any(|l| l.contains(&format!("token_id=\"{standard}\""))
                && l.contains("lifetime_class=\"standard\""))
    );
    assert!(
        token_lines
            .iter()
            .any(|l| l.contains(&format!("token_id=\"{lt30d}\""))
                && l.contains("lifetime_class=\"lt30d\""))
    );
    assert!(
        token_lines
            .iter()
            .any(|l| l.contains(&format!("token_id=\"{short_expired}\""))
                && l.contains("lifetime_class=\"short\""))
    );

    // Coverage: 5 live non-revoked tokens, 4 with an expiry; 2 active licences, 1 with one.
    let coverage = samples(&text, "nonagentic_license_expiry_coverage_rows");
    assert_eq!(coverage.len(), 4, "{coverage:?}");
    assert!(coverage.contains(
        &"nonagentic_license_expiry_coverage_rows{has_expiry=\"false\",kind=\"license\"} 1"
    ));
    assert!(coverage.contains(
        &"nonagentic_license_expiry_coverage_rows{has_expiry=\"true\",kind=\"license\"} 1"
    ));
    assert!(coverage.contains(
        &"nonagentic_license_expiry_coverage_rows{has_expiry=\"false\",kind=\"token\"} 1"
    ));
    assert!(coverage.contains(
        &"nonagentic_license_expiry_coverage_rows{has_expiry=\"true\",kind=\"token\"} 4"
    ));

    // Expired rows: all twelve pairs seeded, only the three real ones non-zero.
    let expired = samples(&text, "nonagentic_license_expired_rows");
    assert_eq!(expired.len(), 12, "{expired:?}");
    let want = [
        ("license", "standard", "false", 1),
        ("license", "standard", "true", 1),
        ("token", "short", "false", 1),
        ("token", "standard", "false", 1),
    ];
    for (kind, class, revoked, n) in want {
        let line = format!(
            "nonagentic_license_expired_rows{{kind=\"{kind}\",lifetime_class=\"{class}\",revoked=\"{revoked}\"}} {n}"
        );
        assert!(
            expired.contains(&line.as_str()),
            "missing {line} in {expired:?}"
        );
    }
    // Every other pair is present at zero (an assertion, not an absent series).
    assert!(expired.contains(&"nonagentic_license_expired_rows{kind=\"token\",lifetime_class=\"lt7d\",revoked=\"false\"} 0"));

    let truncated = samples(&text, "nonagentic_license_expiry_series_truncated");
    assert_eq!(truncated.len(), 6, "{truncated:?}");
    assert!(truncated.iter().all(|line| line.ends_with(" 0")));
}

#[tokio::test]
async fn short_cap_does_not_crowd_out_standard() {
    let db = migrated().await;
    let seed = seed_base(&db.pool).await;
    let now = Utc::now();

    // 201 live short tokens (whole lifetime <= 65 minutes) under one licence.
    sqlx::query(
        "INSERT INTO token (id, license_id, token_hash, token_type, created_at, expires_at, revoked)
         SELECT gen_random_uuid(), $1, 'short-'||g, 'api', now() - interval '1 minute',
                now() + interval '1 minute', false
         FROM generate_series(1, 201) g",
    )
    .bind(seed.license)
    .execute(&db.pool)
    .await
    .expect("short tokens");
    // Two standard tokens that would be evicted by a single shared cap.
    for _ in 0..2 {
        insert_token(
            &db.pool,
            seed.license,
            Some(now - Duration::days(10)),
            Some(now + Duration::days(80)),
            false,
        )
        .await;
    }

    let text = scrape(router::router(state(db.pool.clone()))).await;
    let truncated = samples(&text, "nonagentic_license_expiry_series_truncated");
    assert!(truncated.contains(
        &"nonagentic_license_expiry_series_truncated{kind=\"token\",lifetime_class=\"short\"} 1"
    ));
    assert!(truncated.contains(
        &"nonagentic_license_expiry_series_truncated{kind=\"token\",lifetime_class=\"standard\"} 0"
    ));

    let token_lines = samples(&text, "nonagentic_license_token_expiry_timestamp_seconds");
    let short = token_lines
        .iter()
        .filter(|line| line.contains("lifetime_class=\"short\""))
        .count();
    let standard = token_lines
        .iter()
        .filter(|line| line.contains("lifetime_class=\"standard\""))
        .count();
    assert_eq!(short, 200, "the short cap exports exactly 200");
    assert_eq!(standard, 2, "both standard rows survive the short class");
}

// ── P1: the fault path ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn expiry_fault_is_isolated() {
    let db = migrated().await;
    let seed = seed_base(&db.pool).await;
    token_with_lifetime(&db.pool, seed.license, Duration::days(90)).await;
    // Sabotage only the tables the expiry collector reads.
    sqlx::query("ALTER TABLE token RENAME TO token_hidden")
        .execute(&db.pool)
        .await
        .expect("rename token");

    let text = scrape(router::router(state(db.pool.clone()))).await;
    assert_eq!(
        samples(&text, "nonagentic_license_expiry_collector_up"),
        vec!["nonagentic_license_expiry_collector_up 0"]
    );
    assert!(
        samples(&text, "nonagentic_license_token_expiry_timestamp_seconds").is_empty(),
        "a failed expiry snapshot emits NO expiry families:\n{text}"
    );
    assert!(
        samples(&text, "nonagentic_license_expiry_coverage_rows").is_empty(),
        "{text}"
    );
    // The other collector is untouched.
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_subject_collector_up"),
        vec!["nonagentic_license_tenant_subject_collector_up 1"]
    );
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_is_subject_rows").len(),
        6
    );
}

#[tokio::test]
async fn both_collectors_down_without_a_database() {
    let text = scrape(router::router(router::test_state())).await;
    assert_eq!(
        samples(&text, "nonagentic_license_expiry_collector_up"),
        vec!["nonagentic_license_expiry_collector_up 0"]
    );
    assert!(
        samples(&text, "nonagentic_license_token_expiry_timestamp_seconds").is_empty(),
        "{text}"
    );
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_subject_collector_up"),
        vec!["nonagentic_license_tenant_subject_collector_up 0"]
    );
    // The refusal families are registers, not snapshot facts: they survive the fault gate.
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_seed_principal_refusals"),
        vec!["nonagentic_license_tenant_seed_principal_refusals 0"]
    );
    assert_eq!(
        samples(&text, "nonagentic_license_subject_principal_refusals"),
        vec!["nonagentic_license_subject_principal_refusals 0"]
    );
    assert!(
        samples(&text, "nonagentic_license_tenant_is_subject_rows").is_empty(),
        "{text}"
    );
}

// ── P2: the tenant/subject collector ──────────────────────────────────────────────────────────

#[tokio::test]
async fn tenant_families() {
    let db = migrated().await;
    let seed = seed_base(&db.pool).await;

    const SENTINEL: &str = "00000000-0000-0000-0000-000000000000";
    let sentinel = Uuid::parse_str(SENTINEL).expect("sentinel uuid");
    let insert_tenant = |id: Uuid, name: &'static str| {
        let pool = db.pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO tenants (id, name, created_at, owner_key, byo_mode)
                 VALUES ($1, $2, now(), 'owner-m', 'local_allowed')",
            )
            .bind(id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("tenant");
        }
    };
    let insert_subject = |id: Uuid, keycloak_sub: String| {
        let pool = db.pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO subject (id, keycloak_sub, keycloak_realm, created_at)
                 VALUES ($1, $2, 'portal', now())",
            )
            .bind(id)
            .bind(keycloak_sub)
            .execute(&pool)
            .await
            .expect("subject");
        }
    };

    // The sentinel tenant matches BOTH doors: one subject claims it as its keycloak_sub, another
    // carries it as its row id.
    insert_tenant(sentinel, "default-anon").await;
    insert_subject(fresh_uuid(), SENTINEL.to_owned()).await;
    insert_subject(sentinel, "sentinel-row-id-door".to_owned()).await;

    // keycloak_sub door, with a CASE DIFFERENCE: the SQL folds both sides.
    let sub_tenant = fresh_uuid();
    insert_tenant(sub_tenant, "tenant-sub").await;
    insert_subject(fresh_uuid(), sub_tenant.to_string().to_uppercase()).await;

    // subject_id door: the tenant's own id IS a subject row id.
    let sid_subject = fresh_uuid();
    insert_subject(sid_subject, "unrelated-sid-sub".to_owned()).await;
    insert_tenant(sid_subject, "tenant-sid").await;

    // Both doors at once: one tenant id that is a keycloak_sub AND another subject's row id.
    let both = fresh_uuid();
    insert_tenant(both, "tenant-both").await;
    insert_subject(fresh_uuid(), both.to_string()).await;
    insert_subject(both, "unrelated-both-sub".to_owned()).await;

    let text = scrape(router::router(state(db.pool.clone()))).await;

    assert_eq!(
        samples(&text, "nonagentic_license_tenant_subject_collector_up"),
        vec!["nonagentic_license_tenant_subject_collector_up 1"]
    );
    let measured = samples(
        &text,
        "nonagentic_license_tenant_is_subject_source_measured",
    );
    assert_eq!(measured.len(), 2, "{measured:?}");
    assert!(measured.contains(
        &"nonagentic_license_tenant_is_subject_source_measured{source=\"licences_subject\"} 1"
    ));
    assert!(measured.contains(
        &"nonagentic_license_tenant_is_subject_source_measured{source=\"keycloak_principal\"} 0"
    ));

    let rows = samples(&text, "nonagentic_license_tenant_is_subject_rows");
    assert_eq!(rows.len(), 6, "{rows:?}");
    for line in [
        "nonagentic_license_tenant_is_subject_rows{class=\"sentinel\",match=\"keycloak_sub\",source=\"licences_subject\"} 1",
        "nonagentic_license_tenant_is_subject_rows{class=\"sentinel\",match=\"subject_id\",source=\"licences_subject\"} 1",
        "nonagentic_license_tenant_is_subject_rows{class=\"retired-principal\",match=\"keycloak_sub\",source=\"licences_subject\"} 0",
        "nonagentic_license_tenant_is_subject_rows{class=\"retired-principal\",match=\"subject_id\",source=\"licences_subject\"} 0",
        "nonagentic_license_tenant_is_subject_rows{class=\"violation\",match=\"keycloak_sub\",source=\"licences_subject\"} 2",
        "nonagentic_license_tenant_is_subject_rows{class=\"violation\",match=\"subject_id\",source=\"licences_subject\"} 2",
    ] {
        assert!(rows.contains(&line), "missing {line} in {rows:?}");
    }

    let named = samples(&text, "nonagentic_license_tenant_is_subject");
    assert_eq!(named.len(), 6, "{named:?}");
    assert!(
        named
            .iter()
            .any(|line| line.contains(&format!("tenant_id=\"{SENTINEL}\""))
                && line.contains("class=\"sentinel\""))
    );
    assert!(
        named
            .iter()
            .any(|line| line.contains(&format!("tenant_id=\"{both}\""))
                && line.contains("match=\"keycloak_sub\"")
                && line.contains("class=\"violation\""))
    );
    assert!(
        named
            .iter()
            .any(|line| line.contains(&format!("tenant_id=\"{both}\""))
                && line.contains("match=\"subject_id\"")
                && line.contains("class=\"violation\""))
    );
    // The reference collects the realm but does not emit it.
    assert!(named.iter().all(|line| !line.contains("keycloak_realm")));

    assert_eq!(
        samples(
            &text,
            "nonagentic_license_tenant_is_subject_series_truncated"
        ),
        vec!["nonagentic_license_tenant_is_subject_series_truncated 0"]
    );
    // The clean base tenant is not named.
    assert!(
        !named
            .iter()
            .any(|line| line.contains(&seed.tenant.to_string()))
    );
}

#[tokio::test]
async fn tenant_fault_is_isolated() {
    let db = migrated().await;
    let seed = seed_base(&db.pool).await;
    token_with_lifetime(&db.pool, seed.license, Duration::days(90)).await;
    sqlx::query("ALTER TABLE subject RENAME TO subject_hidden")
        .execute(&db.pool)
        .await
        .expect("rename subject");

    let text = scrape(router::router(state(db.pool.clone()))).await;
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_subject_collector_up"),
        vec!["nonagentic_license_tenant_subject_collector_up 0"]
    );
    assert!(
        samples(&text, "nonagentic_license_tenant_is_subject_rows").is_empty(),
        "{text}"
    );
    // The refusal registers are emitted before the gate, exactly as the reference does.
    assert_eq!(
        samples(&text, "nonagentic_license_tenant_seed_principal_refusals"),
        vec!["nonagentic_license_tenant_seed_principal_refusals 0"]
    );
    assert_eq!(
        samples(&text, "nonagentic_license_subject_principal_refusals"),
        vec!["nonagentic_license_subject_principal_refusals 0"]
    );
    // The expiry collector is untouched.
    assert_eq!(
        samples(&text, "nonagentic_license_expiry_collector_up"),
        vec!["nonagentic_license_expiry_collector_up 1"]
    );
    assert_eq!(
        samples(&text, "nonagentic_license_token_expiry_timestamp_seconds").len(),
        1
    );
}
