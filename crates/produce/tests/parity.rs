//! P1 — every operation in the captured document has exactly one disposition.
//!
//! The document is the reference service's own output (`contract/capture.sh`, pinned image).
//! A change to its bytes fails this test; so does an operation with no row, two rows, or a
//! disposition outside the four legal values. The table is the artifact: the assertions here
//! are what keep it complete.

use std::collections::BTreeMap;
use std::fs;

use conveyor_produce::capture;

const EVIDENCE_MD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/evidence/produce-capture-2026-11-16.md"
);

/// P1.2 — the digest of the exact captured bytes, and of the image they came from.
const DOCUMENT_SHA256: &str = "28de38166d3c3dc4068e54471ec0f6fd0bd4fab5c033fe73e40c824fb3d5027d";
const IMAGE_DIGEST: &str = "sha256:71a79a0fd5392e36b8920f696f398c6dc8906933276735d9dfab88ae24e9e743";

const OPERATIONS: usize = 25;
const PORTED: usize = 6;
const TRANSPORT: usize = 12;
const NO_CONSUMER: usize = 2;
const DEFERRED: usize = 5;

const LEGAL: [&str; 4] = [
    "ported",
    "dropped: transport",
    "dropped: no consumer",
    "dropped: deferred",
];

/// A drop-table line, or `None` for a line that is not an operation row (the provenance table,
/// the header, the total). A row is only a row when its first cell is a method and a path.
fn row(line: &str) -> Option<(String, String, String)> {
    let rest = line.trim().strip_prefix('|')?;
    let mut cells = rest.split('|').map(str::trim);
    let (method, path) = cells.next()?.trim_matches('`').trim().split_once(' ')?;
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method) || !path.starts_with('/') {
        return None;
    }
    let disposition = cells.next()?.to_string();
    let reason = cells.next()?.to_string();
    if disposition.is_empty() || reason.is_empty() {
        return None;
    }
    Some((format!("{method} {path}"), disposition, reason))
}

#[test]
fn capture() {
    assert_eq!(
        capture::sha256_hex(),
        DOCUMENT_SHA256,
        "the committed capture changed: recapture it from the pinned image, never edit it by hand"
    );
    assert_eq!(capture::document()["openapi"], "3.1.0");

    let operations = capture::operations();
    assert_eq!(
        operations.len(),
        OPERATIONS,
        "the document's operation count changed: recapture and re-classify, never adjust this number"
    );

    let md = fs::read_to_string(EVIDENCE_MD).expect("the capture evidence markdown is committed");
    assert!(
        md.contains(DOCUMENT_SHA256),
        "the evidence does not record the committed digest"
    );
    assert!(
        md.contains(IMAGE_DIGEST),
        "the evidence does not record the captured image digest"
    );
    assert!(md.contains(&format!("| operations | {OPERATIONS} |")));

    let mut rows: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for line in md.lines() {
        if let Some((operation, disposition, reason)) = row(line) {
            rows.entry(operation).or_default().push((disposition, reason));
        }
    }

    for operation in &operations {
        let dispositions = rows.get(operation).unwrap_or_else(|| {
            panic!("operation `{operation}` has no disposition row in the evidence")
        });
        assert_eq!(
            dispositions.len(),
            1,
            "operation `{operation}` has more than one disposition row"
        );
    }

    for operation in rows.keys() {
        assert!(
            operations.contains(operation),
            "the evidence classifies `{operation}`, which the document does not define"
        );
    }

    let mut counts: BTreeMap<&str, usize> = LEGAL.iter().map(|legal| (*legal, 0)).collect();
    for (operation, dispositions) in &rows {
        for (disposition, reason) in dispositions {
            assert!(
                LEGAL.contains(&disposition.as_str()),
                "illegal disposition `{disposition}` for `{operation}`"
            );
            assert!(
                !reason.trim().is_empty(),
                "`{operation}` is classified without a reason"
            );
            *counts
                .get_mut(disposition.as_str())
                .expect("the disposition is legal, checked above") += 1;
        }
    }

    assert_eq!(counts["ported"], PORTED);
    assert_eq!(counts["dropped: transport"], TRANSPORT);
    assert_eq!(counts["dropped: no consumer"], NO_CONSUMER);
    assert_eq!(counts["dropped: deferred"], DEFERRED);
    assert_eq!(PORTED + TRANSPORT + NO_CONSUMER + DEFERRED, OPERATIONS);
    assert_eq!(
        rows.len(),
        OPERATIONS,
        "the table and the document must classify the same number of operations"
    );
}
