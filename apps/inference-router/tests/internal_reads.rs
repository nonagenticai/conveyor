//! The six internal reads, against a real Postgres, through the real router.
//!
//! The golden replay (`verify.sh`) is the parity verdict; this suite pins the semantics that a
//! golden diff would only name: the window rules, NULL shares (never 0), grouping by
//! `request_sha256`, the unhashed-call count, and the capture allowlist.

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use serde_json::Value;
use tower::ServiceExt;

const SECRET: &str = "stub-internal-secret";
const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const HASH_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const HASH_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

fn state(pool: sqlx::PgPool, capture_allowlist: &str) -> AppState {
    let allowlist = capture_allowlist.to_owned();
    let config = Config::from_lookup(move |key| match key {
        "INFERENCE_ROUTER_INTERNAL_SECRET" => Some(SECRET.to_owned()),
        "INFERENCE_CAPTURE_ALLOWED_CALLERS" => Some(allowlist.clone()),
        _ => None,
    })
    .expect("config");
    let catalog = Catalog::from_yaml(
        "providers:\n  - name: local\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://127.0.0.1:9/v1\n    model: local-llm\ndefault_chain: [local]\n",
    )
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    AppState::with_deps(
        config,
        router,
        AppDeps {
            ledger: Some(pool),
            ..AppDeps::default()
        },
    )
}

async fn seed(pool: &sqlx::PgPool) {
    // Two identical requests (same request hash, same answer) by one caller; one unhashed answer;
    // one failed call with no response hash. `traffic_source` NULL on the third = `unknown`.
    for (requested, request_sha, response_sha, status, caller, temperature, seed, source) in [
        (
            "local-llm",
            HASH_A,
            Some(HASH_B),
            "ok",
            Some("alice"),
            0.0_f64,
            Some(42_i64),
            Some("live"),
        ),
        (
            "local-llm",
            HASH_A,
            Some(HASH_B),
            "ok",
            Some("alice"),
            0.0,
            Some(42),
            Some("live"),
        ),
        (
            "local-llm",
            HASH_C,
            Some(HASH_D),
            "ok",
            None,
            0.1,
            None,
            None,
        ),
        // ok but with NO response hash: it cannot be compared, so repeat-groups must not count
        // it (it is reported as `unhashed_calls` instead).
        (
            "other-model",
            HASH_E,
            None,
            "ok",
            None,
            0.1,
            None,
            Some("live"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO inference_call (
                requested_model, request_sha256, response_sha256, latency_ms, status,
                correlation_id, caller_worker, licence_id, licence_source, served_provider,
                served_model, temperature, seed, traffic_source)
             VALUES ($1, $2, $3, 12, $4,
                '00000000-0000-4000-8000-0000000000e1', $5,
                '00000000-0000-4000-8000-0000000000c1', 'header', 'local',
                'local-llm', $6, $7, $8)",
        )
        .bind(requested)
        .bind(request_sha)
        .bind(response_sha)
        .bind(status)
        .bind(caller)
        .bind(temperature)
        .bind(seed)
        .bind(source)
        .execute(pool)
        .await
        .expect("seed row");
    }
}

async fn get(state: AppState, path: &str) -> (StatusCode, Value) {
    get_with(state, path, &[("x-internal-secret", SECRET)]).await
}

async fn get_with(state: AppState, path: &str, headers: &[(&str, &str)]) -> (StatusCode, Value) {
    let mut request = Request::get(path);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = conveyor_inference_router::router::router(state)
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 22)
        .await
        .expect("body");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn seeded() -> sqlx::PgPool {
    let db = common::fresh().await;
    conveyor_inference_router::db::migrate(&db.pool)
        .await
        .expect("migrations");
    seed(&db.pool).await;
    db.pool.clone()
}

#[tokio::test]
async fn the_summary_counts_the_whole_table_and_the_window() {
    let pool = seeded().await;
    let (status, body) = get(state(pool, ""), "/api/internal/inference-calls/summary").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"], "inference_call");
    assert_eq!(body["window"]["days"], 1);
    assert_eq!(body["rows_total"], 4);
    assert_eq!(body["rows_24h"], 4);
    assert_eq!(body["rows_window"], 4);
    assert!(body["last_row_at"].is_string());
}

#[tokio::test]
async fn the_call_list_filters_totals_and_never_returns_bodies() {
    let pool = seeded().await;
    let (status, body) = get(
        state(pool.clone(), ""),
        "/api/internal/inference-calls?correlation_id=00000000-0000-4000-8000-0000000000e1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 4);
    assert_eq!(body["limit"], 100);
    assert_eq!(body["rows"].as_array().map(Vec::len), Some(4));
    let row = &body["rows"][0];
    assert_eq!(row["requested_model"], "other-model");
    assert_eq!(row["status"], "ok");
    assert_eq!(row["captured"], false);
    assert_eq!(row["licence_id"], "00000000-0000-4000-8000-0000000000c1");
    assert!(row.get("request_json").is_none());
    assert!(row.get("request_text").is_none());

    // `caller` narrows, and a page limit applies.
    let (status, body) = get(
        state(pool, ""),
        "/api/internal/inference-calls?caller=alice&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2);
    assert_eq!(body["limit"], 1);
    assert_eq!(body["rows"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn out_of_range_parameters_are_422_like_fastapi() {
    let pool = seeded().await;
    for path in [
        "/api/internal/inference-calls?limit=0",
        "/api/internal/inference-calls?limit=501",
        "/api/internal/inference-calls/summary?days=0",
        "/api/internal/inference-calls/summary?days=91",
        "/api/internal/served-models/timeline?days=91",
        "/api/internal/inference-calls/repeat-groups?source=weird",
        "/api/internal/inference-calls/pinning-summary?source=weird",
    ] {
        let (status, body) = get(state(pool.clone(), ""), path).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert!(body["detail"].is_array(), "{path}");
    }
}

#[tokio::test]
async fn the_timeline_is_zero_filled_and_its_shares_are_null_not_zero() {
    let pool = seeded().await;
    let (status, body) = get(
        state(pool.clone(), ""),
        "/api/internal/served-models/timeline?days=2",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["window"]["days"], 2);
    let series = body["series"].as_array().expect("series");
    // The empty day, then today split by requested model (the row order is day, model).
    assert_eq!(series.len(), 3, "the empty day plus two model groups today");
    let empty = &series[0];
    assert_eq!(empty["n"], 0);
    assert!(empty["requested_model"].is_null());
    assert!(empty["t0_share"].is_null());
    assert!(empty["seed_share"].is_null());
    assert!(empty["licensed_share"].is_null());
    let local = &series[1];
    assert_eq!(local["requested_model"], "local-llm");
    assert_eq!(local["n"], 3);
    assert_eq!(local["t0_share"], 2.0 / 3.0);
    assert_eq!(local["seed_share"], 2.0 / 3.0);
    assert_eq!(local["licensed_share"], 1.0);
    assert_eq!(local["n_licence_recorded"], 3);
    assert_eq!(series[2]["requested_model"], "other-model");
    assert_eq!(series[2]["n"], 1);
}

#[tokio::test]
async fn repeat_groups_keys_on_the_request_hash_and_counts_the_unhashed() {
    let pool = seeded().await;
    let (status, body) = get(
        state(pool.clone(), ""),
        "/api/internal/inference-calls/repeat-groups",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["grouped_on"], "request_sha256");
    let groups = body["groups"].as_array().expect("groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["request_sha256"], HASH_A);
    assert_eq!(groups[0]["n_calls"], 2);
    assert_eq!(groups[0]["n_distinct_responses"], 1);
    assert_eq!(
        groups[0]["requested_models"],
        serde_json::json!(["local-llm"])
    );
    assert_eq!(groups[0]["sources"], serde_json::json!(["live"]));
    let summary = &body["summary"];
    assert_eq!(summary["groups"], 1);
    assert_eq!(summary["disagreeing"], 0);
    assert_eq!(summary["calls"], 2);
    assert_eq!(summary["unhashed_calls"], 1);
    assert_eq!(summary["by_source"]["live"]["groups"], 1);
    assert_eq!(summary["by_source"]["live"]["calls"], 2);
    assert_eq!(summary["by_source"]["benchmark"]["groups"], 0);
    assert_eq!(summary["callers"][0]["caller"], "alice");
    assert_eq!(summary["callers"][0]["groups"], 1);

    // `source=all` never filters; `source=benchmark` empties the groups but not by_source.
    let (status, body) = get(
        state(pool, ""),
        "/api/internal/inference-calls/repeat-groups?source=benchmark",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["groups"].as_array().map(Vec::len), Some(0));
    assert_eq!(body["summary"]["by_source"]["live"]["groups"], 1);
}

#[tokio::test]
async fn the_pinning_summary_counts_every_call_whatever_its_status() {
    let pool = seeded().await;
    let (status, body) = get(
        state(pool, ""),
        "/api/internal/inference-calls/pinning-summary",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"], "all");
    assert_eq!(body["totals"]["n"], 4);
    assert_eq!(body["totals"]["temperature_zero_n"], 2);
    assert_eq!(body["totals"]["pinned_n"], 2);
    let callers = body["callers"].as_array().expect("callers");
    assert_eq!(callers.len(), 2);
    assert_eq!(callers[0]["caller"], "alice");
    assert_eq!(callers[0]["n"], 2);
    assert_eq!(callers[0]["pinned_n"], 2);
    // NULL caller last when the counts tie.
    assert!(callers[1]["caller"].is_null());
    assert_eq!(callers[1]["n"], 2);
}

#[tokio::test]
async fn the_capture_route_is_forbidden_then_not_found() {
    let pool = seeded().await;
    // The live state: no allowlist, so even a correct secret cannot read a body.
    let (status, body) = get(
        state(pool.clone(), ""),
        "/api/internal/inference-calls/1/capture",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body["detail"],
        "captured bodies are readable only by INFERENCE_CAPTURE_ALLOWED_CALLERS"
    );

    // Allowlisted, but the row stored no bodies (this port does not write them): 404.
    let (status, body) = get_with(
        state(pool, "tester"),
        "/api/internal/inference-calls/1/capture",
        &[("x-internal-secret", SECRET), ("x-app-id", "tester")],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["detail"], "not captured");
}
