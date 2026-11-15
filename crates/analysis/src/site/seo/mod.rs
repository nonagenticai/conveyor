//! The SEO scan: nineteen base checks, their audit groups, and the group scorecard.
//!
//! The check list and its order are the source scanner's (`SEOScanner.scan`), because the order
//! is what a caller sees in `findings` and dedupe keeps first-seen order. The group weights and
//! the legacy `check_id` → group map are the source's too: a report that re-derived them would
//! score differently from the service it replaces, which is the one thing a port must not do.

pub mod content_utils;
pub mod mobile;
pub mod on_page;
pub mod readability;
pub mod social;
pub mod structural;
pub mod technical;

use std::collections::BTreeMap;

use scraper::{ElementRef, Html, Selector};

use crate::model::{Category, Document, Finding, Scorecard};
use crate::rubric::Rubric;

/// One weighted audit group. The weights sum to 1.0.
pub const GROUP_WEIGHTS: &[(&str, f64)] = &[
    ("on_page", 0.22),
    ("technical", 0.18),
    ("content", 0.12),
    ("keyword", 0.10),
    ("schema", 0.10),
    ("eeat", 0.10),
    ("internal_linking", 0.07),
    ("snippet", 0.06),
    ("geo", 0.05),
];

/// The pre-`seo-<group>-<check>` checks, mapped explicitly. Consulted before the prefix
/// convention, because these ids predate it.
pub const LEGACY_CHECK_GROUPS: &[(&str, &str)] = &[
    ("seo-missing-title", "on_page"),
    ("seo-title-too-short", "on_page"),
    ("seo-title-too-long", "on_page"),
    ("seo-meta-description-missing", "on_page"),
    ("seo-meta-description-too-short", "on_page"),
    ("seo-meta-description-too-long", "on_page"),
    ("seo-missing-h1", "on_page"),
    ("seo-multiple-h1", "on_page"),
    ("seo-heading-hierarchy-skip", "on_page"),
    ("seo-images-missing-alt", "on_page"),
    ("seo-keyword-alignment", "on_page"),
    ("seo-duplicate-meta", "on_page"),
    ("seo-canonical-missing", "technical"),
    ("seo-canonical-protocol-mismatch", "technical"),
    ("seo-internal-nofollow", "technical"),
    ("seo-url-underscores", "technical"),
    ("seo-url-query-params", "technical"),
    ("seo-url-depth", "technical"),
    ("seo-meta-robots-noindex", "technical"),
    ("seo-meta-robots-nofollow", "technical"),
    ("seo-favicon-missing", "technical"),
    ("seo-hreflang-no-default", "technical"),
    ("seo-hreflang-invalid", "technical"),
    ("seo-hreflang-duplicate", "technical"),
    ("seo-robots-txt-missing", "technical"),
    ("seo-sitemap-missing", "technical"),
    ("seo-viewport-missing", "technical"),
    ("seo-mobile-font-size", "technical"),
    ("seo-og-tags-missing", "technical"),
    ("seo-twitter-card-missing", "technical"),
    ("seo-twitter-meta-incomplete", "technical"),
    ("seo-structured-data-missing", "schema"),
    ("seo-structured-data-no-type", "schema"),
    ("seo-structured-data-no-name", "schema"),
    ("seo-structured-data-invalid", "schema"),
    ("seo-breadcrumb-missing", "schema"),
];

/// Where a `check_id` that matches nothing lands. The source's safe default, kept so a stray id
/// still contributes to the composite instead of vanishing from it.
pub const DEFAULT_GROUP: &str = "on_page";

/// Prefix token → group, where the token is shorter than the group name.
pub const PREFIX_ALIASES: &[(&str, &str)] = &[("linking", "internal_linking")];

/// Map a `check_id` to its audit group.
pub fn group_of(check_id: &str) -> String {
    if check_id.is_empty() {
        return DEFAULT_GROUP.to_owned();
    }
    if let Some((_, group)) = LEGACY_CHECK_GROUPS.iter().find(|(id, _)| *id == check_id) {
        return (*group).to_owned();
    }
    let parts: Vec<&str> = check_id.split('-').collect();
    if parts.len() >= 3 && parts[0] == "seo" {
        let token = parts[1];
        let group = PREFIX_ALIASES
            .iter()
            .find(|(alias, _)| *alias == token)
            .map(|(_, group)| *group)
            .unwrap_or(token);
        if GROUP_WEIGHTS.iter().any(|(name, _)| *name == group) {
            return group.to_owned();
        }
    }
    DEFAULT_GROUP.to_owned()
}

/// The group weights as a map, in the rubric's own shape.
pub fn weights() -> BTreeMap<String, f64> {
    GROUP_WEIGHTS
        .iter()
        .map(|(group, weight)| ((*group).to_owned(), *weight))
        .collect()
}

/// The SEO scorecard: per-group decay, weighted mean, grade.
pub fn scorecard(findings: &[Finding]) -> Scorecard {
    Rubric::default().scorecard(findings, Category::Seo, &weights(), group_of)
}

/// Run the nineteen base checks, in the source's order, over one document.
///
/// Order is part of the contract: dedupe keeps first-seen order, so a caller that diffs two
/// reports sees the same sequence the old service produced.
pub fn scan(document: &Document) -> Vec<Finding> {
    let html = Html::parse_document(&document.html);
    scan_parsed(&html, &document.url)
}

/// The full audit scan: the base checks **plus** the seven deterministic audit groups.
///
/// The source had two surfaces — `scan` for critique consumers whose scores must not move, and
/// `scan_audit` for the audit endpoint — and so does this crate. The audit groups run in the
/// source's order after the base checks (`readability`, `keyword`, `snippet`, `eeat`,
/// `schema_coverage`, `internal_linking`, `geo`), because dedupe keeps first-seen order.
///
/// The network-dependent checks are deliberately absent: cross-page link analysis
/// (`seo-linking-broken`/`-orphan`/`-hub`) needs a crawl and robots/llms discovery
/// (`seo-geo-ai-crawler-blocked`, `seo-geo-no-llms-txt`) needs a fetch, and fetching belongs to
/// `apps/research`/`apps/media`. The LLM annotations (`seo-keyword-intent`, `seo-eeat-depth`)
/// need a model; they are deferred, not forgotten.
pub fn scan_audit(document: &Document, _as_of: &chrono::DateTime<chrono::Utc>) -> Vec<Finding> {
    let html = Html::parse_document(&document.html);
    let url = document.url.as_str();
    let mut findings = scan_parsed(&html, url);
    findings.extend(readability::check_readability(&html, url));
    findings
}

/// The base checks over an already-parsed document — one parse for both scan surfaces.
fn scan_parsed(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(on_page::check_title(html, url));
    findings.extend(on_page::check_meta_description(html, url));
    findings.extend(social::check_og_tags(html, url));
    findings.extend(on_page::check_heading_hierarchy(html, url));
    findings.extend(structural::check_canonical(html, url));
    findings.extend(on_page::check_image_alt(html, url));
    findings.extend(structural::check_structured_data(html, url));
    findings.extend(social::check_twitter_cards(html, url));
    findings.extend(technical::check_hreflang(html, url));
    findings.extend(structural::check_url_structure(url));
    findings.extend(technical::check_meta_robots(html, url));
    findings.extend(technical::check_favicon(html, url));
    findings.extend(mobile::check_viewport(html, url));
    findings.extend(structural::check_internal_nofollow(html, url));
    findings.extend(on_page::check_keyword_alignment(html, url));
    findings.extend(structural::check_breadcrumb_schema(html, url));
    findings.extend(structural::check_canonical_mismatch(html, url));
    findings.extend(on_page::check_duplicate_meta(html, url));
    findings.extend(mobile::check_mobile_font_sizes(html, url));
    findings
}

// ---------------------------------------------------------------------------
// Shared DOM helpers. `scraper` parses like a browser, so a selector means the same thing here
// as it did in the source's parser for every rule this scan uses.
// ---------------------------------------------------------------------------

pub(crate) fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a static selector is valid")
}

pub(crate) fn first<'a>(html: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    html.select(&selector(css)).next()
}

pub(crate) fn all<'a>(html: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    html.select(&selector(css)).collect()
}

pub(crate) fn attr<'a>(element: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name)
}

/// All descendant text, concatenated. The source used `.string` (a single text child) in a few
/// places and `.get_text()` elsewhere; both answer "what does this element say" for the markup
/// these checks inspect, and the concatenation is the one that cannot silently return empty for
/// a title that wraps its text in a child element.
pub(crate) fn text(element: &ElementRef<'_>) -> String {
    element.text().collect::<String>()
}

pub(crate) fn text_trimmed(element: &ElementRef<'_>) -> String {
    text(element).trim().to_owned()
}

/// The `rel` attribute as its token list. HTML parses `rel` as a space-separated set.
pub(crate) fn rel_tokens(element: &ElementRef<'_>) -> Vec<String> {
    attr(element, "rel")
        .unwrap_or_default()
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect()
}

/// `rel` exactly one token — how the source matched `attrs={"rel": "canonical"}`.
pub(crate) fn rel_is(element: &ElementRef<'_>, token: &str) -> bool {
    rel_tokens(element) == [token.to_lowercase()]
}

/// The page host, lower-cased, with an explicit port when the URL carries one — the source
/// compared `urlparse(...).netloc`.
pub(crate) fn host_of(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or_default().to_lowercase();
            match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            }
        }
        Err(_) => String::new(),
    }
}

pub(crate) fn scheme_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|parsed| parsed.scheme().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weights_are_a_distribution() {
        let total: f64 = GROUP_WEIGHTS.iter().map(|(_, weight)| weight).sum();
        assert!((total - 1.0).abs() < 1e-9, "weights sum to {total}");
    }

    #[test]
    fn legacy_ids_win_over_the_prefix_convention() {
        assert_eq!(group_of("seo-missing-title"), "on_page");
        assert_eq!(group_of("seo-og-tags-missing"), "technical");
        assert_eq!(group_of("seo-structured-data-invalid"), "schema");
    }

    #[test]
    fn the_prefix_convention_maps_new_ids() {
        assert_eq!(group_of("seo-keyword-density"), "keyword");
        assert_eq!(group_of("seo-linking-orphans"), "internal_linking");
        assert_eq!(group_of("seo-eeat-author"), "eeat");
    }

    #[test]
    fn an_unknown_id_lands_in_the_default_group_rather_than_vanishing() {
        assert_eq!(group_of("seo-nonsense-thing"), DEFAULT_GROUP);
        assert_eq!(group_of("other-scanner-id"), DEFAULT_GROUP);
        assert_eq!(group_of(""), DEFAULT_GROUP);
    }
}
