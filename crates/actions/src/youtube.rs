#![forbid(unsafe_code)]
//! The tenant's own YouTube channel: its OAuth client, the consent exchange and the connection
//! state (#182 slice A).
//!
//! The owner ruling of 2026-11-16 put the OAuth client **in this app, per tenant** rather than in
//! a tool-local admin page, so the reference's `/api/admin/*` routes are deliberately not ported.
//! A tenant configures the flow in three steps:
//!
//! 1. `youtube.client_set` — its Google OAuth client id and secret (the secret is write-only);
//! 2. `youtube.connect` — the consent `code` and the `redirect_uri` it was issued for;
//! 3. `youtube.status` / `youtube.disconnect` — what the connection is, and how to drop it.
//!
//! # What is stored, and what can never come back
//!
//! Both values live in `conveyor_store::secret` — the workspace's one sealed store, never a second
//! encryption scheme:
//!
//! | row name | sealed value |
//! |---|---|
//! | [`CLIENT_SECRET`] | `{"client_id": …, "client_secret": …}` |
//! | [`REFRESH_SECRET`] | `{"refresh_token": …, "expires_at": …}` |
//!
//! **No action here can return either value.** `client_get` answers the client id and whether a
//! secret is set; `status` answers the connection state and the access token's expiry. There is no
//! reveal action, and the test suite greps every response body for the value it supplied.
//!
//! # Publishing (slice B)
//!
//! `youtube.upload` validates one upload and enqueues the `youtube.upload` job; it never calls
//! Google itself, because the bytes are read and the video uploaded by the worker. `youtube.privacy`
//! and `youtube.retract` are synchronous: they refresh the tenant's access token and call
//! `videos.update` / `videos.delete` through the same provider.
//!
//! **The upload job's idempotency is honest and bounded.** YouTube has no idempotency key. The job
//! records the provider's `video_id` in its own `payload.result`; before uploading, the worker reads
//! that value and, if it is already there, finishes with it instead of uploading again. A retried
//! attempt of the SAME job therefore cannot double-upload **once the id has been recorded**. Two
//! different jobs for the same document can upload twice, and a crash in the window between
//! YouTube's acceptance and the result write leaves no id to find; both are documented, not hidden.
//!
//! # The provider
//!
//! The token exchange is behind [`YouTubeProvider`], so these handlers never speak HTTP and a test
//! can supply a stub. The Google implementation is `integrations::youtube`; production installs
//! nothing and `provider()` falls back to it. [`install_provider`] is the one test seam, and
//! nothing else calls it.
//!
//! # A new client id disconnects
//!
//! A refresh token is issued to the client that performed the consent. When `client_set` stores a
//! **different** client id, the stored grant can no longer be refreshed, so it is deleted in the
//! same transaction and `status` reports `connected: false` until the tenant consents again. A
//! rotation of the same client's secret keeps the grant (Google allows that).

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use conveyor_domain::{TenantId, Version};
use conveyor_store as store;
use conveyor_store::secret::{SecretError, SecretRecord};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::secret_error;
use crate::integrations::youtube::GoogleTokenProvider;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;
use crate::views::EmptyIn;

/// Store the tenant's Google OAuth client.
pub const CLIENT_SET: &str = "youtube.client_set";
/// Read the configured client id (never the secret).
pub const CLIENT_GET: &str = "youtube.client_get";
/// Exchange a consent code for a stored refresh token.
pub const CONNECT: &str = "youtube.connect";
/// Report the connection state (never a token).
pub const STATUS: &str = "youtube.status";
/// Drop the stored refresh token. Idempotent.
pub const DISCONNECT: &str = "youtube.disconnect";
/// Validate one upload and enqueue the `youtube.upload` job.
pub const UPLOAD: &str = "youtube.upload";
/// Set an uploaded video's privacy (`videos.update`). Idempotent.
pub const PRIVACY: &str = "youtube.privacy";
/// Delete an uploaded video (`videos.delete`). Idempotent.
pub const RETRACT: &str = "youtube.retract";

/// Every action this module publishes, in registration order.
pub const ACTIONS: [&str; 8] = [
    CLIENT_SET, CLIENT_GET, CONNECT, STATUS, DISCONNECT, UPLOAD, PRIVACY, RETRACT,
];

/// The job kind `youtube.upload` enqueues. The action and the worker share this one spelling.
pub const UPLOAD_KIND: &str = UPLOAD;

/// The upload job's attempts: the first dispatch plus one retry. A retry can only double-upload in
/// the crash window between YouTube's acceptance and the `result.video_id` write — a recorded id
/// makes the retry a finish, not a second upload.
pub const UPLOAD_MAX_ATTEMPTS: i32 = 2;

/// The sealed secret holding `{"client_id", "client_secret"}`.
pub const CLIENT_SECRET: &str = "youtube.oauth_client";

/// The sealed secret holding `{"refresh_token", "expires_at"}`.
pub const REFRESH_SECRET: &str = "youtube.refresh_token";

/// A client id is short; this bound only stops an accidental paste of a whole document.
const CLIENT_ID_MAX: usize = 512;
/// A Google client secret is ~35 characters; 2 KiB is generous and bounded.
const CLIENT_SECRET_MAX: usize = 2048;
/// An authorization code is opaque but short-lived; a certificate chain must not fit here.
const CODE_MAX: usize = 4096;
/// The redirect URI must equal the one registered with Google.
const REDIRECT_URI_MAX: usize = 2048;
/// YouTube's title cap.
const TITLE_MAX: usize = 100;
/// YouTube's description cap.
const DESCRIPTION_MAX: usize = 5_000;
/// The reference's DoS ceiling on the tag list (YouTube's real cap is a byte budget).
const TAGS_MAX: usize = 500;
/// The reference's per-tag cap.
const TAG_MAX: usize = 100;
/// A YouTube video id is 11 base64url characters; this bound only stops a pasted document.
const VIDEO_ID_MAX: usize = 64;

/// The three visibility values `videos.insert`/`videos.update` accept.
pub const PRIVACIES: [&str; 3] = ["public", "unlisted", "private"];

// ── the provider trait ──────────────────────────────────────────────────────────────────────

/// A boxed provider future. The trait is object-safe, so production can hold one provider for the
/// process and a test can hold a stub.
pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, YouTubeError>> + Send + 'a>>;

/// What a token exchange or refresh yields.
///
/// The tokens are secrets: [`Debug`] is hand-written to redact them, because a future `?` or a
/// `tracing::debug!` must not be able to print one.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    /// The bearer for the Data API. Never returned by an action.
    pub access_token: String,
    /// The durable grant. `None` on a refresh that did not rotate it.
    pub refresh_token: Option<String>,
    /// When [`Self::access_token`] expires.
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Why a provider call did not produce a token set.
///
/// The provider's response body is deliberately absent: it is untrusted text that may echo the
/// client secret, and this enum's `Display` reaches a problem document.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum YouTubeError {
    /// The token endpoint could not be reached, or no HTTP client could be built.
    #[error("the YouTube token endpoint could not be reached")]
    Unavailable,
    /// The endpoint answered non-2xx (400 for a spent code, 401 for a bad client).
    #[error("the YouTube token endpoint refused the request with status {status}")]
    Refused {
        /// The upstream HTTP status.
        status: u16,
    },
    /// A 2xx whose body was not the shape this client reads.
    #[error("the YouTube token response could not be read")]
    InvalidResponse,
}

/// One video to upload: the metadata the Data API takes, plus the document's bytes.
///
/// [`Debug`] is hand-written so a 10 MiB document cannot be printed into a log line by a future
/// `tracing::debug!(?request)`.
#[derive(Clone, PartialEq, Eq)]
pub struct UploadRequest {
    /// The video title (1–100 characters).
    pub title: String,
    /// The optional description (≤ 5000 characters).
    pub description: Option<String>,
    /// One of [`PRIVACIES`].
    pub privacy: String,
    /// Video tags, each ≤ 100 characters.
    pub tags: Vec<String>,
    /// The document's content type, e.g. `video/mp4`.
    pub content_type: String,
    /// The video bytes.
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for UploadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadRequest")
            .field("title", &self.title)
            .field("privacy", &self.privacy)
            .field("tags", &self.tags)
            .field("content_type", &self.content_type)
            .field("bytes", &format_args!("{} bytes", self.bytes.len()))
            .finish_non_exhaustive()
    }
}

/// The token endpoint and the three Data API calls, behind one trait so actions and the worker
/// depend on the contract and not on the network.
pub trait YouTubeProvider: Send + Sync {
    /// Exchange an authorization code for a token set.
    fn exchange_code<'a>(
        &'a self,
        client_id: &'a str,
        client_secret: &'a str,
        code: &'a str,
        redirect_uri: &'a str,
    ) -> ProviderFuture<'a, TokenSet>;

    /// Refresh an access token. Slice A stores the grant; the upload slice calls this.
    fn refresh<'a>(
        &'a self,
        client_id: &'a str,
        client_secret: &'a str,
        refresh_token: &'a str,
    ) -> ProviderFuture<'a, TokenSet>;

    /// Upload one video and answer its id (`videos.insert`).
    fn upload<'a>(
        &'a self,
        access_token: &'a str,
        request: UploadRequest,
    ) -> ProviderFuture<'a, String>;

    /// Set one video's visibility (`videos.update` with `part=status`).
    fn set_privacy<'a>(
        &'a self,
        access_token: &'a str,
        video_id: &'a str,
        privacy: &'a str,
    ) -> ProviderFuture<'a, ()>;

    /// Delete one video (`videos.delete`). A video the account no longer has is a success: the
    /// caller asked for it to be gone, and it is.
    fn retract<'a>(&'a self, access_token: &'a str, video_id: &'a str) -> ProviderFuture<'a, ()>;
}

/// The production provider, a zero-sized handle over a process-wide HTTP client.
static GOOGLE: GoogleTokenProvider = GoogleTokenProvider;

/// The installed provider, if a test installed one. Set once, before the first invocation.
static INSTALLED: OnceLock<Box<dyn YouTubeProvider>> = OnceLock::new();

/// Install the process's provider. Returns `false` when one is already installed.
///
/// The only caller is a test binary: production leaves this unset and `provider()` answers
/// [`GoogleTokenProvider`]. It exists because `youtube.connect` must be exercisable end to end
/// through the real HTTP/MCP transports without a network call, and threading a provider through
/// `HandlerCtx` would put a test concern on every action.
pub fn install_provider(provider: Box<dyn YouTubeProvider>) -> bool {
    INSTALLED.set(provider).is_ok()
}

/// The provider this process uses.
pub fn installed_provider() -> &'static dyn YouTubeProvider {
    provider()
}

fn provider() -> &'static dyn YouTubeProvider {
    match INSTALLED.get() {
        Some(provider) => provider.as_ref(),
        None => &GOOGLE,
    }
}

/// Map a provider failure to a refusal. The detail is constant: no upstream text and no argument
/// (a client secret or an authorization code) can reach it.
fn provider_error(error: YouTubeError, action: &str) -> ActionError {
    tracing::error!(%error, action, "youtube: the token request failed");
    ActionError::new(
        502,
        "youtube-provider-failed",
        "the YouTube token request failed; the connection was not stored",
    )
}

/// Map a Data API failure to a refusal. Same rule: the upstream body never reaches this document.
fn data_error(error: YouTubeError, action: &str, what: &str) -> ActionError {
    tracing::error!(%error, action, "youtube: the {what} request failed");
    ActionError::new(
        502,
        "youtube-provider-failed",
        format!("the YouTube {what} request failed"),
    )
}

/// The tenant has no stored client or grant: an upload cannot be attempted and a video cannot be
/// changed. Names the two actions that fix it rather than surfacing a provider error.
fn not_connected(action: &str) -> ActionError {
    tracing::warn!(action, "youtube: no stored connection");
    ActionError::new(
        422,
        "youtube_not_configured",
        "no YouTube connection is stored for this tenant; call youtube.client_set and youtube.connect first",
    )
}

/// The tenant's OAuth client and durable grant, opened from the sealed store. Never serialisable:
/// it exists in a caller's memory for one provider call.
#[derive(Clone)]
pub struct Credentials {
    /// The Google OAuth client id.
    pub client_id: String,
    /// The Google OAuth client secret.
    pub client_secret: String,
    /// The durable refresh token.
    pub refresh_token: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .finish()
    }
}

/// Why the stored connection could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    /// The sealed store refused the read (a missing KEK, a database failure).
    #[error("the YouTube secret store failed: {0}")]
    Store(#[from] SecretError),
    /// A row exists but is not the envelope this module wrote.
    #[error("the stored YouTube envelope is not readable")]
    Unreadable,
}

/// Why an access token could not be refreshed.
#[derive(Debug, thiserror::Error)]
pub enum AccessTokenError {
    /// No connection is stored, or it could not be read.
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    /// The provider refused the refresh.
    #[error(transparent)]
    Provider(#[from] YouTubeError),
}

/// The tenant's stored client and grant, or `None` when either is missing.
///
/// The worker reads this too: it is the one door to the sealed connection, so a test can install a
/// stub provider without also stubbing the secret store.
pub async fn connection(
    tx: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<Credentials>, ConnectionError> {
    let Some(client_raw) = open_raw(&mut *tx, tenant, CLIENT_SECRET).await? else {
        return Ok(None);
    };
    let Some(token_raw) = open_raw(&mut *tx, tenant, REFRESH_SECRET).await? else {
        return Ok(None);
    };
    let client: StoredClient =
        serde_json::from_slice(&client_raw).map_err(|_| ConnectionError::Unreadable)?;
    let token: StoredToken =
        serde_json::from_slice(&token_raw).map_err(|_| ConnectionError::Unreadable)?;
    Ok(Some(Credentials {
        client_id: client.client_id,
        client_secret: client.client_secret,
        refresh_token: token.refresh_token,
    }))
}

/// Refresh the tenant's access token through `provider`.
///
/// `Ok(None)` means there is no stored connection (a caller answers [`not_connected`]). A rotated
/// refresh token is deliberately discarded: Google does not rotate on a refresh, and storing one
/// here would put a write in the middle of a read path.
pub async fn access_token(
    tx: &mut PgConnection,
    provider: &dyn YouTubeProvider,
    tenant: TenantId,
) -> Result<Option<String>, AccessTokenError> {
    let Some(credentials) = connection(&mut *tx, tenant).await? else {
        return Ok(None);
    };
    let tokens = provider
        .refresh(
            &credentials.client_id,
            &credentials.client_secret,
            &credentials.refresh_token,
        )
        .await?;
    Ok(Some(tokens.access_token))
}

// ── stored envelopes (never serialised to a response) ──────────────────────────────────────

/// The sealed client envelope.
#[derive(Clone, Serialize, Deserialize)]
struct StoredClient {
    client_id: String,
    client_secret: String,
}

/// The sealed grant envelope.
#[derive(Clone, Serialize, Deserialize)]
struct StoredToken {
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

// ── input and output documents ─────────────────────────────────────────────────────────────

/// `youtube.client_set` input. The secret is write-only.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientSetIn {
    /// The Google OAuth client id, e.g. `123-abc.apps.googleusercontent.com`.
    pub client_id: String,
    /// The client secret. Sealed at rest; never returned, logged or put in an error.
    #[schemars(extend("x-secret" = true))]
    pub client_secret: String,
}

/// `youtube.client_set` output — the id, and the fact that a secret is now set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientSetOut {
    /// The stored client id.
    pub client_id: String,
    /// Always `true` after a successful set; a boolean rather than the value.
    pub secret_set: bool,
}

/// `youtube.client_get` output — the id and whether a secret is set, never the secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientGetOut {
    /// The stored client id, or `null` when the tenant has not configured a client.
    pub client_id: Option<String>,
    /// Whether a client secret is stored.
    pub secret_set: bool,
}

/// `youtube.connect` input — the consent answer and the URI it was issued for.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectIn {
    /// The `code` Google redirected back with. Single-use and short-lived.
    pub code: String,
    /// The exact redirect URI registered with Google for that client.
    pub redirect_uri: String,
}

/// `youtube.connect` output — the connection exists, and when the access token expires.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConnectOut {
    /// `true` when the refresh token was sealed and stored.
    pub connected: bool,
    /// When the access token obtained in this exchange expires.
    pub expires_at: DateTime<Utc>,
}

/// `youtube.status` output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StatusOut {
    /// Whether a refresh token is stored for this tenant.
    pub connected: bool,
    /// When the stored access token expires; `null` when disconnected.
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the tenant has configured its OAuth client.
    pub client_configured: bool,
}

/// `youtube.disconnect` output. Always `connected: false`, including when already disconnected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DisconnectOut {
    /// Always `false`: the action succeeds by making this so.
    pub connected: bool,
}

/// `youtube.upload` input — the document and the metadata `videos.insert` takes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadIn {
    /// The document whose bytes are the video (`video/*`, stored in this tenant's object store).
    pub document_id: Uuid,
    /// The video title (1–100 characters).
    pub title: String,
    /// The optional description (≤ 5000 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `public`, `unlisted` or `private`.
    pub privacy: String,
    /// Optional tags, each ≤ 100 characters, at most 500 of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// `youtube.upload` output — the job the worker will run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UploadOut {
    /// The `job` row this upload became. A live job with the same parameters is joined, not raced.
    pub job_id: Uuid,
}

/// `youtube.privacy` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivacyIn {
    /// The video id `youtube.upload` recorded.
    pub video_id: String,
    /// `public`, `unlisted` or `private`.
    pub privacy: String,
}

/// `youtube.privacy` output — the video id and the visibility it now holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyOut {
    /// The video id the caller supplied.
    pub video_id: String,
    /// The visibility the provider accepted.
    pub privacy: String,
}

/// `youtube.retract` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetractIn {
    /// The video id `youtube.upload` recorded.
    pub video_id: String,
}

/// `youtube.retract` output. Always `retracted: true` on success, including when the video was
/// already gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RetractOut {
    /// Always `true`: the action succeeds by making the video absent.
    pub retracted: bool,
}

// ── validation ─────────────────────────────────────────────────────────────────────────────

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// A configuration value: non-empty after trimming, no whitespace inside, bounded.
///
/// Trimming is deliberate: a copied client id or secret commonly arrives with a trailing newline,
/// and no real value contains one. The value stored is the trimmed one.
fn require_value(field: &str, value: &str, max: usize) -> Result<String, ActionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid(field, "required", "must not be empty"));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(invalid(
            field,
            "invalid_value",
            "must not contain whitespace",
        ));
    }
    if trimmed.chars().count() > max {
        return Err(invalid(
            field,
            "invalid_value",
            format!("must be at most {max} characters"),
        ));
    }
    Ok(trimmed.to_owned())
}

/// The redirect URI must be the absolute one Google is configured with. `http` is admitted for a
/// local-development callback; Google itself refuses one that is not registered.
fn require_redirect_uri(value: &str) -> Result<String, ActionError> {
    let trimmed = require_value("redirect_uri", value, REDIRECT_URI_MAX)?;
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(invalid(
            "redirect_uri",
            "invalid_value",
            "must be an absolute http(s) URI, exactly as registered with Google",
        ));
    }
    Ok(trimmed)
}

/// One of [`PRIVACIES`], trimmed.
fn require_privacy(field: &str, value: &str) -> Result<String, ActionError> {
    let trimmed = value.trim();
    if !PRIVACIES.contains(&trimmed) {
        return Err(invalid(
            field,
            "invalid_value",
            "must be one of public, unlisted, private",
        ));
    }
    Ok(trimmed.to_owned())
}

/// The title `videos.insert` accepts: non-empty after trimming, at most 100 characters.
fn require_title(value: &str) -> Result<String, ActionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if trimmed.chars().count() > TITLE_MAX {
        return Err(invalid(
            "title",
            "invalid_value",
            format!("must be at most {TITLE_MAX} characters"),
        ));
    }
    Ok(trimmed.to_owned())
}

/// The description `videos.insert` accepts: optional, at most 5000 characters.
fn require_description(value: &str) -> Result<String, ActionError> {
    let trimmed = value.trim();
    if trimmed.chars().count() > DESCRIPTION_MAX {
        return Err(invalid(
            "description",
            "invalid_value",
            format!("must be at most {DESCRIPTION_MAX} characters"),
        ));
    }
    Ok(trimmed.to_owned())
}

/// The tag list `videos.insert` accepts: at most 500 tags, each non-empty and ≤ 100 characters.
fn require_tags(tags: Vec<String>) -> Result<Vec<String>, ActionError> {
    if tags.len() > TAGS_MAX {
        return Err(invalid(
            "tags",
            "invalid_value",
            format!("must hold at most {TAGS_MAX} tags"),
        ));
    }
    let mut out = Vec::with_capacity(tags.len());
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(invalid("tags", "invalid_value", "a tag must not be empty"));
        }
        if trimmed.chars().count() > TAG_MAX {
            return Err(invalid(
                "tags",
                "invalid_value",
                format!("each tag must be at most {TAG_MAX} characters"),
            ));
        }
        out.push(trimmed.to_owned());
    }
    Ok(out)
}

/// A video id this module could have recorded: base64url-ish and bounded, so a pasted blob is a
/// validation refusal rather than a request URL.
fn require_video_id(value: &str) -> Result<String, ActionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid("video_id", "required", "must not be empty"));
    }
    if trimmed.chars().count() > VIDEO_ID_MAX
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(invalid(
            "video_id",
            "invalid_value",
            format!("must be at most {VIDEO_ID_MAX} characters of [A-Za-z0-9_-]"),
        ));
    }
    Ok(trimmed.to_owned())
}

// ── secret row helpers ────────────────────────────────────────────────────────────────────

/// The tenant's secret metadata for `name`, or `None`.
async fn named_record(
    tx: &mut PgConnection,
    tenant: TenantId,
    name: &str,
    action: &str,
) -> Result<Option<SecretRecord>, ActionError> {
    let records = store::secret::list_secrets_tx(&mut *tx, tenant)
        .await
        .map_err(|error| secret_error(error, action))?;
    Ok(records.into_iter().find(|record| record.name == name))
}

/// Open a sealed value by name; a missing row is `None`, not an error.
async fn open_raw(
    tx: &mut PgConnection,
    tenant: TenantId,
    name: &str,
) -> Result<Option<Vec<u8>>, SecretError> {
    match store::secret::open_named_tx(&mut *tx, tenant, name).await {
        Ok(value) => Ok(Some(value)),
        Err(SecretError::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

/// [`open_raw`] with an action's error document attached.
async fn open_named(
    tx: &mut PgConnection,
    tenant: TenantId,
    name: &str,
    action: &str,
) -> Result<Option<Vec<u8>>, ActionError> {
    open_raw(&mut *tx, tenant, name)
        .await
        .map_err(|error| secret_error(error, action))
}

fn version_of(record: &SecretRecord) -> Result<Version, ActionError> {
    u64::try_from(record.version)
        .ok()
        .and_then(|raw| Version::new(raw).ok())
        .ok_or_else(ActionError::internal)
}

/// Create the secret, or rotate it when the tenant already has one under this name.
async fn store_named(
    tx: &mut PgConnection,
    tenant: TenantId,
    name: &str,
    value: &[u8],
    action: &str,
) -> Result<SecretRecord, ActionError> {
    if let Some(record) = named_record(&mut *tx, tenant, name, action).await? {
        return store::secret::rotate_secret_tx(
            &mut *tx,
            tenant,
            record.id,
            version_of(&record)?,
            value,
        )
        .await
        .map_err(|error| secret_error(error, action));
    }
    match store::secret::create_secret_tx(&mut *tx, tenant, name, value, "tenant").await {
        Ok(record) => Ok(record),
        // A concurrent writer won the insert between our read and this create. Rotate what is
        // now there rather than surfacing a conflict the caller did not cause.
        Err(SecretError::NameTaken(_)) => {
            let record = named_record(&mut *tx, tenant, name, action)
                .await?
                .ok_or_else(ActionError::internal)?;
            store::secret::rotate_secret_tx(
                &mut *tx,
                tenant,
                record.id,
                version_of(&record)?,
                value,
            )
            .await
            .map_err(|error| secret_error(error, action))
        }
        Err(error) => Err(secret_error(error, action)),
    }
}

/// Delete the secret by name. Returns whether a row existed; a concurrent delete is `Ok(false)`.
///
/// A version mismatch means a connect rotated the grant between our read and the delete. The
/// caller asked to disconnect, so deleting the newest grant is correct: retry once.
async fn delete_named(
    tx: &mut PgConnection,
    tenant: TenantId,
    name: &str,
    action: &str,
) -> Result<bool, ActionError> {
    for _ in 0..2 {
        let Some(record) = named_record(&mut *tx, tenant, name, action).await? else {
            return Ok(false);
        };
        match store::secret::delete_secret_tx(&mut *tx, tenant, record.id, version_of(&record)?)
            .await
        {
            Ok(()) => return Ok(true),
            Err(SecretError::NotFound) => return Ok(false),
            Err(SecretError::VersionMismatch { .. }) => continue,
            Err(error) => return Err(secret_error(error, action)),
        }
    }
    Err(ActionError::internal())
}

/// The client envelope, or `None` when the tenant has not configured a client.
async fn stored_client(
    tx: &mut PgConnection,
    tenant: TenantId,
    action: &str,
) -> Result<Option<StoredClient>, ActionError> {
    let Some(raw) = open_named(&mut *tx, tenant, CLIENT_SECRET, action).await? else {
        return Ok(None);
    };
    serde_json::from_slice(&raw).map(Some).map_err(|_| {
        tracing::error!(action, "the stored YouTube client envelope is not readable");
        ActionError::internal()
    })
}

// ── actions ───────────────────────────────────────────────────────────────────────────────

/// Store the tenant's Google OAuth client. The secret is sealed and never echoed.
#[crate::action(
    name = "youtube.client_set",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_client_set(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ClientSetIn,
) -> Result<ClientSetOut, ActionError> {
    let action = CLIENT_SET;
    let client_id = require_value("client_id", &input.client_id, CLIENT_ID_MAX)?;
    let client_secret = require_value("client_secret", &input.client_secret, CLIENT_SECRET_MAX)?;

    // A grant issued to a different client cannot be refreshed by the new one. Drop it in the same
    // transaction so `status` never claims a connection that cannot work.
    if let Some(existing) = stored_client(&mut *tx, ctx.tenant, action).await?
        && existing.client_id != client_id
    {
        delete_named(&mut *tx, ctx.tenant, REFRESH_SECRET, action).await?;
    }

    let encoded = serde_json::to_vec(&StoredClient {
        client_id: client_id.clone(),
        client_secret,
    })
    .map_err(|_| ActionError::internal())?;
    store_named(&mut *tx, ctx.tenant, CLIENT_SECRET, &encoded, action).await?;
    Ok(ClientSetOut {
        client_id,
        secret_set: true,
    })
}

/// Read the configured client id. Never the secret.
#[crate::action(
    name = "youtube.client_get",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_client_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<ClientGetOut, ActionError> {
    let client = stored_client(&mut *tx, ctx.tenant, CLIENT_GET).await?;
    Ok(ClientGetOut {
        client_id: client.as_ref().map(|stored| stored.client_id.clone()),
        secret_set: client.is_some(),
    })
}

/// Exchange a consent code through the provider and seal the refresh token.
#[crate::action(
    name = "youtube.connect",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_connect(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConnectIn,
) -> Result<ConnectOut, ActionError> {
    connect_with(&mut *tx, ctx, input, provider()).await
}

/// The handler body, separated from [`provider`] so the test seam is one line.
async fn connect_with(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConnectIn,
    provider: &dyn YouTubeProvider,
) -> Result<ConnectOut, ActionError> {
    let action = CONNECT;
    let code = require_value("code", &input.code, CODE_MAX)?;
    let redirect_uri = require_redirect_uri(&input.redirect_uri)?;

    let Some(client) = stored_client(&mut *tx, ctx.tenant, action).await? else {
        return Err(invalid(
            "client_id",
            "not_configured",
            "no YouTube OAuth client is configured for this tenant; call youtube.client_set first",
        ));
    };

    let tokens = provider
        .exchange_code(
            &client.client_id,
            &client.client_secret,
            &code,
            &redirect_uri,
        )
        .await
        .map_err(|error| provider_error(error, action))?;

    // Without a refresh token the connection would die with the access token, and a later
    // reconnect would need another consent. Refuse instead of storing a half-connection.
    let Some(refresh_token) = tokens.refresh_token.clone() else {
        return Err(ActionError::new(
            422,
            "youtube-refresh-token-missing",
            "Google returned no refresh token; remove this app's access in the Google account and connect again",
        ));
    };

    let encoded = serde_json::to_vec(&StoredToken {
        refresh_token,
        expires_at: tokens.expires_at,
    })
    .map_err(|_| ActionError::internal())?;
    store_named(&mut *tx, ctx.tenant, REFRESH_SECRET, &encoded, action).await?;

    Ok(ConnectOut {
        connected: true,
        expires_at: tokens.expires_at,
    })
}

/// Report the connection state. Never a token.
#[crate::action(
    name = "youtube.status",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_status(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<StatusOut, ActionError> {
    let action = STATUS;
    let client_configured = named_record(&mut *tx, ctx.tenant, CLIENT_SECRET, action)
        .await?
        .is_some();
    let token = open_named(&mut *tx, ctx.tenant, REFRESH_SECRET, action).await?;
    let (connected, expires_at) = match token {
        Some(raw) => {
            let stored: StoredToken = serde_json::from_slice(&raw).map_err(|_| {
                tracing::error!(action, "the stored YouTube token envelope is not readable");
                ActionError::internal()
            })?;
            (true, Some(stored.expires_at))
        }
        None => (false, None),
    };
    Ok(StatusOut {
        connected,
        expires_at,
        client_configured,
    })
}

/// Delete the sealed refresh token. Disconnecting twice is a success, not a 404.
#[crate::action(
    name = "youtube.disconnect",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_disconnect(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<DisconnectOut, ActionError> {
    delete_named(&mut *tx, ctx.tenant, REFRESH_SECRET, DISCONNECT).await?;
    Ok(DisconnectOut { connected: false })
}

// ── slice B: publishing ─────────────────────────────────────────────────────────────────

/// The job payload: the upload's parameters only. The worker reads `payload.params`, and the
/// dedupe key is the SHA-256 of exactly these bytes, so two identical calls made while the first
/// job is live join that job instead of racing it.
fn upload_payload(
    input: &UploadIn,
    title: &str,
    description: Option<&str>,
    privacy: &str,
    tags: Option<&[String]>,
) -> Result<(Value, String), ActionError> {
    let mut params = serde_json::Map::new();
    params.insert(
        "document_id".to_owned(),
        serde_json::json!(input.document_id),
    );
    params.insert("title".to_owned(), serde_json::json!(title));
    params.insert("privacy".to_owned(), serde_json::json!(privacy));
    if let Some(description) = description {
        params.insert("description".to_owned(), serde_json::json!(description));
    }
    if let Some(tags) = tags {
        params.insert("tags".to_owned(), serde_json::json!(tags));
    }
    let payload = serde_json::json!({ "params": params });
    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        tracing::error!(%error, "youtube.upload: the job payload does not serialise");
        ActionError::internal()
    })?;
    Ok((payload, crate::spec::sha256_hex(&encoded)))
}

/// Validate one upload and enqueue the `youtube.upload` job. The upload itself is the worker's.
#[crate::action(
    name = "youtube.upload",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_upload(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: UploadIn,
) -> Result<UploadOut, ActionError> {
    let title = require_title(&input.title)?;
    let privacy = require_privacy("privacy", &input.privacy)?;
    let description = match input.description.as_deref() {
        Some(description) => Some(require_description(description)?),
        None => None,
    };
    let tags = match input.tags.clone() {
        Some(tags) => Some(require_tags(tags)?),
        None => None,
    };

    let (payload, dedupe) = upload_payload(
        &input,
        &title,
        description.as_deref(),
        &privacy,
        tags.as_deref(),
    )?;
    let enqueued = store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        UPLOAD_KIND,
        &dedupe,
        payload,
        None,
        UPLOAD_MAX_ATTEMPTS,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "youtube.upload: enqueue");
        ActionError::internal()
    })?;
    let job_id = match enqueued {
        Some(job_id) => job_id,
        // A live job already holds this key: the newest row is the live one, and the store's
        // partial unique index is what makes this a read rather than a race.
        None => store::jobs::by_dedupe(&mut *tx, ctx.tenant, UPLOAD_KIND, &dedupe)
            .await
            .map_err(|error| {
                tracing::error!(%error, "youtube.upload: read the live job");
                ActionError::internal()
            })?
            .into_iter()
            .next()
            .map(|job| job.id)
            .ok_or_else(ActionError::internal)?,
    };
    Ok(UploadOut { job_id })
}

/// The access token both synchronous publishing actions use, or a named refusal.
async fn publishing_token(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    action: &str,
) -> Result<String, ActionError> {
    match access_token(&mut *tx, provider(), ctx.tenant).await {
        Ok(Some(token)) => Ok(token),
        Ok(None) => Err(not_connected(action)),
        Err(AccessTokenError::Connection(error)) => {
            tracing::error!(%error, action, "youtube: the stored connection is unusable");
            Err(ActionError::internal())
        }
        Err(AccessTokenError::Provider(error)) => Err(provider_error(error, action)),
    }
}

/// Set an uploaded video's visibility. Re-sending the same visibility is a success.
#[crate::action(
    name = "youtube.privacy",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_privacy(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PrivacyIn,
) -> Result<PrivacyOut, ActionError> {
    let action = PRIVACY;
    let video_id = require_video_id(&input.video_id)?;
    let privacy = require_privacy("privacy", &input.privacy)?;
    let token = publishing_token(&mut *tx, &ctx, action).await?;
    provider()
        .set_privacy(&token, &video_id, &privacy)
        .await
        .map_err(|error| data_error(error, action, "privacy update"))?;
    Ok(PrivacyOut { video_id, privacy })
}

/// Delete an uploaded video. A video the account no longer has is still a retraction: the caller
/// asked for it to be gone, and it is.
#[crate::action(
    name = "youtube.retract",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn youtube_retract(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RetractIn,
) -> Result<RetractOut, ActionError> {
    let action = RETRACT;
    let video_id = require_video_id(&input.video_id)?;
    let token = publishing_token(&mut *tx, &ctx, action).await?;
    provider()
        .retract(&token, &video_id)
        .await
        .map_err(|error| data_error(error, action, "retraction"))?;
    Ok(RetractOut { retracted: true })
}

#[cfg(test)]
mod tests {
    //! The pure half of the module's contract. The behavioural suite — a connect through the real
    //! HTTP transport with a stub provider, tenant isolation and every response body grepped for
    //! the sealed values — is `crates/actions/tests/youtube.rs`, where the shared Postgres
    //! harness lives.

    use super::*;

    #[test]
    fn a_token_set_never_prints_its_tokens() {
        let tokens = TokenSet {
            access_token: "access-value-must-not-print".into(),
            refresh_token: Some("refresh-value-must-not-print".into()),
            expires_at: DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                .expect("a fixed timestamp")
                .with_timezone(&Utc),
        };
        let text = format!("{tokens:?}");
        assert!(!text.contains("access-value-must-not-print"), "{text}");
        assert!(!text.contains("refresh-value-must-not-print"), "{text}");
        assert!(text.contains("<redacted>"), "{text}");
    }

    #[test]
    fn a_client_id_and_secret_are_bounded_and_whitespace_free() {
        assert_eq!(
            require_value(
                "client_id",
                " 123-abc.apps.googleusercontent.com\n",
                CLIENT_ID_MAX
            )
            .expect("trimmed"),
            "123-abc.apps.googleusercontent.com"
        );
        assert!(require_value("client_id", "   ", CLIENT_ID_MAX).is_err());
        assert!(require_value("client_secret", "a b", CLIENT_SECRET_MAX).is_err());
        assert!(require_value("client_id", &"x".repeat(CLIENT_ID_MAX + 1), CLIENT_ID_MAX).is_err());
    }

    #[test]
    fn a_redirect_uri_must_be_absolute_http() {
        assert!(require_redirect_uri("https://conveyor.example/oauth/youtube").is_ok());
        assert!(require_redirect_uri("http://localhost:3000/callback").is_ok());
        assert!(require_redirect_uri("/relative/callback").is_err());
        assert!(require_redirect_uri("conveyor.example/callback").is_err());
    }

    #[test]
    fn a_privacy_is_one_of_three_values() {
        for allowed in PRIVACIES {
            assert_eq!(
                require_privacy("privacy", allowed).expect("an allowed value"),
                allowed
            );
        }
        assert_eq!(
            require_privacy("privacy", " public ").expect("trimmed"),
            "public"
        );
        assert!(require_privacy("privacy", "world").is_err());
        assert!(require_privacy("privacy", "").is_err());
    }

    #[test]
    fn a_video_id_is_base64url_and_bounded() {
        assert_eq!(
            require_video_id(" dQw4w9WgXcQ ").expect("trimmed"),
            "dQw4w9WgXcQ"
        );
        assert!(require_video_id("").is_err());
        assert!(require_video_id("has space").is_err());
        assert!(require_video_id(&"x".repeat(VIDEO_ID_MAX + 1)).is_err());
    }

    #[test]
    fn an_upload_title_description_and_tags_are_bounded() {
        assert_eq!(require_title("  A video  ").expect("trimmed"), "A video");
        assert!(require_title("   ").is_err());
        assert!(require_title(&"x".repeat(TITLE_MAX)).is_ok());
        assert!(require_title(&"x".repeat(TITLE_MAX + 1)).is_err());

        assert!(require_description(&"x".repeat(DESCRIPTION_MAX)).is_ok());
        assert!(require_description(&"x".repeat(DESCRIPTION_MAX + 1)).is_err());

        assert_eq!(
            require_tags(vec![" a ".into(), "b".into()]).expect("trimmed"),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert!(require_tags(vec!["".into()]).is_err());
        assert!(require_tags(vec!["x".repeat(TAG_MAX + 1)]).is_err());
        assert!(require_tags(vec!["x".to_owned(); TAGS_MAX + 1]).is_err());
    }

    #[test]
    fn every_input_schema_is_closed() {
        for name in [CLIENT_SET, CONNECT, UPLOAD, PRIVACY, RETRACT] {
            let spec = crate::registry::registry()
                .get(name)
                .unwrap_or_else(|| panic!("{name} is not registered"));
            let schema = spec.input_schema();
            assert_eq!(
                schema["additionalProperties"],
                serde_json::json!(false),
                "{name}: the input schema must refuse unknown fields"
            );
        }
    }

    #[test]
    fn the_write_actions_require_capabilities_write() {
        for name in [CLIENT_SET, CONNECT, DISCONNECT, UPLOAD, PRIVACY, RETRACT] {
            let spec = crate::registry::registry()
                .get(name)
                .unwrap_or_else(|| panic!("{name} is not registered"));
            assert_eq!(spec.scope_names(), vec!["capabilities:write"], "{name}");
            assert!(!spec.permits(conveyor_auth::Role::Viewer), "{name}");
        }
        for name in [CLIENT_GET, STATUS] {
            let spec = crate::registry::registry()
                .get(name)
                .unwrap_or_else(|| panic!("{name} is not registered"));
            assert_eq!(spec.scope_names(), vec!["capabilities:read"], "{name}");
            assert!(spec.permits(conveyor_auth::Role::Viewer), "{name}");
        }
    }
}
