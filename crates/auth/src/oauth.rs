//! Client-credentials tokens: the ONE service-to-service credential this build holds.
//!
//! The conveyor calls three services as itself, never as a user:
//!
//! * **provisioning** (`POST /api/provision`) — realm role `provision-caller`;
//! * **licences** (`GET /licenses/active`) — `license-reader`;
//! * **billing** (`GET /api/costs/tenant/{id}/actors`) — `billing-internal`.
//!
//! All three accept a Keycloak **client** token from realm `secure-apps`, so one credential
//! (`conveyor-backend`) serves all of them, refreshed from one endpoint. The realm-conveyor
//! admin client is a DIFFERENT audience (it administers users in the `conveyor` realm) and is
//! built the same way with a different token URL.
//!
//! The token is cached until `expires_in - [`REFRESH_SKEW`], with a floor so a token that is
//! already near expiry cannot pin a dead credential: a failed refresh is an error to the
//! caller, never a stale token served as fresh. There is no "allow on error" arm — a service
//! call without a credential would be an anonymous call, which these services refuse (401),
//! so the honest outcome is the same refusal earlier and with a better message.
//!
//! `Debug` is implemented by hand: the secret never reaches a log line.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

/// Refresh this long before the endpoint says the token expires.
pub const REFRESH_SKEW: Duration = Duration::from_secs(60);

/// The token request's timeout.
pub const TOKEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Why no token could be produced.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TokenError {
    /// A required setting is absent or empty. The message names the setting.
    #[error("{0} is not configured")]
    Unconfigured(&'static str),
    /// The token endpoint answered a non-success status. The body is deliberately not
    /// included: Keycloak echoes the client id, and an internal error can echo more.
    #[error("{client_id}: token endpoint answered {status}")]
    Refused {
        /// The client the request was made as.
        client_id: String,
        /// The HTTP status.
        status: u16,
    },
    /// The endpoint could not be reached.
    #[error("{client_id}: token endpoint unreachable: {error}")]
    Transport {
        /// The client the request was made as.
        client_id: String,
        /// The transport error, already rendered.
        error: String,
    },
    /// The endpoint answered, but not with a usable token document.
    #[error("{client_id}: token response: {error}")]
    Body {
        /// The client the request was made as.
        client_id: String,
        /// What was wrong with the body.
        error: String,
    },
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Clone)]
struct Cached {
    token: String,
    valid_until: Instant,
}

/// One Keycloak client, with its token cached for the life of the process.
#[derive(Clone)]
pub struct ClientCredentials {
    client_id: String,
    client_secret: String,
    token_url: String,
    http: reqwest::Client,
    cached: Arc<Mutex<Option<Cached>>>,
}

impl fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // ⛔ Never print the secret.
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .field("token_url", &self.token_url)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

impl ClientCredentials {
    /// Build for one client.
    ///
    /// Refuses an empty setting up front: a client that cannot possibly authenticate should
    /// fail at construction, where the operator sees the setting's name, not on the first
    /// tenant creation.
    pub fn new(token_url: &str, client_id: &str, client_secret: &str) -> Result<Self, TokenError> {
        let token_url = token_url.trim();
        let client_id = client_id.trim();
        if token_url.is_empty() {
            return Err(TokenError::Unconfigured("SERVICE_TOKEN_URL"));
        }
        if client_id.is_empty() {
            return Err(TokenError::Unconfigured("SERVICE_CLIENT_ID"));
        }
        if client_secret.trim().is_empty() {
            return Err(TokenError::Unconfigured("SERVICE_CLIENT_SECRET"));
        }
        if !(token_url.starts_with("http://") || token_url.starts_with("https://")) {
            return Err(TokenError::Unconfigured(
                "SERVICE_TOKEN_URL (not an http(s) URL)",
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(TOKEN_TIMEOUT)
            .build()
            .map_err(|error| TokenError::Transport {
                client_id: client_id.to_owned(),
                error: error.to_string(),
            })?;
        Ok(Self {
            client_id: client_id.to_owned(),
            client_secret: client_secret.trim().to_owned(),
            token_url: token_url.to_owned(),
            http,
            cached: Arc::new(Mutex::new(None)),
        })
    }

    /// The client id this credential authenticates as.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// A live access token, minting one only when the cached one is inside the skew.
    pub async fn token(&self) -> Result<String, TokenError> {
        let mut cached = self.cached.lock().await;
        if let Some(entry) = cached.as_ref()
            && Instant::now() < entry.valid_until
        {
            return Ok(entry.token.clone());
        }
        let minted = self.mint().await?;
        *cached = Some(minted.clone());
        Ok(minted.token)
    }

    /// `Authorization: Bearer …`.
    pub async fn authorization(&self) -> Result<String, TokenError> {
        Ok(format!("Bearer {}", self.token().await?))
    }

    async fn mint(&self) -> Result<Cached, TokenError> {
        // Hand-encoded rather than `.form()`: the workspace's reqwest does not enable the
        // `form` feature, and a percent-encoder is smaller than the dependency change.
        let body = form_body(&[
            ("grant_type", "client_credentials"),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ]);
        let response = self
            .http
            .post(&self.token_url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|error| TokenError::Transport {
                client_id: self.client_id.clone(),
                error: error.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(TokenError::Refused {
                client_id: self.client_id.clone(),
                status: status.as_u16(),
            });
        }
        let body: TokenBody = response.json().await.map_err(|error| TokenError::Body {
            client_id: self.client_id.clone(),
            error: error.to_string(),
        })?;
        if body.access_token.trim().is_empty() {
            return Err(TokenError::Body {
                client_id: self.client_id.clone(),
                error: "access_token is empty".into(),
            });
        }
        let lifetime = body.expires_in.unwrap_or(60);
        // Keep at least one second so a short-lived token is still usable once; the skew is
        // capped at half the lifetime so a 30-second token is not discarded on arrival.
        let skew = REFRESH_SKEW.as_secs().min(lifetime / 2);
        let ttl = lifetime.saturating_sub(skew).max(1);
        Ok(Cached {
            token: body.access_token,
            valid_until: Instant::now() + Duration::from_secs(ttl),
        })
    }
}

/// `application/x-www-form-urlencoded`, per WHATWG: unreserved bytes pass, space is `+`,
/// everything else is `%XX`. A secret is not guaranteed to be alphanumeric, so this is not
/// a cosmetic function.
fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        encode_into(&mut out, key);
        out.push('=');
        encode_into(&mut out, value);
    }
    out
}

fn encode_into(out: &mut String, value: &str) {
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_body_percent_encodes_reserved_bytes() {
        assert_eq!(
            form_body(&[
                ("grant_type", "client_credentials"),
                ("client_id", "conveyor-backend"),
                ("client_secret", "a b+c&d"),
            ]),
            "grant_type=client_credentials&client_id=conveyor-backend&client_secret=a+b%2Bc%26d"
        );
    }

    #[test]
    fn an_empty_setting_is_refused_and_named() {
        let error = ClientCredentials::new("", "id", "secret").unwrap_err();
        assert_eq!(error, TokenError::Unconfigured("SERVICE_TOKEN_URL"));
        let error = ClientCredentials::new("http://x/token", "", "secret").unwrap_err();
        assert_eq!(error, TokenError::Unconfigured("SERVICE_CLIENT_ID"));
        let error = ClientCredentials::new("http://x/token", "id", "  ").unwrap_err();
        assert_eq!(error, TokenError::Unconfigured("SERVICE_CLIENT_SECRET"));
        let error = ClientCredentials::new("x/token", "id", "secret").unwrap_err();
        assert_eq!(
            error,
            TokenError::Unconfigured("SERVICE_TOKEN_URL (not an http(s) URL)")
        );
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let client =
            ClientCredentials::new("http://x/token", "id", "super-secret-value").expect("client");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("super-secret-value"));
        assert!(rendered.contains("<redacted>"));
    }
}
