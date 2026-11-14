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

/// Every operation that has no golden, and why a golden cannot exist.
///
/// A golden is the old service's own answer to a fixed input. That is only meaningful when the
/// answer is a function of the input: a clock reading, a process counter, or a number a model
/// produced is not. Naming each one here is what stops "no golden" from silently becoming "not
/// checked" — every entry has a unit or contract test in its own module instead.
const DECLARED_UNCAPTURABLE: &[(&str, &str)] = &[
    ("GET /health", "reports a clock reading"),
    (
        "GET /health/live",
        "reports a clock reading and a process counter",
    ),
    ("GET /health/ready", "probes live downstreams"),
    (
        "GET /api/cache/stats",
        "process state, not an answer to the request",
    ),
    (
        "POST /api/cache/invalidate",
        "process state, not an answer to the request",
    ),
    (
        "POST /api/cache/clear",
        "process state, not an answer to the request",
    ),
    (
        "POST /api/verify/ragas",
        "three upstream calls; a capture would record one model answer, not replay a function",
    ),
    ("POST /api/formal/translate", "a model call in every branch"),
    ("POST /api/formal/verify", "a proof search on a live prover"),
    (
        "GET /api/formal/proofs/{claim_id}",
        "reads object-store contents the process did not compute",
    ),
    (
        "POST /api/formal/proofs/{claim_id}",
        "writes an object; the answer carries a fresh timestamp",
    ),
    ("GET /api/pipeline/status", "process counters"),
    (
        "GET /api/pipeline/history/{entity_id}",
        "reads object-store contents the process did not compute",
    ),
    (
        "GET /api/pipeline/anomalies",
        "reads object-store contents the process did not compute",
    ),
];

#[test]
fn every_registered_operation_is_either_replayed_or_declared() {
    let replayed: std::collections::BTreeSet<&str> = routes().values().copied().collect();
    let declared: std::collections::BTreeSet<&str> =
        DECLARED_UNCAPTURABLE.iter().map(|(op, _)| *op).collect();

    for &(method, path) in router::ROUTES {
        let operation = format!("{method} {path}");
        let covered = replayed.contains(path) || declared.contains(operation.as_str());
        assert!(
            covered,
            "{operation} has neither a golden nor a declared reason it cannot have one"
        );
    }

    // And no declaration outlives its route: a dropped operation must not keep a slot in this list.
    let registered: Vec<String> = router::ROUTES
        .iter()
        .map(|(method, path)| format!("{method} {path}"))
        .collect();
    for (operation, why) in DECLARED_UNCAPTURABLE {
        assert!(
            registered.iter().any(|row| row == operation),
            "{operation} is declared uncapturable but no longer registered ({why})"
        );
    }
}
