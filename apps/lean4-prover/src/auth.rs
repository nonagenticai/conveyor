//! Licence-token authentication.
//!
//! Owner ruling: every call to a compute backend must carry a licence,
//! enforced. The credential is `X-Tenant-Id: Bearer <licence-token>` (the fleet
//! convention) or `Authorization: Bearer <licence-token>`. The token is
//! introspected against `LICENSE_INTROSPECT_URL`; **the tenant comes from the
//! licence, never from a header** — a bare `X-Tenant-Id` is never identity.
//!
//! This replaces `w2v_common.auth.AuthMiddleware` for this service. The
//! behaviour deliberately kept is the part that prevents incidents:
//!
//! * a positive cache (60 s) so a busy caller does not introspect per request;
//! * a negative cache (60 s) so a revocation storm does not hammer licences;
//! * **stale-while-error** (15 min) — a transient licences outage must not
//!   401 a token that was valid moments ago;
//! * per-token single-flight, so a cold burst collapses into one POST;
//! * fail closed: no cache, no answer, no request.
//!
//! The credential is a licence token, not a Keycloak JWT, so there is no JWT
//! lane and no trusted-network lane. Both were removed from this service on
//! 2026-10-06 and must not come back.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::error::AppError;
use crate::state::AppState;

/// Paths that never require a licence. This is the fleet `PUBLIC_ENDPOINTS`
/// list: health probes and docs must answer to Kubernetes and to a human, and
/// `/metrics` must be scrapeable by Prometheus without a token.
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/",
    "/health",
    "/health/ready",
    "/health/live",
    "/healthz",
    "/ready",
    "/readyz",
    "/live",
    "/livez",
    "/docs",
    "/redoc",
    "/openapi.json",
    "/api/health",
    "/api/health/ready",
    "/api/health/live",
    "/favicon.ico",
    "/metrics",
];

/// The compute paths. Kept explicit for the OpenAPI document and for the
/// attribution log line; enforcement covers every non-public path.
pub const PROTECTED_PREFIXES: &[&str] = &["/api/check", "/api/elaborate", "/api/prove"];

const POSITIVE_TTL: Duration = Duration::from_secs(60);
const NEGATIVE_TTL: Duration = Duration::from_secs(60);
const STALE_TTL: Duration = Duration::from_secs(900);

#[derive(Debug, Clone)]
pub struct Identity {
    pub tenant_id: Option<String>,
    pub license_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenceError {
    /// The licences service answered, and the answer was no.
    Inactive,
    /// The licences service could not be reached and there is no
    /// last-known-good answer to serve.
    Unavailable,
}

#[derive(Debug, Clone)]
struct Cached {
    expires: Instant,
    payload: Value,
}

pub struct LicenceClient {
    http: reqwest::Client,
    url: String,
    positive: tokio::sync::Mutex<HashMap<String, Cached>>,
    negative: tokio::sync::Mutex<HashMap<String, Instant>>,
    stale: tokio::sync::Mutex<HashMap<String, Cached>>,
    inflight: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl std::fmt::Debug for LicenceClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LicenceClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl LicenceClient {
    pub fn new(url: String, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            http,
            url,
            positive: Default::default(),
            negative: Default::default(),
            stale: Default::default(),
            inflight: Default::default(),
        })
    }

    /// Validate a token. Returns the introspection payload on success.
    pub async fn introspect(&self, token: &str) -> Result<Value, LicenceError> {
        let key = digest(token);

        if let Some(payload) = self.cached_positive(&key).await {
            return Ok(payload);
        }
        if self.cached_negative(&key).await {
            return Err(LicenceError::Inactive);
        }

        // Single-flight per token: a cold burst for one token must produce one
        // upstream POST, not one per request.
        let lock = {
            let mut inflight = self.inflight.lock().await;
            Arc::clone(
                inflight
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let _guard = lock.lock().await;

        // A prior waiter may have populated the caches while we blocked.
        if let Some(payload) = self.cached_positive(&key).await {
            return Ok(payload);
        }
        if self.cached_negative(&key).await {
            return Err(LicenceError::Inactive);
        }

        let attempted = self
            .http
            .post(&self.url)
            .json(&serde_json::json!({ "token": token }))
            .send()
            .await;

        match attempted {
            Ok(response) if response.status().is_success() => {
                match response.json::<Value>().await {
                    Ok(payload) if payload.get("active") == Some(&Value::Bool(true)) => {
                        let now = Instant::now();
                        self.positive.lock().await.insert(
                            key.clone(),
                            Cached {
                                expires: now + POSITIVE_TTL,
                                payload: payload.clone(),
                            },
                        );
                        self.stale.lock().await.insert(
                            key.clone(),
                            Cached {
                                expires: now + STALE_TTL,
                                payload: payload.clone(),
                            },
                        );
                        self.negative.lock().await.remove(&key);
                        Ok(payload)
                    }
                    Ok(_) => {
                        // A definitive 200 + active:false is authoritative: it
                        // is never served from the stale cache.
                        self.negative
                            .lock()
                            .await
                            .insert(key.clone(), Instant::now() + NEGATIVE_TTL);
                        Err(LicenceError::Inactive)
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "licence introspection body was not JSON");
                        self.fallback(&key).await
                    }
                }
            }
            Ok(response) => {
                tracing::warn!(status = %response.status(), "licence introspection answered non-2xx");
                self.fallback(&key).await
            }
            Err(err) => {
                tracing::warn!(error = %err, "licence introspection failed");
                self.fallback(&key).await
            }
        }
    }

    /// Serve the last-known-good answer for a token, if one is still fresh
    /// enough. A cold cache returns `Unavailable`, which is a 503 — never a
    /// silent allow.
    async fn fallback(&self, key: &str) -> Result<Value, LicenceError> {
        let stale = self.stale.lock().await;
        match stale.get(key) {
            Some(entry) if entry.expires > Instant::now() => {
                tracing::warn!("serving stale last-known-good licence answer");
                Ok(entry.payload.clone())
            }
            _ => Err(LicenceError::Unavailable),
        }
    }

    async fn cached_positive(&self, key: &str) -> Option<Value> {
        let cache = self.positive.lock().await;
        match cache.get(key) {
            Some(entry) if entry.expires > Instant::now() => Some(entry.payload.clone()),
            _ => None,
        }
    }

    async fn cached_negative(&self, key: &str) -> bool {
        let cache = self.negative.lock().await;
        matches!(cache.get(key), Some(expiry) if *expiry > Instant::now())
    }
}

fn digest(token: &str) -> String {
    // Not cryptographic — only a stable, non-reversible cache key so the token
    // itself is never held in a map that might be logged or dumped.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// `Bearer <token>`, case-insensitive scheme, exactly one space (or more).
pub fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, rest) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

fn credential(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get("x-tenant-id")
        && let Ok(value) = value.to_str()
        && let Some(token) = bearer_token(value)
    {
        return Some(token.to_string());
    }
    if let Some(value) = req.headers().get(header::AUTHORIZATION)
        && let Ok(value) = value.to_str()
        && let Some(token) = bearer_token(value)
    {
        return Some(token.to_string());
    }
    None
}

fn is_public(path: &str) -> bool {
    let normalized = path.trim_end_matches('/');
    let normalized = if normalized.is_empty() {
        "/"
    } else {
        normalized
    };
    PUBLIC_ENDPOINTS.contains(&normalized)
}

pub fn identity_from_payload(payload: &Value) -> Identity {
    let string_at = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.trim().is_empty())
    };
    Identity {
        tenant_id: string_at("tenant_id"),
        license_id: string_at("license_id"),
    }
}

/// axum middleware. Mounted for every request; a no-op when `ENABLE_AUTH` is
/// false, which the service logs loudly at boot.
pub async fn licence_layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !state.cfg.enable_auth || is_public(req.uri().path()) {
        return next.run(req).await;
    }

    let Some(client) = state.licence.as_ref() else {
        // Boot refuses this combination, so reaching here means the process was
        // tampered with. Fail closed.
        return AppError::Unauthorized("licence authentication is not configured").into_response();
    };

    let Some(token) = credential(&req) else {
        state
            .metrics
            .auth_rejected_total
            .fetch_add(1, Ordering::Relaxed);
        return AppError::Unauthorized("missing licence token").into_response();
    };

    let path = req.uri().path().to_string();
    match client.introspect(&token).await {
        Ok(payload) => {
            let identity = identity_from_payload(&payload);
            let mut req = req;
            req.extensions_mut().insert(identity.clone());
            let response = next.run(req).await;
            log_attribution(&path, &identity, response.status().as_u16());
            response
        }
        Err(LicenceError::Inactive) => {
            state
                .metrics
                .auth_rejected_total
                .fetch_add(1, Ordering::Relaxed);
            AppError::Unauthorized("invalid or inactive licence").into_response()
        }
        Err(LicenceError::Unavailable) => {
            state
                .metrics
                .auth_unavailable_total
                .fetch_add(1, Ordering::Relaxed);
            AppError::Busy("licence introspection is unavailable".to_string()).into_response()
        }
    }
}

/// One line per protected call, so proof work is attributable to a tenant and
/// a licence after the fact.
fn log_attribution(path: &str, identity: &Identity, status: u16) {
    if !PROTECTED_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return;
    }
    tracing::info!(
        target: "lean4_request",
        tenant_id = identity.tenant_id.as_deref().unwrap_or(""),
        license_id = identity.license_id.as_deref().unwrap_or(""),
        path = %path,
        status = status,
        outcome = if status == 401 { "rejected" } else { "allowed" },
        "lean4_request"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fleet_bearer_shapes() {
        assert_eq!(bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(bearer_token("bearer abc123"), Some("abc123"));
        assert_eq!(bearer_token("  Bearer   abc123  "), Some("abc123"));
        assert_eq!(bearer_token("abc123"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("Basic abc123"), None);
    }

    #[test]
    fn health_and_metrics_are_public_but_compute_is_not() {
        assert!(is_public("/health"));
        assert!(is_public("/health/ready"));
        assert!(is_public("/metrics"));
        assert!(is_public("/health/"));
        assert!(!is_public("/api/prove"));
        assert!(!is_public("/api/check"));
    }

    #[test]
    fn identity_comes_from_the_payload_not_a_header() {
        let payload = serde_json::json!({"active": true, "tenant_id": "t-1", "license_id": "l-1"});
        let identity = identity_from_payload(&payload);
        assert_eq!(identity.tenant_id.as_deref(), Some("t-1"));
        assert_eq!(identity.license_id.as_deref(), Some("l-1"));
    }

    #[test]
    fn an_absent_tenant_stays_absent() {
        let payload = serde_json::json!({"active": true});
        let identity = identity_from_payload(&payload);
        assert!(identity.tenant_id.is_none());
    }
}
