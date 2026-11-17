//! P2/P4 — the sixteen blueprints are data over the actions the registry actually serves.
//!
//! Loading refuses a phase naming an unregistered action, reports the phases whose action is not
//! built yet instead of dropping them, and pins that report to `blueprints/unresolved.json` so a
//! capability landing is a visible, reviewed change rather than a silent one.

use std::collections::BTreeSet;
use std::fs;

use conveyor_produce::manifest::{self, Family, ManifestError, UnresolvedPhase};

/// The sixteen keys of plan 04 §2, sorted.
const SIXTEEN: [&str; 16] = [
    "action_reel",
    "app_review",
    "audiobook",
    "audit_video",
    "course",
    "cro_audit",
    "demo_video",
    "generic",
    "growth_bundle",
    "lesson",
    "marketing_scorecard",
    "promo",
    "seo_audit",
    "web_design",
    "web_design_v2",
    "website_review",
];

fn registered() -> Vec<&'static str> {
    conveyor_actions::registry().names()
}

#[test]
fn unknown_action_is_refused() {
    let json = r#"{
      "key": "sample",
      "title": "A sample",
      "family": "production",
      "phases": [
        { "id": "render", "action": "media.not_registered", "params": {} }
      ],
      "unresolved": []
    }"#;

    let error = manifest::load(json, &registered())
        .expect_err("a phase naming an unregistered action is refused");
    match &error {
        ManifestError::UnknownAction {
            blueprint,
            phase,
            action,
        } => {
            assert_eq!(blueprint, "sample");
            assert_eq!(phase, "render");
            assert_eq!(action, "media.not_registered");
        }
        other => panic!("expected UnknownAction, got {other:?}"),
    }
    let message = error.to_string();
    for named in ["sample", "render", "media.not_registered"] {
        assert!(
            message.contains(named),
            "the refusal must name `{named}`: {message}"
        );
    }
}

#[test]
fn all_sixteen_load() {
    let loaded = manifest::load_all(&registered())
        .expect("every committed blueprint loads against the live registry");

    let keys: Vec<&str> = loaded
        .blueprints
        .iter()
        .map(|blueprint| blueprint.key.as_str())
        .collect();
    assert_eq!(keys, SIXTEEN, "the committed set is plan 04 §2's sixteen");

    let production = loaded
        .blueprints
        .iter()
        .filter(|blueprint| blueprint.family == Family::Production)
        .count();
    assert_eq!(
        production, 7,
        "plan 04 §2 names seven production blueprints"
    );
    assert_eq!(
        loaded.blueprints.len() - production,
        9,
        "and nine analysis blueprints"
    );

    let known: BTreeSet<&str> = registered().into_iter().collect();
    let declared: BTreeSet<(&str, &str)> = loaded
        .unresolved
        .iter()
        .map(|phase| (phase.blueprint.as_str(), phase.phase.as_str()))
        .collect();
    for blueprint in &loaded.blueprints {
        assert!(
            !blueprint.phases.is_empty(),
            "blueprint `{}` has no phases",
            blueprint.key
        );
        for phase in &blueprint.phases {
            assert!(
                known.contains(phase.action.as_str())
                    || declared.contains(&(blueprint.key.as_str(), phase.id.as_str())),
                "blueprint `{}` phase `{}` names `{}`, which is neither registered nor declared unresolved",
                blueprint.key,
                phase.id,
                phase.action
            );
        }
    }
}

#[test]
fn unresolved_phases_are_declared() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/blueprints/unresolved.json");
    let committed = fs::read_to_string(path).expect("blueprints/unresolved.json is committed");
    let expected: Vec<UnresolvedPhase> =
        serde_json::from_str(&committed).expect("blueprints/unresolved.json is valid JSON");

    let loaded = manifest::load_all(&registered()).expect("every committed blueprint loads");
    assert_eq!(
        loaded.unresolved, expected,
        "a capability landed or a blueprint changed: update blueprints/unresolved.json in the same change"
    );
}
