//! `/api/admin/*` authentication, through the real router.
//!
//! The reference's matrix (`tests/test_admin_auth.py` at `f9f236e`): unconfigured 503 naming the
//! setting, public host 404, absent/malformed/wrong-slot/forged/expired tokens 401, an `azp`
//! outside the allowlist or a human without an admin role 403. The positive control proves the
//! handler was reached (501 in this slice, because the bodies land in the next one).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_auth::jwks::{FetchFuture, JwksSource};
use conveyor_inference_router::admin_auth::AdminVerifier;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use tower::ServiceExt;

const ISSUER: &str = "https://auth.example.test/realms/portal";
const SA_URL: &str = "https://auth.example.test/realms/portal/certs";
const USER_URL: &str = "https://portal.example.test/realms/portal/certs";
const SA_KID: &str = "sa-kid";
const USER_KID: &str = "user-kid";
const SA_AZP: &str = "provisioning-sa";
const FIXTURE: &str = include_str!("fixtures/admin-test-key.jwks.json");
const DER: &[u8] = include_bytes!("fixtures/admin-test-key.der");

/// The fixture key published under the kid the requested slot expects. One key, two kids: a token
/// minted for the user slot still fails in the SA slot because its kid is unknown THERE.
struct SlotJwks;

impl JwksSource for SlotJwks {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a> {
        let kid = if url == SA_URL { SA_KID } else { USER_KID };
        Box::pin(async move {
            let mut set: JwkSet = serde_json::from_str(FIXTURE).expect("fixture JWKS");
            for key in &mut set.keys {
                key.common.key_id = Some(kid.to_owned());
            }
            Ok(set)
        })
    }
}

fn config(
    sa_url: Option<&str>,
    user_url: Option<&str>,
    allowlist: Option<&str>,
    issuers: Option<&str>,
) -> Config {
    Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_KEYCLOAK_JWKS_URL" => sa_url.map(str::to_owned),
        "INFERENCE_ROUTER_PORTAL_JWKS_URL" => user_url.map(str::to_owned),
        "INFERENCE_ROUTER_ADMIN_AZP_ALLOWLIST" => allowlist.map(str::to_owned),
        "INFERENCE_ROUTER_ADMIN_JWT_ISSUERS" => issuers.map(str::to_owned),
        _ => None,
    })
    .expect("config")
}

fn configured() -> Config {
    config(Some(SA_URL), Some(USER_URL), Some(SA_AZP), Some(ISSUER))
}

fn state(config: Config) -> AppState {
    let catalog =
        Catalog::from_yaml("providers: []\ndefault_chain: []\n").expect("an empty catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let admin = Arc::new(AdminVerifier::from_config(&config, Arc::new(SlotJwks)));
    AppState::with_admin(config, router, AppDeps::default(), admin)
}

fn encode(kid: &str, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    jsonwebtoken::encode(&header, claims, &EncodingKey::from_rsa_der(DER)).expect("sign")
}

fn sa_token() -> String {
    encode(
        SA_KID,
        &json!({"azp": SA_AZP, "sub": "sa-subject", "iss": ISSUER, "exp": 4102444800u64}),
    )
}

fn user_token(roles: &[&str], tenant: Option<&str>) -> String {
    let mut claims =
        json!({"roles": roles, "sub": "human-subject", "iss": ISSUER, "exp": 4102444800u64});
    if let Some(tenant) = tenant {
        claims["tenant_id"] = json!(tenant);
    }
    encode(USER_KID, &claims)
}

fn admin_headers(sa: &str, user: &str) -> Vec<(&'static str, String)> {
    vec![
        ("authorization", format!("Bearer {sa}")),
        ("x-forwarded-user-authorization", format!("Bearer {user}")),
    ]
}

async fn request(
    state: AppState,
    path: &str,
    headers: &[(&'static str, String)],
) -> (StatusCode, Value) {
    let mut builder = Request::get(path);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let response = conveyor_inference_router::router::router(state)
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    // The 501 placeholder has an empty body; every refusal carries JSON.
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON body")
    };
    (status, body)
}

#[tokio::test]
async fn an_unconfigured_admin_api_is_503_naming_the_settings() {
    let (status, body) = request(
        state(config(None, None, None, None)),
        "/api/admin/catalog",
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["detail"],
        "admin API disabled: KEYCLOAK_JWKS_URL / ADMIN_AZP_ALLOWLIST not set"
    );
}

#[tokio::test]
async fn either_setting_missing_alone_is_503() {
    for (sa_url, allowlist, named) in [
        (None, Some(SA_AZP), "KEYCLOAK_JWKS_URL"),
        (Some(SA_URL), None, "ADMIN_AZP_ALLOWLIST"),
    ] {
        let (status, body) = request(
            state(config(sa_url, Some(USER_URL), allowlist, None)),
            "/api/admin/catalog",
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            body["detail"].as_str().expect("detail").contains(named),
            "{body}"
        );
    }
}

#[tokio::test]
async fn a_public_host_is_404_even_with_a_valid_pair() {
    let mut headers = admin_headers(&sa_token(), &user_token(&["super_admin"], None));
    headers.push(("host", "api.nonagentic.ai".to_owned()));
    let (status, _) = request(state(configured()), "/api/admin/catalog", &headers).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_tokens_are_401() {
    let configured_state = || state(configured());
    let cases: [Vec<(&'static str, String)>; 3] = [
        vec![],
        vec![("authorization", format!("Bearer {}", sa_token()))],
        vec![(
            "x-forwarded-user-authorization",
            format!("Bearer {}", user_token(&["super_admin"], None)),
        )],
    ];
    for headers in cases {
        let (status, _) = request(configured_state(), "/api/admin/catalog", &headers).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{headers:?}");
    }
}

#[tokio::test]
async fn an_azp_outside_the_allowlist_is_403() {
    let sa = encode(
        SA_KID,
        &json!({"azp": "some-other-client", "sub": "sa", "iss": ISSUER, "exp": 4102444800u64}),
    );
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&sa, &user_token(&["super_admin"], None)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_token_cannot_verify_in_the_other_slot() {
    // The user-slot token presented as the service account: its kid is not in the SA key set.
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(
            &user_token(&["super_admin"], None),
            &user_token(&["super_admin"], None),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // The service-account token presented as the forwarded human: it has no admin role anyway,
    // but the slot's key set does not know its kid either.
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&sa_token(), &sa_token()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_forged_signature_is_401() {
    // A real header and payload with a signature that is not the one the key produced: the
    // signature check must fail even though the kid resolves.
    let valid = sa_token();
    let mut parts = valid.split('.');
    let forged = format!(
        "{}.{}.AAAA",
        parts.next().expect("header"),
        parts.next().expect("payload")
    );
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&forged, &user_token(&["super_admin"], None)),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn alg_none_is_401() {
    use base64::Engine;
    let b64 = |raw: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes());
    let unsigned = format!(
        "{}.{}.",
        b64(&format!(r#"{{"alg":"none","kid":"{SA_KID}"}}"#)),
        b64(&format!(
            r#"{{"azp":"{SA_AZP}","sub":"sa","iss":"{ISSUER}","exp":4102444800}}"#
        ))
    );
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&unsigned, &user_token(&["super_admin"], None)),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_expired_token_is_401() {
    let expired = encode(
        SA_KID,
        &json!({"azp": SA_AZP, "sub": "sa", "iss": ISSUER, "exp": 1_000_000_000u64}),
    );
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&expired, &user_token(&["super_admin"], None)),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_unlisted_issuer_is_401_when_the_allowlist_is_set() {
    let other = encode(
        USER_KID,
        &json!({"roles": ["super_admin"], "sub": "human", "iss": "https://evil.example", "exp": 4102444800u64}),
    );
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&sa_token(), &other),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_human_without_an_admin_role_is_403() {
    let (status, _) = request(
        state(configured()),
        "/api/admin/catalog",
        &admin_headers(&sa_token(), &user_token(&["member"], None)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_valid_pair_reaches_the_handler() {
    // 501 is this slice's body placeholder: the point is that the auth layers passed.
    for roles in [vec!["super_admin"], vec!["tenant_admin"]] {
        let (status, _) = request(
            state(configured()),
            "/api/admin/catalog",
            &admin_headers(&sa_token(), &user_token(&roles, Some("tenant-a"))),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{roles:?}");
    }
}

#[tokio::test]
async fn every_admin_route_is_guarded_before_its_body() {
    for path in [
        "/api/admin/catalog",
        "/api/admin/tenants/tenant-a/calls?correlation_id=x",
        "/api/admin/tenants/tenant-a/inference-policy",
    ] {
        let (status, _) = request(state(configured()), path, &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
}
