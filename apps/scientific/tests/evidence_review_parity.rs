//! P5 — the capture, the served contract and the route registry agree in every direction.
//!
//! The capture (`docs/evidence/evidence-review-capture-2026-11-18.md`) is the source: an
//! operation is either `ported` (and then it must be registered and locked) or dropped with a
//! reason that names what replaces it. A route that exists in the binary and not in the capture is
//! the failure this test exists to catch, and so is a captured operation nobody can find.

use conveyor_scientific::contract;
use conveyor_scientific::router::ROUTES;

const CAPTURE: &str = include_str!("../../../docs/evidence/evidence-review-capture-2026-11-18.md");

struct Row {
    method: String,
    path: String,
    operation: String,
    state: String,
    reason: String,
    ground: String,
}

fn capture_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in CAPTURE.lines() {
        if line.starts_with("<!-- BEGIN CAPTURE -->") {
            in_table = true;
            continue;
        }
        if line.starts_with("<!-- END CAPTURE -->") {
            in_table = false;
            continue;
        }
        if !in_table || !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 7 || !matches!(cells[0], "GET" | "POST" | "PATCH" | "PUT" | "DELETE") {
            continue;
        }
        rows.push(Row {
            method: cells[0].to_owned(),
            path: cells[1].to_owned(),
            operation: cells[2].to_owned(),
            state: cells[4].to_owned(),
            reason: cells[5].to_owned(),
            ground: cells[6].to_owned(),
        });
    }
    rows
}

fn registered(method: &str, path: &str) -> bool {
    ROUTES
        .iter()
        .any(|(route_method, route_path)| *route_method == method && *route_path == path)
}

fn locked(method: &str, path: &str) -> bool {
    contract::lock()
        .map(|document| {
            document
                .get("paths")
                .and_then(|paths| paths.get(path))
                .and_then(|item| item.get(method.to_ascii_lowercase()))
                .is_some()
        })
        .unwrap_or(false)
}

/// Every operation the capture calls `ported` is registered AND in the served lock.
#[test]
fn every_ported_operation_is_registered_and_locked() {
    let rows = capture_rows();
    assert!(!rows.is_empty(), "the capture table parses");
    let mut problems = Vec::new();
    for row in rows.iter().filter(|row| row.state == "ported") {
        if !registered(&row.method, &row.path) {
            problems.push(format!(
                "ported but not registered: {} {} ({})",
                row.method, row.path, row.operation
            ));
        }
        if !locked(&row.method, &row.path) {
            problems.push(format!(
                "ported but not locked: {} {} ({})",
                row.method, row.path, row.operation
            ));
        }
        if row.ground.is_empty() || row.ground == "—" {
            problems.push(format!(
                "ported with no ground: {} {}",
                row.method, row.path
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A registered route must be a captured, ported operation — nothing is served that the capture
/// does not account for.
#[test]
fn every_registered_review_operation_is_ported_in_the_capture() {
    let rows = capture_rows();
    let mut problems = Vec::new();
    for (method, path) in ROUTES {
        if !path.starts_with("/api/reviews") {
            continue;
        }
        let matched = rows
            .iter()
            .find(|row| row.method == *method && row.path == *path);
        match matched {
            Some(row) if row.state == "ported" => {}
            Some(row) => problems.push(format!(
                "registered but captured as {}: {} {}",
                row.state, method, path
            )),
            None => problems.push(format!("registered but not captured: {method} {path}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A dropped operation carries a reason that names what replaces it, the measured caller, or the
/// decision that retired it — never an empty cell.
#[test]
fn every_dropped_row_carries_its_reason() {
    let rows = capture_rows();
    let mut problems = Vec::new();
    for row in rows.iter().filter(|row| row.state.starts_with("dropped")) {
        if row.reason.trim().is_empty()
            || row.reason == "—"
            || row.reason == "deferred"
            || row.ground.trim().is_empty()
            || row.ground == "—"
        {
            problems.push(format!(
                "dropped without a final reason/ground: {} {} ({} / {})",
                row.method, row.path, row.reason, row.ground
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
