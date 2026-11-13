//! JWKS verification with a per-issuer cache, and the issuer pin.
//!
//! The unverified `iss` only SELECTS a trusted entry. A token from an issuer that is not
//! listed is refused **before any key lookup**, so a caller cannot make this service fetch
//! from an issuer of its choosing. The token is then validated with `iss` pinned to exactly
//! the issuer whose keys signed it, and `aud` + `azp` are both required.
//!
//! `JWKS_MIN_REFRESH` is a NEGATIVE floor: without it, an unknown `kid` re-fetches the key
//! set on every request, which turns a forged token into a request-amplified fetch loop and
//! a Keycloak outage into one fetch per request.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::config::AuthConfig;
use crate::error::AuthError;

/// A key set is reused for this long.
pub const JWKS_TTL: Duration = Duration::from_secs(600);

/// A failed or unknown-`kid` fetch is not retried more often than this.
pub const JWKS_MIN_REFRESH: Duration = Duration::from_secs(30);

/// A boxed fetch future, so the source trait needs no async-trait dependency.
pub type FetchFuture<'a> = Pin<Box<dyn Future<Output = Result<JwkSet, String>> + Send + 'a>>;

/// Where a JWKS comes from. Injected so tests run without a network.
pub trait JwksSource: Send + Sync {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a>;
}

/// The real source: HTTPS with a bounded timeout.
pub struct HttpJwks {
    client: reqwest::Client,
}

impl HttpJwks {
    /// Build the client once; a per-request client would redo TLS setup every time.
    pub fn new() -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AuthError::Config(format!("building the JWKS HTTP client: {e}")))?;
        Ok(Self { client })
    }
}

impl JwksSource for HttpJwks {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a> {
        Box::pin(async move {
            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| format!("GET {url}: {e}"))?;
            if !response.status().is_success() {
                return Err(format!("GET {url}: HTTP {}", response.status()));
            }
            response
                .json::<JwkSet>()
                .await
                .map_err(|e| format!("GET {url}: {e}"))
        })
    }
}

#[derive(Default)]
struct CacheState {
    /// Keyed by `(jwks_url, kid)`. Two realms can publish the same `kid`; keying by `kid` alone
    /// would let realm A's key answer for realm B's token (the doc below promises the opposite).
    keys: HashMap<(String, String), DecodingKey>,
    fetched: Option<Instant>,
    last_attempt: Option<Instant>,
    last_failed: bool,
}

/// Per-issuer key cache. One instance per issuer: a key from realm A must never validate a
/// token whose `iss` is realm B.
pub struct JwksCache {
    source: Arc<dyn JwksSource>,
    ttl: Duration,
    min_refresh: Duration,
    state: Mutex<CacheState>,
}

impl JwksCache {
    /// The production timings.
    pub fn new(source: Arc<dyn JwksSource>) -> Self {
        Self::with_ttl(source, JWKS_TTL, JWKS_MIN_REFRESH)
    }

    /// Injectable timings, so the cache's expiry behaviour is tested in milliseconds rather
    /// than by waiting ten minutes.
    pub fn with_ttl(source: Arc<dyn JwksSource>, ttl: Duration, min_refresh: Duration) -> Self {
        Self {
            source,
            ttl,
            min_refresh,
            state: Mutex::new(CacheState::default()),
        }
    }

    /// The decoding key for `kid` at `url`.
    ///
    /// The lock is held across the fetch on purpose: concurrent callers wait for ONE
    /// attempt and reuse its outcome, rather than stampeding the issuer.
    pub async fn key(&self, url: &str, kid: &str) -> Result<DecodingKey, AuthError> {
        let mut state = self.state.lock().await;
        let cache_key = (url.to_owned(), kid.to_owned());
        let fresh = state.fetched.is_some_and(|t| t.elapsed() < self.ttl);
        let known = state.keys.contains_key(&cache_key);
        let may_fetch = state
            .last_attempt
            .is_none_or(|t| t.elapsed() >= self.min_refresh);

        if !(fresh && known) && may_fetch {
            state.last_attempt = Some(Instant::now());
            match self.source.fetch(url).await {
                Ok(set) => {
                    // Replace only THIS issuer's keys: a refresh of one realm must not forget
                    // another realm's set.
                    state
                        .keys
                        .retain(|(cached_url, _), _| cached_url.as_str() != url);
                    for (kid, key) in keys_of(set) {
                        state.keys.insert((url.to_owned(), kid), key);
                    }
                    state.fetched = Some(Instant::now());
                    state.last_failed = false;
                }
                Err(error) => {
                    tracing::warn!(%url, %error, "jwks fetch failed");
                    state.last_failed = true;
                }
            }
        }

        if let Some(key) = state.keys.get(&cache_key) {
            return Ok(key.clone());
        }
        if state.last_failed {
            // "Rotated" and "forged" are indistinguishable without a key set; refuse and let
            // the client retry rather than accept an unverified token.
            return Err(AuthError::JwksUnavailable(url.to_owned()));
        }
        Err(AuthError::InvalidToken)
    }
}

fn keys_of(set: JwkSet) -> Vec<(String, DecodingKey)> {
    set.keys
        .into_iter()
        .filter_map(|jwk| {
            let kid = jwk.common.key_id.clone()?;
            DecodingKey::from_jwk(&jwk).ok().map(|key| (kid, key))
        })
        .collect()
}

/// One trusted issuer and the key set that validates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedIssuer {
    pub issuer: String,
    pub jwks_url: String,
}

/// A verified token's fields. No roles, no tenant: those come from `membership`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub sub: String,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
    /// `email_verified` exactly as the issuer stated it. `None` is NOT the same as `false`:
    /// an email-keyed binding requires `Some(true)`, so a token that omits the claim cannot
    /// bind an existing user or redeem an invite.
    pub email_verified: Option<bool>,
}

/// OIDC verifier. One realm (CONTRACT §9); the list exists so the pin is explicit, not so
/// extra realms can be trusted casually.
pub struct Verifier {
    audience: String,
    client_id: String,
    issuers: Vec<TrustedIssuer>,
    cache: JwksCache,
}

#[derive(Deserialize)]
struct RawClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    azp: Option<String>,
}

impl Verifier {
    /// Build from parts (tests inject a [`JwksCache`] over a fake source).
    pub fn new(
        audience: impl Into<String>,
        client_id: impl Into<String>,
        issuers: Vec<TrustedIssuer>,
        cache: JwksCache,
    ) -> Self {
        Self {
            audience: audience.into(),
            client_id: client_id.into(),
            issuers,
            cache,
        }
    }

    /// Build the production verifier from configuration.
    pub fn from_config(cfg: &AuthConfig) -> Result<Self, AuthError> {
        let issuer = cfg
            .oidc_issuer
            .clone()
            .ok_or_else(|| AuthError::Config("CONVEYOR_OIDC_ISSUER is not set".into()))?;
        let jwks_url = cfg
            .jwks_url()
            .ok_or_else(|| AuthError::Config("no JWKS URL for the configured issuer".into()))?;
        let source: Arc<dyn JwksSource> = Arc::new(HttpJwks::new()?);
        Ok(Self::new(
            cfg.oidc_audience.clone(),
            cfg.oidc_client_id.clone(),
            vec![TrustedIssuer { issuer, jwks_url }],
            JwksCache::new(source),
        ))
    }

    /// Verify a person token: signature, expiry, pinned `iss`, required `aud`, and
    /// `azp == client_id`.
    pub async fn verify(&self, token: &str) -> Result<VerifiedClaims, AuthError> {
        let header = decode_header(token).map_err(|_| AuthError::InvalidToken)?;
        // Only asymmetric algorithms. Accepting an HS* token lets an attacker sign with the
        // public key as an HMAC secret (key confusion); the key set never gets consulted.
        if !is_asymmetric(header.alg) {
            return Err(AuthError::InvalidToken);
        }
        let iss = unverified_iss(token).ok_or(AuthError::InvalidToken)?;
        let trusted = self
            .issuers
            .iter()
            .find(|t| t.issuer == iss)
            .ok_or(AuthError::UntrustedIssuer)?;
        let kid = header.kid.as_deref().ok_or(AuthError::InvalidToken)?;
        let key = self.cache.key(&trusted.jwks_url, kid).await?;

        let mut validation = Validation::new(header.alg);
        // jsonwebtoken checks these only when present: make them (and `sub`) mandatory, so a
        // token minted without an `aud` cannot pass.
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.set_issuer(&[&trusted.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.leeway = 5;

        let claims = decode::<RawClaims>(token, &key, &validation)
            .map_err(|_| AuthError::InvalidToken)?
            .claims;
        if claims.sub.trim().is_empty() {
            return Err(AuthError::InvalidToken);
        }
        // `azp` is the only cross-app replay guard: realm clients commonly carry every
        // client's audience, so only the authorised party says who the token was issued to.
        if claims.azp.as_deref() != Some(self.client_id.as_str()) {
            return Err(AuthError::InvalidToken);
        }
        Ok(VerifiedClaims {
            sub: claims.sub,
            email: claims.email.filter(|e| !e.trim().is_empty()),
            preferred_username: claims.preferred_username.filter(|n| !n.trim().is_empty()),
            email_verified: claims.email_verified,
        })
    }
}

/// Only asymmetric signature algorithms are accepted.
pub fn is_asymmetric(alg: Algorithm) -> bool {
    matches!(
        alg,
        Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::ES256
            | Algorithm::ES384
            | Algorithm::PS256
            | Algorithm::PS384
            | Algorithm::PS512
    )
}

/// The `iss` of a token, read WITHOUT verifying it — only to select which trusted issuer
/// validates the token. Never used for a decision.
pub fn unverified_iss(token: &str) -> Option<String> {
    use base64::Engine;
    #[derive(Deserialize)]
    struct Iss {
        iss: Option<String>,
    }
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    serde_json::from_slice::<Iss>(&bytes).ok()?.iss
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use base64::Engine;

    use super::*;

    const OCT_SET: &str = r#"{"keys":[{"kty":"oct","kid":"k1","k":"c2VjcmV0LXNlY3JldA"}]}"#;

    struct CountingSource {
        calls: AtomicUsize,
        json: String,
    }

    impl CountingSource {
        fn new(json: &str) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                json: json.to_owned(),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl JwksSource for CountingSource {
        fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let json = self.json.clone();
            Box::pin(
                async move { serde_json::from_str::<JwkSet>(&json).map_err(|e| e.to_string()) },
            )
        }
    }

    fn b64(json: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
    }

    fn token(header: &str, payload: &str) -> String {
        format!("{}.{}.c2ln", b64(header), b64(payload))
    }

    fn good_token(iss: &str) -> String {
        token(
            r#"{"alg":"RS256","kid":"k1","typ":"JWT"}"#,
            &format!(
                r#"{{"iss":"{iss}","sub":"alice","aud":"conveyor","azp":"conveyor","exp":4102444800}}"#
            ),
        )
    }

    fn verifier(source: Arc<CountingSource>) -> Verifier {
        Verifier::new(
            "conveyor",
            "conveyor",
            vec![TrustedIssuer {
                issuer: "https://good.example/realms/conveyor".into(),
                jwks_url: "https://good.example/certs".into(),
            }],
            JwksCache::new(source),
        )
    }

    #[test]
    fn unverified_iss_reads_only_the_payload() {
        assert_eq!(
            unverified_iss(&good_token("https://good.example")).as_deref(),
            Some("https://good.example")
        );
        for bad in ["", "a.b", "not-a-token", "a.!!!.c"] {
            assert_eq!(unverified_iss(bad), None, "{bad:?}");
        }
    }

    #[tokio::test]
    async fn an_unlisted_issuer_is_refused_before_any_fetch() {
        let source = CountingSource::new(OCT_SET);
        let v = verifier(source.clone());
        let token = good_token("https://evil.example/realms/conveyor");
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, AuthError::UntrustedIssuer), "{err:?}");
        assert_eq!(source.calls(), 0, "a forged iss must not cause a fetch");
    }

    #[tokio::test]
    async fn a_symmetric_algorithm_is_refused_before_any_fetch() {
        let source = CountingSource::new(OCT_SET);
        let v = verifier(source.clone());
        let token = token(
            r#"{"alg":"HS256","kid":"k1","typ":"JWT"}"#,
            r#"{"iss":"https://good.example/realms/conveyor","sub":"alice","aud":"conveyor","azp":"conveyor","exp":4102444800}"#,
        );
        let err = v.verify(&token).await.unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken), "{err:?}");
        assert_eq!(source.calls(), 0, "HS* must not reach the key set");
    }

    #[tokio::test]
    async fn keys_are_cached_within_the_ttl() {
        let source = CountingSource::new(OCT_SET);
        let cache = JwksCache::new(source.clone());
        cache.key("https://good.example/certs", "k1").await.unwrap();
        cache.key("https://good.example/certs", "k1").await.unwrap();
        assert_eq!(source.calls(), 1);
    }

    #[tokio::test]
    async fn an_unknown_kid_obeys_the_negative_floor_and_then_refetches() {
        let source = CountingSource::new(OCT_SET);
        let cache = JwksCache::with_ttl(
            source.clone(),
            Duration::from_secs(300),
            Duration::from_millis(80),
        );
        cache.key("https://good.example/certs", "k1").await.unwrap();
        assert_eq!(source.calls(), 1);

        let err = cache
            .key("https://good.example/certs", "missing")
            .await
            .expect_err("an unknown kid must fail");
        assert!(matches!(err, AuthError::InvalidToken), "{err:?}");
        assert_eq!(source.calls(), 1, "an unknown kid must not stampede");

        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = cache.key("https://good.example/certs", "missing").await;
        assert_eq!(source.calls(), 2, "after the floor a rotation is retried");
    }

    #[tokio::test]
    async fn an_expired_cache_refetches() {
        let source = CountingSource::new(OCT_SET);
        let cache = JwksCache::with_ttl(
            source.clone(),
            Duration::from_millis(40),
            Duration::from_millis(10),
        );
        cache.key("https://good.example/certs", "k1").await.unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        cache.key("https://good.example/certs", "k1").await.unwrap();
        assert_eq!(source.calls(), 2);
    }

    #[tokio::test]
    async fn a_failed_fetch_fails_closed() {
        let source = CountingSource::new("not json");
        let cache = JwksCache::new(source.clone());
        let err = cache
            .key("https://good.example/certs", "k1")
            .await
            .expect_err("a failed fetch must fail closed");
        assert!(matches!(err, AuthError::JwksUnavailable(_)), "{err:?}");
        // The floor applies to failures too: a second call does not retry.
        let _ = cache.key("https://good.example/certs", "k1").await;
        assert_eq!(source.calls(), 1);
    }

    /// Two realms that publish the SAME `kid` with different keys. The cache must key by
    /// `(url, kid)`: realm A's key must not validate realm B's token, and a refresh of one realm
    /// must not forget the other's set.
    const SET_A: &str = r#"{"keys":[{"kty":"oct","kid":"k1","k":"YWFhYWFhYWFhYWFhYWFhYQ"}]}"#;
    const SET_B: &str = r#"{"keys":[{"kty":"oct","kid":"k1","k":"YmJiYmJiYmJiYmJiYmJiYg"}]}"#;

    struct PerUrlSource {
        urls: std::sync::Mutex<Vec<String>>,
    }

    impl JwksSource for PerUrlSource {
        fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a> {
            self.urls.lock().unwrap().push(url.to_owned());
            let json = if url == "https://a.example/certs" {
                SET_A
            } else {
                SET_B
            };
            Box::pin(async move { serde_json::from_str::<JwkSet>(json).map_err(|e| e.to_string()) })
        }
    }

    #[tokio::test]
    async fn a_shared_kid_never_crosses_realms() {
        let source = Arc::new(PerUrlSource {
            urls: std::sync::Mutex::new(Vec::new()),
        });
        // A LONG TTL is the point: the dangerous window is "kid known, set still fresh", where a
        // kid-keyed cache answers from another realm's set without fetching. `min_refresh` is 0
        // so realm B's first call always fetches regardless of machine load — a nonzero floor made
        // this test pass alone and fail under a loaded `cargo test --workspace`.
        let cache = JwksCache::with_ttl(source.clone(), Duration::from_secs(60), Duration::ZERO);
        let token_b = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &serde_json::json!({"sub": "bob"}),
            &jsonwebtoken::EncodingKey::from_secret(b"bbbbbbbbbbbbbbbb"),
        )
        .expect("sign realm B's token");

        // Realm A warms the cache. Realm B publishes the SAME `kid` with a different key.
        cache.key("https://a.example/certs", "k1").await.unwrap();
        let key_b = cache
            .key("https://b.example/certs", "k1")
            .await
            .expect("realm B's key");
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        // The token carries no `exp`; the property here is which KEY answered, not lifetime.
        validation.required_spec_claims.clear();
        jsonwebtoken::decode::<serde_json::Value>(&token_b, &key_b, &validation)
            .expect("realm B's token verifies with realm B's key, not realm A's");
        assert_eq!(
            source.urls.lock().unwrap().len(),
            2,
            "a shared kid must force a fetch for the second realm"
        );
    }

    #[tokio::test]
    async fn a_refresh_of_one_realm_keeps_another_realms_keys() {
        let source = Arc::new(PerUrlSource {
            urls: std::sync::Mutex::new(Vec::new()),
        });
        // Short TTL so realm A's second call refetches; a 2 s floor keeps realm B from refetching
        // in the same breath, with a margin wide enough that a loaded test run cannot close it.
        let cache = JwksCache::with_ttl(
            source.clone(),
            Duration::from_millis(50),
            Duration::from_secs(2),
        );
        let token_b = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &serde_json::json!({"sub": "bob"}),
            &jsonwebtoken::EncodingKey::from_secret(b"bbbbbbbbbbbbbbbb"),
        )
        .expect("sign realm B's token");

        cache.key("https://a.example/certs", "k1").await.unwrap();
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        cache.key("https://b.example/certs", "k1").await.unwrap();
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        // Realm A refetches. Under a whole-map replace this drops realm B's key.
        cache.key("https://a.example/certs", "k1").await.unwrap();

        let key_b = cache
            .key("https://b.example/certs", "k1")
            .await
            .expect("realm B's key survives realm A's refresh");
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.required_spec_claims.clear();
        jsonwebtoken::decode::<serde_json::Value>(&token_b, &key_b, &validation)
            .expect("realm B still resolves to realm B's key after realm A refetched");
    }
}
