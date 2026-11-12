//! `licence.sync` against a local HTTP stub, and the fail-closed verdict.
//!
//! `cargo test -p conveyor-auth --test licence -- --nocapture`. The stub is a real axum
//! server on an ephemeral port: the client's transport, status handling and body parsing are
//! exercised over HTTP, not through a mocked trait.

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use chrono::{DateTime, Utc};
use conveyor_auth::licence::{LicenceSyncClient, SyncError, Verdict, evaluate};
use conveyor_domain::TenantId;
use tokio::net::TcpListener;

/// Start a stub that answers `/licenses/active` with one fixed status + body.
async fn stub(status: StatusCode, body: &'static str) -> String {
    let app = Router::new().route(
        "/licenses/active",
        get(move || async move { (status, body) }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the stub");
    let addr = listener.local_addr().expect("stub addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("stub server");
    });
    format!("http://{addr}")
}

fn tenant() -> TenantId {
    TenantId::new()
}

#[tokio::test]
async fn a_200_active_licence_is_synced_with_its_expiry() {
    let base = stub(
        StatusCode::OK,
        r#"{"active":true,"status":"active","expires_at":"2099-01-01T00:00:00Z","tenant_id":"x"}"#,
    )
    .await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let outcome = client.fetch(tenant(), "conveyor").await.expect("fetch");
    println!("200 active -> {outcome:?}");
    assert_eq!(outcome.status, "active");
    assert_eq!(
        outcome.expires_at,
        Some(
            DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    assert_eq!(outcome.raw["active"], true);

    // The synced row is what the request path reads; an active, unexpired, fresh row allows.
    let row = conveyor_store::licence::LicenceRow {
        tenant_id: tenant().as_uuid(),
        app_key: "conveyor".into(),
        status: outcome.status,
        expires_at: outcome.expires_at,
        checked_at: Utc::now(),
        raw: outcome.raw,
    };
    assert_eq!(evaluate(Some(&row), Utc::now()), Verdict::Allowed);
}

#[tokio::test]
async fn a_404_is_unknown_and_never_active() {
    let base = stub(
        StatusCode::NOT_FOUND,
        r#"{"detail":{"error":"no_active_license"}}"#,
    )
    .await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let outcome = client.fetch(tenant(), "conveyor").await.expect("fetch");
    println!("404 -> {outcome:?}");
    assert_eq!(outcome.status, "unknown");
    assert!(outcome.expires_at.is_none());
}

#[tokio::test]
async fn a_409_conflict_is_entitled_like_frs_gate() {
    let base = stub(StatusCode::CONFLICT, r#"{"detail":"multiple"}"#).await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let outcome = client.fetch(tenant(), "conveyor").await.expect("fetch");
    println!("409 -> {outcome:?}");
    assert_eq!(outcome.status, "active");
}

#[tokio::test]
async fn an_upstream_error_is_a_retryable_failure_not_a_licence() {
    let base = stub(StatusCode::INTERNAL_SERVER_ERROR, "boom").await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let error = client
        .fetch(tenant(), "conveyor")
        .await
        .expect_err("a 500 is not an entitlement");
    println!("500 -> {error}");
    assert!(matches!(error, SyncError::Status { status: 500, .. }));
}

#[tokio::test]
async fn an_unparsable_body_or_expiry_fails_closed() {
    let not_json = stub(StatusCode::OK, "not json at all").await;
    let client = LicenceSyncClient::new(&not_json).expect("client");
    assert!(matches!(
        client.fetch(tenant(), "conveyor").await,
        Err(SyncError::Body { .. })
    ));

    let bad_expiry = stub(
        StatusCode::OK,
        r#"{"active":true,"status":"active","expires_at":"next tuesday"}"#,
    )
    .await;
    let client = LicenceSyncClient::new(&bad_expiry).expect("client");
    assert!(
        matches!(
            client.fetch(tenant(), "conveyor").await,
            Err(SyncError::Body { .. })
        ),
        "an unparsable expiry must not be dropped into 'never expires'"
    );
}

#[tokio::test]
async fn an_inactive_200_body_is_unknown() {
    let base = stub(
        StatusCode::OK,
        r#"{"active":false,"status":"active","expires_at":null}"#,
    )
    .await;
    let client = LicenceSyncClient::new(&base).expect("client");
    let outcome = client.fetch(tenant(), "conveyor").await.expect("fetch");
    assert_eq!(
        outcome.status, "unknown",
        "`active: false` wins over a contradictory status string"
    );
}

#[test]
fn a_malformed_base_url_is_refused_at_construction() {
    assert!(matches!(
        LicenceSyncClient::new("licences.example.com:8080"),
        Err(SyncError::BaseUrl(_))
    ));
    assert!(LicenceSyncClient::new("http://licences.example.com:8080/").is_ok());
}
