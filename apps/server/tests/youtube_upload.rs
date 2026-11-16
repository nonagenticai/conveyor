//! `youtube.upload` — the worker half of #182 slice B.
//!
//! The action's enqueue is asserted in `crates/actions/tests/youtube.rs`; this binary runs the job
//! itself, through `worker::run_job` (the same entry point the tick uses), with the Google
//! transport replaced by a stub provider that records what it was asked to upload.
//!
//! Two properties are the point:
//!
//! 1. one run uploads the document's bytes and records the provider's `video_id` on the job row;
//! 2. a retried attempt of the SAME job reads that id and does **not** call the provider again.
//!
//! The object backend is `object_store`'s in-memory one (`CONVEYOR_S3_BACKEND=memory`), as in
//! `documents_purge.rs`: the same trait and key scheme, without a MinIO image.

mod common;

use std::sync::{Mutex, Once, OnceLock};

use chrono::{DateTime, Utc};
use conveyor_actions::integrations::objects::{Objects, key_for};
use conveyor_actions::youtube::{
    CLIENT_SECRET, ProviderFuture, REFRESH_SECRET, TokenSet, UploadRequest, YouTubeError,
    YouTubeProvider, install_provider,
};
use conveyor_domain::TenantId;
use conveyor_server::worker;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The stub's access token; it must never reach the job row or a log the test asserts on.
const STUB_ACCESS: &str = "stub-access-token-for-the-worker";
/// The id the stub's upload answers with — the value the job must record.
const STUB_VIDEO: &str = "stub-video1";
/// The document's bytes.
const VIDEO_BYTES: &[u8] = b"pretend this is an mp4";

/// One upload the stub saw.
#[derive(Clone, Debug)]
struct Uploaded {
    title: String,
    privacy: String,
    tags: Vec<String>,
    content_type: String,
    bytes: Vec<u8>,
}

fn calls() -> &'static Mutex<Vec<Uploaded>> {
    static CALLS: OnceLock<Mutex<Vec<Uploaded>>> = OnceLock::new();
    CALLS.get_or_init(|| Mutex::new(Vec::new()))
}

fn take_calls() -> Vec<Uploaded> {
    std::mem::take(&mut *calls().lock().expect("the stub's call log"))
}

struct StubProvider;

impl YouTubeProvider for StubProvider {
    fn exchange_code<'a>(
        &'a self,
        _client_id: &'a str,
        _client_secret: &'a str,
        _code: &'a str,
        _redirect_uri: &'a str,
    ) -> ProviderFuture<'a, TokenSet> {
        Box::pin(async move { Err(YouTubeError::Unavailable) })
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
                expires_at: fixed_expiry(),
            })
        })
    }

    fn upload<'a>(
        &'a self,
        _access_token: &'a str,
        request: UploadRequest,
    ) -> ProviderFuture<'a, String> {
        Box::pin(async move {
            calls().lock().expect("the stub's call log").push(Uploaded {
                title: request.title,
                privacy: request.privacy,
                tags: request.tags,
                content_type: request.content_type,
                bytes: request.bytes,
            });
            Ok(STUB_VIDEO.into())
        })
    }

    fn set_privacy<'a>(
        &'a self,
        _access_token: &'a str,
        _video_id: &'a str,
        _privacy: &'a str,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move { Ok(()) })
    }

    fn retract<'a>(&'a self, _access_token: &'a str, _video_id: &'a str) -> ProviderFuture<'a, ()> {
        Box::pin(async move { Ok(()) })
    }
}

fn fixed_expiry() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
        .expect("a fixed timestamp")
        .with_timezone(&Utc)
}

/// Install the one provider this test binary uses.
fn install_stub() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        assert!(
            install_provider(Box::new(StubProvider)),
            "the stub provider is installed once per test binary"
        );
    });
}

/// The worker's config: the same defaults `documents_purge.rs` uses.
fn local_config() -> conveyor_api::Config {
    conveyor_api::Config::from_lookup(|_| None).expect("a default config")
}

/// Configure the two services this job reads. Edition 2024 made `set_var` unsafe; this runs before
/// the runtime starts and before the first call that reads them.
fn init_env() {
    unsafe {
        std::env::set_var(
            "CONVEYOR_SECRET_KEK",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        );
        if std::env::var("CONVEYOR_S3_ENDPOINT").is_err() {
            std::env::set_var("CONVEYOR_S3_BACKEND", "memory");
        }
    }
}

async fn seed_tenant(db: &common::TestDb) -> TenantId {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('upload', 'Upload') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed the tenant");
    TenantId::from_uuid(tenant_id).expect("non-nil tenant")
}

/// A live document whose bytes are a video, stored through the real object path.
async fn seed_document(db: &common::TestDb, tenant: TenantId) -> Uuid {
    let id = Uuid::now_v7();
    let hash: String = Sha256::digest(VIDEO_BYTES)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let key = key_for(tenant.as_uuid(), id, &hash);
    Objects::from_env()
        .expect("the test object backend")
        .put(&key, VIDEO_BYTES.to_vec().into())
        .await
        .expect("store the video object");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, kind, title, status, object_key, content_hash, meta)
         VALUES ($1, $2, 'document', 'A video', 'active', $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant.as_uuid())
    .bind(&key)
    .bind(&hash)
    .bind(json!({"content_type": "video/mp4", "size_bytes": VIDEO_BYTES.len() as i64}))
    .execute(&db.super_pool)
    .await
    .expect("seed the document");
    id
}

/// Store the sealed client and grant envelopes the worker opens.
async fn seed_connection(db: &common::TestDb, tenant: TenantId) {
    let client = serde_json::to_vec(&json!({
        "client_id": "parity-client.apps.googleusercontent.com",
        "client_secret": "parity-client-secret",
    }))
    .expect("the client envelope");
    let grant = serde_json::to_vec(&json!({
        "refresh_token": "stub-refresh-token",
        "expires_at": "2027-01-01T00:00:00Z",
    }))
    .expect("the grant envelope");
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope the tenant");
    conveyor_store::secret::create_secret_tx(&mut tx, tenant, CLIENT_SECRET, &client, "tenant")
        .await
        .expect("seal the client");
    conveyor_store::secret::create_secret_tx(&mut tx, tenant, REFRESH_SECRET, &grant, "tenant")
        .await
        .expect("seal the grant");
    tx.commit().await.expect("commit the connection");
}

/// Enqueue the job exactly as `youtube.upload` does, and load the row the worker will run.
async fn enqueue(
    db: &common::TestDb,
    tenant: TenantId,
    document: Uuid,
) -> (Uuid, conveyor_store::Job) {
    let payload = json!({
        "params": {
            "document_id": document,
            "title": "A parity video",
            "description": "what it shows",
            "privacy": "unlisted",
            "tags": ["parity", "video"],
        },
    });
    let job_id = conveyor_store::jobs::enqueue(
        &db.runtime,
        tenant,
        conveyor_actions::youtube::UPLOAD_KIND,
        "youtube-upload-test",
        payload,
        None,
        conveyor_actions::youtube::UPLOAD_MAX_ATTEMPTS,
    )
    .await
    .expect("enqueue the upload job")
    .expect("a fresh job id");
    let job = conveyor_store::jobs::get(&db.runtime, tenant, job_id)
        .await
        .expect("read the job")
        .expect("the job row");
    (job_id, job)
}

#[tokio::test]
async fn an_upload_job_uploads_once_and_records_the_video_id() {
    init_env();
    install_stub();
    let db = common::TestDb::fresh().await;
    let tenant = seed_tenant(&db).await;
    let document = seed_document(&db, tenant).await;
    seed_connection(&db, tenant).await;
    take_calls(); // this binary has one test; nothing else may contribute

    let (job_id, job) = enqueue(&db, tenant, document).await;
    worker::run_job(&db.runtime, &local_config(), None, tenant, &job)
        .await
        .expect("the upload job");

    // One upload, carrying the document's bytes and the action's metadata.
    let uploaded = take_calls();
    assert_eq!(uploaded.len(), 1, "the job must upload exactly once");
    assert_eq!(uploaded[0].title, "A parity video");
    assert_eq!(uploaded[0].privacy, "unlisted");
    assert_eq!(uploaded[0].tags, vec!["parity", "video"]);
    assert_eq!(uploaded[0].content_type, "video/mp4");
    assert_eq!(uploaded[0].bytes, VIDEO_BYTES, "the stored bytes are sent");

    // The provider's id is recorded on the job's own row.
    let payload: Value = sqlx::query_scalar("SELECT payload FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the job payload");
    assert_eq!(payload["result"]["video_id"], STUB_VIDEO, "{payload}");

    // A retried attempt of the SAME job reads the recorded id and finishes without uploading
    // again. The row is re-read from the database, exactly as the next claim would read it.
    let retried = conveyor_store::jobs::get(&db.runtime, tenant, job_id)
        .await
        .expect("read the job")
        .expect("the job row");
    worker::run_job(&db.runtime, &local_config(), None, tenant, &retried)
        .await
        .expect("the retried attempt");
    assert!(
        take_calls().is_empty(),
        "the retry must not call the provider a second time"
    );
}
