//! Featured-snippet / answer-readiness checks (group: `snippet`).
//!
//! Featured snippets and AI answer engines reward pages that pose a question in a heading and
//! answer it immediately in a tight 40-60 word block. These are structural checks — no model is
//! needed to see whether a question heading is followed by a concise answer.
//!
//! Findings are opportunities (all `low`): a content page that never structures an answerable
//! question forfeits snippet and "People Also Ask" real estate.
//!
//! The answer search walks the document **forward** from the heading, not its siblings, so an
//! answer wrapped in its own `<div>` is still found — the source's hardening fix, kept.

use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node};

use crate::model::{Finding, Severity};

use super::content_utils::{main_text, spaced_text, words};
use super::{all, attr, text};

/// Snippet structure is only assessed on pages with real content.
const MIN_WORDS: usize = 200;
/// Featured-snippet paragraph sweet spot.
const ANSWER_MIN: usize = 40;
const ANSWER_MAX: usize = 60;

/// A heading is "answerable" if it opens with an interrogative or ends with `?`.
fn question_re() -> &'static Regex {
    static QUESTION: OnceLock<Regex> = OnceLock::new();
    QUESTION.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(what|how|why|when|where|who|which|is|are|can|do|does|should|will|could|would|did|was|were)\b",
        )
        .expect("static pattern")
    })
}

fn is_question(text: &str) -> bool {
    !text.is_empty() && (text.trim_end().ends_with('?') || question_re().is_match(text))
}

/// Run the snippet group's checks over one document.
pub fn check_snippet(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if words(&main_text(html)).len() < MIN_WORDS {
        return findings;
    }

    let headings: Vec<ElementRef<'_>> = all(html, "h2, h3");
    let question_headings: Vec<ElementRef<'_>> = headings
        .into_iter()
        .filter(|heading| is_question(&spaced_text(heading)))
        .collect();

    if question_headings.is_empty() {
        findings.push(
            Finding::seo(
                "seo-snippet-no-question-heading",
                Severity::Low,
                "No question-format headings for snippet capture",
                "No <h2>/<h3> is phrased as a question. Question headings answered concisely are \
                 how pages win featured snippets and 'People Also Ask' slots.",
            )
            .with_url(url)
            .with_recommendation(
                "Add question-style subheadings (e.g. 'How does X work?') with direct answers.",
            ),
        );
        return findings;
    }

    let snippet_ready = question_headings
        .iter()
        .any(|heading| (ANSWER_MIN..=ANSWER_MAX).contains(&answer_word_count(html, heading)));
    if !snippet_ready {
        findings.push(
            Finding::seo(
                "seo-snippet-no-concise-answer",
                Severity::Low,
                "Question headings lack a concise answer",
                format!(
                    "{} question heading(s) found, but none is immediately followed by a \
                     {ANSWER_MIN}-{ANSWER_MAX} word answer. Snippets are pulled from a tight \
                     direct answer right after the question.",
                    question_headings.len()
                ),
            )
            .with_url(url)
            .with_recommendation(format!(
                "Answer each question heading in a {ANSWER_MIN}-{ANSWER_MAX} word paragraph \
                 directly beneath it."
            )),
        );
    }

    if question_headings.len() >= 2 && !has_faq_schema(html) {
        findings.push(
            Finding::seo(
                "seo-snippet-no-faq-schema",
                Severity::Low,
                "Multiple Q&A sections without FAQPage schema",
                format!(
                    "The page has {} question sections but no FAQPage structured data. FAQPage \
                     markup makes the Q&A eligible for rich results.",
                    question_headings.len()
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Add FAQPage JSON-LD structured data describing the questions and answers.",
            ),
        );
    }

    findings
}

/// Word count of the first answer block following a heading, else 0.
///
/// Walks the document forward from the heading's own position (its descendants included, as the
/// source's `find_all_next` did), stopping at the next heading of any level so the following
/// section's copy is never borrowed as the answer.
fn answer_word_count(html: &Html, heading: &ElementRef<'_>) -> usize {
    let target = heading.id();
    let mut seen_heading = false;
    for node in html.root_element().descendants() {
        if node.id() == target {
            seen_heading = true;
            continue;
        }
        if !seen_heading {
            continue;
        }
        let Node::Element(element) = node.value() else {
            continue;
        };
        match element.name() {
            "h1" | "h2" | "h3" | "h4" => break,
            "p" | "ul" | "ol" | "table" => {
                if let Some(block) = ElementRef::wrap(node) {
                    return words(&spaced_text(&block)).len();
                }
            }
            _ => {}
        }
    }
    0
}

fn has_faq_schema(html: &Html) -> bool {
    all(html, "script").iter().any(|script| {
        attr(script, "type") == Some("application/ld+json")
            && text(script).to_lowercase().contains("faqpage")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(Finding::check_id).collect()
    }

    /// `count` paragraphs of 100 words each — enough to clear the 200-word gate.
    fn filler_paragraphs(count: usize) -> String {
        let paragraph = std::iter::repeat_n("padding", 100)
            .collect::<Vec<_>>()
            .join(" ");
        std::iter::repeat_n(format!("<p>{paragraph}</p>"), count)
            .collect::<Vec<_>>()
            .join("")
    }

    /// A 50-word answer, inside the 40-60 window.
    const ANSWER: &str = "Fresh coffee tastes better because the beans keep their oils and aromas \
                          only briefly after roasting so grinding right before you brew keeps the \
                          flavour bright and lively instead of flat and stale the way pre ground \
                          material always tastes after sitting around in a warm open bag.";

    fn page(main: &str) -> Html {
        Html::parse_document(&format!("<html><body><main>{main}</main></body></html>"))
    }

    #[test]
    fn an_answered_question_is_not_flagged() {
        let main = format!(
            "<h1>Coffee brewing guide</h1>{}{}<div><h2>Why does fresh coffee taste \
             better?</h2></div><div><p>{ANSWER}</p></div>",
            filler_paragraphs(1),
            filler_paragraphs(1)
        );
        let findings = check_snippet(&page(&main), "");
        assert!(ids(&findings).is_empty(), "{:?}", ids(&findings));
    }

    #[test]
    fn a_page_without_a_question_heading_is_flagged() {
        let main = format!(
            "<h1>Coffee</h1>{}<h2>Our process</h2><p>Short.</p>",
            filler_paragraphs(2)
        );
        let findings = check_snippet(&page(&main), "");
        assert_eq!(ids(&findings), ["seo-snippet-no-question-heading"]);
    }

    #[test]
    fn a_question_without_a_concise_answer_is_flagged() {
        let main = format!(
            "<h1>Coffee</h1>{}<h2>What is a burr grinder?</h2><p>It crushes beans.</p>",
            filler_paragraphs(2)
        );
        let findings = check_snippet(&page(&main), "");
        assert!(
            ids(&findings).contains(&"seo-snippet-no-concise-answer"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn two_questions_without_faq_schema_are_flagged() {
        let main = format!(
            "<h1>Coffee</h1>{}<h2>What is a burr grinder?</h2><p>{ANSWER}</p>\
             <h2>How do I store beans?</h2><p>{ANSWER}</p>",
            filler_paragraphs(1)
        );
        let findings = check_snippet(&page(&main), "");
        assert!(
            ids(&findings).contains(&"seo-snippet-no-faq-schema"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn a_thin_page_is_not_assessed() {
        let findings = check_snippet(&page("<h1>Coffee</h1><p>Short page.</p>"), "");
        assert!(findings.is_empty());
    }
}
