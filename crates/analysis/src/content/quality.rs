//! The content-quality checks that join the CRO/marketing scan: required headings, a cited
//! source, and filler.
//!
//! These are the structural checks a content gate needs before the conversion and marketing
//! scanners are worth reading: a page without a heading, without a source and full of filler is
//! thin regardless of how many buttons it has. They are deterministic and read only the document.

use scraper::{Html, Selector};

use crate::model::{Category, Document, Finding, Severity};

/// Filler phrases the quality scan refuses. Matched case-insensitively against the visible text.
const FILLER: [&str; 8] = [
    "in today's fast-paced world",
    "it is important to note",
    "at the end of the day",
    "lorem ipsum",
    "click here",
    "read more",
    "cutting-edge",
    "world-class",
];

/// Run the heading, source and filler checks over one page.
pub fn scan(document: &Document) -> Vec<Finding> {
    let html = Html::parse_document(&document.html);
    let mut findings = Vec::new();

    let h1 = selector("h1");
    let sections = selector("h2, h3");
    if html.select(&h1).next().is_none() {
        findings.push(quality_finding(
            "content-heading-missing",
            Severity::High,
            "the page has no h1",
            "A page without a level-one heading gives a reader no statement of its subject.",
            "h1",
        ));
    }
    let section_count = html.select(&sections).count();
    if section_count < 2 {
        findings.push(quality_finding(
            "content-heading-shallow",
            Severity::Medium,
            "the page has fewer than two sections",
            "A page with fewer than two h2/h3 sections is not structured for scanning.",
            format!("{section_count} h2/h3"),
        ));
    }

    if !has_source(&html) {
        findings.push(quality_finding(
            "content-no-sources",
            Severity::Medium,
            "the page cites no source",
            "No absolute link, cite element or blockquote citation appears on the page.",
            "no a[href^=http], cite or blockquote[cite]",
        ));
    }

    let text = html
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    for phrase in FILLER {
        if text.contains(phrase) {
            findings.push(quality_finding(
                "content-filler",
                Severity::Low,
                format!("the filler phrase {phrase:?} occurs"),
                "The page carries a phrase that says nothing and can be removed.",
                phrase,
            ));
        }
    }

    findings
}

fn has_source(html: &Html) -> bool {
    let links = selector("a[href]");
    let cites = selector("cite");
    let citations = selector("blockquote[cite]");
    html.select(&links).any(|element| {
        element
            .value()
            .attr("href")
            .is_some_and(|href| href.starts_with("http://") || href.starts_with("https://"))
    }) || html.select(&cites).next().is_some()
        || html.select(&citations).next().is_some()
}

fn selector(pattern: &str) -> Selector {
    Selector::parse(pattern).unwrap_or_else(|_| panic!("{pattern} is a valid selector"))
}

fn quality_finding(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    evidence: impl Into<String>,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Content,
        severity,
        title: title.into(),
        description: description.into(),
        evidence: Some(evidence.into()),
        source_tool: Some("content_quality".to_owned()),
        scanner_count: 1,
        ..Finding::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(html: &str) -> Document {
        Document::new("https://example.test/", html)
    }

    #[test]
    fn a_thin_page_is_named() {
        let findings = scan(&page("<html><body><p>Words.</p></body></html>"));
        let ids: Vec<&str> = findings
            .iter()
            .filter_map(|finding| finding.check_id.as_deref())
            .collect();
        assert!(ids.contains(&"content-heading-missing"));
        assert!(ids.contains(&"content-heading-shallow"));
        assert!(ids.contains(&"content-no-sources"));
    }

    #[test]
    fn a_sourced_structured_page_is_clean() {
        let findings = scan(&page(
            r#"<html><body><h1>Report</h1><h2>One</h2><h2>Two</h2>
            <p>See <a href="https://source.test/report">the source</a>.</p></body></html>"#,
        ));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn each_filler_phrase_is_one_finding() {
        let findings = scan(&page(
            r#"<html><body><h1>Report</h1><h2>One</h2><h2>Two</h2>
            <p>In today's fast-paced world, click here.</p>
            <cite>a source</cite></body></html>"#,
        ));
        let filler: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.check_id.as_deref() == Some("content-filler"))
            .filter_map(|finding| finding.evidence.as_deref())
            .collect();
        assert_eq!(filler, ["in today's fast-paced world", "click here"]);
    }
}
