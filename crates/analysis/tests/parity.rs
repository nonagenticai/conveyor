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
