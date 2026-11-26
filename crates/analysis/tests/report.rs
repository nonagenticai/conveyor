//! `analyse.report` — each claim checked against the page's visible text.

use conveyor_analysis::model::Document;
use conveyor_analysis::report::{ClaimRef, analyse};

fn page(html: &str) -> Document {
    Document::new("https://example.test/report", html)
}

fn claim(claim: &str, reference: &str) -> ClaimRef {
    ClaimRef {
        claim: claim.to_owned(),
        reference: reference.to_owned(),
    }
}

#[test]
fn an_unsupported_claim_is_named_and_a_supported_one_is_not() {
    let claims = [
        claim("names Leeds", "covers Leeds"),
        claim("names Wakefield", "covers Wakefield"),
    ];
    let report = analyse(&page("<p>The plumber covers Leeds.</p>"), &claims);
    let unverified: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| finding.check_id() == "report-claim-unverified")
        .filter_map(|finding| finding.evidence.as_deref())
        .collect();

    assert_eq!(unverified.len(), 1, "{unverified:?}");
    assert!(unverified[0].contains("claim 1"));
    assert!(unverified[0].contains("covers Wakefield"));
}

#[test]
fn a_fully_referenced_report_is_clean() {
    let claims = [claim("names Leeds", "plumber covers Leeds")];
    let report = analyse(&page("<p>The plumber covers Leeds.</p>"), &claims);

    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.scorecard.overall_score, 100.0);
}

#[test]
fn an_empty_row_is_named() {
    let claims = [claim("   ", "some reference")];
    let report = analyse(&page("<p>Some reference.</p>"), &claims);

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.check_id() == "report-claim-empty"),
        "{:?}",
        report.findings
    );
}

#[test]
fn the_same_claims_twice_is_the_same_report() {
    let claims = [claim("names Leeds", "covers Leeds")];
    let first = analyse(&page("<p>The plumber covers Leeds.</p>"), &claims);
    let second = analyse(&page("<p>The plumber covers Leeds.</p>"), &claims);

    assert_eq!(
        serde_json::to_string(&first).expect("serialisable"),
        serde_json::to_string(&second).expect("serialisable")
    );
}
