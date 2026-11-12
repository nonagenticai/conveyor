//! The client-credentials client against a local HTTP stub.
//!
//! `cargo test -p conveyor-auth --test oauth`. The stub is a real axum server on an
//! ephemeral port: the form body, the status handling, the body parsing and the cache are
//! exercised over HTTP, not through a mocked trait.
//!
//! Three promises, each a way this could be wrong in production:
//! 1. two calls inside the lifetime cost ONE token request (otherwise every action would
//!    mint a token and Keycloak would see the load);
//! 2. a refusal is an error to the caller, never a stale token (a revoked client must not
//!    keep working for an hour off a cache);
//! 3. the token request carries `grant_type=client_credentials` and the configured client.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::routing::post;
use conveyor_auth::oauth::{ClientCredentials, TokenError};
use tokio::net::TcpListener;

#[derive(Clone)]
struct Stub {
    hits: Arc<AtomicUsize>,
    status: StatusCode,
    body: &'static str,
}

async fn token(
    State(stub): State<Stub>,
    Form(form): Form<Vec<(String, String)>>,
) -> (StatusCode, &'static str) {
    stub.hits.fetch_add(1, Ordering::SeqCst);
    let got: std::collections::HashMap<_, _> = form.into_iter().collect();
    assert_eq!(
        got.get("grant_type").map(String::as_str),
        Some("client_credentials")
    );
    assert_eq!(
        got.get("client_id").map(String::as_str),
        Some("conveyor-backend")
    );
    assert_eq!(
        got.get("client_secret").map(String::as_str),
        Some("s3cret-value")
    );
    (stub.status, stub.body)
}

async fn start(status: StatusCode, body: &'static str) -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route("/token", post(token)).with_state(Stub {
        hits: hits.clone(),
        status,
        body,
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("stub server");
    });
    (format!("http://{addr}/token"), hits)
}

fn client(url: &str) -> ClientCredentials {
    ClientCredentials::new(url, "conveyor-backend", "s3cret-value").expect("client")
}

#[tokio::test]
async fn two_calls_inside_the_lifetime_mint_one_token() {
    let (url, hits) = start(
        StatusCode::OK,
        r#"{"access_token":"tok-1","expires_in":3600,"token_type":"Bearer"}"#,
    )
    .await;
    let client = client(&url);
    assert_eq!(client.token().await.expect("first"), "tok-1");
    assert_eq!(client.token().await.expect("second"), "tok-1");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        client.authorization().await.expect("header"),
        "Bearer tok-1"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_refused_client_is_an_error_not_a_token() {
    let (url, _hits) = start(
        StatusCode::UNAUTHORIZED,
        r#"{"error":"unauthorized_client"}"#,
    )
    .await;
    let client = client(&url);
    let error = client.token().await.expect_err("must refuse");
    assert!(matches!(error, TokenError::Refused { status: 401, .. }));
}

#[tokio::test]
async fn a_body_without_a_token_is_an_error() {
    let (url, _hits) = start(StatusCode::OK, r#"{"expires_in":3600}"#).await;
    let client = client(&url);
    let error = client.token().await.expect_err("must refuse");
    assert!(matches!(error, TokenError::Body { .. }));
}

#[tokio::test]
async fn an_unreachable_endpoint_is_a_transport_error() {
    // Bind and drop, so the port is closed but a connection attempt still gets ECONNREFUSED.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    let client = client(&format!("http://{addr}/token"));
    let error = client.token().await.expect_err("must refuse");
    assert!(matches!(error, TokenError::Transport { .. }));
}
