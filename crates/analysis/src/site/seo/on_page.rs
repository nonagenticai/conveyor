//! On-page checks: title, meta description, headings, image alt text, keyword alignment and
//! duplicate meta tags.

use std::collections::BTreeMap;

use scraper::Html;

use crate::model::{Finding, Severity};
use crate::site::seo::{all, attr, first, text, text_trimmed};

pub fn check_title(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    match first(html, "title") {
        None => findings.push(
            Finding::seo(
                "seo-missing-title",
                Severity::High,
                "Missing title tag",
                "The page is missing a <title> tag, which is critical for SEO.",
            )
            .with_url(url)
            .with_recommendation("Add a descriptive <title> tag between 50-60 characters.")
            .with_selector("head > title"),
        ),
        Some(title) => {
            // The source guarded on `title.string` being truthy before stripping, so a title
            // with no text at all produced nothing while a whitespace-only one counted as short.
            let raw = text(&title);
            if !raw.is_empty() {
                let title_text = raw.trim();
                let length = title_text.chars().count();
                if length < 10 {
                    findings.push(
                        Finding::seo(
                            "seo-title-too-short",
                            Severity::Medium,
                            "Title tag too short",
                            format!("Title is only {length} characters: '{title_text}'."),
                        )
                        .with_url(url)
                        .with_recommendation(
                            "Expand the title to 50-60 characters with relevant keywords.",
                        )
                        .with_selector("head > title"),
                    );
                } else if length > 70 {
                    findings.push(
                        Finding::seo(
                            "seo-title-too-long",
                            Severity::Low,
                            "Title tag too long",
                            format!(
                                "Title is {length} characters and may be truncated in search results."
                            ),
                        )
                        .with_url(url)
                        .with_recommendation("Shorten the title to 50-60 characters.")
                        .with_selector("head > title"),
                    );
                }
            }
        }
    }
    findings
}

pub fn check_meta_description(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let meta = all(html, "meta")
        .into_iter()
        .find(|element| attr(element, "name") == Some("description"));
    match meta {
        None => findings.push(
            Finding::seo(
                "seo-meta-description-missing",
                Severity::High,
                "Missing meta description",
                "No meta description found. Search engines use this for snippets.",
            )
            .with_url(url)
            .with_recommendation("Add a meta description of 150-160 characters.")
            .with_selector("meta[name=\"description\"]"),
        ),
        Some(element) => {
            let content = attr(&element, "content").unwrap_or_default().trim();
            let length = content.chars().count();
            if length < 50 {
                findings.push(
                    Finding::seo(
                        "seo-meta-description-too-short",
                        Severity::Medium,
                        "Meta description too short",
                        format!("Meta description is only {length} characters."),
                    )
                    .with_url(url)
                    .with_recommendation(
                        "Expand to 150-160 characters with a compelling description.",
                    )
                    .with_selector("meta[name=\"description\"]"),
                );
            } else if length > 170 {
                findings.push(
                    Finding::seo(
                        "seo-meta-description-too-long",
                        Severity::Low,
                        "Meta description too long",
                        format!("Meta description is {length} characters and may be truncated."),
                    )
                    .with_url(url)
                    .with_recommendation("Shorten to 150-160 characters.")
                    .with_selector("meta[name=\"description\"]"),
                );
            }
        }
    }
    findings
}

pub fn check_heading_hierarchy(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let h1s = all(html, "h1");
    if h1s.is_empty() {
        findings.push(
            Finding::seo(
                "seo-missing-h1",
                Severity::High,
                "Missing H1 heading",
                "No H1 heading found. Every page should have exactly one H1.",
            )
            .with_url(url)
            .with_recommendation("Add a single H1 heading that describes the page content.")
            .with_selector("h1"),
        );
    } else if h1s.len() > 1 {
        findings.push(
            Finding::seo(
                "seo-multiple-h1",
                Severity::Medium,
                "Multiple H1 headings",
                format!(
                    "Found {} H1 headings. Best practice is exactly one per page.",
                    h1s.len()
                ),
            )
            .with_url(url)
            .with_recommendation("Keep only one H1 and convert others to H2 or lower.")
            .with_selector("h1"),
        );
    }

    // Document order: the source walked the parsed tree, and `scraper` selects in tree order.
    let headings = all(html, "h1, h2, h3, h4, h5, h6");
    if !headings.is_empty() {
        let levels: Vec<u8> = headings
            .iter()
            .map(|element| {
                element
                    .value()
                    .name()
                    .as_bytes()
                    .get(1)
                    .map(|digit| digit - b'0')
                    .unwrap_or(0)
            })
            .collect();
        for index in 1..levels.len() {
            if levels[index] > levels[index - 1] + 1 {
                let tag = headings[index].value().name().to_owned();
                findings.push(
                    Finding::seo(
                        "seo-heading-hierarchy-skip",
                        Severity::Low,
                        "Heading hierarchy skip",
                        format!(
                            "Heading jumps from H{} to H{}, skipping a level.",
                            levels[index - 1],
                            levels[index]
                        ),
                    )
                    .with_url(url)
                    .with_recommendation(
                        "Maintain sequential heading hierarchy (H1 > H2 > H3, etc.).",
                    )
                    .with_selector(tag),
                );
                break;
            }
        }
    }
    findings
}

pub fn check_image_alt(html: &Html, url: &str) -> Vec<Finding> {
    let images = all(html, "img");
    let missing = images
        .iter()
        .filter(|element| {
            attr(element, "alt")
                .map(|alt| alt.is_empty())
                .unwrap_or(true)
        })
        .count();
    let mut findings = Vec::new();
    if missing > 0 {
        let total = images.len();
        let severity = if (missing as f64) > (total as f64) * 0.5 {
            Severity::High
        } else {
            Severity::Medium
        };
        findings.push(
            Finding::seo(
                "seo-images-missing-alt",
                severity,
                format!("{missing} images missing alt text"),
                format!(
                    "{missing} of {total} images lack alt attributes, hurting accessibility and SEO."
                ),
            )
            .with_url(url)
            .with_recommendation("Add descriptive alt text to all images.")
            .with_selector("img:not([alt])"),
        );
    }
    findings
}

/// Does the `<title>` share any significant word with the `<h1>`?
pub fn check_keyword_alignment(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let (Some(title), Some(h1)) = (first(html, "title"), first(html, "h1")) else {
        return findings;
    };
    let title_text = text_trimmed(&title).to_lowercase();
    let h1_text = text(&h1).trim().to_lowercase();
    if title_text.is_empty() || h1_text.is_empty() {
        return findings;
    }

    let words = |value: &str| -> std::collections::BTreeSet<String> {
        split_words(value)
            .into_iter()
            .filter(|word| word.chars().count() > 2)
            .collect()
    };
    let title_words = words(&title_text);
    let h1_words = words(&h1_text);

    if !title_words.is_empty()
        && !h1_words.is_empty()
        && title_words.intersection(&h1_words).next().is_none()
    {
        findings.push(
            Finding::seo(
                "seo-keyword-alignment",
                Severity::Low,
                "Title and H1 have no keyword overlap",
                "The <title> and <h1> share no significant words. Aligning them reinforces the \
                 page topic for search engines.",
            )
            .with_url(url)
            .with_recommendation(
                "Include the primary keyword in both the <title> and <h1> to strengthen topical \
                 relevance.",
            )
            .with_evidence(format!("Title: '{title_text}' | H1: '{h1_text}'")),
        );
    }
    findings
}

pub fn check_duplicate_meta(html: &Html, url: &str) -> Vec<Finding> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for element in all(html, "meta") {
        if let Some(name) = attr(&element, "name") {
            let name = name.to_lowercase();
            let name = name.trim();
            if !name.is_empty() {
                *counts.entry(name.to_owned()).or_insert(0) += 1;
            }
        }
    }
    let duplicates: Vec<(String, usize)> =
        counts.into_iter().filter(|(_, count)| *count > 1).collect();

    let mut findings = Vec::new();
    if !duplicates.is_empty() {
        let examples = duplicates
            .iter()
            .map(|(name, count)| format!("'{name}' ({count}x)"))
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(
            Finding::seo(
                "seo-duplicate-meta",
                Severity::Medium,
                format!("{} duplicate meta tag(s)", duplicates.len()),
                format!(
                    "Found duplicate <meta name> tags: {examples}. Duplicate meta tags can \
                     confuse search engines about which value to use."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Remove duplicate meta tags so each name attribute appears only once.",
            )
            .with_selector("meta[name]")
            .with_evidence(examples),
        );
    }
    findings
}

/// Python's `re.split(r"\W+", text)` with the empty pieces it leaves at the edges.
fn split_words(value: &str) -> Vec<String> {
    static SPLIT: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let split = SPLIT.get_or_init(|| regex::Regex::new(r"\W+").expect("static pattern"));
    split
        .split(value)
        .filter(|piece| !piece.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Html {
        Html::parse_document(html)
    }

    #[test]
    fn a_page_with_nothing_finds_nothing_silent() {
        let findings = check_title(&doc("<html><head></head><body></body></html>"), "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-missing-title");
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn a_title_is_measured_in_characters_not_bytes() {
        let findings = check_title(&doc("<title>日本語のタイトル</title>"), "");
        assert_eq!(findings[0].check_id(), "seo-title-too-short");
        assert!(findings[0].description.contains("8 characters"));
    }

    #[test]
    fn a_heading_jump_is_reported_once() {
        let findings =
            check_heading_hierarchy(&doc("<h1>a</h1><h2>b</h2><h4>c</h4><h5>d</h5>"), "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-heading-hierarchy-skip");
        assert!(findings[0].description.contains("H2 to H4"));
    }

    #[test]
    fn most_images_without_alt_is_high_severity() {
        let findings = check_image_alt(&doc("<img src=a><img src=b><img src=c alt=\"x\">"), "");
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].title, "2 images missing alt text");
    }

    #[test]
    fn title_and_h1_with_no_shared_word_is_flagged() {
        let findings = check_keyword_alignment(
            &doc("<title>Buy garden tools</title><h1>Kitchen renovation ideas</h1>"),
            "",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-keyword-alignment");
    }

    #[test]
    fn a_shared_word_clears_keyword_alignment() {
        let findings =
            check_keyword_alignment(&doc("<title>Garden tools</title><h1>Garden ideas</h1>"), "");
        assert!(findings.is_empty());
    }

    #[test]
    fn duplicate_meta_names_are_counted_and_sorted() {
        let findings = check_duplicate_meta(
            &doc(
                "<meta name=\"description\" content=a><meta name=\"description\" content=b>\
                 <meta name=\"robots\" content=c><meta name=\"robots\" content=d>\
                 <meta name=\"viewport\" content=e>",
            ),
            "",
        );
        assert_eq!(findings[0].check_id(), "seo-duplicate-meta");
        assert_eq!(
            findings[0].evidence.as_deref(),
            Some("'description' (2x), 'robots' (2x)")
        );
    }
}
