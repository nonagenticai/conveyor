//! #173 — `produce.speech` and `media.transcribe` over the estate's model hosts.
//!
//! Four properties, one per acceptance criterion on the issue: a synthesis stores one WAV document
//! and one ledger row; an unconfigured endpoint refuses before any call; a transcription uploads the
//! document's bytes as one multipart part and stores the text as a second document; and an unknown
//! document is refused before anything leaves the process.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Bytes;
use axum::http::HeaderMap;
use conveyor_actions::integrations::objects::Objects;
use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::{Value, json};
use uuid::Uuid;

/// A 44-byte WAV header with no samples. The object store's content-type check only reads the
/// `RIFF`/`WAVE` magic, but a header-shaped fixture is what the route actually answers.
fn wav() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&36u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&24_000u32.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

/// The speech host's stub: answers the JSON envelope the narration route produces when the request
/// asks for JSON, records how often it was called, and records the `Accept` it was sent.
async fn stub_speech_host() -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<String>>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let accept = Arc::new(std::sync::Mutex::new(String::new()));
    let counter = hits.clone();
    let seen = accept.clone();
    let app = axum::Router::new().route(
        "/api/tts/synthesize",
        axum::routing::post(
            move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
                let counter = counter.clone();
                let seen = seen.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    *seen.lock().expect("the accept lock") = headers
                        .get(axum::http::header::ACCEPT)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_owned();
                    axum::Json(json!({
                        "audio_base64": base64(&wav()),
                        "sample_rate": 24_000,
                        "duration_sec": 0.0,
                        "provider": "kokoro",
                        "voice": body.get("voice").and_then(Value::as_str).unwrap_or("af_heart"),
                        "format": "wav",
                        "speech_seconds": 0.0,
                        "pause_seconds": 0.0,
                    }))
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the speech stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, hits, accept)
}

/// The transcription host's stub: returns OpenAI's `{"text": …}` body and keeps the uploaded bytes.
async fn stub_transcription_host() -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<u8>>>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let body = Arc::new(std::sync::Mutex::new(Vec::new()));
    let counter = hits.clone();
    let seen = body.clone();
    let app = axum::Router::new().route(
        "/v1/audio/transcriptions",
        axum::routing::post(move |bytes: Bytes| {
            let counter = counter.clone();
            let seen = seen.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                *seen.lock().expect("the body lock") = bytes.to_vec();
                axum::Json(json!({"text": "hello from the transcript"}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the transcription stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, hits, body)
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn request(action: &str, user_id: Uuid, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-audio".to_owned()),
        trace_id: Some("trace-audio".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: Some(UsageContext {
            caller_kind: CallerKind::AgentRun,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("agent:audio".to_owned()),
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: Some("corr-audio".to_owned()),
        }),
    }
}

/// P1.1 — one synthesis produces one object, one document row and one `inference_call`, and the
/// request asked the narration route for its JSON envelope.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn speech_writes_one_object() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits, accept) = stub_speech_host().await;
    support::set_env("CONVEYOR_EXTERNAL_TTS_URL", &base);

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "produce.speech",
            seeded.user_id,
            json!({"text": "a short line", "voice": "af_heart", "speed": 1.0}),
        ),
    )
    .await
    .expect("the synthesis");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "exactly one synthesis");
    assert_eq!(
        *accept.lock().expect("the accept lock"),
        "application/json",
        "the envelope is only returned when JSON is asked for"
    );

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        title: String,
        status: String,
        content_hash: Option<String>,
        meta: Value,
    }
    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, title, status, content_hash, meta FROM document ORDER BY id")
            .fetch_all(&pg.super_pool)
            .await
            .expect("the document rows");
    assert_eq!(rows.len(), 1, "one synthesis, one document");
    let row = &rows[0];
    assert_eq!(row.title, "Narration");
    assert_eq!(row.status, "active");
    assert_eq!(row.meta["content_type"], "audio/wav");
    assert_eq!(
        outcome.body["output"]["document"]["id"].as_str(),
        Some(row.id.to_string().as_str()),
        "the caller is given the document, not the base64"
    );

    let objects = Objects::from_env().expect("the test object backend");
    let stored = objects
        .get(&conveyor_actions::integrations::objects::key_for(
            seeded.tenant_id,
            row.id,
            row.content_hash.as_deref().expect("a hash"),
        ))
        .await
        .expect("the stored bytes");
    assert!(stored.starts_with(b"RIFF"), "the WAV the host returned");

    let call: (String, Option<String>, Option<f64>) =
        sqlx::query_as("SELECT model, usage_unit, usage_units::float8 FROM inference_call")
            .fetch_one(&pg.super_pool)
            .await
            .expect("the ledger");
    assert_eq!(call.0, "tts", "the surface is the pricing identity");
    assert_eq!(call.1.as_deref(), Some("character"));
    assert_eq!(
        call.2,
        Some("a short line".chars().count() as f64),
        "the commercial TTS unit is the input text"
    );
}

/// P1.2 — no configured endpoint is a visible refusal, before any call.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn speech_without_an_endpoint_refuses() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits, _) = stub_speech_host().await;
    // A reachable stub, but the action is pointed nowhere: the engine must refuse rather than
    // guess the host the stub is running on.
    let _ = base;
    support::set_env("CONVEYOR_EXTERNAL_TTS_URL", "");

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("produce.speech", seeded.user_id, json!({"text": "hi"})),
    )
    .await
    .expect_err("no endpoint must refuse");

    assert_eq!(error.status, 503, "{}", error.detail);
    assert_eq!(hits.load(Ordering::SeqCst), 0, "refused before the call");
}

/// P2.1 — the document is uploaded as one multipart part and its text becomes a second document.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transcribe_stores_a_transcript() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, created) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "narration take one",
                "content_type": "audio/wav",
                "content_base64": base64(&wav()),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let document = created["output"]["id"]
        .as_str()
        .expect("the document id")
        .to_owned();

    let (base, hits, uploaded) = stub_transcription_host().await;
    support::set_env("CONVEYOR_EXTERNAL_ASR_URL", &base);

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "media.transcribe",
            seeded.user_id,
            json!({"document": document}),
        ),
    )
    .await
    .expect("the transcription");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "exactly one transcription");
    let body = uploaded.lock().expect("the body lock").clone();
    assert!(
        body.windows(4).any(|window| window == b"RIFF"),
        "the uploaded part carries the document's bytes"
    );
    let disposition = b"name=\"file\"";
    assert!(
        body.windows(disposition.len())
            .any(|window| window == disposition),
        "the part is named as the service expects"
    );

    let rows: Vec<(String, Value)> =
        sqlx::query_as("SELECT title, meta FROM document WHERE title = 'Transcript'")
            .fetch_all(&pg.super_pool)
            .await
            .expect("the transcript row");
    assert_eq!(rows.len(), 1, "one transcription, one transcript document");
    assert_eq!(rows[0].1["content_type"], "text/plain");

    let transcript = outcome.body["output"]["document"]["id"]
        .as_str()
        .expect("the stored transcript id");
    let objects = Objects::from_env().expect("the test object backend");
    let key: (String,) = sqlx::query_as("SELECT object_key FROM document WHERE id = $1::uuid")
        .bind(transcript)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the transcript's key");
    let bytes = objects.get(&key.0).await.expect("the stored transcript");
    assert_eq!(
        String::from_utf8(bytes.to_vec()).expect("the transcript is text"),
        "hello from the transcript"
    );

    let call: (String, Option<String>, Option<f64>) =
        sqlx::query_as("SELECT model, usage_unit, usage_units::float8 FROM inference_call")
            .fetch_one(&pg.super_pool)
            .await
            .expect("the ledger");
    assert_eq!(call.0, "asr", "the surface is the pricing identity");
    assert_eq!(call.1.as_deref(), Some("second"));
    assert_eq!(
        call.2,
        Some(0.0),
        "the 44-byte fixture has an empty data chunk, so its WAV duration is zero"
    );
}

/// P2.2 — an unknown document is refused before the service is called.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_document_refuses() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let (base, hits, _) = stub_transcription_host().await;
    support::set_env("CONVEYOR_EXTERNAL_ASR_URL", &base);

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "media.transcribe",
            seeded.user_id,
            json!({"document": Uuid::now_v7().to_string()}),
        ),
    )
    .await
    .expect_err("an unknown document must refuse");

    assert_eq!(error.status, 404, "{}", error.detail);
    assert_eq!(hits.load(Ordering::SeqCst), 0, "refused before the call");
}
