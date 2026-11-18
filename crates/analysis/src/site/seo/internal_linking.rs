//! Internal-link quality checks (group: `internal_linking`).
//!
//! Single-page, deterministic: anchor-text quality and whether a content page links anywhere at
//! all. Site-wide orphan/hub analysis needs a crawl and is **not** ported — it belongs to a
//! caller that has a crawl, not to this library.
//!
//! Namespace `seo-linking-<check>` → group `internal_linking`.

use std::sync::OnceLock;

use regex::Regex;
use scraper::Html;

use crate::model::{Finding, Severity};

use super::content_utils::{main_text, spaced_text, words};
use super::signals::internal_and_external_links;
use super::{all, attr};

const MIN_WORDS: usize = 200;
const GENERIC_ANCHOR_THRESHOLD: usize = 3;

/// The anchor labels that carry no relevance signal.
fn generic_anchor_re() -> &'static Regex {
    static GENERIC: OnceLock<Regex> = OnceLock::new();
    GENERIC.get_or_init(|| {
        Regex::new(
            r"(?i)^(click here|read more|learn more|more|here|this|link|continue|click|details|see more|find out more)$",
        )
        .expect("static pattern")
    })
}

/// Run the internal-linking group's checks over one document.
pub fn check_internal_linking(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let word_count = words(&main_text(html)).len();

    let generic = all(html, "a")
        .iter()
        .filter(|anchor| attr(anchor, "href").is_some())
        .filter(|anchor| {
            let label = spaced_text(anchor);
            !label.is_empty() && generic_anchor_re().is_match(&label)
        })
        .count();

    if generic >= GENERIC_ANCHOR_THRESHOLD {
        findings.push(
            Finding::seo(
                "seo-linking-generic-anchors",
                Severity::Low,
                format!("{generic} generic anchor texts"),
                format!(
                    "{generic} links use generic anchor text ('click here', 'read more', …). \
                     Descriptive anchors pass clearer relevance signals and help accessibility."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Replace generic anchors with descriptive, keyword-relevant text.",
            ),
        );
    }

    if word_count >= MIN_WORDS {
        let (internal, _external) = internal_and_external_links(html, url);
        if internal == 0 {
            findings.push(
                Finding::seo(
                    "seo-linking-no-internal",
                    Severity::Low,
                    "No internal links",
                    "This content page links to no other pages on the site. Internal links \
                     distribute link equity and help search engines discover related content.",
                )
                .with_url(url)
                .with_recommendation("Add contextual internal links to related pages."),
            );
        }
    } else {
        findings.push(
            Finding::seo(
                "seo-linking-insufficient-content",
                Severity::Low,
                "Not enough content to assess internal linking",
                "The page has too little body text to evaluate its internal-link structure. \
                 Content pages should link contextually to related pages.",
            )
            .with_url(url)
            .with_recommendation(
                "Add substantive content with contextual internal links to related pages.",
            ),
        );
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://example.test/page";

    fn page(body: &str) -> Html {
        Html::parse_document(&format!("<html><body><main>{body}</main></body></html>"))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(Finding::check_id).collect()
    }

    fn long_prose() -> String {
        std::iter::repeat_n("padding", 210)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn generic_anchors_are_counted_and_flagged_at_three() {
        let html = page(
            "<p><a href=\"/a\">click here</a><a href=\"/b\">read more</a>\
             <a href=\"/c\">Learn more</a></p>",
        );
        let findings = check_internal_linking(&html, URL);
        assert!(
            ids(&findings).contains(&"seo-linking-generic-anchors"),
            "{:?}",
            ids(&findings)
        );
        assert!(findings[0].title.contains("3 generic"));
    }

    #[test]
    fn two_generic_anchors_are_below_the_threshold() {
        let html = page("<p><a href=\"/a\">click here</a><a href=\"/b\">read more</a></p>");
        let findings = check_internal_linking(&html, URL);
        assert!(!ids(&findings).contains(&"seo-linking-generic-anchors"));
    }

    #[test]
    fn a_content_page_with_no_internal_links_is_flagged() {
        let html = page(&format!("<p>{}</p>", long_prose()));
        let findings = check_internal_linking(&html, URL);
        assert!(
            ids(&findings).contains(&"seo-linking-no-internal"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn a_content_page_with_one_internal_link_is_not_flagged() {
        let html = page(&format!(
            "<p>{}</p><a href=\"/related\">Related guide</a>",
            long_prose()
        ));
        let findings = check_internal_linking(&html, URL);
        assert!(ids(&findings).is_empty(), "{:?}", ids(&findings));
    }

    #[test]
    fn a_thin_page_reports_insufficient_content_instead() {
        let findings = check_internal_linking(&page("<p>Short.</p>"), URL);
        assert_eq!(ids(&findings), ["seo-linking-insufficient-content"]);
    }
}
