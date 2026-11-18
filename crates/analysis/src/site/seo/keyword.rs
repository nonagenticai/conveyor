//! Keyword focus and stuffing checks (group: `keyword`).
//!
//! A page earns topical relevance by repeating one term it actually wants to rank for, in the
//! title, the H1 and the opening copy — and loses it by repeating it so often that the copy reads
//! as spam. The primary term is detected deterministically: the most frequent stop-word-filtered
//! unigram in the main content, and only when the page is substantial enough for that to mean
//! something.
//!
//! Findings are emitted in the `seo-keyword-<check>` namespace.

use std::collections::BTreeMap;

use scraper::Html;

use crate::model::{Finding, Severity};

use super::content_utils::{content_words, main_text, phrase_in_tokens, spaced_text, words};
use super::first;

/// Below this the detector has too little text to trust a dominant term.
const MIN_CONTENT_WORDS: usize = 60;
/// A term seen fewer times than this is not the page's focus.
const MIN_PRIMARY_COUNT: usize = 3;
/// "The opening" for the intro check.
const INTRO_WORDS: usize = 100;
/// Density above this reads as keyword stuffing (over-optimization penalty).
const STUFFING_DENSITY: f64 = 0.03;

/// The page's most likely primary keyword, or `None` if it is not clear.
///
/// Ties go to the term that appeared first, which is what Python's `Counter.most_common` does.
pub fn detect_primary_keyword(html: &Html) -> Option<String> {
    let tokens = content_words(&main_text(html));
    if tokens.len() < MIN_CONTENT_WORDS {
        return None;
    }
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (index, token) in tokens.iter().enumerate() {
        counts.entry(token.as_str()).or_insert((0, index)).0 += 1;
    }
    let (term, (frequency, _)) = counts
        .iter()
        .max_by(|left, right| left.1.0.cmp(&right.1.0).then(right.1.1.cmp(&left.1.1)))?;
    if *frequency < MIN_PRIMARY_COUNT {
        return None;
    }
    Some((*term).to_owned())
}

/// Run the keyword group's checks over one document.
pub fn check_keyword(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(primary) = detect_primary_keyword(html) else {
        return findings;
    };

    let text = main_text(html);
    let tokens = words(&text);
    let total = tokens.len();

    let title_text = first(html, "title")
        .map(|title| spaced_text(&title).to_lowercase())
        .unwrap_or_default();
    let h1_text = first(html, "h1")
        .map(|h1| spaced_text(&h1).to_lowercase())
        .unwrap_or_default();
    let intro = tokens
        .iter()
        .take(INTRO_WORDS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    if !title_text.is_empty() && !phrase_in_tokens(&primary, &title_text) {
        findings.push(
            Finding::seo(
                "seo-keyword-missing-in-title",
                Severity::Medium,
                "Primary keyword missing from the title tag",
                format!(
                    "The page's dominant term ('{primary}') does not appear in the <title>. \
                     The title is the single strongest on-page relevance signal."
                ),
            )
            .with_url(url)
            .with_recommendation(format!("Work '{primary}' naturally into the title tag."))
            .with_evidence(format!("primary='{primary}'")),
        );
    }

    if !h1_text.is_empty() && !phrase_in_tokens(&primary, &h1_text) {
        findings.push(
            Finding::seo(
                "seo-keyword-missing-in-h1",
                Severity::Low,
                "Primary keyword missing from the H1",
                format!(
                    "The dominant term ('{primary}') is absent from the <h1>. Aligning the H1 \
                     with the page's focus reinforces topical relevance."
                ),
            )
            .with_url(url)
            .with_recommendation(format!(
                "Include '{primary}' in the H1 where it reads naturally."
            ))
            .with_evidence(format!("primary='{primary}'")),
        );
    }

    if total >= INTRO_WORDS && !phrase_in_tokens(&primary, &intro) {
        findings.push(
            Finding::seo(
                "seo-keyword-not-in-intro",
                Severity::Low,
                "Primary keyword missing from the opening content",
                format!(
                    "'{primary}' does not appear in the first {INTRO_WORDS} words. Stating the \
                     topic early helps both readers and search engines confirm relevance."
                ),
            )
            .with_url(url)
            .with_recommendation(format!(
                "Introduce '{primary}' within the opening paragraph."
            ))
            .with_evidence(format!("primary='{primary}'")),
        );
    }

    if total > 0 {
        let density =
            tokens.iter().filter(|token| **token == primary).count() as f64 / total as f64;
        if density > STUFFING_DENSITY {
            findings.push(
                Finding::seo(
                    "seo-keyword-stuffing",
                    Severity::Medium,
                    format!(
                        "Possible keyword stuffing ('{primary}' at {:.1}%)",
                        density * 100.0
                    ),
                    format!(
                        "'{primary}' makes up {:.1}% of the copy (>{:.0}%). Over-optimization \
                         reads as spam to search engines and hurts readability.",
                        density * 100.0,
                        STUFFING_DENSITY * 100.0
                    ),
                )
                .with_url(url)
                .with_recommendation("Reduce repetition; use synonyms and natural phrasing.")
                .with_evidence(format!("density {:.1}%", density * 100.0)),
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

    /// A page where `term` appears `count` times, interleaved with five filler words so the term
    /// is the most frequent content word and the page clears the 60-word substance floor.
    fn prose(term: &str, count: usize) -> String {
        let fillers = ["garden", "music", "coffee", "travel", "bread"];
        let mut parts = Vec::new();
        for index in 0..count {
            parts.push(term.to_owned());
            parts.push(fillers[index % fillers.len()].to_owned());
        }
        parts.join(" ")
    }

    /// `count` distinct alphabetic words — filler that cannot dominate a term count.
    fn unique_words(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| {
                let letter = (b'a' + (index % 26) as u8) as char;
                let group = (b'a' + (index / 26) as u8) as char;
                format!("k{letter}{group}")
            })
            .collect()
    }

    #[test]
    fn the_dominant_term_is_detected() {
        let html = page(&format!("<p>{}</p>", prose("plumber", 30)));
        assert_eq!(detect_primary_keyword(&html).as_deref(), Some("plumber"));
    }

    #[test]
    fn a_thin_page_has_no_primary_keyword() {
        let html = page("<p>plumber plumber plumber and a few more words</p>");
        assert_eq!(detect_primary_keyword(&html), None);
    }

    #[test]
    fn a_term_seen_twice_is_not_the_primary_keyword() {
        let filler = unique_words(60).join(" ");
        let html = page(&format!("<p>{filler} plumber plumber</p>"));
        assert_eq!(detect_primary_keyword(&html), None);
    }

    #[test]
    fn a_keyword_absent_from_the_title_is_flagged() {
        let body = format!("<h1>Leeds plumber</h1><p>{}</p>", prose("plumber", 30));
        let html = Html::parse_document(&format!(
            "<html><head><title>Emergency pipes guide</title></head><body><main>{body}</main></body></html>"
        ));
        let findings = check_keyword(&html, "");
        assert!(
            ids(&findings).contains(&"seo-keyword-missing-in-title"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn a_keyword_present_in_the_title_is_not_flagged() {
        let body = format!("<h1>Leeds plumber</h1><p>{}</p>", prose("plumber", 30));
        let html = Html::parse_document(&format!(
            "<html><head><title>Leeds plumber</title></head><body><main>{body}</main></body></html>"
        ));
        let findings = check_keyword(&html, "");
        assert!(!ids(&findings).contains(&"seo-keyword-missing-in-title"));
        assert!(!ids(&findings).contains(&"seo-keyword-missing-in-h1"));
    }

    #[test]
    fn a_substring_of_a_title_word_does_not_count_as_present() {
        // The primary term is "art"; "Smart" contains it but is a different word.
        let text = format!(
            "{} {}",
            std::iter::repeat_n("art", 20).collect::<Vec<_>>().join(" "),
            unique_words(40).join(" ")
        );
        let html = Html::parse_document(&format!(
            "<html><head><title>Smart marketing guide</title></head><body><main><h1>Smart marketing</h1>\
             <p>{text}</p></main></body></html>"
        ));
        assert_eq!(detect_primary_keyword(&html).as_deref(), Some("art"));
        let findings = check_keyword(&html, "");
        assert!(
            ids(&findings).contains(&"seo-keyword-missing-in-title"),
            "{:?}",
            ids(&findings)
        );
    }

    #[test]
    fn a_stuffed_page_is_flagged() {
        let body = format!("<h1>plumber</h1><p>{}</p>", prose("plumber", 80));
        let html = Html::parse_document(&format!(
            "<html><head><title>plumber</title></head><body><main>{body}</main></body></html>"
        ));
        let findings = check_keyword(&html, "");
        assert!(
            ids(&findings).contains(&"seo-keyword-stuffing"),
            "{:?}",
            ids(&findings)
        );
    }
}
