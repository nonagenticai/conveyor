//! P3.3 — the parity table has no unaccounted row.
//!
//! The capture is evidence of what the reference served; the parity table is the port's accounting
//! of it. This test holds the two against each other in both directions, because a table checked
//! only one way always passes: an operation missing from the table, or a table row naming an
//! operation no capture has, is a failure. A `kind` row must name a capability some image actually
//! implements, a `drop` row must carry a reason, and every implemented kind must be reachable from
//! the table — otherwise a binary could grow a capability nobody agreed to.

use std::collections::{BTreeMap, BTreeSet};

use conveyor_media::contract::{self, Capture};

/// One row of `contract/parity.toml`.
#[derive(serde::Deserialize)]
struct Row {
    service: String,
    operation: String,
    disposition: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// A capability this workspace publishes that no reference operation evidences.
#[derive(serde::Deserialize)]
struct New {
    kind: String,
    reason: String,
}

#[derive(serde::Deserialize)]
struct Parity {
    #[serde(default)]
    operation: Vec<Row>,
    #[serde(default)]
    new: Vec<New>,
}

fn table() -> Parity {
    toml::from_str(include_str!("../contract/parity.toml")).expect("parity.toml parses")
}

fn captures() -> Vec<Capture> {
    let mut all = Vec::new();
    all.extend_from_slice(conveyor_media::browser::CAPTURES);
    all.extend_from_slice(conveyor_media::ffmpeg::CAPTURES);
    all
}

fn kinds_implemented() -> BTreeSet<&'static str> {
    let mut all = BTreeSet::new();
    all.extend(conveyor_media::browser::KINDS.iter().copied());
    all.extend(conveyor_media::ffmpeg::KINDS.iter().copied());
    all
}

#[test]
fn every_captured_operation_has_exactly_one_row() {
    let parity = table();
    let mut rows: BTreeMap<(String, String), usize> = BTreeMap::new();
    for row in &parity.operation {
        *rows
            .entry((row.service.clone(), row.operation.clone()))
            .or_default() += 1;
    }

    let mut unaccounted = Vec::new();
    for capture in captures() {
        let document: serde_json::Value =
            serde_json::from_str(capture.lock_json).expect("a captured lock is JSON");
        for operation in contract::operations(&document) {
            match rows.get(&(capture.service.to_string(), operation.clone())) {
                None => unaccounted.push(format!("{} {operation}", capture.service)),
                Some(1) => {}
                Some(count) => panic!("{} {operation} has {count} rows", capture.service),
            }
        }
    }
    assert!(
        unaccounted.is_empty(),
        "operations with no parity row: {unaccounted:#?}"
    );
}

#[test]
fn no_row_names_an_operation_no_capture_has() {
    let captured: BTreeSet<(String, String)> = captures()
        .iter()
        .flat_map(|capture| {
            let document: serde_json::Value =
                serde_json::from_str(capture.lock_json).expect("a captured lock is JSON");
            contract::operations(&document)
                .into_iter()
                .map(|operation| (capture.service.to_string(), operation))
        })
        .collect();

    let orphans: Vec<String> = table()
        .operation
        .iter()
        .filter(|row| !captured.contains(&(row.service.clone(), row.operation.clone())))
        .map(|row| format!("{} {}", row.service, row.operation))
        .collect();
    assert!(orphans.is_empty(), "rows naming nothing captured: {orphans:#?}");
}

#[test]
fn a_served_row_names_an_implemented_kind_and_a_drop_says_why() {
    let implemented = kinds_implemented();
    for row in table().operation {
        match row.disposition.as_str() {
            "kind" => {
                let kind = row
                    .kind
                    .unwrap_or_else(|| panic!("{} {} is a kind row with no kind", row.service, row.operation));
                assert!(
                    implemented.contains(kind.as_str()),
                    "{} {} names {kind}, which no image implements",
                    row.service,
                    row.operation
                );
            }
            "drop" => {
                let reason = row
                    .reason
                    .unwrap_or_else(|| panic!("{} {} is dropped with no reason", row.service, row.operation));
                assert!(
                    !reason.trim().is_empty(),
                    "{} {} is dropped with an empty reason",
                    row.service,
                    row.operation
                );
            }
            other => panic!("{} {} has an unknown disposition {other:?}", row.service, row.operation),
        }
    }
}

#[test]
fn every_implemented_kind_is_accounted_for() {
    let parity = table();
    let mut accounted: BTreeSet<String> = parity
        .operation
        .iter()
        .filter_map(|row| row.kind.clone())
        .collect();
    for new in &parity.new {
        assert!(
            !new.reason.trim().is_empty(),
            "{} is new with an empty reason",
            new.kind
        );
        accounted.insert(new.kind.clone());
    }
    let implemented = kinds_implemented();
    let unaccounted: Vec<&&str> = implemented
        .iter()
        .filter(|kind| !accounted.contains(**kind))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "implemented kinds with no parity entry: {unaccounted:?}"
    );
}
