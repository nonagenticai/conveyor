//! The `/api/admin/*` policy surface, through the real router and a real Postgres.
//!
//! The reference's matrix (`tests/test_admin_tenants_api.py`, `tests/test_policy_store.py` at
//! `f9f236e`): a PUT creates and activates version 1, a replay of the active document is
//! `unchanged`, an invalid document is 422 with every violation, activation rolls back by
//! version, the catalogue is the eligible platform providers, a `tenant_admin` reads its own
//! tenant only, and a policy written here is served by the store on the next epoch poll.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use conveyor_auth::jwks::{FetchFuture, JwksSource};
use conveyor_inference_router::admin_auth::AdminVerifier;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::policy::PolicyStore;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const ISSUER: &str = "https://auth.example.test/realms/portal";
const SA_URL: &str = "https://auth.example.test/realms/portal/certs";
const USER_URL: &str = "https://portal.example.test/realms/portal/certs";
const SA_KID: &str = "sa-kid";
const USER_KID: &str = "user-kid";
const SA_AZP: &str = "provisioning-sa";
const FIXTURE: &str = include_str!("fixtures/admin-test-key.jwks.json");
const DER: &[u8] = include_bytes!("fixtures/admin-test-key.der");

const TENANT_A: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
const TENANT_B: &str = "1b4e28ba-2fa1-41d2-883f-0016d3cca427";
const PLATFORM_TENANT: &str = "0195f48b-d000-7000-8000-000000000001";

const CATALOG: &str = "providers:
  - name: pgx-local
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:9/v1
    model: local-llm
  - name: pgx-local-b
    enabled: true
    customer_eligible: true
    protocol: openai
    endpoint: http://127.0.0.1:9/v1
    model: local-llm
default_chain: [pgx-local]
";

/// The fixture key published under the kid the requested slot expects.
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

fn config(policy_mode: &str, platform_ids: Option<&str>) -> Config {
    let policy_mode = policy_mode.to_owned();
    let platform_ids = platform_ids.map(str::to_owned);
    Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_KEYCLOAK_JWKS_URL" => Some(SA_URL.to_owned()),
        "INFERENCE_ROUTER_PORTAL_JWKS_URL" => Some(USER_URL.to_owned()),
        "INFERENCE_ROUTER_ADMIN_AZP_ALLOWLIST" => Some(SA_AZP.to_owned()),
        "INFERENCE_ROUTER_ADMIN_JWT_ISSUERS" => Some(ISSUER.to_owned()),
        "INFERENCE_ROUTER_POLICY_MODE" => Some(policy_mode.clone()),
        "COMMERCIAL_PLATFORM_TENANT_IDS" => platform_ids.clone(),
        _ => None,
    })
    .expect("config")
}

fn state(config: Config, pool: Option<PgPool>) -> AppState {
    let catalog = Catalog::from_yaml(CATALOG).expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let admin = Arc::new(AdminVerifier::from_config(&config, Arc::new(SlotJwks)));
    let deps = AppDeps {
        ledger: pool,
        ..AppDeps::default()
    };
    AppState::with_admin(config, router, deps, admin)
}

fn encode(kid: &str, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    jsonwebtoken::encode(&header, claims, &EncodingKey::from_rsa_der(DER)).expect("sign")
}

fn admin_headers(roles: &[&str], tenant: Option<&str>) -> Vec<(&'static str, String)> {
    let sa = encode(
        SA_KID,
        &json!({"azp": SA_AZP, "sub": "sa-subject", "iss": ISSUER, "exp": 4102444800u64}),
    );
    let mut user = json!({"realm_access": {"roles": roles}, "sub": "human-subject", "iss": ISSUER, "exp": 4102444800u64});
    if let Some(tenant) = tenant {
        user["tenant_id"] = json!(tenant);
    }
    let user = encode(USER_KID, &user);
    vec![
        ("authorization", format!("Bearer {sa}")),
        ("x-forwarded-user-authorization", format!("Bearer {user}")),
    ]
}

fn hop(provider: &str) -> Value {
    json!({
        "target": {"kind": "platform", "provider": provider},
        "model": "local-llm",
        "failover_on_error": true,
        "failover_on_validation": false,
    })
}

fn local_doc(hops: Option<Vec<Value>>) -> Value {
    json!({
        "schema": 1,
        "setup": "local",
        "byo_mode": "local_allowed",
        "tiers": {"medium": hops.unwrap_or_else(|| vec![hop("pgx-local")])},
        "tier_inheritance": "nearest_larger_then_smaller",
        "limits": {
            "rpm": 120,
            "max_concurrency": 8,
            "caller_rpm": 60,
            "eur_per_minute": "1.00",
            "eur_per_hour": "20.00",
            "loop_threshold": 20,
            "loop_window_s": 300,
            "max_hops": 2,
            "max_cost_eur_per_request": "0.50",
        },
        "cascade": {"enabled": false},
    })
}

fn owner_doc() -> Value {
    let mut doc = local_doc(Some(vec![hop("pgx-local")]));
    doc["setup"] = json!("owner_commercial");
    doc["tiers"] = json!({
        "small": [hop("pgx-local")],
        "medium": [
            {"target": {"kind": "owner", "provider": "deepseek"}, "model": "deepseek-v4-pro"},
            hop("pgx-local"),
        ],
    });
    doc
}

async fn send(
    state: AppState,
    method: Method,
    path: &str,
    headers: &[(&'static str, String)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = conveyor_inference_router::router::router(state)
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON body")
    };
    (status, body)
}

fn policy_path(tenant: &str) -> String {
    format!("/api/admin/tenants/{tenant}/inference-policy")
}

#[tokio::test]
async fn put_get_and_the_unchanged_replay() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let path = policy_path(TENANT_A);

    let (status, body) = send(
        state.clone(),
        Method::PUT,
        &path,
        &headers,
        Some(local_doc(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], 1);
    assert_eq!(body["active_version"], 1);
    assert_eq!(body["unchanged"], false);
    assert_eq!(body["doc_sha256"].as_str().expect("sha").len(), 64);

    // A replay of the ACTIVE document creates no version.
    let (status, again) = send(
        state.clone(),
        Method::PUT,
        &path,
        &headers,
        Some(local_doc(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["unchanged"], true);
    assert_eq!(again["version"], 1);

    let (status, got) = send(state, Method::GET, &path, &headers, None).await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["active_version"], 1);
    assert_eq!(got["doc"]["setup"], "local");
    assert_eq!(got["versions"][0]["version"], 1);
    assert_eq!(got["versions"][0]["actor_kind"], "super_admin");
    assert!(
        got["versions"][0]["created_at"]
            .as_str()
            .expect("created_at")
            .ends_with("+00:00")
    );
}

#[tokio::test]
async fn an_invalid_document_is_422_with_every_violation() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);

    let (status, body) = send(
        state,
        Method::PUT,
        &policy_path(TENANT_A),
        &headers,
        Some(owner_doc()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"], "policy_invalid");
    let violations = body["violations"].as_array().expect("violations");
    assert!(!violations.is_empty());
    for violation in violations {
        assert!(violation.get("path").is_some(), "{violation}");
        assert!(violation.get("code").is_some(), "{violation}");
    }
    let codes: Vec<&str> = violations
        .iter()
        .filter_map(|violation| violation["code"].as_str())
        .collect();
    assert!(codes.contains(&"setup2_disabled"), "{codes:?}");
    assert!(codes.contains(&"owner_provider_unconfigured"), "{codes:?}");
}

#[tokio::test]
async fn activation_rolls_back_by_version_and_an_unknown_version_is_404() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let path = policy_path(TENANT_A);

    send(
        state.clone(),
        Method::PUT,
        &path,
        &headers,
        Some(local_doc(None)),
    )
    .await;
    send(
        state.clone(),
        Method::PUT,
        &path,
        &headers,
        Some(local_doc(Some(vec![hop("pgx-local"), hop("pgx-local-b")]))),
    )
    .await;

    let activate = format!("{path}/activate");
    let (status, body) = send(
        state.clone(),
        Method::POST,
        &activate,
        &headers,
        Some(json!({"version": 1})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["active_version"], 1);
    let (status, _) = send(
        state,
        Method::POST,
        &activate,
        &headers,
        Some(json!({"version": 7})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_non_uuid_tenant_is_422() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let (status, body) = send(
        state,
        Method::GET,
        "/api/admin/tenants/not-a-uuid/inference-policy",
        &headers,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"], "tenant_id must be a non-nil uuid");
}

#[tokio::test]
async fn the_catalogue_is_the_eligible_platform_providers() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let (status, body) = send(state, Method::GET, "/api/admin/catalog", &headers, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["platform"],
        json!([
            {"provider": "pgx-local", "models": ["local-llm"]},
            {"provider": "pgx-local-b", "models": ["local-llm"]},
        ])
    );
    assert_eq!(body["owner"], json!([]));
    assert_eq!(
        body["byo_providers"],
        json!([
            "openai",
            "anthropic",
            "xai",
            "deepseek",
            "openai_compatible"
        ])
    );
}

#[tokio::test]
async fn a_tenant_admin_reads_its_own_tenant_only_and_never_writes() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["tenant_admin"], Some(TENANT_A));

    let (status, _) = send(
        state.clone(),
        Method::GET,
        &policy_path(TENANT_A),
        &headers,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        state.clone(),
        Method::GET,
        &policy_path(TENANT_B),
        &headers,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        state,
        Method::PUT,
        &policy_path(TENANT_A),
        &headers,
        Some(local_doc(None)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_platform_tenant_is_not_administered_here() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(
        config("store", Some(PLATFORM_TENANT)),
        Some(db.pool.clone()),
    );
    let headers = admin_headers(&["super_admin"], None);
    let (status, body) = send(
        state,
        Method::GET,
        &policy_path(PLATFORM_TENANT),
        &headers,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"], "platform_tenant");
}

#[tokio::test]
async fn calls_requires_a_correlation_id() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let (status, _) = send(
        state,
        Method::GET,
        &format!("/api/admin/tenants/{TENANT_A}/calls"),
        &headers,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_store_is_disabled_in_off_mode() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let state = state(config("off", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let (status, _) = send(state, Method::GET, &policy_path(TENANT_A), &headers, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn a_written_policy_is_served_after_the_epoch_poll() {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    let store = Arc::new(PolicyStore::new(
        Some(db.pool.clone()),
        Duration::from_secs(15),
    ));
    // Cache the "no policy" answer first: only the epoch poll can clear it.
    assert!(store.get_active(TENANT_A).await.is_none());

    let state = state(config("store", None), Some(db.pool.clone()));
    let headers = admin_headers(&["super_admin"], None);
    let (status, body) = send(
        state,
        Method::PUT,
        &policy_path(TENANT_A),
        &headers,
        Some(local_doc(None)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    tokio::spawn(Arc::clone(&store).run_epoch_poller(Duration::from_millis(50)));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(policy) = store.get_active(TENANT_A).await {
            assert_eq!(policy.version, 1);
            assert_eq!(policy.doc.setup, "local");
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the epoch poll did not clear the cached negative"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
