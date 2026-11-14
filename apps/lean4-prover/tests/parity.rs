//! The captured-contract replay: every golden in `contract/golden/` is answered by this crate.
//!
//! The goldens were taken from the LIVE deployment by `contract/capture-goldens.sh`, which is the
//! only direction that can fail for the reason the test exists. A licensed 200 cannot be captured
//! (no licence token is provisioned to the deployment), so the three compute routes are replayed in
//! their **refusal** shape and their success shape is named in `DECLARED_UNCAPTURABLE` — see
//! `contract/README.md`. Every declared entry carries a reason, and a stale declaration fails.

use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
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
use conveyor_lean4_prover::routes::ROUTES;
use conveyor_lean4_prover::runner::LeanRunner;
use conveyor_lean4_prover::state::AppState;

const GOLDEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/golden");

/// Operations whose success shape cannot be captured: capturing them needs a licensed caller, and
/// the deployment provisions no licence token. Their success shape is proved by
/// `tests/http_contract.rs` against a stub toolchain; their refusal shape is replayed here.
const DECLARED_UNCAPTURABLE: &[(&str, &str)] = &[
    (
        "POST /api/check",
        "200 needs a licence token; the refusal shape is replayed by check_unauthorised",
    ),
    (
        "POST /api/elaborate",
        "200 needs a licence token; the refusal shape is replayed by elaborate_unauthorised",
    ),
    (
        "POST /api/prove",
        "200 needs a licence token; the refusal shape is replayed by prove_unauthorised",
    ),
];

/// Fields whose value is environment-dependent and therefore compared by presence, not equality.
const VOLATILE_FIELDS: &[&str] = &["lean_version"];

/// `name -> (request line, status, content type, body)` for every captured golden.
fn goldens() -> BTreeMap<String, Golden> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(GOLDEN_DIR).expect("golden dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("request") {
            continue;
        }
        let name = path
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .to_string();
        let read = |suffix: &str| {
            std::fs::read_to_string(path.with_extension(suffix))
                .unwrap_or_else(|error| panic!("golden {name}.{suffix}: {error}"))
        };
        let golden = Golden {
            request: read("request"),
            status: read("status").trim().to_string(),
            content_type: read("content-type").trim().to_string(),
            body: read("body"),
        };
        out.insert(name, golden);
    }
    out
}

struct Golden {
    request: String,
    status: String,
    content_type: String,
    body: String,
}

impl Golden {
    fn method_and_path(&self) -> (String, String) {
        let mut parts = self.request.lines().next().unwrap_or_default().split(' ');
        (
            parts.next().unwrap_or_default().to_string(),
            parts.next().unwrap_or_default().to_string(),
        )
    }

    fn request_body(&self) -> Option<String> {
        self.request
            .split_once('\n')
            .map(|(_, rest)| rest.to_string())
    }

    fn operation(&self) -> String {
        let (method, path) = self.method_and_path();
        format!("{method} {path}")
    }
}

const FAKE_LEAN: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Lean (version 4.30.0-rc2, x86_64-unknown-linux-gnu)"
  exit 0
fi
for last; do :; done
cat "$last" >/dev/null
echo "Nat → Nat"
exit 0
"#;

fn config(dir: &tempfile::TempDir, overrides: &[(&str, &str)]) -> Config {
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
    let mut pairs: HashMap<String, String> = [
        // The goldens are the UNLICENSED caller's view, so the licence gate is on.
        ("ENABLE_AUTH", "true"),
        (
            "LEAN4_PROVER_PROJECT_DIR",
            project.display().to_string().as_str(),
        ),
        (
            "LEAN4_PROVER_LEAN_BINARY",
            format!("{}/lean", bin.display()).as_str(),
        ),
        (
            "LEAN4_PROVER_LAKE_BINARY",
            format!("{}/lake", bin.display()).as_str(),
        ),
        (
            "LEAN4_PROVER_SUBPROCESS_PATH",
            format!("{}:/bin:/usr/bin", bin.display()).as_str(),
        ),
        ("LEAN4_PROVER_TIMEOUT", "5"),
        (
            "LICENSE_INTROSPECT_URL",
            "http://licences.invalid/license/introspect",
        ),
        ("LEAN4_PROVER_SERVICE_NAME", "lean4-prover"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    for (k, v) in overrides {
        pairs.insert((*k).to_string(), (*v).to_string());
    }
    Config::from_lookup(|key| pairs.get(key).cloned()).expect("test config parses")
}

fn app(cfg: Config) -> Router {
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

async fn replay(app: &Router, golden: &Golden) -> (StatusCode, String, String) {
    let (method, path) = golden.method_and_path();
    let mut builder = Request::builder()
        .method(method.as_str())
        .uri(path.as_str());
    let body = match golden.request_body() {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            Body::from(body)
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        content_type,
        String::from_utf8_lossy(&bytes).to_string(),
    )
}

#[tokio::test]
async fn health_live_replays_the_capture_exactly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(config(&dir, &[]));
    let goldens = goldens();
    let golden = goldens.get("health_live").expect("captured");
    let (status, content_type, body) = replay(&app, golden).await;
    assert_eq!(status.as_u16().to_string(), golden.status);
    assert_eq!(content_type, golden.content_type);
    assert_eq!(body, golden.body, "the liveness body is a constant");
}

#[tokio::test]
async fn the_health_probes_answer_their_captured_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(config(&dir, &[]));
    let goldens = goldens();
    for name in ["health", "health_ready"] {
        let golden = goldens.get(name).expect("captured");
        let (status, content_type, body) = replay(&app, golden).await;
        assert_eq!(status.as_u16().to_string(), golden.status, "{name}");
        assert_eq!(content_type, golden.content_type, "{name}");
        let captured: serde_json::Value =
            serde_json::from_str(&golden.body).expect("captured json");
        let served: serde_json::Value = serde_json::from_str(&body).expect("served json");
        let captured_keys: BTreeSet<&String> = captured.as_object().unwrap().keys().collect();
        let served_keys: BTreeSet<&String> = served.as_object().unwrap().keys().collect();
        assert_eq!(served_keys, captured_keys, "{name}: same fields");
        for (key, value) in captured.as_object().unwrap() {
            if VOLATILE_FIELDS.contains(&key.as_str()) {
                // `lean_version` is whatever elan reports where the process runs; the deployment
                // reports an empty string. Presence is the contract, the value is environment.
                assert!(served.get(key).is_some(), "{name}: {key} present");
                continue;
            }
            assert_eq!(&served[key], value, "{name}: {key}");
        }
    }
}

#[tokio::test]
async fn metrics_replays_its_content_type_and_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(config(&dir, &[]));
    let goldens = goldens();
    let golden = goldens.get("metrics").expect("captured");
    let (status, content_type, body) = replay(&app, golden).await;
    assert_eq!(status.as_u16().to_string(), golden.status);
    assert_eq!(content_type, golden.content_type);
    // The values move; the metric families are the contract.
    for family in [
        "gpu_admission_active",
        "gpu_admission_waiting",
        "gpu_admission_rejected_total",
    ] {
        assert!(
            golden.body.contains(family),
            "the capture must contain {family} — re-capture if the deployment stopped exporting it"
        );
        assert!(body.contains(family), "the port must still export {family}");
    }
}

#[tokio::test]
async fn every_compute_route_refuses_without_a_licence_exactly_as_captured() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = app(config(&dir, &[]));
    let goldens = goldens();
    let mut replayed = BTreeSet::new();
    for (name, golden) in &goldens {
        if !name.ends_with("_unauthorised") {
            continue;
        }
        let (status, content_type, body) = replay(&app, golden).await;
        assert_eq!(status.as_u16().to_string(), golden.status, "{name}");
        assert_eq!(content_type, golden.content_type, "{name}");
        assert_eq!(body.trim_end(), golden.body.trim_end(), "{name}");
        replayed.insert(golden.operation());
    }
    // All three compute operations are covered by a refusal golden.
    for operation in ["POST /api/check", "POST /api/elaborate", "POST /api/prove"] {
        assert!(
            replayed.contains(operation),
            "{operation} has no refusal golden"
        );
    }
}

#[tokio::test]
async fn every_route_is_replayed_or_declared() {
    let goldens = goldens();
    let mut covered: BTreeSet<String> = goldens.values().map(Golden::operation).collect();
    covered.extend(
        DECLARED_UNCAPTURABLE
            .iter()
            .map(|(op, _)| (*op).to_string()),
    );

    let declared: BTreeSet<String> = ROUTES
        .iter()
        .map(|(method, path)| format!("{method} {path}"))
        .collect();

    for operation in &declared {
        assert!(
            covered.contains(operation),
            "route {operation} is neither replayed from a golden nor declared uncapturable"
        );
    }
    if let Some(stale) = covered.difference(&declared).next() {
        panic!("stale declaration or golden: {stale} is not a registered route");
    }
    for (operation, reason) in DECLARED_UNCAPTURABLE {
        assert!(
            !reason.trim().is_empty(),
            "{operation} must say WHY it cannot be captured"
        );
    }
}
