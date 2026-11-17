//! conveyor#226 — the reference's tenant gate, restored.
//!
//! Three credential shapes, each with a positive control that differs in ONE input, so no
//! assertion can pass because the gate is inert or because it refuses everything:
//!
//! * no credential                          → 401 `AUTHENTICATION_REQUIRED`
//! * `X-Tenant-Id: Bearer <licence>`        → the licence's tenant
//! * bare `X-Tenant-Id` + `Authorization`   → the NAMED tenant, only for an allowlisted subject
//! * bare `X-Tenant-Id` with no vouch       → 401; with a stranger subject → 403
//! * a non-uuid tenant                      → 400 `INVALID_TENANT_ID`
//!
//! The introspection endpoint is a process-local stub (`common::introspect_url`) that answers by
//! token prefix, so the credential is exercised end to end through the real extractor.

mod common;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use conveyor_research::media;
use uuid::Uuid;

const TENANT: &str = "00000000-0000-7000-8000-000000000001";
const OTHER_TENANT: &str = "00000000-0000-7000-8000-000000000002";
const ITEM: &str = "10000000-0000-7000-8000-000000000001";
const FOREIGN_ITEM: &str = "20000000-0000-7000-8000-000000000001";
const SEED: &str = include_str!("../contract/media/seed.sql");

fn app(pool: sqlx::PgPool) -> axum::Router {
    media::build_app(media::AppState::with_pool(common::media_config(), pool))
}

async fn seeded() -> common::TestDb {
    let db = common::fresh().await;
    media::db::migrate(&db.pool).await.expect("migrate");
    sqlx::raw_sql(SEED)
        .execute(&db.pool)
        .await
        .expect("load the capture's pre-state");
    // The other tenant's row: the cross-tenant control every scoping assertion needs.
    sqlx::query(
        "INSERT INTO media_items (id, tenant_id, title, url, media_type, source, external_ids,
                                  tags, metadata, created_at, updated_at)
         VALUES ($1, $2, 'Foreign item', 'https://example.com/foreign', 'video', 'web_scraper',
                 '{}'::jsonb, '[]'::jsonb, '{}'::jsonb, now(), now())",
    )
    .bind(FOREIGN_ITEM)
    .bind(Uuid::parse_str(OTHER_TENANT).expect("other tenant is a uuid"))
    .execute(&db.pool)
    .await
    .expect("foreign item");
    db
}

async fn get(
    app: &axum::Router,
    path: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let mut builder = Request::builder().uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("the app answers");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, headers, value)
}

#[tokio::test]
async fn no_credential_is_refused() {
    let db = seeded().await;
    let path = format!("/api/items/{ITEM}");
    let (status, headers, body) = get(&app(db.pool.clone()), &path, &[]).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED", "{body}");
    assert_eq!(
        headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer"),
        "a 401 without the challenge tells a client to retry with nothing"
    );
}

#[tokio::test]
async fn a_service_token_scopes_the_tenant() {
    let db = seeded().await;
    let app = app(db.pool.clone());
    let vouch = common::vouch_auth();

    // The named tenant's own row, with an allowlisted service token vouching for it.
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", TENANT), ("authorization", &vouch)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], TENANT);
    assert_eq!(body["title"], "Synthetic video one");

    // The vouch never RE-LABELS: the stub's licence names `common::LICENCE_OWN_TENANT`, and the
    // answer is still the tenant the HEADER named — not the licence's own.
    assert_ne!(body["tenant_id"], common::LICENCE_OWN_TENANT);

    // Another tenant's row is refused under the name of that other tenant.
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", OTHER_TENANT), ("authorization", &vouch)],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Negative controls, each one input away from the positive case.
    let (status, _, _) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", TENANT)],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a bare tenant with no service token is a claim, not a credential"
    );

    let stranger = format!("Bearer stranger-{}", common::STRANGER_SUBJECT);
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", TENANT), ("authorization", &stranger)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "TENANT_TOKEN_REQUIRED", "{body}");

    let (status, _, _) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[
            ("x-tenant-id", TENANT),
            ("authorization", "Bearer expired-token"),
        ],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "an inactive licence cannot vouch"
    );
}

#[tokio::test]
async fn a_licence_token_names_the_tenant() {
    let db = seeded().await;
    let app = app(db.pool.clone());

    // `licence-<uuid>` is the stub's ACTIVE licence naming that tenant.
    let licence = format!("Bearer licence-{TENANT}");
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", &licence)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tenant_id"], TENANT);

    // The licence for one tenant cannot read another's row.
    let other_licence = format!("Bearer licence-{OTHER_TENANT}");
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", &other_licence)],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Unknown or inactive → the reference's 401, not a 200 with no tenant.
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", "Bearer expired-token")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED", "{body}");

    // An active licence with no tenant context is refused the same way.
    let (status, _, body) = get(
        &app,
        &format!("/api/items/{ITEM}"),
        &[("x-tenant-id", "Bearer licence-not-a-uuid")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body["error"]["message"],
        "License token missing tenant context"
    );
}

#[tokio::test]
async fn a_malformed_tenant_is_a_400() {
    let db = seeded().await;
    // The reference's measured case: a slug where the column is a uuid.
    let (status, _, body) = get(
        &app(db.pool.clone()),
        &format!("/api/items/{ITEM}"),
        &[
            ("x-tenant-id", "n8n-dev"),
            ("authorization", &common::vouch_auth()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "INVALID_TENANT_ID", "{body}");
}
