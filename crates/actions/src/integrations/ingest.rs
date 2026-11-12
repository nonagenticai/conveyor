#![forbid(unsafe_code)]
//! The `strata_ingest` handoff — `documents.ingest`'s one transport (S09 §5.6).
//!
//! The upstream is `strata-ingest`, whose contract was read from its own source (`~/Movies/strata_ingest`,
//! `src/api/mod.rs` + `src/api/upload.rs`) on 2026-11-08 rather than assumed:
//!
//! * `POST /api/ingest/jobs`, `multipart/form-data`.
//! * **`Authorization: Bearer <token>` and the token *is* the tenant** — the service maps tokens to tenants
//!   (`STRATA_INGEST_API_TOKENS`) and every route except `/health` is behind it. A conveyor tenant therefore
//!   needs its own upstream token, which is why the caller resolves one per tenant and refuses without it.
//! * The `store_id` part **must come before the first `file` part**, and the upstream validates it as a leaf
//!   store of the caller's tenant. The conveyor has no notion of a strata store, so `store_id` travels in from
//!   the action's input and is never invented here.
//! * `202 Accepted` answers `{"jobs":[{"job_id":<uuid>,"status":"queued"}]}`. A top-level `job_id` exists only
//!   for a single JSON `source` body (crawl/sitemap), which this client never sends.
//!
//! ⚠️ This is a **transport**, like `invoke.rs`, `webhooks.rs` and `channels.rs`: `externals_declared` carries
//! it by name. It is not an external action — `documents.ingest` is `kind: internal` because the job id it
//! returns becomes a column on `document`, which no passthrough action could write.

use std::time::Duration;

use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use uuid::Uuid;

/// The default per-call timeout: generous for a 10 MiB multipart push, short enough that a dead upstream
/// cannot hold the caller's transaction for minutes.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// The floor and ceiling for `CONVEYOR_STRATA_INGEST_TIMEOUT_MS`.
pub const MIN_TIMEOUT_MS: u64 = 1_000;
/// See [`MIN_TIMEOUT_MS`].
pub const MAX_TIMEOUT_MS: u64 = 120_000;

/// Why a push did not produce a job id.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// `CONVEYOR_STRATA_INGEST_URL` is absent or malformed: the service is not configured.
    #[error("the ingest service is not configured")]
    Unavailable,
    /// The upstream answered 401 — the tenant's token is wrong or revoked.
    #[error("the ingest service rejected the tenant token")]
    Unauthorized,
    /// The upstream answered 503 (`not_ready`, `busy`): retryable, the caller may try again.
    #[error("the ingest service is not ready")]
    NotReady,
    /// The upstream answered 4xx other than 401 — a malformed request, not a transient fault.
    #[error("the ingest service refused the document: {status} {body}")]
    Rejected { status: u16, body: String },
    /// The upstream answered 2xx but not a shape this client can read.
    #[error("the ingest service answered a body this client cannot read")]
    Protocol,
    /// The call did not complete (connection, timeout, DNS).
    #[error("the ingest service could not be reached: {0}")]
    Transport(String),
}

/// One document to ingest.
#[derive(Debug)]
pub struct IngestRequest {
    /// The strata store the document lands in — a leaf of the tenant, validated upstream.
    pub store_id: Uuid,
    /// The upstream's own source identifier, when the caller keeps one.
    pub source_id: Option<String>,
    /// The run the source belongs to; only legal together with [`Self::source_id`].
    pub source_run: Option<String>,
    /// Free options forwarded verbatim.
    pub options: Option<Value>,
    /// The file name the upstream records.
    pub filename: String,
    /// The declared MIME type, already checked against the bytes by the caller.
    pub content_type: String,
    /// The bytes.
    pub bytes: Bytes,
}

/// What the upstream accepted.
#[derive(Debug, Clone)]
pub struct IngestAccepted {
    /// The job id — stored as `document.external_ref`.
    pub job_id: Uuid,
    /// The upstream's status word (`queued`).
    pub status: String,
}

/// The `strata_ingest` client.
#[derive(Clone, Debug)]
pub struct Ingest {
    base: String,
    timeout: Duration,
}

impl Ingest {
    /// Build from the environment; `None` when `CONVEYOR_STRATA_INGEST_URL` is absent or is not an
    /// `http`/`https` URL. Absent configuration is a refusal, never a guessable default.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Testable core.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |key: &str| get(key).filter(|value| !value.trim().is_empty());
        let base = get("CONVEYOR_STRATA_INGEST_URL")?;
        let base = base.trim().trim_end_matches('/').to_owned();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return None;
        }
        let timeout_ms = get("CONVEYOR_STRATA_INGEST_TIMEOUT_MS")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        Some(Self {
            base,
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    /// The base URL this client posts to (tests assert against it).
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Push one document and return the accepted job.
    ///
    /// The upstream's refusal is classified rather than collapsed: `401` is the tenant's token, `503` is
    /// retryable, and any other `4xx` is a malformed request the caller must not retry blindly.
    pub async fn submit(
        &self,
        token: &str,
        request: IngestRequest,
    ) -> Result<IngestAccepted, IngestError> {
        let mut form = Form::new().text("store_id", request.store_id.to_string());
        if let Some(source_id) = request.source_id.as_deref() {
            form = form.text("source_id", source_id.to_owned());
        }
        if let Some(source_run) = request.source_run.as_deref() {
            form = form.text("source_run", source_run.to_owned());
        }
        if let Some(options) = request.options.as_ref() {
            form = form.text("options", options.to_string());
        }
        // The file part is added LAST: the upstream refuses a `file` that arrives before `store_id`.
        let part = Part::bytes(request.bytes.to_vec())
            .file_name(request.filename)
            .mime_str(&request.content_type)
            .map_err(|_| IngestError::Rejected {
                status: 415,
                body: "the document's content type is not a legal MIME type".to_owned(),
            })?;
        form = form.part("file", part);

        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| IngestError::Transport(error.to_string()))?;
        let response = client
            .post(format!("{}/api/ingest/jobs", self.base))
            .bearer_auth(token)
            .header("accept", "application/json")
            .multipart(form)
            .send()
            .await
            .map_err(|error| IngestError::Transport(error.to_string()))?;

        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        match status {
            202 => parse_accepted(&body),
            401 => Err(IngestError::Unauthorized),
            503 => Err(IngestError::NotReady),
            400..=499 => Err(IngestError::Rejected { status, body }),
            _ => Err(IngestError::Rejected { status, body }),
        }
    }
}

/// Read `jobs[0]` out of a 202 body.
fn parse_accepted(body: &str) -> Result<IngestAccepted, IngestError> {
    let value: Value = serde_json::from_str(body).map_err(|_| IngestError::Protocol)?;
    let first = value
        .get("jobs")
        .and_then(Value::as_array)
        .and_then(|jobs| jobs.first())
        .ok_or(IngestError::Protocol)?;
    let job_id = first
        .get("job_id")
        .and_then(Value::as_str)
        .and_then(|text| Uuid::parse_str(text).ok())
        .ok_or(IngestError::Protocol)?;
    let status = first
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("queued")
        .to_owned();
    Ok(IngestAccepted { job_id, status })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_url_is_not_a_configuration() {
        let client = Ingest::from_lookup(|key| match key {
            "CONVEYOR_STRATA_INGEST_URL" => Some("strata-ingest:9135".to_owned()),
            _ => None,
        });
        assert!(client.is_none(), "a scheme-less URL must not be accepted");
    }

    #[test]
    fn the_timeout_is_clamped() {
        let client = Ingest::from_lookup(|key| match key {
            "CONVEYOR_STRATA_INGEST_URL" => Some("http://127.0.0.1:1".to_owned()),
            "CONVEYOR_STRATA_INGEST_TIMEOUT_MS" => Some("99".to_owned()),
            _ => None,
        })
        .expect("configured");
        assert_eq!(client.timeout, Duration::from_millis(MIN_TIMEOUT_MS));
    }

    #[test]
    fn a_202_without_jobs_is_a_protocol_error() {
        assert!(matches!(
            parse_accepted(r#"{"job_id":"0197a0c0-0000-7000-8000-000000000000"}"#),
            Err(IngestError::Protocol)
        ));
    }

    #[test]
    fn a_202_reads_the_first_job() {
        let accepted = parse_accepted(
            r#"{"jobs":[{"job_id":"0197a0c0-0000-7000-8000-000000000001","status":"queued"}]}"#,
        )
        .expect("parsed");
        assert_eq!(accepted.status, "queued");
        assert_eq!(
            accepted.job_id.to_string(),
            "0197a0c0-0000-7000-8000-000000000001"
        );
    }
}
