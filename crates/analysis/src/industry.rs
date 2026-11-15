//! Industry classification from page content — the narrow view `schema_coverage` needs.
//!
//! The schema-coverage check asks one question: does this page carry the structured-data types a
//! site *of its kind* should? Answering it needs the site's vertical, and the absorbed service
//! derived that deterministically from keyword density over headings, metadata and body copy,
//! plus the JSON-LD `@type`s already present.
//!
//! This is a deliberately **partial** port of that detector: it returns the industry name only.
//! Sub-industry, confidence and the matched-signal list are not carried, because nothing here
//! reads them; the brand family will need them and can widen this module instead of re-deriving
//! the tables.
//!
//! One documented deviation: the source resolved equal scores by iterating a Python `set`, whose
//! order is randomised per process. This picks the alphabetically first industry on a tie, so the
//! same page always classifies the same way.

use std::collections::BTreeSet;

use scraper::Html;

use crate::site::seo::{all, attr, text_trimmed};

/// Schema.org `@type` → (industry, score).
const SCHEMA_TYPE_INDUSTRY: &[(&str, &str, f64)] = &[
    ("SoftwareApplication", "saas", 5.0),
    ("WebApplication", "saas", 5.0),
    ("Product", "ecommerce", 5.0),
    ("Offer", "ecommerce", 5.0),
    ("ItemList", "ecommerce", 5.0),
    ("NewsArticle", "media", 5.0),
    ("BlogPosting", "media", 5.0),
    ("Article", "media", 5.0),
    ("MedicalOrganization", "healthcare", 5.0),
    ("Hospital", "healthcare", 5.0),
    ("FinancialService", "finance", 5.0),
    ("BankAccount", "finance", 5.0),
    ("EducationalOrganization", "education", 5.0),
    ("Course", "education", 5.0),
    ("ProfessionalService", "agency", 3.0),
    ("NGO", "nonprofit", 5.0),
    ("NonprofitOrganization", "nonprofit", 5.0),
];

/// Every industry's keyword list, in the source's order and spelling.
const INDUSTRY_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "saas",
        &[
            "saas",
            "software",
            "platform",
            "cloud",
            "subscription",
            "api",
            "dashboard",
            "automation",
            "workflow",
            "integration",
            "deploy",
            "scalable",
            "uptime",
            "sla",
        ],
    ),
    (
        "ecommerce",
        &[
            "shop",
            "store",
            "cart",
            "checkout",
            "product",
            "price",
            "buy",
            "shipping",
            "order",
            "catalog",
            "sale",
            "discount",
            "inventory",
        ],
    ),
    (
        "media",
        &[
            "news",
            "article",
            "publish",
            "editor",
            "journalist",
            "media",
            "press",
            "blog",
            "content",
            "story",
            "reporter",
            "breaking",
        ],
    ),
    (
        "healthcare",
        &[
            "health",
            "medical",
            "patient",
            "doctor",
            "clinical",
            "hospital",
            "therapy",
            "wellness",
            "diagnosis",
            "treatment",
            "pharmaceutical",
            "care",
        ],
    ),
    (
        "finance",
        &[
            "bank",
            "invest",
            "finance",
            "loan",
            "credit",
            "insurance",
            "trading",
            "portfolio",
            "wealth",
            "mortgage",
            "savings",
            "interest",
        ],
    ),
    (
        "education",
        &[
            "learn",
            "course",
            "student",
            "teach",
            "education",
            "university",
            "school",
            "curriculum",
            "enrollment",
            "degree",
            "certificate",
            "training",
        ],
    ),
    (
        "agency",
        &[
            "agency",
            "creative",
            "design",
            "branding",
            "strategy",
            "campaign",
            "client",
            "portfolio",
            "case study",
            "consultation",
            "services",
        ],
    ),
    (
        "nonprofit",
        &[
            "donate",
            "nonprofit",
            "charity",
            "volunteer",
            "mission",
            "cause",
            "foundation",
            "community",
            "impact",
            "support",
            "fundraise",
        ],
    ),
];

/// Body copy is capped, as the source capped it, so one long page cannot outweigh its headings.
const BODY_TEXT_LIMIT: usize = 2000;

/// Classify the page into an industry, or `None` when nothing scores.
pub fn classify(html: &Html, schema_types: &BTreeSet<String>) -> Option<String> {
    let headings = headings_text(html);
    let meta = meta_text(html);
    let body = body_text(html);

    let headings_lower = headings.to_lowercase();
    let meta_lower = meta.to_lowercase();
    let body_lower = body.to_lowercase();

    let mut scores: Vec<(&str, f64)> = INDUSTRY_KEYWORDS
        .iter()
        .map(|(industry, keywords)| {
            let mut score = 0.0;
            for keyword in *keywords {
                let keyword = keyword.to_lowercase();
                score += occurrences(&headings_lower, &keyword) as f64 * 3.0;
                score += occurrences(&meta_lower, &keyword) as f64 * 2.0;
                score += occurrences(&body_lower, &keyword) as f64;
            }
            (*industry, score)
        })
        .collect();

    for (industry, score) in schema_scores(schema_types) {
        if let Some(entry) = scores.iter_mut().find(|(name, _)| *name == industry) {
            entry.1 += score;
        }
    }

    let mut best: Option<(&str, f64)> = None;
    for (industry, score) in scores {
        if score <= 0.0 {
            continue;
        }
        // Ties go to the alphabetically first industry — a documented deviation from the
        // source's set-iteration order.
        let wins = match best {
            None => true,
            Some((best_industry, best_score)) => {
                score > best_score || (score == best_score && industry < best_industry)
            }
        };
        if wins {
            best = Some((industry, score));
        }
    }
    best.map(|(industry, _)| industry.to_owned())
}

fn schema_scores(schema_types: &BTreeSet<String>) -> Vec<(&'static str, f64)> {
    let mut scores: Vec<(&str, f64)> = Vec::new();
    for schema_type in schema_types {
        if let Some((_, industry, score)) = SCHEMA_TYPE_INDUSTRY
            .iter()
            .find(|(name, _, _)| name == schema_type)
        {
            match scores.iter_mut().find(|(name, _)| name == industry) {
                Some(entry) => entry.1 += score,
                None => scores.push((industry, *score)),
            }
        }
    }
    scores
}

/// Non-overlapping occurrences of `needle` in `haystack` — Python's `str.count`.
fn occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.matches(needle).count()
}

fn headings_text(html: &Html) -> String {
    all(html, "h1, h2, h3")
        .iter()
        .map(text_trimmed)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn meta_text(html: &Html) -> String {
    let mut parts: Vec<String> = Vec::new();
    let title = all(html, "title")
        .into_iter()
        .next()
        .map(|element| text_trimmed(&element))
        .unwrap_or_default();
    if !title.is_empty() {
        parts.push(title);
    }
    for (name, value) in [
        ("name", "description"),
        ("property", "og:description"),
        ("name", "keywords"),
    ] {
        let content = all(html, "meta")
            .into_iter()
            .find(|meta| attr(meta, name) == Some(value))
            .and_then(|meta| attr(&meta, "content").map(str::to_owned))
            .map(|content| content.trim().to_owned())
            .unwrap_or_default();
        if !content.is_empty() {
            parts.push(content);
        }
    }
    parts.join(" ")
}

fn body_text(html: &Html) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut total_chars = 0usize;
    for paragraph in all(html, "p") {
        let text = text_trimmed(&paragraph);
        if text.is_empty() {
            continue;
        }
        total_chars += text.chars().count();
        parts.push(text);
        if total_chars >= BODY_TEXT_LIMIT {
            break;
        }
    }
    parts.join(" ").chars().take(BODY_TEXT_LIMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(head: &str, body: &str) -> Html {
        Html::parse_document(&format!(
            "<html><head><title>{head}</title></head><body>{body}</body></html>"
        ))
    }

    #[test]
    fn schema_types_alone_classify_the_page() {
        let html = page("Shop", "<p>Hello</p>");
        let types: BTreeSet<String> = ["Product".to_owned()].into();
        assert_eq!(classify(&html, &types).as_deref(), Some("ecommerce"));
    }

    #[test]
    fn keywords_in_headings_outweigh_body_mentions() {
        let html = page(
            "Banking",
            "<h1>Bank invest finance</h1><p>shop store cart checkout product price buy</p>",
        );
        assert_eq!(
            classify(&html, &BTreeSet::new()).as_deref(),
            Some("finance")
        );
    }

    #[test]
    fn an_unremarkable_page_has_no_industry() {
        let html = page("A page", "<p>nothing in particular here</p>");
        assert_eq!(classify(&html, &BTreeSet::new()), None);
    }

    #[test]
    fn a_tie_breaks_alphabetically() {
        // One media keyword in the body and one finance keyword in the body: equal scores.
        let html = page("Page", "<p>news bank</p>");
        assert_eq!(
            classify(&html, &BTreeSet::new()).as_deref(),
            Some("finance")
        );
    }
}
