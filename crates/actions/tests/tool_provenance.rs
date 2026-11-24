//! #558 — the executable tool-provenance gate (SPEC_V2 §11.2, §11.9; plan 04; ADR 0007).
//!
//! `seed/tool_provenance.json` is the capability diff from the prior nonagenticai systems turned
//! into a repository fact. This test is the gate: every manifest tool names where it came from, a
//! retired source can never appear as a manifest `service`, a `PORT` names its consumer and usage
//! sample, and a `DROP` records the measurement that justified it.
//!
//! The seed names sources by **public role**, never by a private repository path, host, address or
//! credential. It is reviewed like the toolkit manifests; no row is typed into a database.

use std::collections::{BTreeMap, BTreeSet};

use conveyor_actions::{registry, toolkits::manifest};
use serde::Deserialize;

/// The reviewed seed, embedded so the gate cannot pass against a different file than the one the
/// pull request shows.
const SEED: &str = include_str!("../../../seed/tool_provenance.json");

#[derive(Debug, Deserialize)]
struct Seed {
    version: u32,
    measured: String,
    sources: Vec<Source>,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Source {
    label: String,
    /// `live | retired | archived | migration`.
    status: String,
    /// The destination alias(es) a **live** source owns. A retired, archived or migration source
    /// carries none, so it cannot become a manifest `service`.
    #[serde(default)]
    services: Vec<String>,
    evidence: String,
}

#[derive(Debug, Deserialize)]
struct Row {
    source: String,
    operation: String,
    method: String,
    path: String,
    consumer: String,
    sample: String,
    /// `ALREADY-PRESENT | PORT | DROP`.
    disposition: String,
    /// `action:<name>`, `tool:<toolkit>.<tool>`, or `none`.
    target: String,
    #[serde(default)]
    measured: Option<String>,
    #[serde(default)]
    query: Option<String>,
    /// The issue that will provide the target, for a `PORT` whose target does not exist yet.
    #[serde(default)]
    pending: Option<String>,
}

fn seed() -> Seed {
    serde_json::from_str(SEED).expect("seed/tool_provenance.json parses")
}

/// Every `tool:<toolkit>.<tool>` the current manifests publish.
fn manifest_tool_targets() -> BTreeSet<String> {
    manifest::manifests()
        .expect("the embedded manifests validate")
        .iter()
        .flat_map(|toolkit| {
            toolkit
                .tools
                .iter()
                .map(move |tool| format!("tool:{}.{}", toolkit.key, tool.key))
        })
        .collect()
}

fn is_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .char_indices()
            .all(|(index, ch)| index == 4 || index == 7 || ch.is_ascii_digit())
}

/// The target vocabulary resolves: an action is registered, a tool is published, and `none` is
/// only legal where the row records what happens instead.
fn target_resolves(target: &str, row: &Row) -> bool {
    if target == "none" {
        return match row.disposition.as_str() {
            // A port with no target yet must name the open unit that will provide it.
            "PORT" => row.pending.as_deref().is_some_and(|pending| {
                pending.len() > 1
                    && pending.starts_with('#')
                    && pending[1..].chars().all(|ch| ch.is_ascii_digit())
            }),
            // A dropped capability has no destination; the DROP rule below records why.
            "DROP" => true,
            _ => false,
        };
    }
    if let Some(action) = target.strip_prefix("action:") {
        return registry::registry().get(action).is_some();
    }
    if target.starts_with("tool:") {
        return manifest_tool_targets().contains(target);
    }
    false
}

#[test]
fn every_row_is_complete_and_resolves() {
    let seed = seed();
    assert_eq!(seed.version, 1, "the seed version is the schema version");
    assert!(
        !seed.measured.is_empty(),
        "the seed carries its measurement date"
    );

    let mut labels: BTreeMap<&str, &Source> = BTreeMap::new();
    for source in &seed.sources {
        assert!(!source.label.is_empty(), "a source has a public label");
        assert!(
            matches!(
                source.status.as_str(),
                "live" | "retired" | "archived" | "migration"
            ),
            "{} has status {:?}",
            source.label,
            source.status
        );
        if source.status == "live" {
            assert!(
                !source.services.is_empty(),
                "{} names no service",
                source.label
            );
        } else {
            assert!(
                source.services.is_empty(),
                "{} is not live but carries a service alias",
                source.label
            );
        }
        assert!(
            !source.evidence.is_empty(),
            "{} cites no evidence",
            source.label
        );
        assert!(
            labels.insert(source.label.as_str(), source).is_none(),
            "source {:?} is declared more than once",
            source.label
        );
    }

    let mut seen: BTreeSet<(&str, &str, &str)> = BTreeSet::new();
    let mut used_sources: BTreeSet<&str> = BTreeSet::new();
    for row in &seed.rows {
        assert!(
            labels.contains_key(row.source.as_str()),
            "row {:?}/{:?} names an undeclared source",
            row.source,
            row.operation
        );
        used_sources.insert(row.source.as_str());
        assert!(!row.operation.trim().is_empty(), "a row has no operation");
        assert!(
            !row.method.trim().is_empty(),
            "{} has no method",
            row.operation
        );
        assert!(!row.path.trim().is_empty(), "{} has no path", row.operation);
        assert!(
            !row.consumer.trim().is_empty(),
            "{} has no consumer",
            row.operation
        );
        assert!(
            !row.sample.trim().is_empty(),
            "{} has no sample",
            row.operation
        );
        assert!(
            matches!(
                row.disposition.as_str(),
                "ALREADY-PRESENT" | "PORT" | "DROP"
            ),
            "{} has disposition {:?}",
            row.operation,
            row.disposition
        );
        assert!(
            target_resolves(&row.target, row),
            "{} target {:?} does not resolve",
            row.operation,
            row.target
        );
        assert!(
            seen.insert((
                row.source.as_str(),
                row.operation.as_str(),
                row.path.as_str()
            )),
            "row {}/{} appears more than once",
            row.source,
            row.operation
        );
    }

    for label in labels.keys() {
        assert!(
            used_sources.contains(label),
            "source {label:?} declares no operation"
        );
    }
}

#[test]
fn every_manifest_tool_has_provenance() {
    let seed = seed();
    let rows: BTreeMap<&str, &Row> = seed
        .rows
        .iter()
        .map(|row| (row.target.as_str(), row))
        .collect();
    for target in manifest_tool_targets() {
        let row = rows
            .get(target.as_str())
            .unwrap_or_else(|| panic!("manifest tool {target} has no provenance row"));
        assert_eq!(
            row.disposition, "ALREADY-PRESENT",
            "manifest tool {target} is not recorded as already present"
        );
    }
}

#[test]
fn a_retired_source_is_never_a_destination() {
    let seed = seed();
    let live_services: BTreeSet<&str> = seed
        .sources
        .iter()
        .filter(|source| source.status == "live")
        .flat_map(|source| source.services.iter().map(String::as_str))
        .collect();
    for source in &seed.sources {
        if source.status != "live" {
            assert!(
                source.services.is_empty(),
                "retired source {:?} carries a service alias",
                source.label
            );
        }
    }
    for toolkit in manifest::manifests().expect("the embedded manifests validate") {
        assert!(
            live_services.contains(toolkit.service.as_str()),
            "manifest service {:?} is not owned by a live source",
            toolkit.service
        );
    }
}

#[test]
fn a_port_names_its_consumer_and_sample() {
    let seed = seed();
    for row in seed.rows.iter().filter(|row| row.disposition == "PORT") {
        assert_ne!(
            row.consumer, "none",
            "{} has no live consumer",
            row.operation
        );
        assert_ne!(row.sample, "none", "{} has no usage sample", row.operation);
        assert!(
            !row.consumer.trim().is_empty(),
            "{} has no consumer",
            row.operation
        );
        assert!(
            !row.sample.trim().is_empty(),
            "{} has no sample",
            row.operation
        );
        if row.target == "none" {
            assert!(
                row.pending.is_some(),
                "{} is a port with no target and no pending unit",
                row.operation
            );
        }
    }
}

#[test]
fn a_drop_records_its_measurement() {
    let seed = seed();
    for row in seed.rows.iter().filter(|row| row.disposition == "DROP") {
        assert_eq!(
            row.target, "none",
            "{} is dropped but names a target",
            row.operation
        );
        let measured = row
            .measured
            .as_deref()
            .unwrap_or_else(|| panic!("{} is dropped without a measurement date", row.operation));
        assert!(
            is_date(measured),
            "{} has measurement {:?}, not YYYY-MM-DD",
            row.operation,
            measured
        );
        assert!(
            row.query
                .as_deref()
                .is_some_and(|query| !query.trim().is_empty()),
            "{} is dropped without the query that measured it",
            row.operation
        );
    }
}
