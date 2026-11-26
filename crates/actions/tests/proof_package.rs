//! #746 — package certificates: a delivered work product proves facts about its own manifest.
//!
//! The issue's P5 scenarios. Rust canonicalises; the three relations are decided by
//! `checkPackage` in `DeliveryModel.lean`, which the live corpus checks on the real prover.
//!
//! `rbuild -- test -p conveyor-actions --test proof_package`.

use conveyor_actions::proof::manifest::{self, PackageManifest};
use conveyor_actions::proof::render::{self, RenderError};

/// A package that is complete by every relation.
fn manifest() -> PackageManifest {
    let migrations = vec!["0076".to_owned(), "0077".to_owned()];
    PackageManifest {
        migrations: migrations.clone(),
        ledger: migrations,
        routes: vec!["POST /api/actions/proofs.record".to_owned()],
        served: vec![
            "GET /api/actions".to_owned(),
            "POST /api/actions/proofs.record".to_owned(),
        ],
        checks: vec![
            ("parity_http_mcp".to_owned(), true),
            ("registry".to_owned(), true),
        ],
    }
}

/// P5.1 — a complete package is proven, and the certificate names every fact.
#[test]
fn a_complete_package_is_proven() {
    let facts = manifest().facts();
    let source = render::render(&facts, "package_ok").expect("a complete package renders");

    assert!(source.contains("def facts : PackageFacts"), "{source}");
    assert!(
        source.contains("theorem package_ok : checkPackage facts = true"),
        "{source}"
    );
    assert!(
        source.contains("migrations := [\"0076\", \"0077\"]"),
        "{source}"
    );
    assert!(
        source.contains("checks := [(\"parity_http_mcp\", true), (\"registry\", true)]"),
        "{source}"
    );
}

/// P5.2 — an unpaired migration, an unserved route and a failed check are each refused by name.
#[test]
fn an_incomplete_package_names_the_item() {
    let mut unpaired = manifest();
    unpaired.ledger = vec!["0076".to_owned()];
    let error = render::render(&unpaired.facts(), "package_ok").expect_err("an unpaired migration");
    assert_eq!(
        error,
        RenderError::Incomplete("migration \"0077\" is not in the ledger".to_owned())
    );

    let mut unserved = manifest();
    unserved.served = vec!["GET /api/actions".to_owned()];
    let error = render::render(&unserved.facts(), "package_ok").expect_err("a route nobody serves");
    assert_eq!(
        error,
        RenderError::Incomplete(
            "route \"POST /api/actions/proofs.record\" is not in the served document".to_owned()
        )
    );

    let mut failed = manifest();
    failed.checks = vec![("registry".to_owned(), false)];
    let error = render::render(&failed.facts(), "package_ok").expect_err("a failed check");
    assert_eq!(
        error,
        RenderError::Incomplete("check \"registry\" did not pass".to_owned())
    );
}

/// The migrations half of the pair is real: the embedded migrator is what a package compares
/// against the database's ledger, so it must actually carry this repository's migrations.
#[test]
fn the_embedded_migrations_are_read_from_the_binary() {
    let migrations = manifest::embedded_migrations();
    assert!(
        migrations.len() > 70,
        "the embedded migrator carries the whole history: {} entries",
        migrations.len()
    );
    assert!(
        migrations.contains(&"0076".to_owned()),
        "the proof table's migration is embedded: {migrations:?}"
    );
}

/// A manifest whose facts name a different spec cannot be rendered: the certificate would answer
/// a document the package was never checked against.
#[test]
fn a_package_answers_the_pinned_spec() {
    let facts = manifest().facts();
    assert_eq!(
        facts["spec_sha256"].as_str(),
        Some(conveyor_actions::proof::programme::SPEC_V2_SHA256)
    );
}
