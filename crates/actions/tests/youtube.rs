//! #182 slice A — the tenant's own YouTube channel, end to end through the real HTTP door.
//!
//! The provider is the one thing these tests replace: `install_provider` puts a stub in the
//! process, so a connect exercises the schema, the scopes, the sealed store and the response
//! documents **without a network call to Google**. Everything else is the production path: the
//! real router, the real invocation engine, real Postgres 18 and the store's own `Sealer`.
//!
//! The five scenarios are the brief's, and the last one is the cross-cutting rule: every response
//! this module can produce is grepped for the secret it was given, and its JSON is walked for a
//! field that could carry a token.

mod support;

use std::sync::{Mutex, Once, OnceLock};

use chrono::{DateTime, Utc};
use conveyor_actions::youtube::{
    ProviderFuture, TokenSet, UploadRequest, YouTubeError, YouTubeProvider, install_provider,
};
use serde_json::{Value, json};

/// The stub's access token. The API must never echo it.
const STUB_ACCESS: &str = "stub-access-token-must-never-escape";
/// The stub's refresh token. The API must never echo it, and the store must not contain it in the
/// clear.
const STUB_REFRESH: &str = "stub-refresh-token-must-never-escape";
/// The stub's expiry: fixed, so the response body is deterministic.
const STUB_EXPIRES: &str = "2027-01-01T00:00:00Z";
/// The client secret the tests configure. The API must never echo it.
const CLIENT_SECRET_VALUE: &str = "s3cr3t-client-value-must-never-escape";
/// The redirect URI a real tenant would register.
const REDIRECT_URI: &str = "https://conveyor.example/oauth/youtube";
/// The id the stub's upload answers with.
const STUB_VIDEO: &str = "stub-video1";
/// The id the privacy/retract tests send.
const VIDEO_ID: &str = "dQw4w9WgXcQ";

fn stub_expiry() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(STUB_EXPIRES)
        .expect("a fixed timestamp")
        .with_timezone(&Utc)
}

/// One Data API call the stub saw.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Recorded {
    Upload { title: String, privacy: String },
    Privacy { video_id: String, privacy: String },
    Retract { video_id: String },
}

fn calls() -> &'static Mutex<Vec<Recorded>> {
    static CALLS: OnceLock<Mutex<Vec<Recorded>>> = OnceLock::new();
    CALLS.get_or_init(|| Mutex::new(Vec::new()))
}

fn record(call: Recorded) {
    calls().lock().expect("the stub's call log").push(call);
}

/// Take every call recorded so far (so one test cannot read another's).
fn take_calls() -> Vec<Recorded> {
    std::mem::take(&mut *calls().lock().expect("the stub's call log"))
}

/// The provider every test in this binary uses. `youtube.connect` would otherwise call Google;
/// the branch named `invalid-grant` proves the refusal path without one.
struct StubProvider;

impl YouTubeProvider for StubProvider {
    fn exchange_code<'a>(
        &'a self,
        _client_id: &'a str,
        _client_secret: &'a str,
        code: &'a str,
        _redirect_uri: &'a str,
    ) -> ProviderFuture<'a, TokenSet> {
        let refusal = code == "invalid-grant";
        Box::pin(async move {
            if refusal {
                return Err(YouTubeError::Refused { status: 400 });
            }
            Ok(TokenSet {
                access_token: STUB_ACCESS.into(),
                refresh_token: Some(STUB_REFRESH.into()),
                expires_at: stub_expiry(),
            })
        })
    }

    fn refresh<'a>(
        &'a self,
        _client_id: &'a str,
        _client_secret: &'a str,
        _refresh_token: &'a str,
    ) -> ProviderFuture<'a, TokenSet> {
        Box::pin(async move {
            Ok(TokenSet {
                access_token: STUB_ACCESS.into(),
                refresh_token: None,
                expires_at: stub_expiry(),
            })
        })
    }

    fn upload<'a>(
        &'a self,
        _access_token: &'a str,
        request: UploadRequest,
    ) -> ProviderFuture<'a, String> {
        Box::pin(async move {
            record(Recorded::Upload {
                title: request.title,
                privacy: request.privacy,
            });
            Ok(STUB_VIDEO.into())
        })
    }

    fn set_privacy<'a>(
        &'a self,
        _access_token: &'a str,
        video_id: &'a str,
        privacy: &'a str,
    ) -> ProviderFuture<'a, ()> {
        let call = Recorded::Privacy {
            video_id: video_id.to_owned(),
            privacy: privacy.to_owned(),
        };
        Box::pin(async move {
            record(call);
            Ok(())
        })
    }

    fn retract<'a>(&'a self, _access_token: &'a str, video_id: &'a str) -> ProviderFuture<'a, ()> {
        let call = Recorded::Retract {
            video_id: video_id.to_owned(),
        };
        Box::pin(async move {
            record(call);
            Ok(())
        })
    }
}

fn install_stub() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        assert!(
            install_provider(Box::new(StubProvider)),
            "the stub provider is installed once per test binary"
        );
    });
}

fn output(body: &Value) -> &Value {
    body.get("output").expect("a success body carries output")
}

/// Every string a response body carries must be free of the values the tenant stored.
fn assert_no_secret(where_: &str, body: &Value) {
    let text = body.to_string();
    for secret in [STUB_ACCESS, STUB_REFRESH, CLIENT_SECRET_VALUE] {
        assert!(
            !text.contains(secret),
            "{where_} leaked a sealed value: {text}"
        );
    }
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(map) => {
                for (key, value) in map {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "value"
                                | "ciphertext"
                                | "plaintext"
                                | "secret"
                                | "client_secret"
                                | "access_token"
                                | "refresh_token"
                                | "token"
                        ),
                        "{where_} has a {key:?} field: {text}"
                    );
                    stack.push(value);
                }
            }
            Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
}

/// Configure a client for `login` and return the response.
async fn configure_client(running: &support::Running, login: &str) -> Value {
    let (status, body) = running
        .invoke(
            "youtube.client_set",
            login,
            json!({
                "client_id": "parity-client.apps.googleusercontent.com",
                "client_secret": CLIENT_SECRET_VALUE,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body
}

/// Connect a configured tenant and return the response.
async fn connect(running: &support::Running, login: &str, code: &str) -> (u16, Value) {
    running
        .invoke(
            "youtube.connect",
            login,
            json!({"code": code, "redirect_uri": REDIRECT_URI}),
            None,
            None,
            None,
        )
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connect_stores_the_refresh_token_for_one_tenant_only() {
    install_stub();
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    configure_client(&running, "dev:alice").await;
    let (status, body) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["connected"], true);
    assert_eq!(output(&body)["expires_at"], STUB_EXPIRES);

    // The row is sealed: the stored ciphertext is neither the plaintext nor a plaintext copy.
    let ciphertext: Vec<u8> =
        sqlx::query_scalar("SELECT ciphertext FROM secret WHERE tenant_id = $1 AND name = $2")
            .bind(alice.tenant_id)
            .bind(conveyor_actions::youtube::REFRESH_SECRET)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the sealed grant row");
    assert!(
        !String::from_utf8_lossy(&ciphertext).contains(STUB_REFRESH),
        "the refresh token was stored in the clear"
    );

    // Opened as the tenant, the envelope is what the action wrote.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope dev-a");
    let raw = conveyor_store::secret::open_named_tx(
        &mut tx,
        alice.tenant,
        conveyor_actions::youtube::REFRESH_SECRET,
    )
    .await
    .expect("the seal opens for its own tenant");
    tx.commit().await.expect("commit");
    let stored: Value = serde_json::from_slice(&raw).expect("the stored envelope");
    assert_eq!(stored["refresh_token"], STUB_REFRESH);
    assert_eq!(stored["expires_at"], STUB_EXPIRES);

    // The second tenant sees nothing — not the grant and not the client.
    let (status, body) = running
        .invoke("youtube.status", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["connected"], false);
    assert_eq!(output(&body)["client_configured"], false);
    assert_no_secret("youtube.status for dev-b", &body);

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM secret WHERE tenant_id = $1")
        .bind(bob.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count dev-b secrets");
    assert_eq!(count, 0, "dev-b must hold no secret row");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn status_never_returns_a_token() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    configure_client(&running, "dev:alice").await;
    let (status, body) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke("youtube.status", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body),
        &json!({
            "connected": true,
            "expires_at": STUB_EXPIRES,
            "client_configured": true,
        })
    );
    assert_no_secret("youtube.status", &body);

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connect_without_a_client_is_refused_by_name() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input");
    let errors = body["errors"].as_array().expect("errors[] is populated");
    assert_eq!(errors.len(), 1, "{body}");
    assert_eq!(errors[0]["field"], "client_id", "{body}");
    assert_eq!(errors[0]["code"], "not_configured", "{body}");
    assert!(
        errors[0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("youtube.client_set")),
        "the refusal names the action that fixes it: {body}"
    );
    assert_no_secret("youtube.connect refusal", &body);

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn disconnect_is_idempotent() {
    install_stub();
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    configure_client(&running, "dev:alice").await;
    let (status, body) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 200, "{body}");

    for attempt in 1..=2 {
        let (status, body) = running
            .invoke(
                "youtube.disconnect",
                "dev:alice",
                json!({}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "disconnect {attempt}: {body}");
        assert_eq!(output(&body)["connected"], false, "disconnect {attempt}");
        assert_no_secret("youtube.disconnect", &body);
    }

    let (status, body) = running
        .invoke("youtube.status", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["connected"], false);
    assert_eq!(
        output(&body)["client_configured"],
        true,
        "disconnecting drops the grant, not the client"
    );

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM secret WHERE tenant_id = $1 AND name = $2")
            .bind(alice.tenant_id)
            .bind(conveyor_actions::youtube::REFRESH_SECRET)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count the tenant's grant rows");
    assert_eq!(count, 0, "the sealed grant must be gone");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn client_get_never_returns_the_secret() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, before) = running
        .invoke(
            "youtube.client_get",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(
        output(&before),
        &json!({"client_id": null, "secret_set": false})
    );

    let set = configure_client(&running, "dev:alice").await;
    assert_eq!(
        output(&set),
        &json!({
            "client_id": "parity-client.apps.googleusercontent.com",
            "secret_set": true,
        }),
        "client_set answers the id, never the secret"
    );
    assert_no_secret("youtube.client_set", &set);

    let (status, body) = running
        .invoke(
            "youtube.client_get",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body),
        &json!({
            "client_id": "parity-client.apps.googleusercontent.com",
            "secret_set": true,
        })
    );
    assert_no_secret("youtube.client_get", &body);

    running.shutdown().await;
}

// ── #182 slice B: publishing ─────────────────────────────────────────────────────────────

/// The document the enqueue tests name. A random id would change the dedupe key between rounds.
const DOCUMENT_ID: &str = "018f3f7c-0000-7000-8000-000000000001";

/// The upload input the tests vary one field of.
fn upload_input(privacy: &str) -> Value {
    json!({
        "document_id": DOCUMENT_ID,
        "title": "  A parity video  ",
        "description": "what it shows",
        "privacy": privacy,
        "tags": ["parity", "video"],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_enqueues_one_job_carrying_the_validated_parameters() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "youtube.upload",
            "dev:alice",
            upload_input("unlisted"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let job_id = output(&body)["job_id"]
        .as_str()
        .expect("the upload answers a job id")
        .to_owned();

    let (kind, payload): (String, Value) =
        sqlx::query_as("SELECT kind, payload FROM job WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&job_id).expect("a uuid"))
            .fetch_one(&pg.super_pool)
            .await
            .expect("the enqueued job");
    assert_eq!(kind, "youtube.upload");
    assert_eq!(payload["params"]["title"], "A parity video");
    assert_eq!(payload["params"]["privacy"], "unlisted");
    assert_eq!(payload["params"]["description"], "what it shows");
    assert_eq!(payload["params"]["tags"], json!(["parity", "video"]));
    assert_eq!(payload["params"]["document_id"], DOCUMENT_ID);

    // A second identical call while the first job is live joins it instead of racing a second
    // worker: the response is the same row.
    let (status, second) = running
        .invoke(
            "youtube.upload",
            "dev:alice",
            upload_input("unlisted"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(output(&second)["job_id"], job_id);

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_privacy_is_refused_by_name_and_enqueues_nothing() {
    install_stub();
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "youtube.upload",
            "dev:alice",
            upload_input("world"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input");
    let errors = body["errors"].as_array().expect("errors[] is populated");
    assert_eq!(errors[0]["field"], "privacy", "{body}");
    assert!(
        errors[0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("public, unlisted, private")),
        "the refusal names the three values: {body}"
    );

    // YouTube's title cap is the other bound the action validates.
    let mut overlong = upload_input("private");
    overlong["title"] = json!("x".repeat(101));
    let (status, body) = running
        .invoke("youtube.upload", "dev:alice", overlong, None, None, None)
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "title", "{body}");

    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM job WHERE tenant_id = $1 AND kind = 'youtube.upload'")
            .bind(alice.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count the tenant's upload jobs");
    assert_eq!(count, 0, "a refused upload enqueues nothing");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn privacy_and_retract_are_idempotent_through_the_stored_connection() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    configure_client(&running, "dev:alice").await;
    let (status, body) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 200, "{body}");
    take_calls();

    for attempt in 1..=2 {
        let (status, body) = running
            .invoke(
                "youtube.privacy",
                "dev:alice",
                json!({"video_id": VIDEO_ID, "privacy": "public"}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "privacy {attempt}: {body}");
        assert_eq!(
            output(&body),
            &json!({"video_id": VIDEO_ID, "privacy": "public"})
        );
        assert_no_secret("youtube.privacy", &body);
    }

    for attempt in 1..=2 {
        let (status, body) = running
            .invoke(
                "youtube.retract",
                "dev:alice",
                json!({"video_id": VIDEO_ID}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "retract {attempt}: {body}");
        assert_eq!(output(&body), &json!({"retracted": true}));
        assert_no_secret("youtube.retract", &body);
    }

    // Re-sending is a second provider call that succeeds: the actions are idempotent because the
    // operations are (an update to the same value, a delete of an absent video), not because a
    // response was cached.
    assert_eq!(
        take_calls(),
        vec![
            Recorded::Privacy {
                video_id: VIDEO_ID.into(),
                privacy: "public".into(),
            },
            Recorded::Privacy {
                video_id: VIDEO_ID.into(),
                privacy: "public".into(),
            },
            Recorded::Retract {
                video_id: VIDEO_ID.into(),
            },
            Recorded::Retract {
                video_id: VIDEO_ID.into(),
            },
        ]
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn publishing_without_a_connection_is_refused_by_name() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "youtube.retract",
            "dev:alice",
            json!({"video_id": VIDEO_ID}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "youtube_not_configured", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("youtube.connect")),
        "the refusal names the action that fixes it: {body}"
    );

    running.shutdown().await;
}

/// The cross-cutting rule: every response this module can produce is grepped for the values the
/// tenant stored, including the provider-refusal problem document.
#[tokio::test(flavor = "multi_thread")]
async fn no_response_ever_carries_a_secret() {
    install_stub();
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let set = configure_client(&running, "dev:alice").await;
    assert_no_secret("youtube.client_set", &set);

    let (status, connected) = connect(&running, "dev:alice", "a-consent-code").await;
    assert_eq!(status, 200, "{connected}");
    assert_no_secret("youtube.connect", &connected);

    // The refusal branch of the same action: the provider's error body is not echoed either.
    let (status, refused) = connect(&running, "dev:alice", "invalid-grant").await;
    assert_eq!(status, 502, "{refused}");
    assert_eq!(refused["type"], "youtube-provider-failed");
    assert_no_secret("youtube.connect refusal", &refused);

    for action in ["youtube.client_get", "youtube.status"] {
        let (status, body) = running
            .invoke(action, "dev:alice", json!({}), None, None, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
        assert_no_secret(action, &body);
    }

    let (status, disconnected) = running
        .invoke(
            "youtube.disconnect",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{disconnected}");
    assert_no_secret("youtube.disconnect", &disconnected);

    // And the sealed row itself never contains the plaintext.
    let ciphertext: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM secret WHERE name = $1")
        .bind(conveyor_actions::youtube::CLIENT_SECRET)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the sealed client row");
    assert!(
        !String::from_utf8_lossy(&ciphertext).contains(CLIENT_SECRET_VALUE),
        "the client secret was stored in the clear"
    );

    running.shutdown().await;
}
