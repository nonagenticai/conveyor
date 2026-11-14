//! End-to-end HTTP contract, exercised through the real router, the real
//! runner and the real process supervision — only `lake`/`lean` are stand-ins.
//!
//! A fake toolchain rather than a mocked runner is deliberate: the parts that
//! have actually broken in this service are the subprocess ones (timeout,
//! process-group kill, environment replacement), and a mock cannot fail those.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use conveyor_lean4_prover::admission::Gate;
use conveyor_lean4_prover::auth::LicenceClient;
use conveyor_lean4_prover::config::Config;
use conveyor_lean4_prover::metrics::Metrics;
use conveyor_lean4_prover::runner::LeanRunner;
use conveyor_lean4_prover::state::AppState;

const FAKE_LEAN: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Lean (version 4.30.0-rc2, x86_64-unknown-linux-gnu)"
  exit 0
fi
for last; do :; done
source_text="$(cat "$last")"
case "$source_text" in
  *TIMEOUT*)
    sleep 30
    ;;
  *bad_tactic*|*BAD*)
    echo "$last:1:1: error: unknown identifier 'nope'" >&2
    exit 1
    ;;
  *)
    echo "Nat → Nat"
    exit 0
    ;;
esac
"#;

struct Harness {
    _dir: tempfile::TempDir,
    project: String,
    bin: String,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("project");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&project).expect("project dir");
    std::fs::create_dir_all(&bin).expect("bin dir");
    for name in ["lake", "lean"] {
        let path = bin.join(name);
        std::fs::write(&path, FAKE_LEAN).expect("write fake toolchain");
        let mut perms = std::fs::metadata(&path).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
    }
    Harness {
        _dir: dir,
        project: project.display().to_string(),
        bin: bin.display().to_string(),
    }
}

impl Harness {
    fn config(&self, overrides: &[(&str, &str)]) -> Config {
        let mut pairs: HashMap<String, String> = [
            ("ENABLE_AUTH", "false"),
            ("LEAN4_PROVER_PROJECT_DIR", self.project.as_str()),
            (
                "LEAN4_PROVER_LEAN_BINARY",
                format!("{}/lean", self.bin).as_str(),
            ),
            (
                "LEAN4_PROVER_LAKE_BINARY",
                format!("{}/lake", self.bin).as_str(),
            ),
            // The fake bin comes first, but the child environment is REPLACED
            // (not extended), so coreutils have to be named explicitly or the
            // stand-in toolchain cannot call `cat`/`sleep` at all.
            (
                "LEAN4_PROVER_SUBPROCESS_PATH",
                format!("{}:/bin:/usr/bin", self.bin).as_str(),
            ),
            ("LEAN4_PROVER_TIMEOUT", "5"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        for (k, v) in overrides {
            pairs.insert((*k).to_string(), (*v).to_string());
        }
        Config::from_lookup(|key| pairs.get(key).cloned()).expect("test config parses")
    }

    fn app(&self, cfg: Config) -> Router {
        let cfg = Arc::new(cfg);
        let metrics = Arc::new(Metrics::default());
        let licence = cfg.enable_auth.then(|| {
            Arc::new(
                LicenceClient::new(
                    cfg.license_introspect_url.clone(),
                    Duration::from_secs(cfg.license_introspect_timeout_s),
                )
                .expect("client"),
            )
        });
        let state = AppState {
            runner: LeanRunner::new(Arc::clone(&cfg), Arc::clone(&metrics)),
            gate: Arc::new(Gate::new(
                cfg.lane_max_active,
                cfg.lane_max_waiting,
                cfg.lane_max_wait_s,
                Arc::clone(&metrics),
            )),
            cfg,
            metrics,
            licence,
        };
        conveyor_lean4_prover::build_app(state)
    }
}

async fn send(app: Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(request).await.expect("router is infallible");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

fn post(path: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request builds")
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("request builds")
}

#[tokio::test]
async fn liveness_answers_without_contacting_lean() {
    let h = harness();
    let (status, body) = send(h.app(h.config(&[])), get("/health/live")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "alive");
}

#[tokio::test]
async fn health_reports_the_lean_version() {
    let h = harness();
    let (status, body) = send(h.app(h.config(&[])), get("/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["lean_available"], true);
    assert!(
        body["lean_version"]
            .as_str()
            .unwrap_or_default()
            .contains("4.30"),
        "{body}"
    );
}

#[tokio::test]
async fn check_returns_valid_for_clean_source() {
    let h = harness();
    let (status, body) = send(
        h.app(h.config(&[])),
        post("/api/check", serde_json::json!({"source": "def x := 1"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);
    assert!(body["computation_time_ms"].as_f64().unwrap_or(0.0) >= 0.0);
}

#[tokio::test]
async fn check_surfaces_lean_errors_as_diagnostics() {
    let h = harness();
    let (status, body) = send(
        h.app(h.config(&[])),
        post("/api/check", serde_json::json!({"source": "def x := BAD"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    assert_eq!(body["diagnostics"][0]["severity"], "error");
    assert_eq!(body["diagnostics"][0]["line"], 1);
    assert_eq!(body["diagnostics"][0]["column"], 1);
}

#[tokio::test]
async fn an_empty_source_is_a_422() {
    let h = harness();
    let (status, _) = send(
        h.app(h.config(&[])),
        post("/api/check", serde_json::json!({"source": ""})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_timeout_is_reported_not_hung() {
    let h = harness();
    let started = std::time::Instant::now();
    let (status, body) = send(
        h.app(h.config(&[("LEAN4_PROVER_TIMEOUT", "1")])),
        post("/api/check", serde_json::json!({"source": "TIMEOUT"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    assert!(
        body["stderr"]
            .as_str()
            .unwrap_or_default()
            .contains("timed out"),
        "{body}"
    );
    // The cap is honoured: the fake toolchain sleeps 30 s.
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn elaborate_returns_the_type_from_stdout() {
    let h = harness();
    let (status, body) = send(
        h.app(h.config(&[])),
        post("/api/elaborate", serde_json::json!({"term": "fun x => x"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);
    assert_eq!(body["elaborated_type"], "Nat → Nat");
}

#[tokio::test]
async fn prove_stops_at_the_first_tactic_that_works() {
    let h = harness();
    let (status, body) = send(
        h.app(h.config(&[])),
        post(
            "/api/prove",
            serde_json::json!({
                "theorem": "example : 2 + 2 = 4",
                "tactics": ["bad_tactic", "norm_num"]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["proven"], true);
    assert_eq!(body["successful_tactic"], "norm_num");
}

#[tokio::test]
async fn prove_accumulates_failures_with_tactic_context() {
    let h = harness();
    let (status, body) = send(
        h.app(h.config(&[])),
        post(
            "/api/prove",
            serde_json::json!({
                "theorem": "example : 2 + 2 = 4",
                "tactics": ["bad_tactic", "also_bad_tactic"]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["proven"], false);
    assert!(body["successful_tactic"].is_null());
    let diagnostics = body["diagnostics"].as_array().expect("array");
    assert_eq!(diagnostics.len(), 2, "{body}");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap_or_default()
            .starts_with("[bad_tactic]")
    );
    assert!(
        diagnostics[1]["message"]
            .as_str()
            .unwrap_or_default()
            .starts_with("[also_bad_tactic]")
    );
}

#[tokio::test]
async fn metrics_are_exposed_in_prometheus_format() {
    let h = harness();
    let app = h.app(h.config(&[]));
    let response = app
        .oneshot(get("/metrics"))
        .await
        .expect("router is infallible");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("gpu_admission_active{lane=\"prove\"}"),
        "{text}"
    );
    assert!(text.contains("lean4_prover_requests_total"), "{text}");
}

#[tokio::test]
async fn the_licence_gate_refuses_compute_without_a_token_but_leaves_health_open() {
    let h = harness();
    let app = h.app(h.config(&[
        ("ENABLE_AUTH", "true"),
        (
            "LICENSE_INTROSPECT_URL",
            "http://127.0.0.1:1/license/introspect",
        ),
    ]));

    let (status, _) = send(
        app.clone(),
        post("/api/check", serde_json::json!({"source": "def x := 1"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = send(app, get("/health/live")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "alive");
}

#[tokio::test]
async fn a_bare_token_without_the_bearer_scheme_is_not_identity() {
    let h = harness();
    let app = h.app(h.config(&[
        ("ENABLE_AUTH", "true"),
        (
            "LICENSE_INTROSPECT_URL",
            "http://127.0.0.1:1/license/introspect",
        ),
    ]));
    let request = Request::builder()
        .method("POST")
        .uri("/api/prove")
        .header("content-type", "application/json")
        .header("x-tenant-id", "plain-token")
        .body(Body::from(r#"{"theorem":"example : True"}"#))
        .expect("request builds");
    let (status, _) = send(app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
