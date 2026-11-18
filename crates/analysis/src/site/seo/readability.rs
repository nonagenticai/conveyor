//! Content-depth and readability checks (group: `content`).
//!
//! Search engines and AI answer engines both favour substantive, readable copy. These are
//! deterministic measures over the page's *main* text:
//!
//! - **Depth** — word count (thin content ranks and gets cited poorly).
//! - **Readability** — Flesch Reading Ease and Gunning Fog from
//!   [`super::content_utils::readability_metrics`].
//!
//! Findings are emitted in the `seo-content-<check>` namespace, so the audit scorer groups them
//! under `content`. A page with substantial, readable copy emits nothing and scores 100 for the
//! group.

use scraper::Html;

use crate::model::{Finding, Severity};

use super::content_utils::{main_text, readability_metrics, words};

/// Below this a content page is unlikely to satisfy intent.
const THIN_WORDS: usize = 300;
/// Below this it is effectively a stub.
const VERY_THIN_WORDS: usize = 150;
/// Readability is only assessed once there is enough text for the formulas to mean anything.
/// Gated at the very-thin floor so a stub reports depth only — no double reporting.
const MIN_WORDS_FOR_READABILITY: usize = VERY_THIN_WORDS;
/// Flesch Reading Ease: higher is easier. `<30` is "very difficult" (post-graduate).
const FRE_VERY_HARD: f64 = 30.0;
const FRE_HARD: f64 = 50.0;
/// Gunning Fog: approximate US school grade needed to read on first pass.
const FOG_TOO_HIGH: f64 = 16.0;

/// Run the content group's checks over one document.
pub fn check_readability(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let text = main_text(html);
    let word_count = words(&text).len();

    if word_count < VERY_THIN_WORDS {
        findings.push(
            Finding::seo(
                "seo-content-very-thin",
                Severity::High,
                format!("Very thin content ({word_count} words)"),
                format!(
                    "The main content is only {word_count} words. Pages this short rarely \
                     satisfy search intent or earn AI-answer citations, and read as stubs to \
                     ranking systems."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Expand the primary content to substantively cover the topic (aim 600+ words \
                 for content pages).",
            )
            .with_evidence(format!("{word_count} words")),
        );
    } else if word_count < THIN_WORDS {
        findings.push(
            Finding::seo(
                "seo-content-thin",
                Severity::Medium,
                format!("Thin content ({word_count} words)"),
                format!(
                    "The main content is {word_count} words — below the ~300-word floor where \
                     pages start to compete for informational and commercial queries."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Add depth: cover sub-topics, add examples, answer related questions.",
            )
            .with_evidence(format!("{word_count} words")),
        );
    }

    if word_count >= MIN_WORDS_FOR_READABILITY {
        let metrics = readability_metrics(&text);
        let flesch = metrics.flesch_reading_ease;
        let fog = metrics.gunning_fog;

        if flesch < FRE_VERY_HARD {
            findings.push(
                Finding::seo(
                    "seo-content-hard-to-read",
                    Severity::Medium,
                    "Content is very difficult to read",
                    format!(
                        "Flesch Reading Ease is {flesch:.0} (very difficult, post-graduate \
                         level). Hard-to-read copy loses users and is less quotable by AI answer \
                         engines."
                    ),
                )
                .with_url(url)
                .with_recommendation(
                    "Shorten sentences, cut jargon, and prefer plain language (aim FRE 50-70).",
                )
                .with_evidence(format!("Flesch Reading Ease {flesch:.0}")),
            );
        } else if flesch < FRE_HARD {
            findings.push(
                Finding::seo(
                    "seo-content-hard-to-read",
                    Severity::Low,
                    "Content is fairly difficult to read",
                    format!(
                        "Flesch Reading Ease is {flesch:.0} (fairly difficult). A broader \
                         reading level widens reach and improves engagement signals."
                    ),
                )
                .with_url(url)
                .with_recommendation("Simplify phrasing where possible; break up long sentences.")
                .with_evidence(format!("Flesch Reading Ease {flesch:.0}")),
            );
        }

        if fog > FOG_TOO_HIGH {
            findings.push(
                Finding::seo(
                    "seo-content-grade-too-high",
                    Severity::Low,
                    format!("High reading-grade level (Gunning Fog {fog:.0})"),
                    format!(
                        "The copy needs roughly a grade-{fog:.0} reading level. Most web \
                         audiences read comfortably around grade 8-10."
                    ),
                )
                .with_url(url)
                .with_recommendation(
                    "Reduce sentence length and multi-syllable words to lower the reading grade.",
                )
                .with_evidence(format!("Gunning Fog {fog:.0}")),
            );
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> Html {
        Html::parse_document(&format!("<html><body><main>{body}</main></body></html>"))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(Finding::check_id).collect()
    }

    /// `count` copies of `word`, space-separated.
    fn repeated(word: &str, count: usize) -> String {
        std::iter::repeat_n(word, count)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `sentences` sentences of ten one-syllable words each: readable, long enough, no finding.
    fn readable_prose(sentences: usize) -> String {
        std::iter::repeat_n("cat cat cat cat cat cat cat cat cat cat.", sentences)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_stub_page_is_very_thin_and_is_not_readability_assessed() {
        let findings = check_readability(&page("<h1>Hi</h1><p>Just a few short words.</p>"), "");
        assert_eq!(ids(&findings), ["seo-content-very-thin"]);
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].evidence.as_deref(), Some("6 words"));
    }

    #[test]
    fn a_page_between_the_floors_is_thin() {
        let findings = check_readability(&page(&format!("<p>{}</p>", repeated("water", 200))), "");
        assert!(
            ids(&findings).contains(&"seo-content-thin"),
            "{:?}",
            ids(&findings)
        );
        assert!(!ids(&findings).contains(&"seo-content-very-thin"));
        assert_eq!(findings[0].severity, Severity::Medium);
    }

    #[test]
    fn a_long_page_of_simple_sentences_is_not_flagged() {
        let findings = check_readability(&page(&format!("<p>{}</p>", readable_prose(40))), "");
        assert!(ids(&findings).is_empty(), "{:?}", ids(&findings));
    }

    #[test]
    fn one_enormous_sentence_is_flagged_for_grade() {
        let findings = check_readability(&page(&format!("<p>{}</p>", repeated("cat", 400))), "");
        assert!(
            ids(&findings).contains(&"seo-content-grade-too-high"),
            "{:?}",
            ids(&findings)
        );
        assert!(
            ids(&findings).contains(&"seo-content-hard-to-read"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn every_finding_carries_the_scanner_and_the_url() {
        let findings = check_readability(&page("<p>tiny</p>"), "https://example.test/");
        assert_eq!(findings[0].url, "https://example.test/");
        assert_eq!(findings[0].source_tool.as_deref(), Some("seo_scanner"));
    }
}
