//! Experience / Expertise / Authoritativeness / Trust checks (group: `eeat`).
//!
//! These read the trust signals a page exposes: HTTPS, a privacy policy, reachable contact
//! details, a named author, outbound citations and a publish or update date. All are markup and
//! text observations — no model, no judgement of quality.
//!
//! Findings are emitted in the `seo-eeat-<check>` namespace. On thin pages the group reports a
//! single `insufficient-content` finding instead of piling up attribution complaints against copy
//! that has none.

use std::sync::OnceLock;

use regex::Regex;
use scraper::Html;

use crate::model::{Finding, Severity};

use super::content_utils::{main_text, spaced_text, words};
use super::signals::{external_link_hrefs, extract_content_date, has_author_signal};
use super::{all, attr, scheme_of};

/// Authority signals only make sense on real content pages.
const MIN_WORDS: usize = 200;

fn privacy_re() -> &'static Regex {
    static PRIVACY: OnceLock<Regex> = OnceLock::new();
    PRIVACY.get_or_init(|| Regex::new(r"(?i)privacy").expect("static pattern"))
}

fn contact_re() -> &'static Regex {
    static CONTACT: OnceLock<Regex> = OnceLock::new();
    CONTACT.get_or_init(|| Regex::new(r"(?i)contact|get in touch|support").expect("static pattern"))
}

/// Run the E-E-A-T group's checks over one document.
pub fn check_eeat(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    let scheme = scheme_of(url);
    if !scheme.is_empty() && scheme != "https" {
        findings.push(
            Finding::seo(
                "seo-eeat-no-https",
                Severity::Medium,
                "Page is not served over HTTPS",
                "The page is served over plain HTTP. HTTPS is a baseline trust and ranking \
                 signal; browsers and search engines penalise insecure pages.",
            )
            .with_url(url)
            .with_recommendation("Serve the site over HTTPS with a valid TLS certificate."),
        );
    }

    if !has_link_like(html, privacy_re()) {
        findings.push(
            Finding::seo(
                "seo-eeat-no-privacy-link",
                Severity::Low,
                "No privacy policy link",
                "No link to a privacy policy was found. A privacy policy is a core \
                 trustworthiness signal, especially for YMYL and lead-gen pages.",
            )
            .with_url(url)
            .with_recommendation("Add a footer link to a privacy policy."),
        );
    }

    let contact_href = all(html, "a").iter().any(|anchor| {
        attr(anchor, "href")
            .is_some_and(|href| href.starts_with("mailto:") || href.starts_with("tel:"))
    });
    if !has_link_like(html, contact_re()) && !contact_href {
        findings.push(
            Finding::seo(
                "seo-eeat-no-contact",
                Severity::Low,
                "No contact information",
                "No contact link, email, or phone was found. Reachable contact details signal a \
                 real, accountable organisation behind the content.",
            )
            .with_url(url)
            .with_recommendation("Add a contact page link or a visible email/phone."),
        );
    }

    if words(&main_text(html)).len() >= MIN_WORDS {
        if !has_author_signal(html) {
            findings.push(
                Finding::seo(
                    "seo-eeat-no-author",
                    Severity::Low,
                    "No author attribution",
                    "The content has no author byline or author metadata. Named, credible \
                     authorship is central to Expertise and Authoritativeness.",
                )
                .with_url(url)
                .with_recommendation(
                    "Add a visible author byline and author metadata (schema Person).",
                ),
            );
        }

        if external_link_hrefs(html, url).is_empty() {
            findings.push(
                Finding::seo(
                    "seo-eeat-no-citations",
                    Severity::Low,
                    "No outbound citations",
                    "The content cites no external sources. Linking to authoritative references \
                     strengthens trust and measurably improves AI-answer citation.",
                )
                .with_url(url)
                .with_recommendation(
                    "Cite authoritative external sources where you make factual claims.",
                ),
            );
        }

        if extract_content_date(html).is_none() {
            findings.push(
                Finding::seo(
                    "seo-eeat-no-date",
                    Severity::Low,
                    "No publish or update date",
                    "No publish/modify date was found in metadata or markup. Dates signal \
                     freshness and let search and AI engines judge currency.",
                )
                .with_url(url)
                .with_recommendation(
                    "Expose datePublished/dateModified via Article schema or a <time> element.",
                ),
            );
        }
    } else {
        findings.push(
            Finding::seo(
                "seo-eeat-insufficient-content",
                Severity::Low,
                "Not enough content to assess expertise & authority",
                "The page has too little body text to evaluate authorship, outbound citations, \
                 or freshness. E-E-A-T cannot be established on thin content.",
            )
            .with_url(url)
            .with_recommendation(
                "Add substantive, authored content with credible sources and dates.",
            ),
        );
    }

    findings
}

/// True if any anchor's text or href matches `pattern`.
///
/// Every `<a>` counts, with or without an `href`, which is what the source iterated.
fn has_link_like(html: &Html, pattern: &Regex) -> bool {
    all(html, "a").iter().any(|anchor| {
        pattern.is_match(&spaced_text(anchor))
            || pattern.is_match(attr(anchor, "href").unwrap_or_default())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(main: &str) -> Html {
        Html::parse_document(&format!("<html><body><main>{main}</main></body></html>"))
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
    fn every_trust_signal_missing_is_reported() {
        let html = page("<p>tiny</p>");
        let findings = check_eeat(&html, "http://example.test/");
        let found = ids(&findings);
        for expected in [
            "seo-eeat-no-https",
            "seo-eeat-no-privacy-link",
            "seo-eeat-no-contact",
            "seo-eeat-insufficient-content",
        ] {
            assert!(
                found.contains(&expected),
                "{expected} missing from {found:?}"
            );
        }
    }

    #[test]
    fn a_complete_page_reports_nothing() {
        let html = Html::parse_document(&format!(
            "<html><body><main><p>{}</p>\
             <a href=\"/privacy\">Privacy</a><a href=\"/contact\">Contact</a>\
             <a href=\"https://source.test/study\">Study</a>\
             <p class=\"byline\">By A. Writer</p><time datetime=\"2026-10-01\"></time>\
             </main></body></html>",
            long_prose()
        ));
        let findings = check_eeat(&html, "https://example.test/");
        assert!(ids(&findings).is_empty(), "{:?}", ids(&findings));
    }

    #[test]
    fn a_mailto_link_is_contact_information() {
        let html = Html::parse_document(&format!(
            "<html><body><main><p>{}</p><a href=\"mailto:hi@example.test\">Email us</a>\
             </main></body></html>",
            long_prose()
        ));
        let findings = check_eeat(&html, "https://example.test/");
        assert!(!ids(&findings).contains(&"seo-eeat-no-contact"));
    }

    #[test]
    fn an_empty_url_does_not_produce_an_https_finding() {
        let findings = check_eeat(&page("<p>x</p>"), "");
        assert!(!ids(&findings).contains(&"seo-eeat-no-https"));
    }
}
