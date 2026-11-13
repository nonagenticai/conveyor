//! `/api/replay/*` and `/api/canary/*` — CRUD parity with the reference routers.
//!
//! The oracle is the private reference checkout at `inference-router` @ f9f236e:
//! `src/routers/replay.py`, `src/routers/canary.py` and `src/security.py`. The two bodies the
//! capture pins are asserted verbatim (the missing-tenant 401 and the canary no-row 404).

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_inference_router::billing::AllowAllGate;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::TestDb;

const TENANT_A: &str = "00000000-0000-4000-8000-0000000000a1";
const TENANT_B: &str = "00000000-0000-4000-8000-0000000000b2";
const LOG_ID: &str = "00000000-0000-4000-8000-0000000000c1";
const OTHER_LOG_ID: &str = "00000000-0000-4000-8000-0000000000c2";
const CORRELATION_ID: &str = "00000000-0000-4000-8000-0000000000e1";

async fn state() -> (TestDb, AppState) {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("the service's own migrations");
    // These routes never dial a provider; the catalog only has to construct.
    let catalog = Catalog::from_yaml(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://127.0.0.1:1/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n",
    )
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let config = Config::from_lookup(|_| None).expect("defaults");
    let state = AppState::with_deps(
        config,
        router,
        AppDeps {
            ledger: Some(db.pool.clone()),
            budget: std::sync::Arc::new(AllowAllGate),
            ..AppDeps::default()
        },
    );
    (db, state)
}

async fn call(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let response = conveyor_inference_router::router::router(state.clone())
        .oneshot(request)
        .await
        .expect("router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a JSON body")
    };
    (status, body)
}

fn json_request(
    method: axum::http::Method,
    path: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(tenant) = tenant {
        builder = builder.header("x-tenant-id", tenant);
    }
    if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request")
    } else {
        builder.body(Body::empty()).expect("request")
    }
}

fn get(path: &str) -> Request<Body> {
    json_request(axum::http::Method::GET, path, None, None)
}

fn get_as(path: &str, tenant: &str) -> Request<Body> {
    json_request(axum::http::Method::GET, path, Some(tenant), None)
}

fn patch_json(path: &str, tenant: &str, body: Value) -> Request<Body> {
    json_request(axum::http::Method::PATCH, path, Some(tenant), Some(body))
}

fn post_json(path: &str, tenant: Option<&str>, body: Value) -> Request<Body> {
    json_request(axum::http::Method::POST, path, tenant, Some(body))
}

#[allow(clippy::too_many_arguments)]
async fn seed_replay(
    pool: &sqlx::PgPool,
    log_id: &str,
    correlation_id: &str,
    tenant: &str,
    worker_name: &str,
    version: &str,
    validation_passed: Option<bool>,
    judge_score: Option<f64>,
) {
    sqlx::query(
        "INSERT INTO replay_invocation_log \
         (id, correlation_id, worker_name, version, started_at, latency_ms, prompt_tokens, \
          completion_tokens, validation_passed, judge_score, input, tenant_id) \
         VALUES ($1::uuid, $2::uuid, $3, $4, now(), 12, 3, 4, $5, $6::float8, \
                 '[{\"role\":\"user\",\"content\":\"hi\"}]'::jsonb, $7::uuid)",
    )
    .bind(log_id)
    .bind(correlation_id)
    .bind(worker_name)
    .bind(version)
    .bind(validation_passed)
    .bind(judge_score)
    .bind(tenant)
    .execute(pool)
    .await
    .expect("seed a replay row");
}

fn canary_body(worker_name: &str) -> Value {
    json!({
        "worker_name": worker_name,
        "active_version": 2,
        "canary_version": 3,
        "canary_percent": 25,
        "canary_started_at": "2026-04-30T12:00:00Z",
        "canary_ends_at": "2026-04-30T13:00:00Z",
        "rollback_thresholds": {"failure_rate": 0.05, "p95_latency_ms": 2000.0}
    })
}

// ── replay ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn replay_list_refuses_a_missing_tenant_with_the_live_body() {
    let (_db, state) = state().await;
    let (status, body) = call(&state, get("/api/replay/invocation-log")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body,
        json!({"detail": "Missing or unresolved X-Tenant-Id; caller is not authenticated"})
    );
}

#[tokio::test]
async fn replay_list_is_scoped_to_the_caller_tenant_and_filters() {
    let (db, state) = state().await;
    seed_replay(
        &db.pool,
        LOG_ID,
        CORRELATION_ID,
        TENANT_A,
        "prompt-x",
        "rev-3",
        Some(false),
        Some(0.25),
    )
    .await;
    seed_replay(
        &db.pool,
        OTHER_LOG_ID,
        "00000000-0000-4000-8000-0000000000e2",
        TENANT_B,
        "prompt-y",
        "rev-9",
        Some(true),
        None,
    )
    .await;

    let (status, body) = call(&state, get_as("/api/replay/invocation-log", TENANT_A)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["rows"][0]["worker_name"], "prompt-x");
    assert_eq!(body["rows"][0]["version"], "rev-3");
    assert_eq!(body["rows"][0]["validation_passed"], false);
    assert_eq!(body["rows"][0]["judge_score"], 0.25);
    assert_eq!(
        body["rows"][0]["input"],
        json!([{"role": "user", "content": "hi"}])
    );

    // Another tenant's rows are invisible, never 403 (the scope is the identity header).
    let (status, body) = call(
        &state,
        get_as("/api/replay/invocation-log?worker_name=prompt-y", TENANT_A),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);

    let (status, body) = call(
        &state,
        get_as("/api/replay/invocation-log?only_failed=true", TENANT_A),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);

    let (status, body) = call(
        &state,
        get_as("/api/replay/invocation-log?only_failed=true", TENANT_B),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the passed row is not a failed example"
    );
    assert_eq!(body["count"], 0);

    let (status, body) = call(
        &state,
        get_as("/api/replay/invocation-log?version=rev-3", TENANT_A),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
}

#[tokio::test]
async fn replay_list_limit_out_of_range_is_a_422() {
    let (_db, state) = state().await;
    let (status, body) = call(
        &state,
        get_as("/api/replay/invocation-log?limit=0", TENANT_A),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"][0]["loc"], json!(["query", "limit"]));

    let (status, _) = call(
        &state,
        get_as("/api/replay/invocation-log?limit=201", TENANT_A),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn replay_patch_backfills_by_id_and_hides_another_tenants_row() {
    let (db, state) = state().await;
    seed_replay(
        &db.pool,
        LOG_ID,
        CORRELATION_ID,
        TENANT_A,
        "prompt-x",
        "rev-3",
        None,
        None,
    )
    .await;

    // Cross-tenant: a 404, so the response cannot confirm the row exists.
    let path = format!("/api/replay/invocation-log/{LOG_ID}");
    let (status, body) = call(
        &state,
        patch_json(&path, TENANT_B, json!({"validation_passed": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"detail": format!("replay_invocation_log row {LOG_ID} not found")})
    );

    let (status, body) = call(
        &state,
        patch_json(
            &path,
            TENANT_A,
            json!({"validation_passed": true, "judge_score": 0.875}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], LOG_ID);
    assert_eq!(body["validation_passed"], true);
    assert_eq!(body["judge_score"], 0.875);

    let (status, body) = call(&state, patch_json(&path, TENANT_A, json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["detail"]
            .as_str()
            .expect("detail")
            .contains("validation_passed")
    );

    // pydantic's [0.0, 1.0] bound is the 422 envelope, not the handler's 400.
    for score in [1.5, -0.1] {
        let (status, body) = call(
            &state,
            patch_json(&path, TENANT_A, json!({"judge_score": score})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["detail"][0]["loc"], json!(["body", "judge_score"]));
    }

    let (status, _) = call(
        &state,
        patch_json(
            "/api/replay/invocation-log/not-a-uuid",
            TENANT_A,
            json!({"validation_passed": true}),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "an invalid path uuid is a 422"
    );
}

#[tokio::test]
async fn replay_patch_by_correlation_backfills_expected() {
    let (db, state) = state().await;
    seed_replay(
        &db.pool,
        LOG_ID,
        CORRELATION_ID,
        TENANT_A,
        "prompt-x",
        "rev-3",
        None,
        None,
    )
    .await;
    let path = format!("/api/replay/invocation-log/by-correlation/{CORRELATION_ID}");

    let (status, body) = call(
        &state,
        patch_json(&path, TENANT_A, json!({"expected": {"answer": "yes"}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], LOG_ID);

    let stored: Value =
        sqlx::query_scalar("SELECT expected FROM replay_invocation_log WHERE id = $1::uuid")
            .bind(LOG_ID)
            .fetch_one(&db.pool)
            .await
            .expect("expected backfilled");
    assert_eq!(stored, json!({"answer": "yes"}));

    // A broken correlation chain is loud, and another tenant's row is the same 404.
    let unknown = "00000000-0000-4000-8000-0000000000e9";
    let (status, body) = call(
        &state,
        patch_json(
            &format!("/api/replay/invocation-log/by-correlation/{unknown}"),
            TENANT_A,
            json!({"validation_passed": true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["detail"]
            .as_str()
            .expect("detail")
            .contains("broken replay correlation chain")
    );

    let (status, _) = call(
        &state,
        patch_json(&path, TENANT_B, json!({"validation_passed": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── canary ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn canary_get_is_permissive_but_404s_with_the_live_body() {
    let (_db, state) = state().await;
    // No identity headers at all: the read is permissive on purpose (reference `caller_identity`).
    let (status, body) = call(&state, get("/api/canary/prompt/golden-worker")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"detail": "No canary routing row for worker 'golden-worker'"})
    );
}

#[tokio::test]
async fn canary_configure_read_promote_round_trip() {
    let (_db, state) = state().await;
    let (status, body) = call(
        &state,
        post_json(
            "/api/canary/prompt/configure",
            Some(TENANT_A),
            canary_body("alpha"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["worker_name"], "alpha");
    assert_eq!(body["active_version"], 2);
    assert_eq!(body["canary_version"], 3);
    assert_eq!(body["canary_percent"], 25);
    assert!(
        body["canary_started_at"]
            .as_str()
            .expect("a timestamp")
            .starts_with("2026-04-30T12:00:00")
    );
    assert_eq!(
        body["rollback_thresholds"],
        json!({"failure_rate": 0.05, "p95_latency_ms": 2000.0})
    );

    let (status, body) = call(&state, get("/api/canary/prompt/alpha")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["canary_version"], 3);

    let (status, body) = call(
        &state,
        post_json(
            "/api/canary/promote-prompt",
            Some(TENANT_A),
            json!({"worker_name": "alpha"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"worker_name": "alpha", "active_version": 3}));

    let (status, body) = call(&state, get("/api/canary/prompt/alpha")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active_version"], 3);
    assert_eq!(body["canary_version"], Value::Null);
    assert_eq!(body["canary_percent"], 0);
    assert_eq!(body["canary_started_at"], Value::Null);
    assert_eq!(body["canary_ends_at"], Value::Null);
}

#[tokio::test]
async fn canary_rollback_keeps_the_active_version_and_is_idempotent() {
    let (_db, state) = state().await;
    let (status, _) = call(
        &state,
        post_json(
            "/api/canary/prompt/configure",
            Some(TENANT_A),
            canary_body("alpha"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(
        &state,
        post_json(
            "/api/canary/rollback-prompt",
            Some(TENANT_A),
            json!({"worker_name": "alpha"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Rollback reverts to the known-good active version; it never promotes the canary.
    assert_eq!(body, json!({"worker_name": "alpha", "active_version": 2}));

    let (status, body) = call(&state, get("/api/canary/prompt/alpha")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active_version"], 2);
    assert_eq!(body["canary_version"], Value::Null);

    // Idempotent: re-calling on a cleared row returns the same version.
    let (status, body) = call(
        &state,
        post_json(
            "/api/canary/rollback-prompt",
            Some(TENANT_A),
            json!({"worker_name": "alpha"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active_version"], 2);

    for path in ["/api/canary/promote-prompt", "/api/canary/rollback-prompt"] {
        let (status, body) = call(
            &state,
            post_json(
                path,
                Some(TENANT_A),
                json!({"worker_name": "golden-worker"}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            body,
            json!({"detail": "No canary routing row for worker 'golden-worker'"})
        );
    }
}

#[tokio::test]
async fn canary_writes_reject_an_invalid_worker_name() {
    let (_db, state) = state().await;
    let body = canary_body("bad name");
    let (status, response) = call(
        &state,
        post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        response["detail"]
            .as_str()
            .expect("detail")
            .contains("worker_name")
    );

    let (status, response) = call(
        &state,
        post_json(
            "/api/canary/rollback-prompt",
            Some(TENANT_A),
            json!({"worker_name": "bad/name"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        response["detail"]
            .as_str()
            .expect("detail")
            .contains("worker_name")
    );

    let (status, _) = call(&state, get("/api/canary/prompt/bad%20name")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn canary_configure_enforces_the_monitor_threshold_allowlist() {
    let (_db, state) = state().await;

    let mut body = canary_body("alpha");
    body["rollback_thresholds"] = json!({
        "failure_rate": 0.05,
        "p95_latency_ms": 2000.0,
        "cost_usd": 1.0
    });
    let (status, response) = call(
        &state,
        post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(
        response["rollback_thresholds"],
        json!({"failure_rate": 0.05, "p95_latency_ms": 2000.0, "cost_usd": 1.0})
    );

    // The pre-fix spellings are a 400 that names the units, not a silent no-op.
    for legacy in ["failurerate", "failure_rate_pct", "p95_ms"] {
        let mut body = canary_body("alpha");
        body["rollback_thresholds"] = json!({legacy: 5.0});
        let (status, response) = call(
            &state,
            post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{legacy}");
        let detail = response["detail"].as_str().expect("detail");
        assert!(detail.contains("rollback_thresholds"), "{detail}");
        assert!(detail.contains(legacy), "{detail}");
        assert!(detail.contains("fraction"), "{detail}");
        assert!(detail.contains("milliseconds"), "{detail}");
    }
}

#[tokio::test]
async fn canary_configure_cross_field_rules_and_field_bounds() {
    let (_db, state) = state().await;

    let mut body = canary_body("alpha");
    body["canary_version"] = json!(2);
    let (status, response) = call(
        &state,
        post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        response["detail"]
            .as_str()
            .expect("detail")
            .contains("must differ from active_version")
    );

    let mut body = canary_body("alpha");
    body["canary_ends_at"] = json!("2026-04-30T11:00:00Z");
    let (status, response) = call(
        &state,
        post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        response["detail"]
            .as_str()
            .expect("detail")
            .contains("canary_ends_at")
    );

    for (field, value) in [("canary_percent", json!(101)), ("canary_version", json!(0))] {
        let mut body = canary_body("alpha");
        body[field] = value;
        let (status, response) = call(
            &state,
            post_json("/api/canary/prompt/configure", Some(TENANT_A), body),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert_eq!(response["detail"][0]["loc"], json!(["body", field]));
    }
}

#[tokio::test]
async fn canary_writes_require_a_resolved_tenant() {
    let (_db, state) = state().await;
    let cases = [
        (
            "/api/canary/promote-prompt",
            json!({"worker_name": "alpha"}),
        ),
        ("/api/canary/prompt/configure", canary_body("alpha")),
        (
            "/api/canary/rollback-prompt",
            json!({"worker_name": "alpha"}),
        ),
    ];
    for (path, body) in cases {
        let (status, response) = call(&state, post_json(path, None, body)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(
            response,
            json!({"detail": "Missing or unresolved X-Tenant-Id; caller is not authenticated"})
        );
    }
}
