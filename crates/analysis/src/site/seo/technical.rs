//! Technical checks: robots directives, favicon and hreflang validity.
//!
//! The source's `scan_live` (robots.txt, sitemap.xml over HTTP) is deliberately absent: a crawl
//! is the caller's job, and `analyse.site` takes the fetched artefacts as inputs or leaves those
//! two checks unmeasured rather than pretending they passed.

use scraper::Html;

use crate::model::{Finding, Severity};
use crate::site::seo::{all, attr};

/// `^x-default$` or a two-letter language code with an optional region subtag.
fn hreflang_re() -> &'static regex::Regex {
    static HREFLANG: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    HREFLANG.get_or_init(|| {
        regex::Regex::new(r"^x-default$|^[a-z]{2}(-[a-zA-Z]{2,4})?$").expect("static pattern")
    })
}

/// The favicon `rel` pattern: `icon`, `shortcut icon` or `apple-touch-icon` as a whole token.
fn favicon_re() -> &'static regex::Regex {
    static FAVICON: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    FAVICON.get_or_init(|| {
        regex::Regex::new(r"(?i)(^|\s)(icon|shortcut icon|apple-touch-icon)(\s|$)")
            .expect("static pattern")
    })
}

pub fn check_meta_robots(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let robots = all(html, "meta").into_iter().find(|element| {
        attr(element, "name").is_some_and(|name| name.eq_ignore_ascii_case("robots"))
    });
    let Some(element) = robots else {
        return findings;
    };
    let content = attr(&element, "content").unwrap_or_default().to_lowercase();

    if content.contains("noindex") {
        findings.push(
            Finding::seo(
                "seo-meta-robots-noindex",
                Severity::High,
                "Meta robots noindex directive",
                "The page has a meta robots noindex directive, which prevents search engines from \
                 indexing it.",
            )
            .with_url(url)
            .with_recommendation(
                "Remove the noindex directive if this page should appear in search results.",
            )
            .with_selector("meta[name=\"robots\"]")
            .with_evidence(content.clone()),
        );
    }
    if content.contains("nofollow") {
        findings.push(
            Finding::seo(
                "seo-meta-robots-nofollow",
                Severity::Medium,
                "Meta robots nofollow directive",
                "The page has a meta robots nofollow directive, which prevents search engines from \
                 following links.",
            )
            .with_url(url)
            .with_recommendation(
                "Remove the nofollow directive if you want search engines to follow links on this \
                 page.",
            )
            .with_selector("meta[name=\"robots\"]")
            .with_evidence(content),
        );
    }
    findings
}

pub fn check_favicon(html: &Html, url: &str) -> Vec<Finding> {
    let present = all(html, "link").iter().any(|element| {
        let rel = attr(element, "rel").unwrap_or_default();
        favicon_re().is_match(&rel.to_lowercase())
    });
    let mut findings = Vec::new();
    if !present {
        findings.push(
            Finding::seo(
                "seo-favicon-missing",
                Severity::Low,
                "Missing favicon",
                "No favicon link tag found. A favicon helps with brand recognition in browser tabs \
                 and bookmarks.",
            )
            .with_url(url)
            .with_recommendation(
                "Add <link rel=\"icon\" href=\"/favicon.ico\"> or <link rel=\"icon\" \
                 type=\"image/png\" href=\"/favicon.png\"> to the page head.",
            )
            .with_selector("link[rel=\"icon\"]"),
        );
    }
    findings
}

pub fn check_hreflang(html: &Html, url: &str) -> Vec<Finding> {
    let tags: Vec<_> = all(html, "link")
        .into_iter()
        .filter(|element| {
            // The source matched `rel` exactly ("alternate" and nothing else), so
            // `rel="alternate canonical"` is not an hreflang tag there and is not one here.
            crate::site::seo::rel_is(element, "alternate") && attr(element, "hreflang").is_some()
        })
        .collect();
    let mut findings = Vec::new();
    if tags.is_empty() {
        // No hreflang tags is fine — not every site is multilingual.
        return findings;
    }

    let mut values: Vec<String> = Vec::new();
    let mut invalid: Vec<String> = Vec::new();
    for tag in &tags {
        let value = attr(tag, "hreflang").unwrap_or_default().trim().to_owned();
        if !hreflang_re().is_match(&value) {
            invalid.push(value.clone());
        }
        values.push(value);
    }

    if !values.iter().any(|value| value == "x-default") {
        findings.push(
            Finding::seo(
                "seo-hreflang-no-default",
                Severity::Low,
                "Missing hreflang x-default",
                "Hreflang tags exist but no x-default is specified. The x-default tells search \
                 engines which URL to show for unsupported languages.",
            )
            .with_url(url)
            .with_recommendation(
                "Add <link rel=\"alternate\" hreflang=\"x-default\" href=\"...\"> to specify a \
                 default language fallback.",
            )
            .with_selector("link[rel=\"alternate\"][hreflang]"),
        );
    }

    if !invalid.is_empty() {
        let joined = invalid.join(", ");
        findings.push(
            Finding::seo(
                "seo-hreflang-invalid",
                Severity::Medium,
                "Invalid hreflang values",
                format!(
                    "Invalid hreflang values found: {joined}. Values must be a valid 2-letter \
                     language code, optionally with a region subtag (e.g. 'en', 'en-US')."
                ),
            )
            .with_url(url)
            .with_recommendation("Fix the hreflang values to use valid ISO 639-1 language codes.")
            .with_selector("link[rel=\"alternate\"][hreflang]"),
        );
    }

    let mut seen: Vec<&str> = Vec::new();
    let mut duplicates: Vec<String> = Vec::new();
    for value in &values {
        if seen.contains(&value.as_str()) {
            duplicates.push(value.clone());
        }
        seen.push(value);
    }
    if !duplicates.is_empty() {
        findings.push(
            Finding::seo(
                "seo-hreflang-duplicate",
                Severity::Medium,
                "Duplicate hreflang values",
                format!(
                    "Duplicate hreflang values found: {}. Each language/region combination should \
                     appear only once.",
                    duplicates.join(", ")
                ),
            )
            .with_url(url)
            .with_recommendation("Remove duplicate hreflang link elements.")
            .with_selector("link[rel=\"alternate\"][hreflang]"),
        );
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Html {
        Html::parse_document(html)
    }

    #[test]
    fn noindex_and_nofollow_are_two_findings_from_one_tag() {
        let findings = check_meta_robots(
            &doc("<meta name=\"Robots\" content=\"NOINDEX, NOFOLLOW\">"),
            "https://example.test/",
        );
        let ids: Vec<&str> = findings.iter().map(|f| f.check_id()).collect();
        assert_eq!(
            ids,
            vec!["seo-meta-robots-noindex", "seo-meta-robots-nofollow"]
        );
    }

    #[test]
    fn a_shortcut_icon_satisfies_the_favicon_check() {
        let findings = check_favicon(&doc("<link rel=\"shortcut icon\" href=\"/f.ico\">"), "");
        assert!(findings.is_empty());
    }

    #[test]
    fn an_apple_touch_icon_also_satisfies_it() {
        let findings = check_favicon(&doc("<link rel=\"apple-touch-icon\" href=\"/f.png\">"), "");
        assert!(findings.is_empty());
    }

    #[test]
    fn hreflang_reports_default_invalid_and_duplicate() {
        let html = r#"<link rel="alternate" hreflang="en" href="/en">
                      <link rel="alternate" hreflang="en" href="/en2">
                      <link rel="alternate" hreflang="english-UK" href="/x">"#;
        let findings = check_hreflang(&doc(html), "");
        let ids: Vec<&str> = findings.iter().map(|f| f.check_id()).collect();
        assert_eq!(
            ids,
            vec![
                "seo-hreflang-no-default",
                "seo-hreflang-invalid",
                "seo-hreflang-duplicate"
            ]
        );
    }

    #[test]
    fn no_hreflang_tags_is_not_a_finding() {
        assert!(check_hreflang(&doc("<link rel=\"alternate\" href=\"/x\">"), "").is_empty());
    }
}
