//! P6.1 — `analyse.content` over a supplied document: the CRO and marketing scanner families
//! produce the findings and the scorecard is their weighted mean.

use conveyor_analysis::content::analyse;
use conveyor_analysis::model::{Category, Document};

fn sparse_page() -> Document {
    Document::new(
        "https://example.test/",
        r#"<html lang="en"><head><title>A page</title></head>
<body><h1>A page</h1><p>Some words about a product that exists.</p></body></html>"#,
    )
}

#[test]
fn a_sparse_page_produces_content_findings() {
    let report = analyse(&sparse_page());
    assert!(
        !report.findings.is_empty(),
        "a page with no aim, no proof and no next step cannot be clean"
    );
    for finding in &report.findings {
        assert!(
            matches!(finding.category, Category::Cro | Category::Marketing),
            "the content family owns only CRO and marketing findings, got {:?}",
            finding.category
        );
        assert!(
            finding.check_id().starts_with("cro-") || finding.check_id().starts_with("mkt-"),
            "the check id carries the source's namespace: {}",
            finding.check_id()
        );
    }
}

#[test]
fn the_report_is_a_pure_function_of_the_document() {
    let first = analyse(&sparse_page());
    let second = analyse(&sparse_page());
    assert_eq!(
        serde_json::to_string(&first).expect("the report serialises"),
        serde_json::to_string(&second).expect("the report serialises")
    );
}

#[test]
fn the_scorecard_holds_one_bucket_per_present_category() {
    let report = analyse(&sparse_page());
    let present: std::collections::BTreeSet<&str> = report
        .findings
        .iter()
        .map(|finding| finding.category.as_str())
        .collect();
    assert_eq!(
        report.scorecard.buckets.len(),
        present.len(),
        "every present category is scored, and no absent one is invented"
    );
    for category in present {
        assert!(
            report.scorecard.buckets.contains_key(category),
            "the {category} bucket exists"
        );
    }
    assert!(report.scorecard.overall_score > 0.0 && report.scorecard.overall_score <= 100.0);
}
