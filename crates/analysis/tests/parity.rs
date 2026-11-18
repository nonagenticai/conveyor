//! Numeric parity with the absorbed service.
//!
//! `tests/fixtures/rubric_parity.json` is the *source's* output, not this crate's: it is written
//! by `tests/fixtures/generate_rubric_parity.py`, which copies the service's
//! `compute_category_score` and `score_to_grade` verbatim and freezes every penalty from 0 to 120
//! plus the grade boundaries. A change to the decay, the floor or the rounding fails here against
//! the source's own numbers rather than against an assumption about them.

use std::collections::BTreeMap;

use conveyor_analysis::model::{Category, Finding, Grade, Severity};
use conveyor_analysis::rubric::{Rubric, grade, round_1dp};

#[derive(serde::Deserialize)]
struct Fixture {
    cases: Vec<PenaltyCase>,
    grades: Vec<GradeCase>,
}

#[derive(serde::Deserialize)]
struct PenaltyCase {
    penalty: i32,
    score: f64,
}

#[derive(serde::Deserialize)]
struct GradeCase {
    score: f64,
    grade: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/rubric_parity.json")).expect("the fixture parses")
}

#[test]
fn every_penalty_matches_the_source_score() {
    let fixture = fixture();
    let rubric = Rubric::default();
    assert_eq!(
        fixture.cases.len(),
        121,
        "every penalty from 0 to 120 is covered"
    );
    for case in &fixture.cases {
        assert_eq!(
            rubric.score_for_penalty(case.penalty),
            case.score,
            "penalty {} disagrees with the source",
            case.penalty
        );
    }
}

#[test]
fn every_grade_boundary_matches_the_source() {
    for case in &fixture().grades {
        assert_eq!(
            grade(case.score).as_str(),
            case.grade,
            "grade for {} disagrees with the source",
            case.score
        );
    }
}

#[test]
fn the_grade_type_serialises_as_the_source_letter() {
    assert_eq!(serde_json::to_string(&Grade::APlus).unwrap(), "\"A+\"");
    assert_eq!(serde_json::to_string(&Grade::F).unwrap(), "\"F\"");
}

#[test]
fn a_category_score_reaches_the_same_number_through_findings() {
    let rubric = Rubric::default();
    // critical + high + medium = 30 + 15 + 7 = 52
    let findings = vec![
        Finding::seo("a", Severity::Critical, "t", "d"),
        Finding::seo("b", Severity::High, "t", "d"),
        Finding::seo("c", Severity::Medium, "t", "d"),
    ];
    assert_eq!(
        rubric.category_score(&findings, Category::Seo),
        rubric.score_for_penalty(52)
    );
}

#[test]
fn the_weighted_mean_matches_the_source_arithmetic() {
    let rubric = Rubric::default();
    let weights: BTreeMap<String, f64> = [
        ("a".to_owned(), 0.5),
        ("b".to_owned(), 0.3),
        ("c".to_owned(), 0.2),
    ]
    .into_iter()
    .collect();
    let buckets: BTreeMap<String, f64> = [
        ("a".to_owned(), 65.1),
        ("b".to_owned(), 100.0),
        ("c".to_owned(), 70.7),
    ]
    .into_iter()
    .collect();
    // Python: round(65.1*0.5 + 100*0.3 + 70.7*0.2, 1) = round(76.69, 1) = 76.7
    assert_eq!(rubric.weighted_overall(&buckets, &weights), 76.7);
    assert_eq!(round_1dp(76.69), 76.7);
}

/// The capture — the record the whole absorption is measured by.
///
/// `docs/evidence/analysis-capture-2026-11-15.md` is the human record and the machine-readable one at
/// the same time: the table is parsed here, so a row cannot exist for an operation the service does
/// not serve, an operation cannot silently disappear from the record, and a drop decision cannot be
/// made without one of the three literals the issue fixes. The served contract beside it is the
/// bytes the running image emitted, pinned by digest.
mod capture {
    use std::collections::{BTreeMap, BTreeSet};

    use sha2::{Digest, Sha256};

    const DOC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/evidence/analysis-openapi-2026-11-15.json"
    ));
    const CAPTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/evidence/analysis-capture-2026-11-15.md"
    ));

    /// The sha256 of the bytes the pod served on 2026-11-15. A re-capture moves this constant.
    const DOC_SHA256: &str = "1bcd3894d5b4bce38151c8df8c4d7d13318cd567c028f0e28929bee0339d3b34";

    const OPERATIONS: usize = 92;
    const PATHS: usize = 79;
    const PORTED: usize = 10;
    const TRANSPORT: usize = 36;
    const DEFERRED: usize = 2;
    const NO_CONSUMER: usize = 44;

    /// Every Rust entry point a `ported` row may name. Adding one here is the whole ceremony for
    /// declaring an operation ported — the port itself still has to exist for the row to be true.
    const ENTRY_POINTS: [&str; 5] = [
        "analyse.site (crates/analysis)",
        "analyse.app (crates/analysis)",
        "analyse.brand (crates/analysis)",
        "analyse.content (crates/analysis)",
        "analyse.report (crates/analysis)",
    ];
    const REASONS: [&str; 3] = ["transport", "no consumer", "deferred"];

    const CELLS: usize = 10;
    const BEGIN: &str = "<!-- BEGIN CAPTURE -->";
    const END: &str = "<!-- END CAPTURE -->";

    #[derive(Debug)]
    struct Row {
        method: String,
        path: String,
        operation_id: String,
        tag: String,
        state: String,
        reason: String,
        ground: String,
        request_ref: String,
        response_ref: String,
    }

    fn document() -> serde_json::Value {
        serde_json::from_str(DOC).expect("the served contract parses as JSON")
    }

    /// Every operation the served document declares: (method, path, operation_id, first tag).
    fn served_operations(doc: &serde_json::Value) -> Vec<(String, String, String, String)> {
        let mut out = Vec::new();
        for (path, item) in doc["paths"].as_object().expect("paths is an object") {
            for (method, op) in item.as_object().expect("a path item is an object") {
                let Some(id) = op.get("operationId").and_then(|v| v.as_str()) else {
                    continue;
                };
                let tag = op
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .and_then(|t| t.first())
                    .and_then(|t| t.as_str())
                    .unwrap_or("-")
                    .to_owned();
                out.push((method.to_uppercase(), path.clone(), id.to_owned(), tag));
            }
        }
        out
    }

    fn schema_names(doc: &serde_json::Value) -> BTreeSet<String> {
        doc["components"]["schemas"]
            .as_object()
            .expect("components.schemas")
            .keys()
            .cloned()
            .collect()
    }

    /// The rows between the markers: one header line, one separator, then one line per operation.
    /// Everything the parser refuses is refused because it would make a row mean two things.
    fn rows() -> Vec<Row> {
        assert!(
            !CAPTURE.contains('\r'),
            "the capture must not carry CRLF line endings"
        );
        assert!(
            !CAPTURE.starts_with('\u{feff}'),
            "the capture must not start with a BOM"
        );

        let mut inside = false;
        let mut header_seen = false;
        let mut separator_seen = false;
        let mut out = Vec::new();

        for line in CAPTURE.lines() {
            if line.trim() == BEGIN {
                assert!(!inside, "a second BEGIN marker");
                inside = true;
                continue;
            }
            if line.trim() == END {
                assert!(inside, "an END marker without a BEGIN");
                inside = false;
                break;
            }
            if !inside {
                continue;
            }
            assert!(
                line.starts_with('|'),
                "a line inside the capture block is not a table row: {line}"
            );
            assert!(
                line.ends_with('|'),
                "a table row must end with a closing pipe: {line}"
            );

            if !header_seen {
                assert!(
                    line.contains("operation_id"),
                    "the first row must be the header"
                );
                header_seen = true;
                continue;
            }
            if !separator_seen {
                assert!(
                    line.chars().all(|c| matches!(c, '|' | '-' | ' ')),
                    "the second row must be the separator"
                );
                separator_seen = true;
                continue;
            }

            // `| a | b |` splits into ["", " a ", " b ", ""]: CELLS cells plus the two edges.
            let parts: Vec<&str> = line.split('|').collect();
            assert_eq!(
                parts.len(),
                CELLS + 2,
                "a row must carry exactly {CELLS} cells (a pipe inside a cell splits it): {line}"
            );
            assert_eq!(parts[0].trim(), "", "the row must open with a pipe: {line}");
            assert_eq!(
                *parts.last().expect("at least one part"),
                "",
                "the row must close: {line}"
            );
            let c: Vec<&str> = parts[1..=CELLS].iter().map(|s| s.trim()).collect();
            assert!(
                c.iter().all(|s| !s.is_empty()),
                "an empty cell is written `-`, not blank: {line}"
            );

            out.push(Row {
                method: c[0].to_owned(),
                path: c[1].to_owned(),
                operation_id: c[2].to_owned(),
                tag: c[3].to_owned(),
                state: c[4].to_owned(),
                reason: c[5].to_owned(),
                ground: c[6].to_owned(),
                request_ref: c[8].to_owned(),
                response_ref: c[9].to_owned(),
            });
        }

        assert!(!inside, "the capture block is not closed");
        assert!(
            header_seen && separator_seen,
            "the capture table has no header"
        );
        out
    }

    fn ref_is_declared(doc: &serde_json::Value, cell: &str) -> bool {
        cell == "-" || cell == "object" || cell.contains('/') || schema_names(doc).contains(cell)
    }

    #[test]
    fn the_served_contract_is_the_bytes_that_were_captured() {
        let digest = Sha256::digest(DOC.as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex, DOC_SHA256,
            "the committed served document is not the captured bytes"
        );
        assert_eq!(DOC.len(), 222_684, "the served document's size moved");
    }

    #[test]
    fn the_capture_accounts_for_every_operation_the_service_serves() {
        let doc = document();
        let served = served_operations(&doc);
        let rows = rows();

        assert_eq!(
            served.len(),
            OPERATIONS,
            "the served document's operation count moved"
        );
        assert_eq!(doc["paths"].as_object().expect("paths").len(), PATHS);

        let served_keys: BTreeSet<(String, String)> = served
            .iter()
            .map(|(m, p, _, _)| (m.clone(), p.clone()))
            .collect();
        let mut row_keys = BTreeSet::new();
        for row in &rows {
            assert!(
                row_keys.insert((row.method.clone(), row.path.clone())),
                "{} {} appears twice",
                row.method,
                row.path
            );
        }
        assert_eq!(
            row_keys, served_keys,
            "the capture and the served document disagree about which operations exist"
        );
        assert_eq!(rows.len(), OPERATIONS);
    }

    #[test]
    fn every_row_carries_a_legal_state_and_reason() {
        let doc = document();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();

        for row in rows() {
            assert!(
                ref_is_declared(&doc, &row.request_ref),
                "{}: request ref {}",
                row.operation_id,
                row.request_ref
            );
            assert!(
                ref_is_declared(&doc, &row.response_ref),
                "{}: response ref {}",
                row.operation_id,
                row.response_ref
            );

            match row.state.as_str() {
                "ported" => {
                    assert_eq!(
                        row.reason, "-",
                        "{}: a ported row carries no drop reason",
                        row.operation_id
                    );
                    assert!(
                        ENTRY_POINTS.contains(&row.ground.as_str()),
                        "{}: `{}` is not a known Rust entry point",
                        row.operation_id,
                        row.ground
                    );
                }
                "dropped" => {
                    assert!(
                        REASONS.contains(&row.reason.as_str()),
                        "{}: `{}` is not one of the three reasons the issue allows",
                        row.operation_id,
                        row.reason
                    );
                    match row.reason.as_str() {
                        "transport" => assert!(
                            row.ground.starts_with("conveyor:"),
                            "{}: a transport row must name the conveyor surface that replaces it",
                            row.operation_id
                        ),
                        "deferred" => assert!(
                            row.ground.starts_with("caller:") || row.ground.starts_with("part:"),
                            "{}: a deferred row must name a measured caller or the part that owns it",
                            row.operation_id
                        ),
                        _ => assert_eq!(
                            row.ground, "-",
                            "{}: a drop with no replacement names no ground",
                            row.operation_id
                        ),
                    }
                }
                other => panic!("{}: `{other}` is not a state", row.operation_id),
            }
            *counts
                .entry(row.state.clone() + " " + &row.reason)
                .or_default() += 1;
        }

        assert_eq!(counts.get("ported -").copied().unwrap_or(0), PORTED);
        assert_eq!(
            counts.get("dropped transport").copied().unwrap_or(0),
            TRANSPORT
        );
        assert_eq!(
            counts.get("dropped deferred").copied().unwrap_or(0),
            DEFERRED
        );
        assert_eq!(
            counts.get("dropped no consumer").copied().unwrap_or(0),
            NO_CONSUMER
        );
    }

    #[test]
    fn every_row_names_the_operation_and_tag_the_service_declares() {
        let doc = document();
        let served: BTreeMap<(String, String), (String, String)> = served_operations(&doc)
            .into_iter()
            .map(|(m, p, id, tag)| ((m, p), (id, tag)))
            .collect();

        for row in rows() {
            let (id, tag) = served
                .get(&(row.method.clone(), row.path.clone()))
                .unwrap_or_else(|| {
                    panic!(
                        "{} has no operation at {} {}",
                        row.operation_id, row.method, row.path
                    )
                });
            assert_eq!(&row.operation_id, id, "the capture renames an operation");
            assert_eq!(
                &row.tag, tag,
                "{}: the tag is not the served document's",
                row.operation_id
            );
        }
    }
}

mod video {
    //! P1 of conveyor#164: the retired video-analysis service's captured operations are each served
    //! by an executor kind or dropped with a reason. The capture and its own enforcement live with
    //! the media app (#139, `apps/media/tests/parity.rs`); this module is the analysis unit's gate
    //! on that same contract, so the two units cannot drift apart.

    const CONTRACT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/media/contract/parity.toml"
    ));

    #[derive(serde::Deserialize)]
    struct Contract {
        #[serde(default)]
        operation: Vec<Operation>,
        #[serde(default)]
        new: Vec<NewKind>,
    }

    #[derive(serde::Deserialize)]
    struct Operation {
        service: String,
        operation: String,
        disposition: String,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct NewKind {
        kind: String,
        #[serde(default)]
        reason: String,
    }

    fn contract() -> Contract {
        toml::from_str(CONTRACT).expect("the media parity contract parses")
    }

    #[test]
    fn the_video_capture_accounts_for_every_operation() {
        let contract = contract();
        let rows: Vec<&Operation> = contract
            .operation
            .iter()
            .filter(|row| row.service == "video_analyzer_service")
            .collect();
        assert_eq!(rows.len(), 14, "the captured operation count moved");

        let mut kinds = 0;
        let mut drops = 0;
        for row in &rows {
            match row.disposition.as_str() {
                "kind" => {
                    kinds += 1;
                    let kind = row
                        .kind
                        .as_deref()
                        .unwrap_or_else(|| panic!("{}: a kind row names its kind", row.operation));
                    assert!(
                        kind.starts_with("media."),
                        "{}: `{kind}` is not an executor kind",
                        row.operation
                    );
                }
                "drop" => {
                    drops += 1;
                    assert!(
                        row.reason
                            .as_deref()
                            .is_some_and(|reason| !reason.trim().is_empty()),
                        "{}: a drop names a reason",
                        row.operation
                    );
                }
                other => panic!("{}: `{other}` is not a disposition", row.operation),
            }
        }
        assert_eq!(
            (kinds, drops),
            (3, 11),
            "three operations are served as kinds and eleven are dropped"
        );

        // Two kinds are new, not ported; both say so, and neither is claimed as parity.
        let new_kinds: Vec<&str> = contract
            .new
            .iter()
            .filter(|row| row.kind == "media.probe_video" || row.kind == "media.ocr")
            .map(|row| row.kind.as_str())
            .collect();
        assert_eq!(new_kinds.len(), 2, "probe_video and ocr are the new kinds");
        for row in &contract.new {
            assert!(
                !row.reason.trim().is_empty(),
                "{}: a new kind states why it exists",
                row.kind
            );
        }
    }
}
