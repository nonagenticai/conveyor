#![forbid(unsafe_code)]
//! Google's OAuth 2.0 token endpoint and the three YouTube Data API calls, behind
//! [`YouTubeProvider`] (#182 slices A and B).
//!
//! This is a **transport**: the actions in `crate::youtube` and `apps/server`'s worker depend on
//! the trait, and only this file names `reqwest`. It is carried in
//! `crates/api/tests/externals_declared.rs`'s allowlist by file name, the same way `github.rs` and
//! `strata.rs` are.
//!
//! Five calls:
//!
//! ```text
//! POST /token  grant_type=authorization_code  code, client_id, client_secret, redirect_uri
//! POST /token  grant_type=refresh_token       refresh_token, client_id, client_secret
//! POST /upload/youtube/v3/videos?uploadType=resumable&part=snippet,status   -> Location
//! PUT  <Location>                            the video bytes
//! PUT  /youtube/v3/videos?part=status        {"id": …, "status": {"privacyStatus": …}}
//! DELETE /youtube/v3/videos?id=…
//! ```
//!
//! # What is never done
//!
//! - **No body is read on a non-2xx.** Google's error body is untrusted text, and a problem
//!   document would render it; the status is the whole of what escapes.
//! - **No request or response is logged.** The form carries the client secret and the code; a
//!   `tracing::debug!` of either would put a credential in the log pipeline.
//! - **No retry.** The engine's deadline is the caller's, and an exchange with a single-use code
//!   must never be replayed by a transport.
//!
//! # Retract is idempotent at the transport
//!
//! `videos.delete` answers 404 for a video the account does not have, which is exactly the state
//! the caller asked for. [`YouTubeProvider::retract`] maps that one status to success; every other
//! non-2xx is [`YouTubeError::Refused`]. The resumable upload's Location is a provider-supplied
//! URL and is followed with the same bearer; it is never logged.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Duration as ClockDuration, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::youtube::{ProviderFuture, TokenSet, UploadRequest, YouTubeError, YouTubeProvider};

/// Google's token endpoint.
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// The Data API's video collection.
pub const DEFAULT_VIDEOS_URL: &str = "https://www.googleapis.com/youtube/v3/videos";

/// The Data API's resumable-upload door.
pub const DEFAULT_UPLOAD_URL: &str = "https://www.googleapis.com/upload/youtube/v3/videos";

/// The per-call timeout for the token and small metadata calls. The endpoints answer in well under
/// a second; a hung Google must not hold the caller's transaction for the action's whole budget.
pub const DEFAULT_TIMEOUT_MS: u64 = 15_000;

/// The upload's own timeout: a 10 MiB document over a slow uplink is still minutes, not seconds.
pub const UPLOAD_TIMEOUT_MS: u64 = 600_000;

/// The access token's lifetime Google falls back to when `expires_in` is absent.
const DEFAULT_EXPIRES_IN_SECONDS: i64 = 3_600;

/// The longest lifetime this client will believe; a nonsense value must not overflow the clock.
const MAX_EXPIRES_IN_SECONDS: i64 = 86_400;

/// The category `videos.insert` requires and the reference defaults to (People & Blogs).
const DEFAULT_CATEGORY_ID: &str = "22";

/// The production provider.
///
/// Zero-sized on purpose: the HTTP client is process-wide (one connection pool, built once), so a
/// handler can name a `static` without a lazy-constructor dance.
#[derive(Clone, Copy, Debug, Default)]
pub struct GoogleTokenProvider;

impl GoogleTokenProvider {
    /// A provider handle.
    pub fn new() -> Self {
        Self
    }
}

/// The process's short-timeout HTTP client (token and small metadata calls). A builder failure
/// (no usable TLS backend) is `Unavailable` rather than a panic at first use.
fn http() -> Result<&'static Client, YouTubeError> {
    static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                .build()
                .ok()
        })
        .as_ref()
        .ok_or(YouTubeError::Unavailable)
}

/// The process's upload client: same one-pool discipline, one long timeout.
fn upload_http() -> Result<&'static Client, YouTubeError> {
    static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_millis(UPLOAD_TIMEOUT_MS))
                .build()
                .ok()
        })
        .as_ref()
        .ok_or(YouTubeError::Unavailable)
}

/// The fields this client reads out of a token response.
///
/// `#[serde(deny_unknown_fields)]` is deliberately absent: Google adds fields over time, and an
/// unknown member is not a protocol failure.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl TokenResponse {
    fn into_token_set(self) -> TokenSet {
        let seconds = self
            .expires_in
            .unwrap_or(DEFAULT_EXPIRES_IN_SECONDS)
            .clamp(1, MAX_EXPIRES_IN_SECONDS);
        TokenSet {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: Utc::now() + ClockDuration::seconds(seconds),
        }
    }
}

/// Post one form to the token endpoint.
async fn post(form: &[(&str, &str)]) -> Result<TokenSet, YouTubeError> {
    let response = http()?
        .post(DEFAULT_TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|_| YouTubeError::Unavailable)?;
    let status = response.status();
    if !status.is_success() {
        // The body is not read: it is untrusted text and this enum's Display reaches a response.
        return Err(YouTubeError::Refused {
            status: status.as_u16(),
        });
    }
    let body: TokenResponse = response
        .json()
        .await
        .map_err(|_| YouTubeError::InvalidResponse)?;
    Ok(body.into_token_set())
}

impl YouTubeProvider for GoogleTokenProvider {
    fn exchange_code<'a>(
        &'a self,
        client_id: &'a str,
        client_secret: &'a str,
        code: &'a str,
        redirect_uri: &'a str,
    ) -> ProviderFuture<'a, TokenSet> {
        Box::pin(async move {
            post(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("redirect_uri", redirect_uri),
            ])
            .await
        })
    }

    fn refresh<'a>(
        &'a self,
        client_id: &'a str,
        client_secret: &'a str,
        refresh_token: &'a str,
    ) -> ProviderFuture<'a, TokenSet> {
        Box::pin(async move {
            post(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .await
        })
    }

    fn upload<'a>(
        &'a self,
        access_token: &'a str,
        request: UploadRequest,
    ) -> ProviderFuture<'a, String> {
        Box::pin(async move {
            let content_length = request.bytes.len().to_string();
            let metadata = insert_metadata(&request);

            // 1. Initiate a resumable session. Its answer carries the URL to PUT the bytes to.
            let initiated = upload_http()?
                .post(DEFAULT_UPLOAD_URL)
                .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
                .bearer_auth(access_token)
                .header("X-Upload-Content-Type", &request.content_type)
                .header("X-Upload-Content-Length", &content_length)
                .json(&metadata)
                .send()
                .await
                .map_err(|_| YouTubeError::Unavailable)?;
            let status = initiated.status();
            if !status.is_success() {
                // The body is not read: untrusted text that this enum's Display reaches.
                return Err(YouTubeError::Refused {
                    status: status.as_u16(),
                });
            }
            let location = initiated
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
                .ok_or(YouTubeError::InvalidResponse)?;

            // 2. One PUT for the whole document. No retry: a replay is the caller's decision.
            let uploaded = upload_http()?
                .put(&location)
                .bearer_auth(access_token)
                .header(reqwest::header::CONTENT_TYPE, &request.content_type)
                .body(request.bytes)
                .send()
                .await
                .map_err(|_| YouTubeError::Unavailable)?;
            let status = uploaded.status();
            if !status.is_success() {
                return Err(YouTubeError::Refused {
                    status: status.as_u16(),
                });
            }
            let body: VideoResponse = uploaded
                .json()
                .await
                .map_err(|_| YouTubeError::InvalidResponse)?;
            if body.id.is_empty() {
                return Err(YouTubeError::InvalidResponse);
            }
            Ok(body.id)
        })
    }

    fn set_privacy<'a>(
        &'a self,
        access_token: &'a str,
        video_id: &'a str,
        privacy: &'a str,
    ) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            let response = http()?
                .put(DEFAULT_VIDEOS_URL)
                .query(&[("part", "status")])
                .bearer_auth(access_token)
                .json(&json!({"id": video_id, "status": {"privacyStatus": privacy}}))
                .send()
                .await
                .map_err(|_| YouTubeError::Unavailable)?;
            let status = response.status();
            if !status.is_success() {
                return Err(YouTubeError::Refused {
                    status: status.as_u16(),
                });
            }
            Ok(())
        })
    }

    fn retract<'a>(&'a self, access_token: &'a str, video_id: &'a str) -> ProviderFuture<'a, ()> {
        Box::pin(async move {
            let response = http()?
                .delete(DEFAULT_VIDEOS_URL)
                .query(&[("id", video_id)])
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|_| YouTubeError::Unavailable)?;
            let status = response.status();
            // 404 is the idempotent case: the video is already absent, which is the requested end
            // state. Everything else non-2xx is a refusal the caller must see.
            if status == reqwest::StatusCode::NOT_FOUND || status.is_success() {
                return Ok(());
            }
            Err(YouTubeError::Refused {
                status: status.as_u16(),
            })
        })
    }
}

/// What `videos.insert` returns that this client reads. The resource carries many more fields;
/// `deny_unknown_fields` is deliberately absent.
#[derive(Deserialize)]
struct VideoResponse {
    id: String,
}

/// The `videos.insert` metadata body. `part=snippet,status` is the only pair sent, so the body
/// carries exactly those two sections. `categoryId` is required by the API and is the reference's
/// default.
fn insert_metadata(request: &UploadRequest) -> Value {
    let mut snippet = json!({
        "title": request.title,
        "categoryId": DEFAULT_CATEGORY_ID,
    });
    if let Some(description) = &request.description
        && !description.is_empty()
    {
        snippet["description"] = json!(description);
    }
    if !request.tags.is_empty() {
        snippet["tags"] = json!(request.tags);
    }
    json!({
        "snippet": snippet,
        "status": { "privacyStatus": request.privacy },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_response_becomes_a_token_set_with_a_bounded_expiry() {
        let now = Utc::now();
        let set = TokenResponse {
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            expires_in: Some(10),
        }
        .into_token_set();
        assert_eq!(set.access_token, "access");
        assert_eq!(set.refresh_token.as_deref(), Some("refresh"));
        let seconds = (set.expires_at - now).num_seconds();
        assert!((9..=11).contains(&seconds), "{seconds}");

        // Absent and nonsense lifetimes still yield a future expiry inside the ceiling.
        for expires_in in [None, Some(-5), Some(i64::MAX)] {
            let set = TokenResponse {
                access_token: "access".into(),
                refresh_token: None,
                expires_in,
            }
            .into_token_set();
            let seconds = (set.expires_at - now).num_seconds();
            assert!(
                (1..=MAX_EXPIRES_IN_SECONDS + 1).contains(&seconds),
                "{expires_in:?} produced {seconds}"
            );
        }
    }

    #[test]
    fn the_provider_is_the_actions_contract_and_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn YouTubeProvider>();
        assert_send_sync::<GoogleTokenProvider>();
    }

    fn request() -> UploadRequest {
        UploadRequest {
            title: "A parity video".into(),
            description: None,
            privacy: "private".into(),
            tags: Vec::new(),
            content_type: "video/mp4".into(),
            bytes: vec![0_u8; 1024],
        }
    }

    #[test]
    fn the_insert_metadata_names_only_snippet_and_status() {
        let metadata = insert_metadata(&request());
        assert_eq!(metadata["snippet"]["title"], "A parity video");
        assert_eq!(metadata["snippet"]["categoryId"], "22");
        assert!(metadata["snippet"].get("description").is_none());
        assert!(metadata["snippet"].get("tags").is_none());
        assert_eq!(metadata["status"]["privacyStatus"], "private");
        assert_eq!(metadata.as_object().expect("an object").len(), 2);

        let full = UploadRequest {
            description: Some("What it shows".into()),
            tags: vec!["parity".into()],
            ..request()
        };
        let metadata = insert_metadata(&full);
        assert_eq!(metadata["snippet"]["description"], "What it shows");
        assert_eq!(metadata["snippet"]["tags"][0], "parity");
    }

    #[test]
    fn an_upload_request_does_not_print_its_bytes() {
        let text = format!("{:?}", request());
        assert!(text.contains("1024 bytes"), "{text}");
        assert!(!text.contains("0, 0, 0"), "the bytes reached Debug: {text}");
    }
}
