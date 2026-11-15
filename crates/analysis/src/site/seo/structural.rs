//! Structural checks: canonical URL, structured data, internal nofollow, breadcrumb schema and
//! URL shape.

use scraper::Html;
use serde_json::Value;

use crate::model::{Finding, Severity};
use crate::site::seo::{all, attr, host_of, rel_is, rel_tokens, scheme_of, text};

pub fn check_canonical(html: &Html, url: &str) -> Vec<Finding> {
    let present = all(html, "link")
        .iter()
        .any(|element| rel_is(element, "canonical"));
    let mut findings = Vec::new();
    if !present {
        findings.push(
            Finding::seo(
                "seo-canonical-missing",
                Severity::Medium,
                "Missing canonical URL",
                "No canonical link tag found. This can cause duplicate content issues.",
            )
            .with_url(url)
            .with_recommendation("Add <link rel=\"canonical\" href=\"...\"> to the page head.")
            .with_selector("link[rel=\"canonical\"]"),
        );
    }
    findings
}

/// The JSON-LD blocks on the page, parsed. A block that does not parse yields `Err(())` exactly
/// once — the source reported one "invalid JSON-LD" finding per bad block, not per item.
///
/// Shared with the audit groups' signal reader, so the crate has one JSON-LD parser rather than
/// two that can disagree about what counts as a block.
pub(crate) fn json_ld_blocks(html: &Html) -> Vec<Result<Value, ()>> {
    let mut blocks = Vec::new();
    for script in all(html, "script") {
        if attr(&script, "type") != Some("application/ld+json") {
            continue;
        }
        let raw = text(&script);
        if raw.is_empty() {
            // The source guarded the parse on `script.string` being truthy: an empty block is
            // not an error, it is simply nothing.
            continue;
        }
        blocks.push(serde_json::from_str::<Value>(&raw).map_err(|_| ()));
    }
    blocks
}

fn items_of(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    }
}

fn type_of(item: &Value) -> Option<&Value> {
    match item {
        Value::Object(map) => match map.get("@type") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value),
        },
        _ => None,
    }
}

pub fn check_structured_data(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let blocks = json_ld_blocks(html);
    if blocks.is_empty() {
        findings.push(
            Finding::seo(
                "seo-structured-data-missing",
                Severity::Low,
                "No structured data found",
                "No JSON-LD structured data found. Rich snippets improve search visibility.",
            )
            .with_url(url)
            .with_recommendation(
                "Add JSON-LD structured data (Organization, WebPage, BreadcrumbList, etc.).",
            )
            .with_selector("script[type=\"application/ld+json\"]"),
        );
        return findings;
    }

    for block in blocks {
        match block {
            Err(()) => findings.push(
                Finding::seo(
                    "seo-structured-data-invalid",
                    Severity::Medium,
                    "Invalid JSON-LD structured data",
                    "A JSON-LD script contains invalid JSON.",
                )
                .with_url(url)
                .with_recommendation("Fix the JSON syntax in the structured data script.")
                .with_selector("script[type=\"application/ld+json\"]"),
            ),
            Ok(value) => {
                for item in items_of(&value) {
                    if !item.is_object() {
                        continue;
                    }
                    match type_of(item) {
                        None => findings.push(
                            Finding::seo(
                                "seo-structured-data-no-type",
                                Severity::Medium,
                                "Structured data missing @type",
                                "A JSON-LD block is missing the @type property, which is required \
                                 for search engines to understand the data.",
                            )
                            .with_url(url)
                            .with_recommendation(
                                "Add an @type property (e.g. 'Organization', 'WebPage', \
                                 'Article') to the JSON-LD block.",
                            )
                            .with_selector("script[type=\"application/ld+json\"]"),
                        ),
                        Some(type_value) => {
                            let named = item
                                .get("name")
                                .map(|value| !value.is_null())
                                .unwrap_or(false)
                                || item
                                    .get("headline")
                                    .map(|value| !value.is_null())
                                    .unwrap_or(false);
                            if !named {
                                let kind = match type_value {
                                    Value::String(text) => text.clone(),
                                    other => other.to_string(),
                                };
                                findings.push(
                                    Finding::seo(
                                        "seo-structured-data-no-name",
                                        Severity::Low,
                                        "Structured data missing name/headline",
                                        format!(
                                            "A JSON-LD block of type '{kind}' is missing both \
                                             'name' and 'headline' properties."
                                        ),
                                    )
                                    .with_url(url)
                                    .with_recommendation(
                                        "Add a 'name' or 'headline' property to provide a title \
                                         for the structured data.",
                                    )
                                    .with_selector("script[type=\"application/ld+json\"]"),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    findings
}

/// Flag `<a rel="nofollow">` links that point at the page's own host.
pub fn check_internal_nofollow(html: &Html, url: &str) -> Vec<Finding> {
    let page_host = host_of(url);
    let mut findings = Vec::new();
    if page_host.is_empty() {
        return findings;
    }

    let mut flagged: Vec<String> = Vec::new();
    for anchor in all(html, "a") {
        // `find_all("a", href=True)` matches an empty `href` as well; the source parsed it as a
        // relative URL and counted it as internal, and that reading is kept here.
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        if !rel_tokens(&anchor).iter().any(|token| token == "nofollow") {
            continue;
        }
        let href_host = host_of(href);
        let href_host = if href_host.is_empty() {
            page_host.clone()
        } else {
            href_host
        };
        if href_host == page_host {
            flagged.push(href.to_owned());
        }
    }

    if !flagged.is_empty() {
        let count = flagged.len();
        let sample = flagged
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = if count > 5 {
            format!(" (and {} more)", count - 5)
        } else {
            String::new()
        };
        findings.push(
            Finding::seo(
                "seo-internal-nofollow",
                Severity::Medium,
                format!("{count} internal link(s) with rel='nofollow'"),
                format!(
                    "{count} internal link(s) use rel='nofollow', which prevents search engines \
                     from following them and wastes internal PageRank."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Remove rel='nofollow' from internal links so search engines can crawl and index \
                 linked pages.",
            )
            .with_selector("a[rel='nofollow']")
            .with_evidence(format!("Links: {sample}{extra}")),
        );
    }
    findings
}

fn has_breadcrumb_list(item: &Value) -> bool {
    if let Value::Object(map) = item {
        if matches!(map.get("@type"), Some(Value::String(kind)) if kind == "BreadcrumbList") {
            return true;
        }
        if let Some(Value::Array(graph)) = map.get("@graph") {
            for node in graph {
                if let Value::Object(node) = node
                    && matches!(node.get("@type"), Some(Value::String(kind)) if kind == "BreadcrumbList")
                {
                    return true;
                }
            }
        }
    }
    false
}

pub fn check_breadcrumb_schema(html: &Html, url: &str) -> Vec<Finding> {
    for block in json_ld_blocks(html) {
        let Ok(value) = block else { continue };
        for item in items_of(&value) {
            if has_breadcrumb_list(item) {
                return Vec::new();
            }
        }
    }
    vec![
        Finding::seo(
            "seo-breadcrumb-missing",
            Severity::Low,
            "Missing BreadcrumbList structured data",
            "No BreadcrumbList JSON-LD structured data found. Breadcrumb schema helps search \
             engines understand site hierarchy and can generate rich breadcrumb trails in search \
             results.",
        )
        .with_url(url)
        .with_recommendation(
            "Add JSON-LD BreadcrumbList structured data to help search engines display breadcrumb \
             navigation in results.",
        )
        .with_selector("script[type=\"application/ld+json\"]"),
    ]
}

pub fn check_canonical_mismatch(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if url.is_empty() {
        return findings;
    }
    let Some(canonical) = all(html, "link")
        .into_iter()
        .find(|element| rel_is(element, "canonical"))
    else {
        return findings;
    };
    let href = attr(&canonical, "href")
        .unwrap_or_default()
        .trim()
        .to_owned();
    if href.is_empty() {
        return findings;
    }

    let page_scheme = scheme_of(url);
    let canonical_scheme = scheme_of(&href);
    if !page_scheme.is_empty() && !canonical_scheme.is_empty() && page_scheme != canonical_scheme {
        findings.push(
            Finding::seo(
                "seo-canonical-protocol-mismatch",
                Severity::Medium,
                "Canonical URL protocol mismatch",
                format!(
                    "The page is served over {} but the canonical URL uses {} ({href}). This can \
                     confuse search engine indexing.",
                    page_scheme.to_uppercase(),
                    canonical_scheme.to_uppercase()
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Ensure the canonical URL uses the same protocol as the page (preferably HTTPS).",
            )
            .with_selector("link[rel=\"canonical\"]")
            .with_evidence(format!("Page: {page_scheme} | Canonical: {href}")),
        );
    }
    findings
}

/// Analyse the URL itself: underscores, query-parameter count and path depth.
pub fn check_url_structure(url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if url.is_empty() {
        return findings;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return findings;
    };

    let path = parsed.path();
    if path.contains('_') {
        findings.push(
            Finding::seo(
                "seo-url-underscores",
                Severity::Low,
                "URL contains underscores",
                "The URL path contains underscores. Search engines treat hyphens as word \
                 separators but not underscores.",
            )
            .with_url(url)
            .with_recommendation("Use hyphens (-) instead of underscores (_) in URLs."),
        );
    }

    if let Some(query) = parsed.query().filter(|query| !query.is_empty()) {
        let parameters = query.split('&').count();
        if parameters > 3 {
            findings.push(
                Finding::seo(
                    "seo-url-query-params",
                    Severity::Low,
                    "Excessive URL query parameters",
                    format!(
                        "The URL has {parameters} query parameters. Too many parameters can \
                         dilute page authority and cause crawl issues."
                    ),
                )
                .with_url(url)
                .with_recommendation(
                    "Reduce query parameters to essential ones. Consider using cleaner URL \
                     patterns.",
                ),
            );
        }
    }

    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count();
    if segments > 4 {
        findings.push(
            Finding::seo(
                "seo-url-depth",
                Severity::Info,
                "Deep URL structure",
                format!(
                    "The URL has {segments} path segments. Deep URLs may receive less crawl \
                     priority."
                ),
            )
            .with_url(url)
            .with_recommendation(
                "Consider flattening the URL structure to keep important pages within 3-4 levels \
                 of depth.",
            ),
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
    fn a_json_ld_block_without_type_is_reported() {
        let findings =
            check_structured_data(&doc("<script type=\"application/ld+json\">{}</script>"), "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-structured-data-no-type");
    }

    #[test]
    fn invalid_json_is_one_finding_not_a_panic() {
        let findings = check_structured_data(
            &doc("<script type=\"application/ld+json\">{not json</script>"),
            "",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-structured-data-invalid");
    }

    #[test]
    fn a_graph_node_counts_as_breadcrumbs() {
        let html = r#"<script type="application/ld+json">
            {"@graph": [{"@type": "BreadcrumbList", "itemListElement": []}]}
        </script>"#;
        assert!(check_breadcrumb_schema(&doc(html), "").is_empty());
    }

    #[test]
    fn a_top_level_breadcrumb_counts_too() {
        let html = r#"<script type="application/ld+json">
            [{"@type": "BreadcrumbList"}]
        </script>"#;
        assert!(check_breadcrumb_schema(&doc(html), "").is_empty());
    }

    #[test]
    fn a_relative_nofollow_link_is_internal() {
        let findings = check_internal_nofollow(
            &doc("<a href=\"/pricing\" rel=\"nofollow\">p</a>"),
            "https://example.test/",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check_id(), "seo-internal-nofollow");
    }

    #[test]
    fn an_external_nofollow_link_is_not_flagged() {
        let findings = check_internal_nofollow(
            &doc("<a href=\"https://other.test/p\" rel=\"nofollow\">p</a>"),
            "https://example.test/",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn url_structure_reads_path_and_query() {
        let findings = check_url_structure("https://example.test/a_b/c/d/e/f?x=1&y=2&z=3&w=4");
        let ids: Vec<&str> = findings.iter().map(|f| f.check_id()).collect();
        assert_eq!(
            ids,
            vec![
                "seo-url-underscores",
                "seo-url-query-params",
                "seo-url-depth"
            ]
        );
    }

    #[test]
    fn a_protocol_mismatch_is_medium() {
        let findings = check_canonical_mismatch(
            &doc("<link rel=\"canonical\" href=\"http://example.test/\">"),
            "https://example.test/",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
    }
}
