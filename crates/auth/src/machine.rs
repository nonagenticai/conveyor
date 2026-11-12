#![forbid(unsafe_code)]
//! Machine authentication: an opaque licence bearer resolves to a tenant, an app, and a
//! subject.
//!
//! CONTRACT §9: "licence tokens are non-JWT; resolution is fail-closed with a short-TTL
//! in-process cache. A JWT-shaped token that fails verification is **never** retried as a
//! licence token."
//!
//! The token cannot be verified locally. `licenses` mints `secrets.token_urlsafe(32)` and
//! stores only its SHA-256 hash (`licenses/models.py:3550-3556`), so there is no signature,
//! no issuer and no public key — the only way to resolve it is
//! **`POST /license/introspect`** (`licenses/main.py:3700-3721`), which answers `active`,
//! `tenant_id`, `app.key`, the subject fields and the token's expiry. Three services in the
//! estate already do exactly this on a request path: `fr` (`licence-introspect.ts`, 60 s
//! cache), `requirements_management` (`fleet_auth.py`, 5 min) and `license-authz`.
//!
//! What that costs is stated rather than hidden: an agent cannot authenticate while the
//! licences service is unreachable. It is reported as `503` / `LicenceUnavailable` — never as
//! a refusal, and never as an admission — and it touches no human login path.
//!
//! The cache is keyed by the **token's hash**, never by tenant: a revoked token therefore
//! cannot ride an entry earned by a different token of the same tenant, and a reused entry
//! always proves the same token.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::AuthError;

/// The longest a verified token is trusted. Matches `fr`'s 60 s.
pub const CACHE_TTL: Duration = Duration::from_secs(60);

/// The introspection HTTP timeout. Short: it is on a request path.
pub const INTROSPECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The cache's hard ceiling, so a burst of distinct tokens cannot grow it without bound.
pub const MAX_CACHE_ENTRIES: usize = 4096;

/// The app key this build answers for, when the caller does not supply one.
pub const DEFAULT_APP_KEY: &str = "conveyor";

/// What a verified licence proves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineIdentity {
    /// The tenant the licence names. This — not a header — is the tenant the request acts in.
    pub tenant_id: TenantId,
    /// The application the licence was minted for.
    pub app_key: String,
    /// `app_user.oidc_subject` for this machine principal (`licence:<subject>`).
    pub subject: String,
    /// The token's own expiry, when the service reports one.
    pub token_expires_at: Option<DateTime<Utc>>,
}

/// True for a token with the three-segment `<header>.<payload>.<signature>` shape.
///
/// This is the ONLY gate between the two credential kinds, and it is deliberately about
/// shape rather than validity: a JWT-shaped token that fails verification must never reach
/// licence introspection, or a revoked credential could be retried as a valid licence.
pub fn is_jwt_shaped(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// The subset of `TokenIntrospectResponse` this build uses (`licenses/main.py:2966-3053`).
#[derive(Debug, Deserialize)]
struct Introspection {
    #[serde(default)]
    active: bool,
    tenant_id: Option<String>,
    app: Option<AppRef>,
    subject_id: Option<String>,
    subject_actor_sub: Option<String>,
    subject_keycloak_sub: Option<String>,
    token_expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppRef {
    key: Option<String>,
}

/// One cached verification.
#[derive(Clone)]
struct Entry {
    identity: MachineIdentity,
    verified_at: Instant,
    ttl: Duration,
}

/// Resolves licence bearers against the licences service. Built once per process.
pub struct MachineAuthenticator {
    introspect_url: String,
    app_key: String,
    http: reqwest::Client,
    cache: Mutex<HashMap<[u8; 32], Entry>>,
}

impl MachineAuthenticator {
    /// Build for `base_url` (for example `http://licences.licences.svc.cluster.local:8003`)
    /// and the app key this build answers for.
    pub fn new(base_url: &str, app_key: &str) -> Result<Self, AuthError> {
        let base = base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return Err(AuthError::Config("licences base URL is empty".into()));
        }
        let http = reqwest::Client::builder()
            .timeout(INTROSPECT_TIMEOUT)
            .build()
            .map_err(|e| AuthError::Config(format!("licences client: {e}")))?;
        let app_key = if app_key.trim().is_empty() {
            DEFAULT_APP_KEY.to_owned()
        } else {
            app_key.trim().to_owned()
        };
        Ok(Self {
            introspect_url: format!("{base}/license/introspect"),
            app_key,
            http,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// The app key this authenticator accepts.
    pub fn app_key(&self) -> &str {
        &self.app_key
    }

    /// Verify a presented licence token.
    ///
    /// `Ok` means the service said the token is active, it names this app, it resolves to a
    /// tenant and a subject. Everything else is an error: an inactive token, another app's
    /// token and a subject-less token are `InvalidToken` (401); an unreachable or malformed
    /// answer is `LicenceUnavailable` (503).
    pub async fn identify(&self, token: &str) -> Result<MachineIdentity, AuthError> {
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        if let Some(entry) = self.cached(&hash) {
            return Ok(entry);
        }

        let response = self
            .http
            .post(&self.introspect_url)
            .json(&serde_json::json!({ "token": token }))
            .send()
            .await
            .map_err(|e| AuthError::LicenceUnavailable(e.to_string()))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(AuthError::LicenceUnavailable(format!(
                "introspect answered {}",
                response.status()
            )));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|e| AuthError::LicenceUnavailable(format!("introspect body: {e}")))?;
        let identity = self.parse(&body)?;

        let ttl = cache_ttl(identity.token_expires_at.as_ref());
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= MAX_CACHE_ENTRIES {
                let now = Instant::now();
                cache.retain(|_, entry| now.duration_since(entry.verified_at) < entry.ttl);
                if cache.len() >= MAX_CACHE_ENTRIES {
                    cache.clear();
                }
            }
            cache.insert(
                hash,
                Entry {
                    identity: identity.clone(),
                    verified_at: Instant::now(),
                    ttl,
                },
            );
        }
        Ok(identity)
    }

    fn cached(&self, hash: &[u8; 32]) -> Option<MachineIdentity> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(hash)?;
        if entry.verified_at.elapsed() < entry.ttl {
            Some(entry.identity.clone())
        } else {
            None
        }
    }

    /// Turn an introspection body into an identity, refusing everything not proven.
    fn parse(&self, body: &Value) -> Result<MachineIdentity, AuthError> {
        let introspection: Introspection = serde_json::from_value(body.clone())
            .map_err(|e| AuthError::LicenceUnavailable(format!("introspect shape: {e}")))?;
        if !introspection.active {
            return Err(AuthError::InvalidToken);
        }
        let app_key = introspection
            .app
            .as_ref()
            .and_then(|app| app.key.clone())
            .filter(|key| !key.trim().is_empty())
            .ok_or(AuthError::InvalidToken)?;
        if app_key != self.app_key {
            // A valid licence for another application is not a credential for this one.
            return Err(AuthError::InvalidToken);
        }
        let tenant_id = introspection
            .tenant_id
            .as_deref()
            .and_then(|raw| TenantId::parse(raw).ok())
            .ok_or(AuthError::InvalidToken)?;
        let subject = subject_of(&introspection).ok_or(AuthError::InvalidToken)?;
        let token_expires_at = introspection
            .token_expires_at
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|at| at.with_timezone(&Utc));

        Ok(MachineIdentity {
            tenant_id,
            app_key,
            subject,
            token_expires_at,
        })
    }
}

/// The `app_user.oidc_subject` a licence resolves to.
///
/// `subject_id` is the licence's own subject (a stable id per seat) and is the identity we
/// provision. The two `sub` fallbacks exist because older mints carry only those; a licence
/// with none of the three proves no subject and is refused rather than collapsed into a
/// shared principal.
fn subject_of(introspection: &Introspection) -> Option<String> {
    let raw = introspection
        .subject_id
        .as_deref()
        .or(introspection.subject_actor_sub.as_deref())
        .or(introspection.subject_keycloak_sub.as_deref())?
        .trim();
    if raw.is_empty() {
        return None;
    }
    Some(format!("licence:{raw}"))
}

/// `CACHE_TTL`, shortened when the token itself expires sooner.
fn cache_ttl(expires_at: Option<&DateTime<Utc>>) -> Duration {
    let Some(expires_at) = expires_at else {
        return CACHE_TTL;
    };
    let now = Utc::now();
    if *expires_at <= now {
        return Duration::ZERO;
    }
    let remaining = (*expires_at - now).to_std().unwrap_or(Duration::ZERO);
    remaining.min(CACHE_TTL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authenticator() -> MachineAuthenticator {
        MachineAuthenticator::new("http://licences.test:8003/", "conveyor").unwrap()
    }

    fn body(active: bool, app: &str, subject: &str) -> Value {
        serde_json::json!({
            "active": active,
            "tenant_id": "018f4d2e-0000-7000-8000-000000000001",
            "app": {"key": app},
            "subject_id": subject,
            "token_expires_at": "2099-01-01T00:00:00Z"
        })
    }

    #[test]
    fn jwt_shape_is_three_base64url_segments() {
        assert!(is_jwt_shaped("aaa.bbb.ccc"));
        assert!(is_jwt_shaped("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ4In0.c2ln"));
        // An opaque licence token is one segment.
        assert!(!is_jwt_shaped("nJ9kQ2x3Wv"));
        assert!(!is_jwt_shaped("aaa.bbb"));
        assert!(!is_jwt_shaped("aaa..ccc"));
        assert!(!is_jwt_shaped("aaa.bbb.ccc."));
        assert!(!is_jwt_shaped("aaa.b$b.ccc"));
        assert!(!is_jwt_shaped(""));
    }

    #[test]
    fn an_active_token_for_this_app_resolves_to_the_licence_subject() {
        let identity = authenticator()
            .parse(&body(true, "conveyor", "sub-1"))
            .unwrap();
        assert_eq!(identity.app_key, "conveyor");
        assert_eq!(identity.subject, "licence:sub-1");
        assert_eq!(
            identity.tenant_id,
            TenantId::parse("018f4d2e-0000-7000-8000-000000000001").unwrap()
        );
    }

    #[test]
    fn inactive_another_app_and_no_subject_are_refused() {
        for body in [
            body(false, "conveyor", "sub-1"),
            body(true, "some-other-app", "sub-1"),
            body(true, "conveyor", ""),
        ] {
            assert!(
                matches!(authenticator().parse(&body), Err(AuthError::InvalidToken)),
                "{body}"
            );
        }
    }

    #[test]
    fn a_malformed_body_is_unavailable_never_an_admission() {
        let err = authenticator()
            .parse(&serde_json::json!({"active": "yes"}))
            .unwrap_err();
        assert!(matches!(err, AuthError::LicenceUnavailable(_)), "{err}");
        assert_eq!(err.code(), "service_unavailable");
    }

    #[test]
    fn cache_ttl_is_capped_by_the_token_expiry() {
        let now = Utc::now();
        assert_eq!(cache_ttl(None), CACHE_TTL);
        let soon = cache_ttl(Some(&(now + chrono::Duration::seconds(5))));
        assert!(soon <= Duration::from_secs(5), "{soon:?}");
        let far = cache_ttl(Some(&(now + chrono::Duration::hours(1))));
        assert!(far <= CACHE_TTL, "{far:?}");
        assert_eq!(
            cache_ttl(Some(&(now - chrono::Duration::seconds(1)))),
            Duration::ZERO
        );
    }

    #[test]
    fn an_empty_base_url_is_a_configuration_error() {
        assert!(matches!(
            MachineAuthenticator::new("  ", "conveyor"),
            Err(AuthError::Config(_))
        ));
    }
}
