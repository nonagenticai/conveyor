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
    /// The subject the licence server says this token belongs to. Only ever read off the
    /// server's answer; `X-Subject-Id` is never a substitute.
    pub subject_id: Option<String>,
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
    /// The subject a verified licence belongs to. `None` on every asserted path — a caller
    /// cannot vouch for itself, so `X-Subject-Id` never lands here.
    pub subject_id: Option<String>,
}

/// The tenant a request is FOR, and who delegated it there.
///
/// `tenant_id` is the caller's own tenant for every outcome except `honoured`. `delegated_by` is
/// set only when an allow-listed, introspected subject named another tenant, and rides the cost
/// event so billing can tell a tenant's own spend from spend a platform incurred on its behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTenant {
    pub tenant_id: Option<String>,
    pub delegated_by: Option<String>,
}

const NIL_UUID_HEX: &str = "00000000000000000000000000000000";

/// Authorization schemes that make a value a CREDENTIAL, not an identifier. Matching only
/// `bearer` would let a `Basic` value — a plaintext password — through the guard written to stop
/// exactly that.
const CREDENTIAL_SCHEMES: [&str; 8] = [
    "bearer",
    "basic",
    "digest",
    "negotiate",
    "token",
    "apikey",
    "hmac",
    "aws4-hmac-sha256",
];

/// True for the nil-UUID sentinel in any spelling (dashed, undashed, braced, `urn:uuid:`,
/// any case). Honouring one would make a real tenant's compute unbillable.
fn is_unattributable(value: &str) -> bool {
    let mut normalised = value.trim().to_ascii_lowercase();
    if let Some(rest) = normalised.strip_prefix("urn:uuid:") {
        normalised = rest.to_owned();
    }
    let without_braces = normalised.strip_prefix('{').unwrap_or(&normalised);
    let without_braces = without_braces.strip_suffix('}').unwrap_or(without_braces);
    without_braces.replace('-', "") == NIL_UUID_HEX
}

/// True when the value parses as a UUID in one of the spellings the reference accepts
/// (hyphenated, simple, braced, `urn:uuid:`).
fn is_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value.trim()).is_ok()
}

/// True when `asserted` names the caller's own tenant in any UUID spelling. Both sides must be
/// real UUIDs and neither may be the nil sentinel, so `self` can only mean "a real tenant named
/// itself".
fn is_own_tenant(asserted: &str, caller_tenant: &str) -> bool {
    if is_unattributable(asserted) || is_unattributable(caller_tenant) {
        return false;
    }
    match (
        uuid::Uuid::parse_str(asserted.trim()),
        uuid::Uuid::parse_str(caller_tenant.trim()),
    ) {
        (Ok(asserted), Ok(caller)) => asserted == caller,
        _ => false,
    }
}

/// The lower-cased auth scheme when the value is `Authorization`-shaped, else `None`.
fn credential_scheme(value: &str) -> Option<&'static str> {
    let trimmed = value.trim();
    let (scheme, rest) = trimmed.split_once(' ')?;
    if rest.trim().is_empty() {
        return None;
    }
    let lowered = scheme.to_ascii_lowercase();
    CREDENTIAL_SCHEMES
        .iter()
        .copied()
        .find(|candidate| *candidate == lowered)
}

/// Resolve the tenant a request is FOR — the reference's `effective_delegation`, in one place.
///
/// Honouring a delegation moves a bill onto a tenant the caller merely named, so it requires an
/// **introspected** subject on the fleet allowlist. Everything else lands on the caller's own
/// tenant: dropped, never refused (this service is meter-and-allow; a billing bug must not become
/// an outage), and never forged.
pub fn effective_tenant(
    header_value: Option<&str>,
    vouched_subject: Option<&str>,
    caller_tenant: Option<&str>,
    allowlist: &[String],
) -> EffectiveTenant {
    let own = |tenant: Option<&str>| EffectiveTenant {
        tenant_id: tenant
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned),
        delegated_by: None,
    };
    let Some(caller) = caller_tenant.filter(|value| !value.trim().is_empty()) else {
        return own(None);
    };
    let Some(asserted) = header_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return own(Some(caller));
    };
    if is_own_tenant(asserted, caller) {
        return own(Some(caller));
    }
    let subject = vouched_subject
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let allowlisted = subject.as_deref().is_some_and(|subject| {
        allowlist
            .iter()
            .any(|allowed| allowed.trim().eq_ignore_ascii_case(subject))
    });
    if !allowlisted {
        return own(Some(caller));
    }
    // The asserted tenant is caller-supplied and is never logged before these checks: a
    // delegating caller that fat-fingered its own licence token here must not leak it.
    if credential_scheme(asserted).is_some() || is_unattributable(asserted) || !is_uuid(asserted) {
        return own(Some(caller));
    }
    EffectiveTenant {
        tenant_id: Some(asserted.to_owned()),
        delegated_by: subject,
    }
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
    /// The licence server's answer to "who is this token?": the only value allowed to vouch for
    /// a cross-tenant delegation.
    subject_id: Option<String>,
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
            subject_id: payload.subject_id,
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
        subject_id: None,
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
            subject_id: licence.subject_id.clone(),
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

    const SUBJECT: &str = "8382f0b3-c082-4f97-8d47-120ce5a18e05";
    const TENANT: &str = "0195f48b-d000-7000-8000-000000000001";
    const OTHER: &str = "0195f48b-d000-7000-8000-0000000000b2";

    fn allowlist() -> Vec<String> {
        vec![SUBJECT.to_owned()]
    }

    fn own(tenant: &str) -> EffectiveTenant {
        EffectiveTenant {
            tenant_id: Some(tenant.to_owned()),
            delegated_by: None,
        }
    }

    #[test]
    fn without_the_header_the_caller_keeps_its_own_tenant() {
        assert_eq!(
            effective_tenant(None, Some(SUBJECT), Some(TENANT), &allowlist()),
            own(TENANT)
        );
        // No caller tenant at all: nothing to delegate FROM, and nothing is invented.
        assert_eq!(
            effective_tenant(Some(OTHER), Some(SUBJECT), None, &allowlist()),
            EffectiveTenant {
                tenant_id: None,
                delegated_by: None
            }
        );
    }

    #[test]
    fn naming_your_own_tenant_is_not_a_delegation() {
        // The same UUID in a different spelling is still `self` — and needs no vouching.
        assert_eq!(
            effective_tenant(
                Some("0195F48B-D000-7000-8000-000000000001"),
                None,
                Some(TENANT),
                &allowlist()
            ),
            own(TENANT)
        );
    }

    #[test]
    fn a_cross_tenant_assertion_without_a_vouched_allow_listed_subject_is_dropped() {
        // No introspected subject (fail-open, observe, off) — the header is not evidence.
        assert_eq!(
            effective_tenant(Some(OTHER), None, Some(TENANT), &allowlist()),
            own(TENANT)
        );
        // A vouched subject that is not on the fleet allowlist cannot delegate either.
        assert_eq!(
            effective_tenant(
                Some(OTHER),
                Some("01a0dd02-d324-77bb-acd8-7c31fb6e95ec"),
                Some(TENANT),
                &allowlist()
            ),
            own(TENANT)
        );
    }

    #[test]
    fn a_vouched_allow_listed_subject_is_honoured() {
        // Case is normalised on both sides; `delegated_by` is the lower-cased subject.
        let decision = effective_tenant(
            Some(OTHER),
            Some("8382F0B3-C082-4F97-8D47-120CE5A18E05"),
            Some(TENANT),
            &allowlist(),
        );
        assert_eq!(
            decision,
            EffectiveTenant {
                tenant_id: Some(OTHER.to_owned()),
                delegated_by: Some(SUBJECT.to_owned()),
            }
        );
    }

    #[test]
    fn an_invalid_assertion_is_dropped_even_from_a_vouched_subject() {
        let dropped = |value: &str| {
            assert_eq!(
                effective_tenant(Some(value), Some(SUBJECT), Some(TENANT), &allowlist()),
                own(TENANT),
                "{value} must never be honoured as a tenant"
            );
        };
        // The nil sentinel in every spelling it is documented to arrive in.
        dropped("00000000-0000-0000-0000-000000000000");
        dropped("00000000000000000000000000000000");
        dropped("{00000000-0000-0000-0000-000000000000}");
        dropped("urn:uuid:00000000-0000-0000-0000-000000000000");
        // Not a UUID at all, and a credential that must never be treated as an identifier.
        dropped("not-a-tenant");
        dropped("Bearer 00000000-0000-0000-0000-0000000000b2");
        dropped("   ");
    }
}
