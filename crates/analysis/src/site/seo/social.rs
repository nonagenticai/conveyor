//! Social preview checks: Open Graph and Twitter Cards.

use scraper::Html;

use crate::model::{Finding, Severity};
use crate::site::seo::{all, attr};

pub fn check_og_tags(html: &Html, url: &str) -> Vec<Finding> {
    let metas = all(html, "meta");
    let mut missing = Vec::new();
    for required in ["og:title", "og:description", "og:image", "og:url"] {
        let present = metas
            .iter()
            .any(|element| attr(element, "property") == Some(required));
        if !present {
            missing.push(required);
        }
    }

    let mut findings = Vec::new();
    if !missing.is_empty() {
        let joined = missing.join(", ");
        findings.push(
            Finding::seo(
                "seo-og-tags-missing",
                Severity::Medium,
                "Missing Open Graph tags",
                format!("Missing OG tags: {joined}. Social sharing will lack rich previews."),
            )
            .with_url(url)
            .with_recommendation(format!("Add the following Open Graph meta tags: {joined}.")),
        );
    }
    findings
}

pub fn check_twitter_cards(html: &Html, url: &str) -> Vec<Finding> {
    let metas = all(html, "meta");
    let has = |name: &str| -> bool {
        metas
            .iter()
            .any(|element| attr(element, "name") == Some(name))
    };

    let mut findings = Vec::new();
    if !has("twitter:card") {
        findings.push(
            Finding::seo(
                "seo-twitter-card-missing",
                Severity::Medium,
                "Missing Twitter Card tag",
                "No twitter:card meta tag found. Twitter shares will lack rich card previews.",
            )
            .with_url(url)
            .with_recommendation(
                "Add <meta name=\"twitter:card\" content=\"summary_large_image\"> to enable \
                 Twitter Card previews.",
            )
            .with_selector("meta[name=\"twitter:card\"]"),
        );
    }

    let mut missing = Vec::new();
    for name in ["twitter:title", "twitter:description", "twitter:image"] {
        if !has(name) {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        let joined = missing.join(", ");
        findings.push(
            Finding::seo(
                "seo-twitter-meta-incomplete",
                Severity::Low,
                "Incomplete Twitter Card meta tags",
                format!(
                    "Missing Twitter meta tags: {joined}. Complete tags improve social sharing \
                     appearance."
                ),
            )
            .with_url(url)
            .with_recommendation(format!("Add the following Twitter meta tags: {joined}.")),
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_social_head_is_quiet() {
        let html = r#"<meta property="og:title" content="t">
            <meta property="og:description" content="d">
            <meta property="og:image" content="i">
            <meta property="og:url" content="u">
            <meta name="twitter:card" content="summary_large_image">
            <meta name="twitter:title" content="t">
            <meta name="twitter:description" content="d">
            <meta name="twitter:image" content="i">"#;
        let document = Html::parse_document(html);
        assert!(check_og_tags(&document, "").is_empty());
        assert!(check_twitter_cards(&document, "").is_empty());
    }

    #[test]
    fn missing_social_tags_are_named_in_order() {
        let document = Html::parse_document("<meta property=\"og:title\" content=\"t\">");
        let og = check_og_tags(&document, "");
        assert_eq!(
            og[0].description,
            "Missing OG tags: og:description, og:image, og:url. Social sharing will lack rich previews."
        );
        let twitter = check_twitter_cards(&document, "");
        assert_eq!(twitter.len(), 2);
        assert_eq!(twitter[0].check_id(), "seo-twitter-card-missing");
        assert_eq!(twitter[1].check_id(), "seo-twitter-meta-incomplete");
    }
}
