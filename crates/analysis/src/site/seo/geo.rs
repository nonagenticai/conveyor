//! Generative-engine / AI-discoverability checks (group: `geo`).
//!
//! AI answer engines favour pages with a substantial, attributable body and a recent update.
//! These are the deterministic, in-page half of that question; the half that needs
//! `robots.txt` and `llms.txt` is a fetch and therefore the caller's.
//!
//! Findings are emitted in the `seo-geo-<check>` namespace.
//!
//! **Dropped: transport.** `seo-geo-ai-crawler-blocked` and `seo-geo-no-llms-txt` came from a
//! live fetch of `robots.txt` and `llms.txt` in the source. This crate never fetches; a caller
//! that has those files can evaluate them, and the deployment cutover records the drop.

use chrono::{DateTime, Utc};
use scraper::Html;

use crate::model::{Finding, Severity};

use super::content_utils::{main_text, words};
use super::signals::{days_since, extract_content_date, has_author_entity};

/// Below this there is too little text to judge anything else.
const MIN_WORDS: usize = 200;
/// AI engines favour recently-updated content for anything time-sensitive.
const STALE_DAYS: i64 = 180;

/// Run the geo group's checks over one document.
pub fn check_geo(html: &Html, url: &str, as_of: DateTime<Utc>) -> Vec<Finding> {
    let mut findings = Vec::new();
    if words(&main_text(html)).len() < MIN_WORDS {
        findings.push(
            Finding::seo(
                "seo-geo-insufficient-content",
                Severity::Low,
                "Not enough content to assess AI-discoverability",
                "The page has too little body text to evaluate GEO/AI-citation signals (author \
                 entity, freshness, definitional depth). Thin pages are rarely cited by AI \
                 answer engines.",
            )
            .with_url(url)
            .with_recommendation(
                "Add substantive, well-structured body content the page can be cited for.",
            ),
        );
        return findings;
    }

    if !has_author_entity(html) {
        findings.push(
            Finding::seo(
                "seo-geo-no-author-entity",
                Severity::Low,
                "Author is not a structured entity",
                "Authorship is not expressed as a Person/Organization entity in structured \
                 data. AI answer engines weight an explicit author entity far more than a plain \
                 byline when deciding whether to cite a page.",
            )
            .with_url(url)
            .with_recommendation(
                "Mark up the author as a Person entity (with sameAs/credentials) in JSON-LD.",
            ),
        );
    }

    if let Some(date) = extract_content_date(html) {
        let age = days_since(as_of, date);
        if age > STALE_DAYS {
            findings.push(
                Finding::seo(
                    "seo-geo-stale-content",
                    Severity::Low,
                    format!("Content not updated in {age} days"),
                    format!(
                        "The last update was {age} days ago. A recent dateModified (within ~90 \
                         days) markedly increases AI-answer citation for time-sensitive queries."
                    ),
                )
                .with_url(url)
                .with_recommendation("Review and refresh the content, updating dateModified.")
                .with_evidence(date.date_naive().format("%Y-%m-%d").to_string()),
            );
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .expect("a valid instant")
            .with_timezone(&Utc)
    }

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
    fn a_thin_page_is_reported_and_nothing_else_is_assessed() {
        let findings = check_geo(&page("<p>tiny</p>"), "", at("2026-11-15T00:00:00Z"));
        assert_eq!(ids(&findings), ["seo-geo-insufficient-content"]);
    }

    #[test]
    fn a_substantial_page_without_an_author_entity_is_flagged() {
        let findings = check_geo(
            &page(&format!("<p>{}</p>", long_prose())),
            "",
            at("2026-11-15T00:00:00Z"),
        );
        assert!(
            ids(&findings).contains(&"seo-geo-no-author-entity"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn a_stale_page_is_flagged_with_its_age() {
        let html = Html::parse_document(&format!(
            "<html><body><main><p>{}</p>\
             <script type=\"application/ld+json\">{{\"@type\": \"Person\", \"name\": \"A\", \
             \"datePublished\": \"2021-01-01\"}}</script></main></body></html>",
            long_prose()
        ));
        let findings = check_geo(&html, "", at("2026-11-15T00:00:00Z"));
        let stale = findings
            .iter()
            .find(|finding| finding.check_id() == "seo-geo-stale-content")
            .expect("a stale finding");
        assert_eq!(stale.evidence.as_deref(), Some("2021-01-01"));
        assert!(
            stale.title.contains("days"),
            "the age is named in the title: {}",
            stale.title
        );
    }

    #[test]
    fn a_recent_page_is_not_flagged_for_staleness() {
        let html = Html::parse_document(&format!(
            "<html><body><main><p>{}</p>\
             <script type=\"application/ld+json\">{{\"@type\": \"Person\", \"name\": \"A\", \
             \"datePublished\": \"2026-10-01\"}}</script></main></body></html>",
            long_prose()
        ));
        let findings = check_geo(&html, "", at("2026-11-15T00:00:00Z"));
        assert!(!ids(&findings).contains(&"seo-geo-stale-content"));
    }
}
