//! T14 (#560): the scientific processor's read surface (SPEC_V2 §11.2, §11.9; ADR 0006 §4).
//!
//! The service is absorbed but still deployed at zero replicas, so the toolkit is a draft: the
//! actions are declared, the manifest publishes a draft version, and nothing resolves a binding.
//! These tests are the static half — the exact declared set, and the closed schemas that make an
//! unknown field a `422` before any call leaves the process. The live probe and call wait on the
//! deployment that scales the service up (`acceptance-blocked-by` it, not this unit).

use conveyor_actions::HandlerRef;
use conveyor_actions::registry::registry;
use conveyor_actions::spec::{ActionKind, Effect, validate_input};
use serde_json::{Value, json};

/// The five locked reads, sorted. The 18-operation lock is the ceiling; these are the reads with
/// a live consumer. The graph surface, the graph-dependent operations and the sandbox are not
/// ported (ADR 0006 §4).
const READS: [&str; 5] = [
    "scientific_formal.translate",
    "scientific_formal.verify",
    "scientific_stats.confidence_interval",
    "scientific_stats.meta_analysis",
    "scientific_stats.trend",
];

#[test]
fn only_the_locked_reads_are_declared() {
    let mut declared: Vec<&str> = registry()
        .iter()
        .filter(|spec| {
            matches!(&spec.handler, HandlerRef::External { service, .. } if service.as_ref() == "scientific")
        })
        .map(|spec| spec.name.as_ref())
        .collect();
    declared.sort_unstable();
    assert_eq!(
        declared, READS,
        "the scientific toolkit is exactly the five locked reads"
    );
    for name in READS {
        let spec = registry()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing"));
        assert_eq!(spec.kind, ActionKind::External, "{name}");
        assert_eq!(spec.effect, Effect::Read, "{name} must stay a read");
        assert!(spec.idempotent, "{name} is a pure read");
    }
    // The dropped surface stays dropped: no graph, sandbox or write action hides behind a name.
    for spec in registry().iter() {
        let name = spec.name;
        if name.starts_with("scientific") {
            assert!(
                !name.contains("graph") && !name.contains("sandbox"),
                "{name} names a dropped operation (ADR 0006 §4)"
            );
            assert_ne!(spec.effect, Effect::Write, "{name} is not a read");
            assert_ne!(spec.effect, Effect::Destructive, "{name} is not a read");
        }
    }
}

fn schema(name: &str) -> Value {
    let spec = registry()
        .get(name)
        .unwrap_or_else(|| panic!("{name} is missing"));
    spec.input_schema()
}

#[test]
fn an_unknown_field_is_refused_before_the_backend() {
    let errors = validate_input(
        &schema("scientific_stats.meta_analysis"),
        &json!({
            "studies": [
                {"effect_size": 0.5, "standard_error": 0.1, "sample_size": 100},
                {"effect_size": 0.3, "standard_error": 0.2, "sample_size": 80}
            ],
            "nope": true
        }),
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "unknown_field" && error.field == "nope"),
        "{errors:?}"
    );
    // The nested study object is closed too, so a misspelt study field cannot reach the service.
    let nested = validate_input(
        &schema("scientific_stats.meta_analysis"),
        &json!({
            "studies": [
                {"effect_size": 0.5, "standard_error": 0.1, "sample_size": 100, "extra": 1},
                {"effect_size": 0.3, "standard_error": 0.2, "sample_size": 80}
            ]
        }),
    );
    assert!(
        nested
            .iter()
            .any(|error| error.code == "unknown_field" && error.field == "studies.0.extra"),
        "{nested:?}"
    );
}

#[test]
fn the_apps_request_bodies_validate() {
    let valid = [
        (
            "scientific_stats.meta_analysis",
            json!({"studies": [
                {"effect_size": 0.5, "standard_error": 0.1, "sample_size": 100},
                {"effect_size": 0.3, "standard_error": 0.2, "sample_size": 80}
            ], "method": "fixed", "confidence_level": 0.95}),
        ),
        (
            "scientific_stats.confidence_interval",
            json!({"values": [1.0, 2.0, 3.0], "method": "normal"}),
        ),
        (
            "scientific_stats.trend",
            json!({"data": [{"timestamp": 0.0, "value": 1.0}, {"timestamp": 1.0, "value": 2.0},
                            {"timestamp": 2.0, "value": 3.0}]}),
        ),
        (
            "scientific_formal.translate",
            json!({"claim_text": "every natural number is non-negative", "target_language": "lean4"}),
        ),
        (
            "scientific_formal.verify",
            json!({"formal_expression": "x > 0", "language": "z3"}),
        ),
    ];
    for (name, body) in valid {
        let errors = validate_input(&schema(name), &body);
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
    // The language is the app's closed set: z3, sympy, lean4 — nothing else is a translation.
    let errors = validate_input(
        &schema("scientific_formal.translate"),
        &json!({"claim_text": "a claim", "target_language": "coq"}),
    );
    assert!(!errors.is_empty(), "an unknown language must be refused");
}
