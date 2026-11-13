//! Caller authentication — the licence introspection gate.
//!
//! The rules are the live service's, because they are what decides whether a call spends tokens:
//!
//! - A token is read from `X-Tenant-Id: Bearer …` first, then `Authorization: Bearer …`.
//! - **Only** a value shaped like a licence token (`[A-Za-z0-9_-]{43}`) is ever sent to the licence
//!   server. An OpenAI SDK's vendor key or an internal secret in `Authorization` must never be
//!   posted across a service boundary on every call.
//! - `INTROSPECT_MODE=enforce` refuses an unverifiable caller with the live 401; `observe` and
//!   `off` pass the caller through, `off` without touching the network at all.
//! - A licence server that cannot be reached is NOT a verdict: with `fail_open` (the default) the
//!   caller proceeds exactly as before, and with it off the caller gets a 503.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;

use crate::routing::Refusal;

/// The app every licence must be issued for.
pub const EXPECTED_APP: &str = "inference-router";

/// The live 401 message, verbatim.
pub const DENIED_MESSAGE: &str =
    "Caller licence could not be introspected; caller is not authorised";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Observe,
    Enforce,
}

impl Mode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "observe" => Some(Self::Observe),
            "enforce" => Some(Self::Enforce),
            _ => None,
        }
    }
}

/// What a request presented, and why it presented nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presentation {
    pub token: Option<String>,
    pub absent_outcome: &'static str,
}

/// A licence the server vouched for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Licence {
    pub tenant_id: String,
    pub license_id: String,
    pub app: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Resolved(Licence),
    Denied,
    Unavailable,
}

/// The caller as the service can prove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerIdentity {
    pub tenant_id: Option<String>,
    pub license_id: Option<String>,
    /// True only when the licence server vouched for the token on THIS request.
    pub verified: bool,
}

fn bearer_value(raw: Option<&str>) -> Option<String> {
    let value = raw.unwrap_or("").trim();
    let lowered = value.to_ascii_lowercase();
    if let Some(rest) = lowered.strip_prefix("bearer ") {
        let token = value[value.len() - rest.len()..].trim();
        return Some(token.to_owned()).filter(|token| !token.is_empty());
    }
    None
}

fn is_licence_shaped(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn is_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
}

/// Read the licence token by the live precedence. Never fails.
pub fn read_license_token(headers: &HeaderMap) -> Presentation {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());

    if let Some(from_tenant) = bearer_value(header("x-tenant-id")) {
        if !is_licence_shaped(&from_tenant) {
            return Presentation {
                token: None,
                absent_outcome: "not_licence_shaped",
            };
        }
        return Presentation {
            token: Some(from_tenant),
            absent_outcome: "absent",
        };
    }
    let Some(from_authorization) = bearer_value(header("authorization")) else {
        return Presentation {
            token: None,
            absent_outcome: "absent",
        };
    };
    let lowered = from_authorization.to_ascii_lowercase();
    if matches!(lowered.as_str(), "empty" | "internal" | "none") {
        return Presentation {
            token: None,
            absent_outcome: "sentinel",
        };
    }
    if is_jwt(&from_authorization) {
        return Presentation {
            token: None,
            absent_outcome: "authorization_jwt_ignored",
        };
    }
    if !is_licence_shaped(&from_authorization) {
        return Presentation {
            token: None,
            absent_outcome: "not_licence_shaped",
        };
    }
    Presentation {
        token: Some(from_authorization),
        absent_outcome: "absent",
    }
}

/// The bounded, process-local introspection cache: 300 s for an active licence, 30 s for an
/// inactive answer. A revocation is therefore honoured within the negative TTL and served for at
/// most the positive one — the live service's explicit trade.
struct Cache {
    positive: HashMap<String, (Licence, Instant)>,
    negative: HashMap<String, Instant>,
    positive_ttl: Duration,
    negative_ttl: Duration,
}

const MAX_CACHE_ENTRIES: usize = 4096;

impl Cache {
    fn get(&mut self, token: &str, now: Instant) -> Option<Verdict> {
        if let Some((licence, stored)) = self.positive.get(token) {
            if now.duration_since(*stored) < self.positive_ttl {
                return Some(Verdict::Resolved(licence.clone()));
            }
            self.positive.remove(token);
        }
        if let Some(stored) = self.negative.get(token) {
            if now.duration_since(*stored) < self.negative_ttl {
                return Some(Verdict::Denied);
            }
            self.negative.remove(token);
        }
        None
    }

    fn store(&mut self, token: &str, verdict: &Verdict, now: Instant) {
        if self.positive.len() + self.negative.len() >= MAX_CACHE_ENTRIES {
            self.positive.clear();
            self.negative.clear();
        }
        match verdict {
            Verdict::Resolved(licence) => {
                self.negative.remove(token);
                self.positive
                    .insert(token.to_owned(), (licence.clone(), now));
            }
            Verdict::Denied => {
                self.positive.remove(token);
                self.negative.insert(token.to_owned(), now);
            }
            Verdict::Unavailable => {}
        }
    }
}

/// The licence-server client.
pub struct Introspector {
    url: String,
    timeout: Duration,
    client: reqwest::Client,
    cache: Mutex<Cache>,
}

#[derive(Debug, Deserialize)]
struct Payload {
    #[serde(default)]
    active: bool,
    tenant_id: Option<String>,
    license_id: Option<String>,
    app: Option<AppInfo>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppInfo {
    key: Option<String>,
}

impl Introspector {
    pub fn new(url: String, timeout: Duration) -> Self {
        Self {
            url,
            timeout,
            client: reqwest::Client::new(),
            cache: Mutex::new(Cache {
                positive: HashMap::new(),
                negative: HashMap::new(),
                positive_ttl: Duration::from_secs(300),
                negative_ttl: Duration::from_secs(30),
            }),
        }
    }

    /// Resolve a token, through the cache.
    pub async fn resolve(&self, token: &str) -> Verdict {
        let now = Instant::now();
        if let Some(cached) = self.cache.lock().expect("cache lock").get(token, now) {
            return judge(cached);
        }
        let verdict = self.introspect(token).await;
        self.cache
            .lock()
            .expect("cache lock")
            .store(token, &verdict, now);
        judge(verdict)
    }

    async fn introspect(&self, token: &str) -> Verdict {
        if self.url.trim().is_empty() {
            return Verdict::Unavailable;
        }
        let response = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({"token": token}))
            .timeout(self.timeout)
            .send()
            .await;
        let Ok(response) = response else {
            return Verdict::Unavailable;
        };
        if !response.status().is_success() {
            // Including 4xx: licences answers an inactive token with 200 {"active": false}. A
            // non-200 is a contract drift or an outage — never a statement about this caller.
            return Verdict::Unavailable;
        }
        let Ok(payload) = response.json::<Payload>().await else {
            return Verdict::Unavailable;
        };
        if !payload.active {
            return Verdict::Denied;
        }
        let (Some(tenant_id), Some(license_id)) = (payload.tenant_id, payload.license_id) else {
            // Active but without the two fields the gate exists to establish — never substituted
            // from the caller's headers.
            return Verdict::Denied;
        };
        Verdict::Resolved(Licence {
            tenant_id,
            license_id,
            app: payload.app.and_then(|app| app.key),
            expires_at: payload.expires_at,
        })
    }
}

/// The app check re-runs on every cache hit, so the cache cannot launder a wrong-app licence.
fn judge(verdict: Verdict) -> Verdict {
    match verdict {
        Verdict::Resolved(licence) => {
            if licence.app.as_deref() != Some(EXPECTED_APP) {
                Verdict::Denied
            } else {
                Verdict::Resolved(licence)
            }
        }
        other => other,
    }
}

/// The gate the completions route runs.
pub async fn authorize(
    mode: Mode,
    introspector: &Introspector,
    fail_open: bool,
    headers: &HeaderMap,
) -> Result<CallerIdentity, Refusal> {
    let asserted = CallerIdentity {
        tenant_id: headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        license_id: None,
        verified: false,
    };
    if mode == Mode::Off {
        // The inert path: no header is read, no request is made.
        return Ok(asserted);
    }
    let presentation = read_license_token(headers);
    let Some(token) = presentation.token else {
        return if mode == Mode::Enforce {
            Err(denied("unlicensed"))
        } else {
            Ok(asserted)
        };
    };
    match introspector.resolve(&token).await {
        Verdict::Resolved(licence) => Ok(CallerIdentity {
            tenant_id: Some(licence.tenant_id.clone()),
            license_id: Some(licence.license_id.clone()),
            verified: true,
        }),
        Verdict::Denied => {
            if mode == Mode::Enforce {
                Err(denied("licence_not_active"))
            } else {
                Ok(asserted)
            }
        }
        Verdict::Unavailable => {
            if mode == Mode::Enforce && !fail_open {
                Err(Refusal::detail(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Licence introspection unavailable",
                    "licence_unavailable",
                ))
            } else {
                Ok(asserted)
            }
        }
    }
}

fn denied(classification: &'static str) -> Refusal {
    let mut refusal = Refusal::detail(StatusCode::UNAUTHORIZED, DENIED_MESSAGE, classification);
    refusal
        .headers
        .push(("WWW-Authenticate", "Bearer".to_owned()));
    refusal
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};
    use tokio::sync::Mutex;

    use super::*;

    const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 43 chars

    /// What the stub licences server saw, its answer, and its status.
    type StubState = (Arc<Mutex<Vec<Value>>>, Value, u16);

    fn headers(pairs: &[(&'static str, String)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().expect("header value"));
        }
        map
    }

    #[test]
    fn the_token_shape_is_the_only_thing_sent_to_licences() {
        // 43 characters of the licence alphabet.
        assert!(is_licence_shaped(TOKEN));
        assert_eq!(TOKEN.len(), 43);

        let send = read_license_token(&headers(&[("authorization", format!("Bearer {TOKEN}"))]));
        assert_eq!(send.token.as_deref(), Some(TOKEN));

        let vendor = read_license_token(&headers(&[(
            "authorization",
            "Bearer sk-proj-abc123".to_owned(),
        )]));
        assert_eq!(vendor.token, None);
        assert_eq!(vendor.absent_outcome, "not_licence_shaped");

        let jwt = read_license_token(&headers(&[(
            "authorization",
            "Bearer aaa.bbb.ccc".to_owned(),
        )]));
        assert_eq!(jwt.token, None);
        assert_eq!(jwt.absent_outcome, "authorization_jwt_ignored");

        let sentinel =
            read_license_token(&headers(&[("authorization", "Bearer internal".to_owned())]));
        assert_eq!(sentinel.absent_outcome, "sentinel");
    }

    #[test]
    fn the_tenant_header_wins_over_authorization() {
        let presentation = read_license_token(&headers(&[
            ("x-tenant-id", format!("Bearer {TOKEN}")),
            ("authorization", "Bearer b".repeat(43)),
        ]));
        assert_eq!(presentation.token.as_deref(), Some(TOKEN));
    }

    fn active_payload() -> Value {
        json!({
            "active": true,
            "tenant_id": "0195f48b-d000-7000-8000-000000000001",
            "license_id": "lic-1",
            "app": {"key": "inference-router"},
            "expires_at": "2027-01-01T00:00:00Z"
        })
    }

    async fn introspector(payload: Value, status: u16) -> (Arc<Mutex<Vec<Value>>>, Introspector) {
        let seen: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let state = (seen.clone(), payload, status);
        let handler =
            |axum::extract::State((seen, payload, status)): axum::extract::State<StubState>,
             Json(body): Json<Value>| async move {
                seen.lock().await.push(body);
                (
                    axum::http::StatusCode::from_u16(status).expect("status"),
                    Json(payload),
                )
            };
        let app = Router::new()
            .route("/license/introspect", post(handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("stub serves") });
        (
            seen,
            Introspector::new(
                format!("http://{address}/license/introspect"),
                Duration::from_secs(2),
            ),
        )
    }

    #[tokio::test]
    async fn an_active_licence_resolves_and_is_cached() {
        let (seen, introspector) = introspector(active_payload(), 200).await;
        let verdict = introspector.resolve(TOKEN).await;
        assert!(
            matches!(verdict, Verdict::Resolved(ref licence) if licence.tenant_id == "0195f48b-d000-7000-8000-000000000001")
        );
        // The second call is served from the cache: one POST, not two.
        let _ = introspector.resolve(TOKEN).await;
        assert_eq!(seen.lock().await.len(), 1);
        assert_eq!(seen.lock().await[0]["token"], TOKEN);
    }

    #[tokio::test]
    async fn an_inactive_licence_is_denied() {
        let (_seen, introspector) = introspector(json!({"active": false}), 200).await;
        assert_eq!(introspector.resolve(TOKEN).await, Verdict::Denied);
    }

    #[tokio::test]
    async fn a_wrong_app_is_denied() {
        let (_seen, introspector) = introspector(
            json!({"active": true, "tenant_id": "t", "license_id": "l", "app": {"key": "other"}}),
            200,
        )
        .await;
        assert_eq!(introspector.resolve(TOKEN).await, Verdict::Denied);
    }

    #[tokio::test]
    async fn an_unreachable_server_is_unavailable() {
        let introspector = Introspector::new(
            "http://127.0.0.1:1/x".to_owned(),
            Duration::from_millis(200),
        );
        assert_eq!(introspector.resolve(TOKEN).await, Verdict::Unavailable);
    }

    #[tokio::test]
    async fn enforce_refuses_with_the_live_401_body() {
        let (_seen, introspector) = introspector(active_payload(), 200).await;
        let refusal = authorize(
            Mode::Enforce,
            &introspector,
            true,
            &headers(&[("authorization", "Bearer nope".to_owned())]),
        )
        .await
        .expect_err("not licence shaped");
        assert_eq!(refusal.status, StatusCode::UNAUTHORIZED);
        assert_eq!(refusal.body["detail"], DENIED_MESSAGE);

        let refusal = authorize(Mode::Enforce, &introspector, true, &HeaderMap::new())
            .await
            .expect_err("absent");
        assert_eq!(refusal.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn enforce_accepts_a_vouched_licence() {
        let (_seen, introspector) = introspector(active_payload(), 200).await;
        let identity = authorize(
            Mode::Enforce,
            &introspector,
            true,
            &headers(&[("authorization", format!("Bearer {TOKEN}"))]),
        )
        .await
        .expect("vouched");
        assert!(identity.verified);
        assert_eq!(
            identity.tenant_id.as_deref(),
            Some("0195f48b-d000-7000-8000-000000000001")
        );
    }

    #[tokio::test]
    async fn an_outage_fails_open_unless_configured_not_to() {
        let introspector = Introspector::new(
            "http://127.0.0.1:1/x".to_owned(),
            Duration::from_millis(200),
        );
        let request = headers(&[("authorization", format!("Bearer {TOKEN}"))]);
        let identity = authorize(Mode::Enforce, &introspector, true, &request)
            .await
            .expect("fail open");
        assert!(!identity.verified);
        let refusal = authorize(Mode::Enforce, &introspector, false, &request)
            .await
            .expect_err("fail closed");
        assert_eq!(refusal.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn the_inert_mode_never_calls_licences() {
        let (_seen, introspector) = introspector(active_payload(), 200).await;
        let identity = authorize(Mode::Off, &introspector, false, &HeaderMap::new())
            .await
            .expect("off");
        assert!(!identity.verified);
    }
}
