//! The provider seam: one trait, one response envelope.
//!
//! The trait is object-safe on purpose (`BoxFuture`, not `async fn`) because the chain holds a
//! heterogeneous set of providers. It carries the verbatim upstream bytes alongside the parsed
//! response: the ledger's capture columns and parity evidence need what actually went on the wire,
//! not a re-serialization.

use std::future::Future;
use std::pin::Pin;

use crate::catalog::ProviderEntry;
use crate::contract::chunks::ChatCompletionChunk;
use crate::contract::schemas::{ChatCompletionRequest, ChatCompletionResponse};

pub mod openai;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A parsed chunk stream. Owned end to end: the upstream response and the frame buffer move
/// into it, so the caller keeps no borrow of the provider or the request.
pub type ChunkStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<ChatCompletionChunk, UpstreamError>> + Send>>;

/// The exact request body sent upstream and the exact response body received.
#[derive(Debug, Clone)]
pub struct UpstreamBodies {
    pub request_text: String,
    pub response_text: String,
}

#[derive(Debug, Clone)]
pub struct UpstreamResponse {
    pub response: ChatCompletionResponse,
    pub bodies: UpstreamBodies,
}

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    /// IR itself was busy: the per-provider concurrency slot did not free in time.
    #[error("provider {provider} had no free slot within the wait budget")]
    QueueRejected { provider: String },
    #[error("provider {provider} answered HTTP {status}: {body}")]
    Status {
        provider: String,
        status: u16,
        body: String,
    },
    /// The upstream's own 400: the request's fault, and its reason is passed through.
    #[error("provider {provider} rejected the request: {message}")]
    BadRequest { provider: String, message: String },
    #[error("provider {provider} is unreachable: {message}")]
    Transport { provider: String, message: String },
    #[error("provider {provider} returned malformed JSON: {body}")]
    Malformed { provider: String, body: String },
    #[error("provider {provider} returned no usable content")]
    Empty { provider: String, body: String },
    /// The provider has no streaming implementation (test doubles).
    #[error("provider {provider} does not implement streaming")]
    Unsupported { provider: String },
}

impl UpstreamError {
    /// The `X-IR-Error-Classification` value the live service returns.
    pub fn classification(&self) -> &'static str {
        match self {
            Self::QueueRejected { .. } => "queue_rejected",
            Self::Status { status, .. } => match status {
                400 => "upstream_400",
                401 | 403 => "upstream_auth",
                429 => "rate_limited",
                500..=599 => "upstream_5xx",
                _ => "upstream_error",
            },
            Self::BadRequest { .. } => "upstream_400",
            Self::Transport { .. } => "upstream_unreachable",
            Self::Malformed { .. } => "malformed_json",
            Self::Empty { .. } => "upstream_empty_response",
            Self::Unsupported { .. } => "stream_unsupported",
        }
    }

    pub fn provider(&self) -> &str {
        match self {
            Self::QueueRejected { provider }
            | Self::Status { provider, .. }
            | Self::BadRequest { provider, .. }
            | Self::Transport { provider, .. }
            | Self::Malformed { provider, .. }
            | Self::Empty { provider, .. }
            | Self::Unsupported { provider } => provider,
        }
    }

    /// The operator-facing description the live service puts in a refusal
    /// (`describe_exception`): the reason, never empty.
    pub fn describe(&self) -> String {
        match self {
            Self::QueueRejected { provider } => {
                format!("{provider}: provider concurrency slot wait timed out")
            }
            Self::Status { status, body, .. } => {
                format!("HTTP {status}: {}", truncate(body, 500))
            }
            Self::BadRequest { message, .. } => message.clone(),
            Self::Transport { message, .. } => message.clone(),
            Self::Malformed { .. } => "malformed JSON from upstream".to_owned(),
            Self::Empty { .. } => "upstream returned empty choices".to_owned(),
            Self::Unsupported { .. } => "provider does not implement streaming".to_owned(),
        }
    }
}

/// A bounded, control-character-free rendering of an upstream body for an error message.
pub(crate) fn truncate_json(value: &serde_json::Value) -> String {
    truncate(&value.to_string(), 500)
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Why a provider could not be constructed at boot.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("provider '{name}' is enabled but has no endpoint")]
    MissingEndpoint { name: String },
    #[error("provider '{name}' needs ${variable}, which is unset or blank")]
    MissingCredential { name: String, variable: String },
    #[error("provider '{name}' uses a protocol this build does not serve: {protocol:?}")]
    UnsupportedProtocol {
        name: String,
        protocol: crate::catalog::Protocol,
    },
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    /// One non-streaming completion. `model_override` is the provider's upstream id when the
    /// caller named an alias of this provider; `None` means "use the provider's default model".
    fn complete<'a>(
        &'a self,
        request: &'a ChatCompletionRequest,
        model_override: Option<&'a str>,
    ) -> BoxFuture<'a, Result<UpstreamResponse, UpstreamError>>;

    /// Open one streamed completion and return its parsed chunk stream.
    ///
    /// The future resolves once the upstream has answered with a success status; an open
    /// failure is this future's error, and a failure before the first chunk is the stream's
    /// first item. The router reads one chunk before it decides the attempt served, so a
    /// provider that accepts the connection and then errors still fails over exactly as the
    /// reference does.
    fn stream<'a>(
        &'a self,
        request: &'a ChatCompletionRequest,
        model_override: Option<&'a str>,
    ) -> BoxFuture<'a, Result<ChunkStream, UpstreamError>>;
}

/// Build the trait object for an enabled catalog entry.
pub fn build(
    entry: &ProviderEntry,
    slot_wait: std::time::Duration,
) -> Result<Box<dyn Provider>, BuildError> {
    match entry.protocol {
        crate::catalog::Protocol::Openai => {
            Ok(Box::new(openai::OpenAiProvider::new(entry, slot_wait)?))
        }
        other => Err(BuildError::UnsupportedProtocol {
            name: entry.name.clone(),
            protocol: other,
        }),
    }
}
