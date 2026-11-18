//! `analyse.site` end to end: what a caller gets for one document.

use chrono::{TimeZone, Utc};
use conveyor_analysis::model::{Document, Report, Severity};
use conveyor_analysis::site::{AnalyseOptions, analyse};

/// The instant every test analyses at: fixed, so a report is reproducible in a test.
fn at_fixed_instant(document: &Document) -> Report {
    let as_of = Utc
        .with_ymd_and_hms(2026, 11, 15, 12, 0, 0)
        .single()
        .expect("a valid instant");
    analyse(document, &AnalyseOptions::at(as_of))
}

const BARE: &str = "<html><head></head><body><p>nothing</p></body></html>";

const REASONABLE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <title>Emergency plumber in Leeds — 24 hour call out</title>
  <meta name="description" content="A local plumber covering Leeds, Wakefield and Bradford. Same-day call outs, fixed prices, and a two year guarantee on every repair we carry out.">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="robots" content="index, follow">
  <meta property="og:title" content="Emergency plumber in Leeds">
  <meta property="og:description" content="Same-day call outs across West Yorkshire.">
  <meta property="og:image" content="https://example.test/card.png">
  <meta property="og:url" content="https://example.test/">
  <meta name="twitter:card" content="summary_large_image">
  <meta name="twitter:title" content="Emergency plumber in Leeds">
  <meta name="twitter:description" content="Same-day call outs across West Yorkshire.">
  <meta name="twitter:image" content="https://example.test/card.png">
  <link rel="canonical" href="https://example.test/">
  <link rel="icon" href="/favicon.ico">
  <script type="application/ld+json">
    {"@type": "BreadcrumbList", "itemListElement": [{"@type": "ListItem", "position": 1}]}
  </script>
</head>
<body>
  <h1>Emergency plumber in Leeds</h1>
  <h2>What we fix</h2>
  <img src="/van.jpg" alt="Our van outside a house in Leeds">
  <a href="/pricing">Pricing</a>
</body>
</html>"#;

#[test]
fn a_page_missing_title_and_description_reports_both() {
    let report = at_fixed_instant(&Document::new("https://example.test/", BARE));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    assert!(ids.contains(&"seo-missing-title"), "got {ids:?}");
    assert!(ids.contains(&"seo-meta-description-missing"), "got {ids:?}");

    let title = report
        .findings
        .iter()
        .find(|f| f.check_id() == "seo-missing-title")
        .expect("the title finding is present");
    assert_eq!(title.severity, Severity::High);
    assert_eq!(title.category.as_str(), "seo");
    assert_eq!(title.source_tool.as_deref(), Some("seo_scanner"));
    assert_eq!(title.url, "https://example.test/");
}

#[test]
fn the_same_document_twice_is_the_same_bytes() {
    let document = Document::new("https://example.test/", REASONABLE);
    let first = serde_json::to_string(&at_fixed_instant(&document)).expect("serialises");
    let second = serde_json::to_string(&at_fixed_instant(&document)).expect("serialises");
    assert_eq!(
        first, second,
        "the scan must be a pure function of its input"
    );
}

#[test]
fn the_breakdown_always_carries_every_weighted_group() {
    let report = at_fixed_instant(&Document::new("https://example.test/", BARE));
    let mut groups: Vec<&str> = report
        .scorecard
        .buckets
        .keys()
        .map(String::as_str)
        .collect();
    groups.sort_unstable();
    let mut expected = vec![
        "content",
        "eeat",
        "geo",
        "internal_linking",
        "keyword",
        "on_page",
        "schema",
        "snippet",
        "technical",
    ];
    expected.sort_unstable();
    assert_eq!(groups, expected);
}

#[test]
fn a_bare_page_scores_lower_than_a_complete_one() {
    let bare = at_fixed_instant(&Document::new("https://example.test/", BARE));
    let good = at_fixed_instant(&Document::new("https://example.test/", REASONABLE));
    assert!(
        bare.scorecard.overall_score < good.scorecard.overall_score,
        "bare {}, good {}",
        bare.scorecard.overall_score,
        good.scorecard.overall_score
    );
    assert!(good.scorecard.overall_score > 60.0);
}

#[test]
fn a_complete_page_does_not_trip_the_presence_checks() {
    let report = at_fixed_instant(&Document::new("https://example.test/", REASONABLE));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    for absent in [
        "seo-missing-title",
        "seo-meta-description-missing",
        "seo-missing-h1",
        "seo-canonical-missing",
        "seo-viewport-missing",
        "seo-favicon-missing",
        "seo-og-tags-missing",
        "seo-twitter-card-missing",
        "seo-breadcrumb-missing",
        "seo-structured-data-missing",
        "seo-meta-robots-noindex",
        "seo-url-underscores",
    ] {
        assert!(
            !ids.contains(&absent),
            "{absent} fired on a complete page: {ids:?}"
        );
    }
}

#[test]
fn a_page_with_no_url_still_analyses() {
    // The URL-dependent checks stand down rather than inventing findings from an empty string.
    let report = at_fixed_instant(&Document::new("", REASONABLE));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    assert!(!ids.contains(&"seo-internal-nofollow"));
    assert!(!ids.contains(&"seo-canonical-protocol-mismatch"));
    assert!(!ids.contains(&"seo-url-underscores"));
}

#[test]
fn duplicate_findings_are_collapsed_before_scoring() {
    // Two images without alt text is one finding (the source emitted one per check, not per
    // image), and the report carries no duplicates of any check id.
    let html = "<title>Emergency plumber in Leeds today</title>\
                <h1>Emergency plumber in Leeds</h1>\
                <img src=a><img src=b>";
    let report = at_fixed_instant(&Document::new("https://example.test/", html));
    let mut ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "no check id appears twice");
}

/// A page built to fail every audit group at once: 250 repeated words, no question heading, no
/// author, no contact or privacy link, no citations, no date, no structured data, no internal
/// links, and a title that does not carry the dominant term.
fn fails_every_audit_group() -> String {
    format!(
        "<html><head><title>Ordinary heading</title></head><body><main><h1>Ordinary heading</h1>\
         <p>{}</p></main></body></html>",
        std::iter::repeat_n("padding", 250)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

#[test]
fn audit_groups_all_run_in_one_pass() {
    let html = fails_every_audit_group();
    let report = at_fixed_instant(&Document::new("https://example.test/", &html));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    for expected in [
        "seo-content-thin",
        "seo-keyword-missing-in-title",
        "seo-snippet-no-question-heading",
        "seo-eeat-no-privacy-link",
        "seo-schema-no-organization",
        "seo-linking-no-internal",
        "seo-geo-no-author-entity",
    ] {
        assert!(ids.contains(&expected), "{expected} missing from {ids:?}");
    }
}

#[test]
fn audit_schema_flags_missing_organization_and_lowers_the_group() {
    let report = at_fixed_instant(&Document::new("https://example.test/", BARE));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    assert!(ids.contains(&"seo-schema-no-organization"), "got {ids:?}");
    let schema = report.scorecard.buckets["schema"];
    assert!(schema < 100.0, "the schema group score is {schema}");
}

#[test]
fn an_empty_group_scores_one_hundred_and_stays_in_the_breakdown() {
    // The bare page is far too short for the snippet group to assess, so it has no findings there.
    let report = at_fixed_instant(&Document::new("https://example.test/", BARE));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    assert!(
        !ids.iter().any(|id| id.starts_with("seo-snippet-")),
        "got {ids:?}"
    );
    assert_eq!(report.scorecard.buckets.get("snippet"), Some(&100.0));
}

#[test]
fn audit_geo_measures_age_against_the_supplied_instant() {
    let html = format!(
        "<html><head><title>Ordinary</title></head><body><main><p>{}</p>\
         <script type=\"application/ld+json\">{{\"@type\": \"Person\", \"name\": \"A\", \
         \"datePublished\": \"2021-01-01\"}}</script></main></body></html>",
        std::iter::repeat_n("padding", 210)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let report = at_fixed_instant(&Document::new("https://example.test/", &html));
    let ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    assert!(
        ids.contains(&"seo-geo-stale-content"),
        "a 2021 date is stale at the fixed 2026 instant: {ids:?}"
    );
}
