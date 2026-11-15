//! Structured-data coverage checks (group: `schema`).
//!
//! The base scanner asks "is there any JSON-LD?"; this group asks the harder question — does the
//! page carry the types its business and content warrant, and are the present types complete?
//!
//! The business type comes from [`crate::industry::classify`], which is deterministic and reuses
//! the page's own `@type`s as a signal. Namespace `seo-schema-<check>` → group `schema`.

use scraper::Html;
use serde_json::Value;

use crate::industry;
use crate::model::{Finding, Severity};

use super::signals::{extract_schema_types, is_truthy, schema_objects_of_type};

/// Schema types worth having per detected industry, beyond the universal base.
const RECOMMENDED: &[(&str, &[&str])] = &[
    (
        "saas",
        &["Organization", "WebSite", "SoftwareApplication", "FAQPage"],
    ),
    (
        "ecommerce",
        &["Organization", "Product", "Offer", "BreadcrumbList"],
    ),
    (
        "media",
        &["Organization", "WebSite", "Article", "BreadcrumbList"],
    ),
    ("healthcare", &["Organization", "LocalBusiness", "WebSite"]),
    ("finance", &["Organization", "WebSite", "FAQPage"]),
    ("education", &["Organization", "Course", "WebSite"]),
    ("agency", &["Organization", "LocalBusiness", "WebSite"]),
    ("nonprofit", &["Organization", "WebSite"]),
];

const UNIVERSAL: &[&str] = &["Organization", "WebSite"];

/// An Organization is satisfied by any of these concrete types.
const ORG_TYPES: &[&str] = &[
    "Organization",
    "LocalBusiness",
    "Corporation",
    "NGO",
    "OnlineBusiness",
];

/// Required properties for the key content types (rich-result essentials).
const REQUIRED_PROPS: &[(&str, &[&str])] = &[
    ("Article", &["headline", "author", "datePublished", "image"]),
    (
        "BlogPosting",
        &["headline", "author", "datePublished", "image"],
    ),
    (
        "NewsArticle",
        &["headline", "author", "datePublished", "image"],
    ),
    ("Product", &["name", "offers"]),
];

/// Run the schema-coverage group's checks over one document.
pub fn check_schema_coverage(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let present = extract_schema_types(html);

    // 1. A fundamental Organization/LocalBusiness entity.
    if !present
        .iter()
        .any(|name| ORG_TYPES.contains(&name.as_str()))
    {
        findings.push(
            Finding::seo(
                "seo-schema-no-organization",
                Severity::Medium,
                "No Organization schema",
                "No Organization/LocalBusiness structured data was found. This entity anchors \
                 your brand for search knowledge panels and AI answer engines.",
            )
            .with_url(url)
            .with_recommendation(
                "Add Organization (or LocalBusiness) JSON-LD with name, url and logo.",
            ),
        );
    }

    // 2. Industry-recommended coverage gap.
    let industry = industry::classify(html, &present);
    let recommended = industry
        .as_deref()
        .and_then(|name| RECOMMENDED.iter().find(|(industry, _)| *industry == name))
        .map(|(_, types)| *types)
        .unwrap_or(UNIVERSAL);
    let mut satisfied: Vec<&str> = present.iter().map(String::as_str).collect();
    if present
        .iter()
        .any(|name| ORG_TYPES.contains(&name.as_str()))
    {
        satisfied.push("Organization");
    }
    let mut missing: Vec<&str> = recommended
        .iter()
        .copied()
        .filter(|name| !satisfied.contains(name))
        .collect();
    missing.sort_unstable();
    if !missing.is_empty() {
        let label = industry
            .as_deref()
            .map(|name| format!(" for a {name} site"))
            .unwrap_or_default();
        let missing_list = missing.join(", ");
        let present_list = present.iter().cloned().collect::<Vec<_>>().join(", ");
        findings.push(
            Finding::seo(
                "seo-schema-coverage-gap",
                Severity::Low,
                format!("Missing recommended schema types: {missing_list}"),
                format!(
                    "The recommended structured-data types{label} are not all present. \
                     Missing: {missing_list}. Richer schema improves rich-result and AI-citation \
                     eligibility."
                ),
            )
            .with_url(url)
            .with_recommendation(format!("Add JSON-LD for: {missing_list}."))
            .with_evidence(format!(
                "industry={} present=[{}]",
                industry.as_deref().unwrap_or("unknown"),
                present_list
            )),
        );
    }

    // 3. Completeness of the present key types.
    for &(type_name, required) in REQUIRED_PROPS {
        for object in schema_objects_of_type(html, type_name) {
            let missing_props: Vec<&str> = required
                .iter()
                .copied()
                .filter(|property| !property_is_present(&object, property))
                .collect();
            if missing_props.is_empty() {
                continue;
            }
            let missing_list = missing_props.join(", ");
            findings.push(
                Finding::seo(
                    format!("seo-schema-{}-incomplete", type_name.to_lowercase()),
                    Severity::Low,
                    format!("{type_name} schema missing {missing_list}"),
                    format!(
                        "A {type_name} JSON-LD block is missing recommended properties \
                         ({missing_list}), reducing rich-result eligibility."
                    ),
                )
                .with_url(url)
                .with_recommendation(format!("Add {missing_list} to the {type_name} schema.")),
            );
            // One finding per type is enough.
            break;
        }
    }

    findings
}

/// Python's `not obj.get(property)`: absent and falsy both count as missing.
fn property_is_present(object: &Value, property: &str) -> bool {
    object.get(property).is_some_and(is_truthy)
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

    #[test]
    fn a_page_with_no_structured_data_is_flagged_for_organization() {
        let findings = check_schema_coverage(&page("<h1>Hello</h1>"), "");
        assert!(
            ids(&findings).contains(&"seo-schema-no-organization"),
            "{:?}",
            ids(&findings)
        );
        assert_eq!(findings[0].severity, Severity::Medium);
    }

    #[test]
    fn an_organization_satisfies_the_entity_check() {
        let html = page(
            r#"<script type="application/ld+json">{"@type": "Organization", "name": "A"}</script>"#,
        );
        let findings = check_schema_coverage(&html, "");
        assert!(!ids(&findings).contains(&"seo-schema-no-organization"));
    }

    #[test]
    fn an_incomplete_article_is_flagged_once() {
        let html = page(
            r#"<script type="application/ld+json">
               {"@type": "Article", "headline": "H", "image": "i.jpg"}
               </script>"#,
        );
        let findings = check_schema_coverage(&html, "");
        let found = ids(&findings);
        let incomplete: Vec<&str> = found
            .iter()
            .copied()
            .filter(|id| id.ends_with("article-incomplete"))
            .collect();
        assert_eq!(incomplete.len(), 1, "{found:?}");
        assert_eq!(incomplete[0], "seo-schema-article-incomplete");
    }

    #[test]
    fn a_schema_type_classifies_the_industry_and_widens_the_gap() {
        let html = page(
            r#"<script type="application/ld+json">{"@type": "Organization", "name": "A"}</script>
               <script type="application/ld+json">{"@type": "Product", "name": "P", "offers": {}}</script>"#,
        );
        let findings = check_schema_coverage(&html, "");
        let gap = findings
            .iter()
            .find(|finding| finding.check_id() == "seo-schema-coverage-gap")
            .expect("a gap finding");
        assert!(
            gap.recommendation.contains("Offer"),
            "ecommerce recommends Offer, which is missing: {}",
            gap.recommendation
        );
    }

    #[test]
    fn a_complete_organization_page_has_no_fully_missing_base() {
        let html = page(
            r#"<script type="application/ld+json">
               {"@graph": [
                 {"@type": "Organization", "name": "A"},
                 {"@type": "WebSite", "name": "A"}
               ]}
               </script>"#,
        );
        let findings = check_schema_coverage(&html, "");
        assert!(
            !ids(&findings).contains(&"seo-schema-coverage-gap"),
            "{:?}",
            ids(&findings)
        );
    }
}
