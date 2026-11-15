//! `analyse.site` end to end: what a caller gets for one document.

use conveyor_analysis::model::{Document, Severity};
use conveyor_analysis::site::analyse;

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
    let report = analyse(&Document::new("https://example.test/", BARE));
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
    let first = serde_json::to_string(&analyse(&document)).expect("serialises");
    let second = serde_json::to_string(&analyse(&document)).expect("serialises");
    assert_eq!(
        first, second,
        "the scan must be a pure function of its input"
    );
}

#[test]
fn the_breakdown_always_carries_every_weighted_group() {
    let report = analyse(&Document::new("https://example.test/", BARE));
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
    let bare = analyse(&Document::new("https://example.test/", BARE));
    let good = analyse(&Document::new("https://example.test/", REASONABLE));
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
    let report = analyse(&Document::new("https://example.test/", REASONABLE));
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
    let report = analyse(&Document::new("", REASONABLE));
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
    let report = analyse(&Document::new("https://example.test/", html));
    let mut ids: Vec<&str> = report.findings.iter().map(|f| f.check_id()).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "no check id appears twice");
}
