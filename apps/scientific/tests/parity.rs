//! The golden replay: what the old service answered, the port must answer.
//!
//! The pairs in `contract/golden/` were captured from the old Python service by
//! `contract/capture-goldens.sh`, so the fixtures are its own outputs rather than numbers typed
//! into a test by hand. Only deterministic operations are captured: `/health` reports a clock
//! reading and probes live downstreams, and the cache routes are stateful, so those are covered by
//! unit tests in the crate and are named here so the omission is not mistaken for coverage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_scientific::config::Config;
use conveyor_scientific::router;
use conveyor_scientific::state::AppState;
use serde::Deserialize;
use serde_json::Value;
use tower::ServiceExt;

/// The operations that have a golden, and the route each one replays against.
fn routes() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("meta_analysis_fixed", "/api/stats/meta-analysis"),
        ("meta_analysis_random", "/api/stats/meta-analysis"),
        (
            "confidence_interval_normal",
            "/api/stats/confidence-interval",
        ),
        (
            "confidence_interval_bootstrap",
            "/api/stats/confidence-interval",
        ),
        ("trend_mann_kendall", "/api/stats/trend"),
        ("trend_linear_regression", "/api/stats/trend"),
        ("status", "/status"),
    ])
}

#[derive(Debug, Deserialize)]
struct Tolerances {
    default: Defaults,
    ignored: Ignored,
    #[serde(default)]
    case: BTreeMap<String, Case>,
}

#[derive(Debug, Deserialize)]
struct Defaults {
    absolute: f64,
}

#[derive(Debug, Deserialize)]
struct Ignored {
    fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Case {
    absolute: f64,
    #[allow(dead_code)]
    why: String,
}

fn contract_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("contract")
}

fn load_tolerances() -> Tolerances {
    let raw = fs::read_to_string(contract_dir().join("tolerances.toml")).expect("tolerances.toml");
    toml::from_str(&raw).expect("tolerances.toml parses")
}

/// Compare an answered document with the capture. Returns every difference, so one run names all
/// of them rather than the first.
fn compare(
    actual: &Value,
    expected: &Value,
    path: &str,
    tolerance: f64,
    ignored: &[String],
    problems: &mut Vec<String>,
) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, expected_value) in e {
                if ignored.iter().any(|field| field == key) {
                    continue;
                }
                match a.get(key) {
                    Some(actual_value) => compare(
                        actual_value,
                        expected_value,
                        &format!("{path}.{key}"),
                        tolerance,
                        ignored,
                        problems,
                    ),
                    None => problems.push(format!("{path}.{key}: missing")),
                }
            }
            for key in a.keys() {
                if !e.contains_key(key) {
                    problems.push(format!("{path}.{key}: not in the capture"));
                }
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                problems.push(format!("{path}: {} items, captured {}", a.len(), e.len()));
                return;
            }
            for (index, (actual_item, expected_item)) in a.iter().zip(e).enumerate() {
                compare(
                    actual_item,
                    expected_item,
                    &format!("{path}[{index}]"),
                    tolerance,
                    ignored,
                    problems,
                );
            }
        }
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (
                a.as_f64().expect("a finite number"),
                e.as_f64().expect("a finite number"),
            );
            if (a - e).abs() > tolerance {
                problems.push(format!(
                    "{path}: {a} vs captured {e} (tolerance {tolerance})"
                ));
            }
        }
        (a, e) if a == e => {}
        (a, e) => problems.push(format!("{path}: {a} vs captured {e}")),
    }
}

#[tokio::test]
async fn golden_replay_matches_the_capture() {
    let tolerances = load_tolerances();
    let state = std::sync::Arc::new(AppState::new(
        Config::from_lookup(&[]).expect("defaults parse"),
    ));
    let app = router::router(state);

    let mut checked = 0;
    for (name, path) in routes() {
        let golden = contract_dir().join("golden");
        let expected: Value = serde_json::from_str(
            &fs::read_to_string(golden.join(format!("{name}.response.json")))
                .unwrap_or_else(|error| panic!("{name}: {error}")),
        )
        .expect("golden response parses");

        let request_path = golden.join(format!("{name}.request.json"));
        let (method, body) = if request_path.exists() {
            (
                "POST",
                Body::from(fs::read_to_string(&request_path).expect("request fixture")),
            )
        } else {
            ("GET", Body::empty())
        };

        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(body)
            .expect("request builds");
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{name}: the old service answered 200"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body collects");
        let actual: Value = serde_json::from_slice(&bytes).expect("the answer is JSON");

        let tolerance = tolerances
            .case
            .get(name)
            .map(|case| case.absolute)
            .unwrap_or(tolerances.default.absolute);
        let mut problems = Vec::new();
        compare(
            &actual,
            &expected,
            name,
            tolerance,
            &tolerances.ignored.fields,
            &mut problems,
        );
        assert!(problems.is_empty(), "{name}: {problems:#?}");
        checked += 1;
    }
    assert_eq!(checked, routes().len());
}

#[test]
fn the_undropped_surface_is_not_covered_by_goldens_yet() {
    // Named explicitly so the gap is visible: the surviving `/api/verify`, `/api/formal`,
    // `/api/sandbox` and `/api/pipeline` operations have no route yet and therefore no golden.
    // They are the next slice's, and `--check-lock` names every one of them on every run.
    let pending = [
        "POST /api/verify/cove",
        "POST /api/verify/integrity",
        "POST /api/verify/ragas",
        "POST /api/formal/translate",
        "POST /api/formal/verify",
        "GET /api/formal/proofs/{claim_id}",
        "POST /api/formal/proofs/{claim_id}",
        "POST /api/sandbox/execute",
        "GET /api/sandbox/jobs/{job_id}",
        "GET /api/sandbox/results/{job_id}",
        "POST /api/pipeline/trigger",
        "POST /api/pipeline/batch",
        "POST /api/pipeline/webhook",
        "GET /api/pipeline/status",
        "GET /api/pipeline/anomalies",
        "GET /api/pipeline/history/{entity_id}",
    ];
    let registered: Vec<String> = router::ROUTES
        .iter()
        .map(|(method, path)| format!("{method} {path}"))
        .collect();
    for operation in pending {
        assert!(
            !registered.iter().any(|row| row == operation),
            "{operation} is registered now — give it a golden instead of this list entry"
        );
    }
}
