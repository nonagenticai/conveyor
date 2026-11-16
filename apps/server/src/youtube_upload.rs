#![forbid(unsafe_code)]
//! `youtube.upload` — one document to the tenant's own YouTube channel (#182 slice B).
//!
//! `youtube.upload` (the action, `crates/actions/src/youtube.rs`) validates the metadata and
//! enqueues this job; the job does the two things an action must not:
//!
//! 1. read the document's bytes from the tenant's object store, verifying the stored hash; and
//! 2. call Google — refresh the access token, then `videos.insert` — through the same
//!    `YouTubeProvider` the OAuth actions use. A test installs a stub with `install_provider`;
//!    production falls back to the Google transport.
//!
//! The result — the provider's `video_id` — is written to the job's own `payload.result`.
//!
//! # Idempotency, stated honestly
//!
//! **YouTube has no idempotency key.** What this job guarantees is narrower: before uploading it
//! reads the job row's recorded `video_id`, and if this job already has one it does not upload
//! again — it finishes with that id. A retried attempt of the SAME job therefore cannot
//! double-upload **once the id has been recorded**.
//!
//! Two things that guarantee does not cover, deliberately not hidden:
//!
//! - two **different** jobs for the same document can each upload, because each has its own empty
//!   result; enqueue-time coalescing only joins jobs that are simultaneously live;
//! - a crash between YouTube's acceptance and the `record_result` write leaves no id to find, and
//!   the retry uploads a second copy. Recording the id after the fact cannot close that window;
//!   only an upstream idempotency key could, and Google does not offer one.
//!
//! Every other failure is retryable or dead-lettered by the queue's own rules: a missing document,
//! a hash mismatch and a refused grant are permanent, a provider outage or a database error is not.

use conveyor_actions::integrations::objects::Objects;
use conveyor_actions::spec::sha256_hex;
use conveyor_actions::youtube::{
    AccessTokenError, UploadRequest, YouTubeError, access_token, installed_provider,
};
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// The job kind (one spelling, shared with the action).
pub const KIND: &str = conveyor_actions::youtube::UPLOAD_KIND;

/// What this job needs that the action validated: the parameters only.
struct Params {
    document_id: Uuid,
    title: String,
    description: Option<String>,
    privacy: String,
    tags: Vec<String>,
}

impl Params {
    /// Read `payload.params`; anything malformed is a permanent failure (the action wrote this
    /// shape, so a mismatch is a hand-crafted job or a schema change, and neither retries away).
    fn from_payload(payload: &Value) -> Result<Self, JobFailure> {
        let params = payload
            .get("params")
            .ok_or_else(|| JobFailure::permanent("youtube.upload payload has no `params`"))?;
        let document_id = params
            .get("document_id")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .ok_or_else(|| {
                JobFailure::permanent("youtube.upload payload has no usable `document_id`")
            })?;
        let title = params
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| JobFailure::permanent("youtube.upload payload has no `title`"))?
            .to_owned();
        let privacy = params
            .get("privacy")
            .and_then(Value::as_str)
            .ok_or_else(|| JobFailure::permanent("youtube.upload payload has no `privacy`"))?
            .to_owned();
        let description = match params.get("description") {
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| {
                        JobFailure::permanent("youtube.upload `description` is not a string")
                    })?
                    .to_owned(),
            ),
            None => None,
        };
        let tags = match params.get("tags") {
            Some(value) => value
                .as_array()
                .ok_or_else(|| JobFailure::permanent("youtube.upload `tags` is not an array"))?
                .iter()
                .map(|tag| {
                    tag.as_str().map(str::to_owned).ok_or_else(|| {
                        JobFailure::permanent("youtube.upload `tags` holds a non-string")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        Ok(Self {
            document_id,
            title,
            description,
            privacy,
            tags,
        })
    }
}

/// Run one upload. `job_id` names the row this attempt is running; the recorded video id lives on
/// it and is the retry's evidence.
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    payload: &Value,
) -> Result<(), JobFailure> {
    // The whole idempotency guarantee, in one read: this job already produced a video, so the
    // retried attempt finishes with it instead of uploading a second copy.
    if let Some(video_id) = payload
        .get("result")
        .and_then(|result| result.get("video_id"))
        .and_then(Value::as_str)
        && !video_id.is_empty()
    {
        tracing::info!(
            job = %job_id,
            %video_id,
            "youtube.upload: this job already recorded its video; nothing to upload"
        );
        return Ok(());
    }

    let params = Params::from_payload(payload)?;
    let provider = installed_provider();

    // Everything the upload needs is read inside one scoped transaction, which is committed
    // before the network call: a slow upload must not sit on a database connection.
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    let token = match access_token(&mut tx, provider, tenant).await {
        Ok(Some(token)) => token,
        Ok(None) => {
            return Err(JobFailure::permanent(
                "no YouTube connection is stored for this tenant; call youtube.client_set and youtube.connect first",
            ));
        }
        Err(error) => return Err(map_access_error(error)),
    };

    let record = store::document::get_document_tx(&mut tx, tenant, params.document_id)
        .await
        .map_err(|error| match error {
            store::document::DocumentStoreError::Database(error) => {
                JobFailure::retryable(format!("database: {error}"))
            }
            other => JobFailure::permanent(format!("youtube.upload: {other}")),
        })?
        .ok_or_else(|| {
            JobFailure::permanent(format!("no document {} in this tenant", params.document_id))
        })?;
    let (object_key, expected_hash) =
        match (record.object_key.as_deref(), record.content_hash.as_deref()) {
            (Some(key), Some(hash)) if record.status == "active" => {
                (key.to_owned(), hash.to_owned())
            }
            _ => {
                return Err(JobFailure::permanent(format!(
                    "document {} has no active bytes to upload",
                    params.document_id
                )));
            }
        };
    let content_type = record
        .meta
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_owned();
    let objects = Objects::from_env().ok_or_else(|| {
        JobFailure::permanent("object storage is not configured; the upload cannot read its bytes")
    })?;
    let bytes = objects
        .get(&object_key)
        .await
        .map_err(|error| JobFailure::retryable(format!("object storage: {error}")))?
        .to_vec();
    let actual_hash = sha256_hex(&bytes);
    if actual_hash != expected_hash {
        tracing::error!(
            document = %params.document_id,
            expected = %expected_hash,
            actual = %actual_hash,
            "the stored bytes do not match the document's hash"
        );
        return Err(JobFailure::permanent(
            "the stored bytes do not match the document's hash",
        ));
    }
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    let request = UploadRequest {
        title: params.title,
        description: params.description,
        privacy: params.privacy,
        tags: params.tags,
        content_type,
        bytes,
    };
    let video_id = provider
        .upload(&token, request)
        .await
        .map_err(map_provider_error)?;

    // The retry's witness. If this write fails the attempt fails; the retry re-uploads, which is
    // the documented crash window, not a silent second copy on top of a recorded id.
    store::jobs::record_result(pool, tenant, job_id, json!({ "video_id": video_id }))
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    tracing::info!(
        job = %job_id,
        document = %params.document_id,
        %video_id,
        "youtube.upload: uploaded"
    );
    Ok(())
}

/// A refused refresh: a 5xx or 429 is worth another attempt; any other refusal means the grant is
/// unusable and the tenant must reconnect, which no retry can fix.
fn map_access_error(error: AccessTokenError) -> JobFailure {
    match error {
        AccessTokenError::Connection(error) => {
            tracing::error!(%error, "youtube.upload: the stored connection is unusable");
            JobFailure::retryable(format!("youtube secret: {error}"))
        }
        AccessTokenError::Provider(error) => map_provider_error(error),
    }
}

/// One place that decides whether a provider failure is worth a retry.
fn map_provider_error(error: YouTubeError) -> JobFailure {
    match error {
        YouTubeError::Unavailable => JobFailure::retryable("the YouTube API could not be reached"),
        YouTubeError::InvalidResponse => {
            JobFailure::retryable("the YouTube API answered a body this client could not read")
        }
        YouTubeError::Refused { status } if status >= 500 || status == 429 => {
            JobFailure::retryable(format!(
                "the YouTube API refused the request with status {status}"
            ))
        }
        YouTubeError::Refused { status } => JobFailure::permanent(format!(
            "the YouTube API refused the request with status {status}; the tenant may need to reconnect with youtube.connect"
        )),
    }
}
