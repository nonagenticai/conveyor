//! S13 — the n8n disposition ledger, proved to resolve against the seeded process catalogue.
//!
//! `parity/n8n-dispositions.tsv` carries one row per node of the 348 retired n8n flows, five
//! tab-separated columns and no header (`node_id`, `name`, `kind`, `action`, `target`). Each row
//! states where that node's work now lives: a seeded process key, one of the three absorbed
//! computed handlers, or an outright retirement. The file is worth keeping only because that claim
//! is checkable — this is the check.
//!
//! It is a **resolution** gate, deliberately not a text snapshot: a snapshot passes while every
//! key is wrong. The properties, each a way the ledger could lie:
//!
//! 1. **The ledger is whole** — 348 rows of five cells. The row count *is* the coverage claim (one
//!    row per old node), so a truncated export has to fail rather than read as a smaller, greener
//!    ledger.
//! 2. **Nothing is deferred** — no cell carries `delta:`, the DAG era's "this node has no home
//!    yet" marker.
//! 3. **Every target has a kind** — a target is a `process:`, an `absorbed:`, or the verbatim
//!    `n8n-retire`. Without this, a row left on a retired DAG target (`matrix:`, `procedure:`,
//!    `workflow:`, `schedule:`, `run.failed:`) would be invisible to the resolution check below.
//! 4. **Every `process:<key>` resolves** — against `conveyor_store::process_seed::catalogue()`:
//!    every `types[].processes[].key` plus `named.processes[].key`. The assertion names the keys
//!    it could not resolve, so a failure says which row to fix or which key to seed.
//! 5. **The absorbed set is exactly the computed handlers** — `projects.create`, `err.card`,
//!    `wip.pull`, and nothing else.
//! 6. **The twelve retirements survive** — an unchanged count, so the ledger is never quietly
//!    grown into "everything was absorbed".
//!
//! The recorded-trace gate (an old n8n execution diffed against a process run) lives in
//! `crates/actions/tests/process_trace.rs`. This file reads no fixture and opens no database.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// One ledger row, in the export's column order.
struct Row {
    node_id: String,
    name: String,
    kind: String,
    action: String,
    target: String,
}

impl Row {
    /// The cells paired with their column name, so a failure says which cell is wrong.
    fn cells(&self) -> [(&'static str, &str); 5] {
        [
            ("node_id", &self.node_id),
            ("name", &self.name),
            ("kind", &self.kind),
            ("action", &self.action),
            ("target", &self.target),
        ]
    }
}

fn ledger_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/n8n-dispositions.tsv")
}

/// Parse the ledger. Blank lines are skipped; every other line must carry all five cells, so a
/// mangled row fails here instead of surfacing as an oddly short target.
fn ledger() -> Vec<Row> {
    let path = ledger_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            cells.len(),
            5,
            "line {}: expected 5 tab-separated cells, found {}: {line}",
            index + 1,
            cells.len()
        );
        rows.push(Row {
            node_id: cells[0].to_owned(),
            name: cells[1].to_owned(),
            kind: cells[2].to_owned(),
            action: cells[3].to_owned(),
            target: cells[4].to_owned(),
        });
    }
    rows
}

/// Every process key the embedded catalogue seeds: the four-per-type processes and `named.yaml`'s.
fn seeded_process_keys() -> BTreeSet<String> {
    let catalogue = conveyor_store::process_seed::catalogue()
        .expect("the default process catalogue parses and validates");
    let mut keys = BTreeSet::new();
    for file in &catalogue.types {
        for process in &file.processes {
            keys.insert(process.key.clone());
        }
    }
    // Fail closed: `named.yaml` carries `cap.*`, `ask_fr` and `kata.open`, and a catalogue without
    // it would make a third of the ledger unaccountably unresolvable.
    let named = catalogue.named.expect("the catalogue ships named.yaml");
    for process in &named.processes {
        keys.insert(process.key.clone());
    }
    keys
}

/// The `process:` keys the ledger targets.
fn process_targets(rows: &[Row]) -> Vec<&str> {
    rows.iter()
        .filter_map(|row| row.target.strip_prefix("process:"))
        .collect()
}

/// The targeted keys with no seed behind them, sorted and deduplicated. A pure function so the
/// negative control can show it can fail.
fn unresolved<'a>(
    targets: impl IntoIterator<Item = &'a str>,
    seeded: &BTreeSet<String>,
) -> Vec<String> {
    let mut missing: Vec<String> = targets
        .into_iter()
        .filter(|key| !seeded.contains(*key))
        .map(str::to_owned)
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

#[test]
fn the_ledger_holds_348_rows_of_five_cells() {
    assert_eq!(
        ledger().len(),
        348,
        "the ledger's coverage claim is one row per old n8n node"
    );
}

#[test]
fn no_cell_carries_a_delta() {
    for row in ledger() {
        for (column, cell) in row.cells() {
            assert!(
                !cell.contains("delta:"),
                "{} ({column}) still carries a delta: {cell}",
                row.node_id
            );
        }
    }
}

#[test]
fn every_target_is_a_process_an_absorbed_handler_or_a_retirement() {
    for row in ledger() {
        let known = row.target.starts_with("process:")
            || row.target.starts_with("absorbed:")
            || row.target == "n8n-retire";
        assert!(
            known,
            "{}: {:?} is neither a seeded process, an absorbed handler nor a retirement",
            row.node_id, row.target
        );
    }
}

#[test]
fn every_process_target_resolves_to_a_seeded_key() {
    let rows = ledger();
    let seeded = seeded_process_keys();
    assert!(
        !seeded.is_empty(),
        "an empty catalogue would make every target unresolvable"
    );

    let targets = process_targets(&rows);
    let missing = unresolved(targets.iter().copied(), &seeded);
    assert!(
        missing.is_empty(),
        "{} of {} process target(s) name no seeded process — fix the ledger or seed the key: \
         {missing:#?}",
        missing.len(),
        targets.len()
    );
}

#[test]
fn the_absorbed_targets_are_the_three_computed_handlers() {
    let rows = ledger();
    let absorbed: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| row.target.strip_prefix("absorbed:"))
        .map(str::to_owned)
        .collect();
    let expected: BTreeSet<String> = ["projects.create", "err.card", "wip.pull"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        absorbed, expected,
        "the absorbed set is exactly conveyor.start, err.card and wip.pull"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.target.starts_with("absorbed:"))
            .count(),
        3,
        "each absorbed handler is one row"
    );
}

#[test]
fn twelve_rows_are_retired_outright() {
    let rows = ledger();
    let retired: Vec<&str> = rows
        .iter()
        .filter(|row| row.target == "n8n-retire")
        .map(|row| row.node_id.as_str())
        .collect();
    assert_eq!(
        retired.len(),
        12,
        "the retirement set is closed: {retired:?}"
    );
}

/// The negative control: `unresolved` must be able to fail, so the green assertion above is a
/// measurement rather than a check that cannot see. The fabricated key stands in for a ledger row
/// whose seeded key was renamed.
#[test]
fn the_resolution_check_can_fail() {
    let seeded = seeded_process_keys();
    let missing = unresolved(
        ["cap.analysis.consult", "cap.nonexistent.invented"],
        &seeded,
    );
    assert_eq!(
        missing,
        vec!["cap.nonexistent.invented".to_owned()],
        "a seeded key resolves and a fabricated one does not"
    );
}
