//! The performance scanner — the source's twenty-four HTML checks.
//!
//! Ported from the source service's `domains/critique/services/scanners/perf/` package
//! (`scanner.py` plus `cls`, `dom`, `inp`, `lcp`, `resources`). `PerformanceScanner.scan` calls
//! these checks in this order; a caller sees the findings in that order. Every check is HTML-only
//! — the source never read network or browser metrics here — so the scanner is a pure function of
//! [`PageEvidence::document`]. The source's `_perf_finding` stamps `source_tool`
//! `"performance_scanner"` (not `"perf_scanner"`); that value is preserved.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::app::PageEvidence;
use crate::model::{Category, Finding, Severity};

pub fn scan(evidence: &PageEvidence) -> Vec<Finding> {
    let document = Html::parse_document(&evidence.document.html);
    let html = evidence.document.html.as_str();
    let url = evidence.document.url.as_str();

    let mut findings = Vec::new();
    findings.extend(check_render_blocking(&document, url));
    findings.extend(check_viewport(&document, url));
    findings.extend(check_image_dimensions(&document, url));
    findings.extend(check_lazy_loading(&document, url));
    findings.extend(check_dom_size(&document, url));
    findings.extend(check_css(&document, url));
    findings.extend(check_inline_styles(&document, url));
    findings.extend(check_scripts(&document, url));
    findings.extend(check_page_weight(&document, html, url));
    findings.extend(check_resource_hints(&document, url));
    findings.extend(check_font_loading(&document, url));
    findings.extend(check_web_vitals_hints(&document, url));
    findings.extend(check_critical_css(&document, url));
    findings.extend(check_image_formats(&document, url));
    findings.extend(check_third_party_scripts(&document, url));
    findings.extend(check_css_duplication(&document, url));
    findings.extend(check_render_blocking_css(&document, url));
    findings.extend(check_dom_depth(&document, url));
    findings.extend(check_large_svg(&document, url));
    findings.extend(check_modulepreload(&document, url));
    findings.extend(check_sync_xhr(&document, url));
    findings.extend(check_document_write(&document, url));
    findings.extend(check_css_import(&document, url));
    findings.extend(check_content_visibility(&document, url));
    findings
}

// ---------------------------------------------------------------------------
// Shared helpers (the source's `perf/base._perf_finding` and soup accessors).
// ---------------------------------------------------------------------------

fn finding(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    url: &str,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Performance,
        severity,
        title: title.into(),
        description: description.into(),
        recommendation: String::new(),
        url: url.to_owned(),
        selector: None,
        evidence: None,
        source_tool: Some("performance_scanner".to_owned()),
        scanner_count: 1,
        confirmed_by: Vec::new(),
    }
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn select<'a>(document: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    document.select(&selector(css)).collect()
}

fn attr<'a>(element: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name)
}

fn text(element: &ElementRef<'_>) -> String {
    element.text().collect()
}

fn rel_tokens(element: &ElementRef<'_>) -> Vec<String> {
    attr(element, "rel")
        .unwrap_or_default()
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn has_rel(element: &ElementRef<'_>, token: &str) -> bool {
    rel_tokens(element).iter().any(|value| value == token)
}

fn ancestor<'a>(element: &ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    element.ancestors().find_map(|node| {
        ElementRef::wrap(node).filter(|candidate| candidate.value().name() == name)
    })
}

fn ancestor_named(element: &ElementRef<'_>, name: &str) -> bool {
    ancestor(element, name).is_some()
}

fn in_landmark(image: &ElementRef<'_>) -> bool {
    ancestor_named(image, "header") || ancestor_named(image, "nav")
}

fn lazy_loading_is_pointless(image: &ElementRef<'_>) -> bool {
    let src = attr(image, "src").unwrap_or("").trim().to_ascii_lowercase();
    src.is_empty() || src.starts_with("data:") || url_path(&src).ends_with(".svg")
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default()
}

/// `urlparse(src).path`, including for relative `src` values the `url` crate rejects.
fn url_path(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) => parsed.path().to_owned(),
        Err(_) => {
            let without_fragment = url.split('#').next().unwrap_or("");
            without_fragment.split('?').next().unwrap_or("").to_owned()
        }
    }
}

fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let length = digits.len();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (length - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn max_depth(element: &ElementRef<'_>) -> usize {
    let mut deepest = 0;
    for child in element.children() {
        if let Some(child) = ElementRef::wrap(child) {
            deepest = deepest.max(1 + max_depth(&child));
        }
    }
    deepest
}

fn font_face_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)@font-face\s*\{[^}]*\}").expect("static pattern"))
}

fn font_display_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)font-display\s*:").expect("static pattern"))
}

fn font_display_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)font-display\s*:\s*block").expect("static pattern"))
}

fn sync_xhr_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?s)XMLHttpRequest.*?\.open\s*\([^)]*,\s*[^)]*,\s*false\s*\)")
            .expect("static pattern")
    })
}

fn document_write_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"document\.write\s*\(").expect("static pattern"))
}

fn css_import_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)@import\s").expect("static pattern"))
}

fn css_selector_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?s)([^{}@/]+?)\s*\{").expect("static pattern"))
}

// ---------------------------------------------------------------------------
// LCP checks (`lcp.py`).
// ---------------------------------------------------------------------------

fn check_render_blocking(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(head) = select(document, "head").into_iter().next() else {
        return findings;
    };
    let mut blocking: Vec<String> = Vec::new();
    for script in head.select(&selector("script")) {
        let Some(src) = attr(&script, "src").filter(|src| !src.is_empty()) else {
            continue;
        };
        if attr(&script, "async").is_some() || attr(&script, "defer").is_some() {
            continue;
        }
        if attr(&script, "type")
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
            == Some("module")
        {
            continue;
        }
        blocking.push(src.to_owned());
    }
    if let Some(first) = blocking.first() {
        findings.push(
            finding(
                "perf-render-blocking-scripts",
                Severity::High,
                format!("{} render-blocking scripts in <head>", blocking.len()),
                format!(
                    "{} scripts in <head> block rendering. First: {first}",
                    blocking.len()
                ),
                url,
            )
            .with_recommendation("Add 'async' or 'defer' attribute to scripts in <head>.")
            .with_selector("head > script[src]:not([async]):not([defer])"),
        );
    }
    findings
}

/// Images treated as the initial viewport: everything in the page chrome (header/nav) plus the
/// first few CONTENT images in document order (the hero).
const ABOVE_FOLD_CONTENT_IMAGES: usize = 2;

fn check_lazy_loading(document: &Html, url: &str) -> Vec<Finding> {
    let content_images: Vec<ElementRef<'_>> = select(document, "img")
        .into_iter()
        .filter(|image| !in_landmark(image))
        .collect();
    let below_fold = content_images
        .get(ABOVE_FOLD_CONTENT_IMAGES..)
        .unwrap_or_default();
    let not_lazy: Vec<&ElementRef<'_>> = below_fold
        .iter()
        .filter(|image| {
            let is_lazy =
                attr(image, "loading").is_some_and(|value| value.eq_ignore_ascii_case("lazy"));
            !is_lazy && !lazy_loading_is_pointless(image)
        })
        .collect();

    let mut findings = Vec::new();
    if !not_lazy.is_empty() {
        let count = not_lazy.len();
        let evidence = not_lazy
            .iter()
            .take(5)
            .map(|image| attr(image, "src").unwrap_or(""))
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(
            finding(
                "perf-no-lazy-loading",
                Severity::Medium,
                format!("{count} below-fold images without lazy loading"),
                format!("{count} images below the fold lack loading='lazy'."),
                url,
            )
            .with_recommendation("Add loading='lazy' to images below the initial viewport.")
            .with_selector("img:not([loading])")
            .with_evidence(evidence),
        );
    }
    findings
}

fn check_resource_hints(document: &Html, url: &str) -> Vec<Finding> {
    let mut rels: BTreeSet<String> = BTreeSet::new();
    for link in select(document, "link") {
        for rel in rel_tokens(&link) {
            rels.insert(rel);
        }
    }
    let has_preconnect = rels.contains("preconnect");
    let has_dns_prefetch = rels.contains("dns-prefetch");
    let has_preload = rels.contains("preload");

    let mut findings = Vec::new();
    if !has_preconnect && !has_dns_prefetch {
        findings.push(
            finding(
                "perf-no-preconnect",
                Severity::Low,
                "No preconnect or dns-prefetch resource hints",
                "No <link rel='preconnect'> or <link rel='dns-prefetch'> tags found. Resource hints can speed up connections to third-party domains.",
                url,
            )
            .with_recommendation(
                "Add <link rel='preconnect'> for critical third-party domains (e.g. fonts, CDNs, analytics).",
            )
            .with_selector("head"),
        );
    }
    if !has_preload {
        findings.push(
            finding(
                "perf-no-preload",
                Severity::Low,
                "No preload resource hints",
                "No <link rel='preload'> tags found. Preloading critical resources can improve Largest Contentful Paint (LCP).",
                url,
            )
            .with_recommendation(
                "Add <link rel='preload'> for critical resources such as fonts, hero images, or key CSS/JS files.",
            )
            .with_selector("head"),
        );
    }
    findings
}

fn is_font_css_link(link: &ElementRef<'_>) -> bool {
    let href = attr(link, "href").unwrap_or("");
    let host = host_of(href);
    if host != "fonts.googleapis.com" && host != "fonts.bunny.net" {
        return false;
    }
    if has_rel(link, "stylesheet") {
        return true;
    }
    has_rel(link, "preload")
        && attr(link, "as").is_some_and(|value| value.eq_ignore_ascii_case("style"))
}

fn query_has_display(href: &str) -> bool {
    if let Ok(parsed) = Url::parse(href) {
        return parsed
            .query_pairs()
            .any(|(key, value)| key == "display" && !value.is_empty());
    }
    let Some((_, query)) = href.split_once('?') else {
        return false;
    };
    let query = query.split('#').next().unwrap_or("");
    query.split('&').any(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        key == "display" && !value.is_empty()
    })
}

fn check_font_loading(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    for style_tag in select(document, "style") {
        let css_text = text(&style_tag);
        for block in font_face_regex().find_iter(&css_text) {
            let block = block.as_str();
            if !font_display_regex().is_match(block) {
                findings.push(
                    finding(
                        "perf-font-no-display",
                        Severity::Medium,
                        "@font-face missing font-display property",
                        "A @font-face declaration lacks the font-display property. This can cause invisible text during font loading (FOIT).",
                        url,
                    )
                    .with_recommendation(
                        "Add 'font-display: swap' (or 'fallback'/'optional') to @font-face declarations.",
                    )
                    .with_selector("style"),
                );
            } else if font_display_block_regex().is_match(block) {
                findings.push(
                    finding(
                        "perf-font-display-block",
                        Severity::Low,
                        "@font-face uses font-display: block",
                        "A @font-face declaration uses 'font-display: block', which causes invisible text while the font loads.",
                        url,
                    )
                    .with_recommendation(
                        "Consider using 'font-display: swap' instead to show fallback text while the font loads.",
                    )
                    .with_selector("style"),
                );
            }
        }
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for link in select(document, "link") {
        if !is_font_css_link(&link) {
            continue;
        }
        let href = attr(&link, "href").unwrap_or("").to_owned();
        if !seen.insert(href.clone()) {
            continue;
        }
        if query_has_display(&href) {
            continue;
        }
        findings.push(
            finding(
                "perf-font-link-no-display",
                Severity::Low,
                "Font link missing display=swap parameter",
                format!(
                    "Font link '{href}' does not include the 'display=swap' parameter, which may cause invisible text during loading."
                ),
                url,
            )
            .with_recommendation(
                "Add '&display=swap' to the font URL to ensure text remains visible during font loading.",
            )
            .with_selector("link[href*='fonts']"),
        );
    }
    findings
}

fn hero_image_candidates<'a>(images: &[ElementRef<'a>]) -> Vec<ElementRef<'a>> {
    let mut candidates: Vec<ElementRef<'a>> = Vec::new();
    for image in images {
        let classes = attr(image, "class")
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let image_id = attr(image, "id").unwrap_or("");
        let haystack = format!("{classes} {image_id}").to_lowercase();
        if ["hero", "banner", "main"]
            .iter()
            .any(|keyword| haystack.contains(keyword))
        {
            candidates.push(*image);
        }
    }
    if !candidates.is_empty() {
        return candidates;
    }
    images
        .iter()
        .filter(|image| !in_landmark(image) && !lazy_loading_is_pointless(image))
        .copied()
        .take(1)
        .collect()
}

fn hero_fetchpriority_findings(document: &Html, url: &str) -> Vec<Finding> {
    let images = select(document, "img");
    if images.is_empty() {
        return Vec::new();
    }
    let candidates = hero_image_candidates(&images);
    if candidates.is_empty()
        || candidates
            .iter()
            .any(|image| attr(image, "fetchpriority").is_some_and(|value| !value.is_empty()))
    {
        return Vec::new();
    }
    vec![
        finding(
            "perf-no-fetchpriority",
            Severity::Low,
            "No fetchpriority on hero/primary image",
            "The primary image does not have fetchpriority='high'. Setting this attribute helps the browser prioritize the Largest Contentful Paint (LCP) image.",
            url,
        )
        .with_recommendation("Add fetchpriority='high' to the main hero or above-the-fold image.")
        .with_selector("img"),
    ]
}

fn check_web_vitals_hints(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = hero_fetchpriority_findings(document, url);
    findings.extend(iframe_dimension_findings(document, url));
    findings
}

fn iframe_dimension_findings(document: &Html, url: &str) -> Vec<Finding> {
    let mut count = 0usize;
    for iframe in select(document, "iframe") {
        if attr(&iframe, "width").is_none() || attr(&iframe, "height").is_none() {
            count += 1;
        }
    }
    if count == 0 {
        return Vec::new();
    }
    vec![
        finding(
            "perf-iframe-no-dimensions",
            Severity::Medium,
            format!("{count} iframes without width/height dimensions"),
            format!(
                "{count} iframe elements lack explicit width/height attributes, which causes layout shifts (CLS)."
            ),
            url,
        )
        .with_recommendation(
            "Add explicit width and height attributes to all iframe elements to prevent layout shifts.",
        )
        .with_selector("iframe:not([width]):not([height])"),
    ]
}

fn check_critical_css(document: &Html, url: &str) -> Vec<Finding> {
    let stylesheets = select(document, "link")
        .iter()
        .filter(|link| has_rel(link, "stylesheet"))
        .count();
    let head_styles = select(document, "head")
        .into_iter()
        .next()
        .map(|head| head.select(&selector("style")).count())
        .unwrap_or(0);

    let mut findings = Vec::new();
    if stylesheets > 0 && head_styles == 0 {
        findings.push(
            finding(
                "perf-no-critical-css",
                Severity::Low,
                "No critical CSS inlined in <head>",
                format!(
                    "Found {stylesheets} external stylesheet(s) but no inline <style> in <head>. Inlining critical CSS improves First Contentful Paint (FCP)."
                ),
                url,
            )
            .with_recommendation(
                "Extract and inline the critical CSS needed for above-the-fold content in a <style> tag in <head>.",
            )
            .with_selector("head"),
        );
    }
    findings
}

fn check_image_formats(document: &Html, url: &str) -> Vec<Finding> {
    let mut legacy_images: Vec<String> = Vec::new();
    for image in select(document, "img") {
        let src = attr(&image, "src").unwrap_or("");
        if src.is_empty() || src.starts_with("data:") {
            continue;
        }
        let lower_src = src.to_lowercase();
        if ![".jpg", ".jpeg", ".png", ".gif"]
            .iter()
            .any(|extension| lower_src.ends_with(extension))
        {
            continue;
        }
        if let Some(picture) = ancestor(&image, "picture") {
            let has_modern = picture.select(&selector("source")).any(|source| {
                let kind = attr(&source, "type").unwrap_or("").to_lowercase();
                kind.contains("webp") || kind.contains("avif")
            });
            if has_modern {
                continue;
            }
        }
        legacy_images.push(src.to_owned());
    }

    let mut findings = Vec::new();
    if !legacy_images.is_empty() {
        let count = legacy_images.len();
        findings.push(
            finding(
                "perf-no-modern-images",
                Severity::Low,
                format!("{count} images using legacy formats"),
                format!(
                    "{count} image(s) use JPEG/PNG/GIF without modern format alternatives (WebP/AVIF). Modern formats can reduce file sizes by 25-50%."
                ),
                url,
            )
            .with_recommendation(
                "Use <picture> elements with WebP/AVIF <source> elements as progressive enhancements.",
            )
            .with_selector("img"),
        );
    }
    findings
}

fn check_modulepreload(document: &Html, url: &str) -> Vec<Finding> {
    let has_module_script = select(document, "script")
        .iter()
        .any(|script| attr(script, "type") == Some("module"));
    if !has_module_script {
        return Vec::new();
    }
    let has_modulepreload = select(document, "link")
        .iter()
        .any(|link| has_rel(link, "modulepreload"));
    if has_modulepreload {
        return Vec::new();
    }
    vec![
        finding(
            "perf-no-modulepreload",
            Severity::Low,
            "ES modules loaded without modulepreload",
            "The page uses <script type='module'> but has no <link rel='modulepreload'> hints. Without modulepreload, the browser discovers module dependencies late, causing a waterfall of network requests.",
            url,
        )
        .with_recommendation(
            "Add <link rel='modulepreload' href='...'> for critical module dependencies to enable parallel fetching.",
        )
        .with_selector("link[rel=\"modulepreload\"]"),
    ]
}

// ---------------------------------------------------------------------------
// CLS checks (`cls.py`).
// ---------------------------------------------------------------------------

fn check_viewport(document: &Html, url: &str) -> Vec<Finding> {
    let present = select(document, "meta")
        .iter()
        .any(|meta| attr(meta, "name") == Some("viewport"));
    let mut findings = Vec::new();
    if !present {
        findings.push(
            finding(
                "perf-missing-viewport",
                Severity::High,
                "Missing viewport meta tag",
                "No viewport meta tag found. The page won't render properly on mobile.",
                url,
            )
            .with_recommendation(
                "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">.",
            )
            .with_selector("meta[name=\"viewport\"]"),
        );
    }
    findings
}

fn check_image_dimensions(document: &Html, url: &str) -> Vec<Finding> {
    let mut missing = 0usize;
    for image in select(document, "img") {
        let has_width = attr(&image, "width").is_some();
        let has_height = attr(&image, "height").is_some();
        if !has_width || !has_height {
            let src = attr(&image, "src").unwrap_or("");
            if !src.is_empty() && !src.starts_with("data:") {
                missing += 1;
            }
        }
    }
    let mut findings = Vec::new();
    if missing > 0 {
        findings.push(
            finding(
                "perf-img-no-dimensions",
                Severity::Medium,
                format!("{missing} images without explicit dimensions"),
                format!(
                    "{missing} images lack width/height attributes, causing layout shifts (CLS)."
                ),
                url,
            )
            .with_recommendation("Add explicit width and height attributes to all images.")
            .with_selector("img:not([width]):not([height])"),
        );
    }
    findings
}

fn check_content_visibility(document: &Html, url: &str) -> Vec<Finding> {
    let dom_count = select(document, "*").len();
    if dom_count <= 500 {
        return Vec::new();
    }

    let mut has_content_visibility = select(document, "[style]").iter().any(|element| {
        attr(element, "style")
            .is_some_and(|style| style.to_lowercase().contains("content-visibility"))
    });
    if !has_content_visibility {
        has_content_visibility = select(document, "style").iter().any(|style_tag| {
            text(style_tag)
                .to_lowercase()
                .contains("content-visibility")
        });
    }
    if has_content_visibility {
        return Vec::new();
    }

    vec![
        finding(
            "perf-no-content-visibility",
            Severity::Info,
            format!("Large DOM ({dom_count} nodes) without content-visibility"),
            format!(
                "The page has {dom_count} DOM nodes but does not use the CSS content-visibility property. content-visibility: auto allows the browser to skip rendering off-screen content, significantly improving initial render time on large pages."
            ),
            url,
        )
        .with_recommendation(
            "Apply content-visibility: auto with contain-intrinsic-size to off-screen sections to reduce rendering work.",
        ),
    ]
}

// ---------------------------------------------------------------------------
// DOM checks (`dom.py`).
// ---------------------------------------------------------------------------

fn check_dom_size(document: &Html, url: &str) -> Vec<Finding> {
    let count = select(document, "*").len();
    if count <= 3000 {
        return Vec::new();
    }
    let severity = if count > 5000 {
        Severity::High
    } else {
        Severity::Medium
    };
    vec![
        finding(
            "perf-large-dom",
            severity,
            format!("Large DOM: {count} elements"),
            format!(
                "The DOM contains {count} elements. Large DOMs slow rendering and increase memory usage."
            ),
            url,
        )
        .with_recommendation(
            "Reduce DOM size by removing unnecessary wrappers, using virtualization, or lazy loading content.",
        ),
    ]
}

fn check_dom_depth(document: &Html, url: &str) -> Vec<Finding> {
    let Some(body) = select(document, "body").into_iter().next() else {
        return Vec::new();
    };
    let depth = max_depth(&body);
    if depth > 20 {
        let severity = if depth > 25 {
            Severity::High
        } else {
            Severity::Medium
        };
        vec![
            finding(
                "perf-deep-dom",
                severity,
                format!("Deep DOM tree ({depth} levels)"),
                format!(
                    "The DOM tree is {depth} levels deep. Deep DOMs increase style recalculation time and memory usage."
                ),
                url,
            )
            .with_recommendation(
                "Flatten the DOM structure by removing unnecessary wrapper elements. Aim for a maximum depth of 15.",
            ),
        ]
    } else if depth > 15 {
        vec![
            finding(
                "perf-deep-dom",
                Severity::Low,
                format!("Moderately deep DOM tree ({depth} levels)"),
                format!(
                    "The DOM tree is {depth} levels deep. Consider simplifying the nesting structure."
                ),
                url,
            )
            .with_recommendation("Review nested elements and reduce unnecessary wrapper divs."),
        ]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// INP checks (`inp.py`).
// ---------------------------------------------------------------------------

fn check_scripts(document: &Html, url: &str) -> Vec<Finding> {
    let scripts = select(document, "script");
    let mut external = 0usize;
    let mut total_inline = 0usize;
    for script in &scripts {
        let has_src = attr(script, "src").is_some_and(|src| !src.is_empty());
        if has_src {
            external += 1;
        } else {
            total_inline += text(script).chars().count();
        }
    }

    let mut findings = Vec::new();
    if external > 15 {
        findings.push(
            finding(
                "perf-excessive-scripts",
                Severity::Medium,
                format!("{external} external scripts"),
                format!("Loading {external} external JavaScript files."),
                url,
            )
            .with_recommendation("Bundle scripts to reduce HTTP requests. Consider code splitting.")
            .with_selector("script[src]"),
        );
    }
    if total_inline > 100_000 {
        findings.push(
            finding(
                "perf-large-inline-js",
                Severity::Medium,
                format!("Large inline JavaScript: {}KB", total_inline / 1000),
                format!(
                    "Inline scripts total {} characters.",
                    thousands(total_inline)
                ),
                url,
            )
            .with_recommendation("Move large inline scripts to external files for caching.")
            .with_selector("script:not([src])"),
        );
    }
    findings
}

fn check_third_party_scripts(document: &Html, url: &str) -> Vec<Finding> {
    let page_host = host_of(url);
    let mut third_party = 0usize;
    for script in select(document, "script") {
        let Some(src) = attr(&script, "src") else {
            continue;
        };
        if !src.starts_with("http") {
            continue;
        }
        let src_host = host_of(src);
        if !src_host.is_empty() && src_host != page_host {
            third_party += 1;
        }
    }

    let mut findings = Vec::new();
    if third_party > 10 {
        findings.push(
            finding(
                "perf-excessive-third-party",
                Severity::Medium,
                format!("{third_party} third-party scripts loaded"),
                format!(
                    "{third_party} scripts are loaded from third-party domains. Excessive third-party scripts increase page load time and create single points of failure."
                ),
                url,
            )
            .with_recommendation(
                "Audit third-party scripts and remove unnecessary ones. Consider self-hosting critical libraries and loading non-essential scripts asynchronously.",
            )
            .with_selector("script[src]"),
        );
    }
    findings
}

fn check_sync_xhr(document: &Html, url: &str) -> Vec<Finding> {
    for script in select(document, "script") {
        if attr(&script, "src").is_some_and(|src| !src.is_empty()) {
            continue;
        }
        if sync_xhr_regex().is_match(&text(&script)) {
            return vec![
                finding(
                    "perf-sync-xhr",
                    Severity::Medium,
                    "Synchronous XMLHttpRequest detected",
                    "An inline script uses synchronous XMLHttpRequest (the third argument to open() is false). Synchronous XHR blocks the main thread and freezes the UI until the request completes.",
                    url,
                )
                .with_recommendation(
                    "Use asynchronous XMLHttpRequest (omit the third argument or set it to true), or migrate to the Fetch API.",
                )
                .with_selector("script"),
            ];
        }
    }
    Vec::new()
}

fn check_document_write(document: &Html, url: &str) -> Vec<Finding> {
    for script in select(document, "script") {
        if attr(&script, "src").is_some_and(|src| !src.is_empty()) {
            continue;
        }
        if document_write_regex().is_match(&text(&script)) {
            return vec![
                finding(
                    "perf-document-write",
                    Severity::High,
                    "document.write() detected in inline script",
                    "An inline script uses document.write(). In modern browsers, document.write() can be blocked on slow connections, causes full page re-parsing, and is a significant performance anti-pattern.",
                    url,
                )
                .with_recommendation(
                    "Replace document.write() with DOM manipulation methods such as document.createElement(), appendChild(), or insertAdjacentHTML().",
                )
                .with_selector("script"),
            ];
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// Resource-weight checks (`resources.py`).
// ---------------------------------------------------------------------------

fn check_css(document: &Html, url: &str) -> Vec<Finding> {
    let stylesheets = select(document, "link")
        .iter()
        .filter(|link| has_rel(link, "stylesheet"))
        .count();
    let style_tags = select(document, "style");
    let total_css: usize = style_tags
        .iter()
        .map(|style_tag| text(style_tag).chars().count())
        .sum();

    let mut findings = Vec::new();
    if stylesheets > 8 {
        findings.push(
            finding(
                "perf-excessive-stylesheets",
                Severity::Medium,
                format!("{stylesheets} external stylesheets"),
                format!("Loading {stylesheets} external CSS files increases HTTP requests."),
                url,
            )
            .with_recommendation("Consolidate stylesheets and consider inlining critical CSS.")
            .with_selector("link[rel=\"stylesheet\"]"),
        );
    }
    if total_css > 50_000 {
        findings.push(
            finding(
                "perf-large-inline-css",
                Severity::Medium,
                format!("Large inline CSS: {}KB", total_css / 1000),
                format!(
                    "Inline <style> tags contain {} characters of CSS.",
                    thousands(total_css)
                ),
                url,
            )
            .with_recommendation("Move large CSS blocks to external files for caching.")
            .with_selector("style"),
        );
    }
    findings
}

fn check_inline_styles(document: &Html, url: &str) -> Vec<Finding> {
    let count = select(document, "[style]").len();
    if count <= 20 {
        return Vec::new();
    }
    vec![
        finding(
            "perf-excessive-inline-styles",
            Severity::Low,
            format!("{count} elements with inline styles"),
            format!("{count} elements use inline style attributes."),
            url,
        )
        .with_recommendation("Move inline styles to CSS classes for maintainability and caching.")
        .with_selector("[style]"),
    ]
}

fn check_page_weight(document: &Html, html: &str, url: &str) -> Vec<Finding> {
    let html_bytes = html.len();
    let style_bytes: usize = select(document, "style")
        .iter()
        .map(|style_tag| text(style_tag).len())
        .sum();
    let inline_script_bytes: usize = select(document, "script")
        .iter()
        .filter(|script| !attr(script, "src").is_some_and(|src| !src.is_empty()))
        .map(|script| text(script).len())
        .sum();
    let total_weight = html_bytes + style_bytes + inline_script_bytes;
    let weight_kb = total_weight as f64 / 1024.0;
    let weight_mb = total_weight as f64 / (1024.0 * 1024.0);

    let mut findings = Vec::new();
    if total_weight > 5 * 1024 * 1024 {
        findings.push(
            finding(
                "perf-page-weight-excessive",
                Severity::High,
                "Excessive page weight",
                format!(
                    "Total page weight is {weight_kb:.0}KB ({weight_mb:.1}MB), which exceeds 5MB. This will severely impact load times."
                ),
                url,
            )
            .with_recommendation(
                "Reduce page size by removing unnecessary content, compressing assets, and deferring non-critical resources.",
            ),
        );
    } else if total_weight > 2 * 1024 * 1024 {
        findings.push(
            finding(
                "perf-page-weight-large",
                Severity::Medium,
                "Large page weight",
                format!(
                    "Total page weight is {weight_kb:.0}KB ({weight_mb:.1}MB), which exceeds 2MB."
                ),
                url,
            )
            .with_recommendation(
                "Reduce page size by optimizing inline content and moving large blocks to external cacheable files.",
            ),
        );
    } else if total_weight > 1024 * 1024 {
        findings.push(
            finding(
                "perf-page-weight-moderate",
                Severity::Low,
                "Moderate page weight",
                format!(
                    "Total page weight is {weight_kb:.0}KB ({weight_mb:.1}MB), which exceeds 1MB."
                ),
                url,
            )
            .with_recommendation("Consider reducing page size for faster initial load."),
        );
    }
    findings
}

fn check_css_duplication(document: &Html, url: &str) -> Vec<Finding> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for style_tag in select(document, "style") {
        let css_text = text(&style_tag);
        for captures in css_selector_regex().captures_iter(&css_text) {
            let selector = captures
                .get(1)
                .map(|value| value.as_str().trim())
                .unwrap_or("");
            if selector.is_empty() || selector.starts_with('@') {
                continue;
            }
            match counts.get_mut(selector) {
                Some(count) => *count += 1,
                None => {
                    counts.insert(selector.to_owned(), 1);
                    order.push(selector.to_owned());
                }
            }
        }
    }

    let mut duplicated: Vec<(String, usize)> = order
        .iter()
        .filter_map(|selector| {
            let count = counts.get(selector).copied().unwrap_or(0);
            (count >= 3).then(|| (selector.clone(), count))
        })
        .collect();
    if duplicated.is_empty() {
        return Vec::new();
    }
    duplicated.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let examples = duplicated
        .iter()
        .take(5)
        .map(|(selector, count)| format!("'{selector}' ({count}x)"))
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        finding(
            "perf-css-duplication",
            Severity::Low,
            format!("{} CSS selectors duplicated 3+ times", duplicated.len()),
            format!(
                "{} CSS selector(s) appear 3 or more times in inline styles. Examples: {examples}. Duplicated rules increase CSS size and parse time.",
                duplicated.len()
            ),
            url,
        )
        .with_recommendation(
            "Consolidate duplicate CSS selectors to reduce stylesheet size and improve maintainability.",
        )
        .with_selector("style"),
    ]
}

fn check_render_blocking_css(document: &Html, url: &str) -> Vec<Finding> {
    let Some(head) = select(document, "head").into_iter().next() else {
        return Vec::new();
    };
    let mut blocking_css: Vec<String> = Vec::new();
    for link in head.select(&selector("link")) {
        if !has_rel(&link, "stylesheet") {
            continue;
        }
        if ancestor_named(&link, "noscript") {
            continue;
        }
        let media = attr(&link, "media").unwrap_or("").trim().to_lowercase();
        if !media.is_empty() && media != "all" && media != "screen" {
            continue;
        }
        let href = attr(&link, "href").unwrap_or("");
        if !href.is_empty() {
            blocking_css.push(href.to_owned());
        }
    }

    let mut findings = Vec::new();
    if let Some(first) = blocking_css.first() {
        findings.push(
            finding(
                "perf-render-blocking-css",
                Severity::Medium,
                format!("{} render-blocking stylesheet(s)", blocking_css.len()),
                format!(
                    "{} stylesheet(s) in <head> block rendering. First: {first}",
                    blocking_css.len()
                ),
                url,
            )
            .with_recommendation(
                "Use media queries to make non-critical stylesheets non-render-blocking, or inline critical CSS and load the rest asynchronously.",
            )
            .with_selector("head > link[rel=\"stylesheet\"]"),
        );
    }
    findings
}

fn check_large_svg(document: &Html, url: &str) -> Vec<Finding> {
    let mut large_svgs = 0usize;
    for svg in select(document, "svg") {
        if svg.html().len() > 5 * 1024 {
            large_svgs += 1;
        }
    }
    if large_svgs == 0 {
        return Vec::new();
    }
    vec![
        finding(
            "perf-large-inline-svg",
            Severity::Low,
            format!("{large_svgs} inline SVG(s) exceed 5 KB"),
            format!(
                "{large_svgs} inline <svg> element(s) are larger than 5 KB. Large inline SVGs bloat the HTML document, increase parse time, and cannot be cached independently."
            ),
            url,
        )
        .with_recommendation(
            "Move large SVGs to external files and reference them with <img> tags or CSS background-image. This allows caching and reduces HTML size.",
        )
        .with_selector("svg"),
    ]
}

fn check_css_import(document: &Html, url: &str) -> Vec<Finding> {
    let import_count: usize = select(document, "style")
        .iter()
        .map(|style_tag| css_import_regex().find_iter(&text(style_tag)).count())
        .sum();
    if import_count == 0 {
        return Vec::new();
    }
    vec![
        finding(
            "perf-css-import-chain",
            Severity::Medium,
            format!("{import_count} CSS @import rule(s) in <style> blocks"),
            format!(
                "Found {import_count} @import rule(s) inside inline <style> blocks. CSS @import creates sequential request chains because the browser cannot discover the imported stylesheet until it finishes parsing the parent CSS."
            ),
            url,
        )
        .with_recommendation(
            "Replace @import with <link rel='stylesheet'> tags to enable parallel downloading, or bundle the CSS into a single file.",
        )
        .with_selector("style"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::HttpEvidence;
    use crate::model::Document;

    fn page(html: &str) -> Vec<Finding> {
        page_at("https://example.com/", html)
    }

    fn page_at(url: &str, html: &str) -> Vec<Finding> {
        scan(&PageEvidence::new(
            Document::new(url, html.to_owned()),
            HttpEvidence::default(),
        ))
    }

    fn ids(findings: &[Finding]) -> Vec<String> {
        findings
            .iter()
            .map(|finding| finding.check_id().to_owned())
            .collect()
    }

    fn find<'a>(findings: &'a [Finding], check_id: &str) -> &'a Finding {
        findings
            .iter()
            .find(|finding| finding.check_id() == check_id)
            .unwrap_or_else(|| panic!("missing {check_id}; got {:?}", ids(findings)))
    }

    fn has(findings: &[Finding], check_id: &str) -> bool {
        findings
            .iter()
            .any(|finding| finding.check_id() == check_id)
    }

    #[test]
    fn a_clean_page_produces_no_findings() {
        let findings = page(
            r#"<!doctype html><html><head>
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <link rel="preconnect" href="https://fonts.example">
            <link rel="preload" href="/app.css" as="style">
            <title>Clean</title>
            </head><body><p>Hello</p></body></html>"#,
        );
        assert!(
            findings.is_empty(),
            "unexpected findings: {:?}",
            ids(&findings)
        );
    }

    #[test]
    fn head_scripts_and_stylesheets_are_render_blocking() {
        let findings = page(
            r#"<html><head><script src="/app.js"></script>
            <link rel="stylesheet" href="/app.css"></head><body></body></html>"#,
        );
        for check_id in [
            "perf-render-blocking-scripts",
            "perf-render-blocking-css",
            "perf-no-critical-css",
            "perf-missing-viewport",
            "perf-no-preconnect",
            "perf-no-preload",
        ] {
            assert!(
                has(&findings, check_id),
                "missing {check_id}: {:?}",
                ids(&findings)
            );
        }
        let blocking = find(&findings, "perf-render-blocking-scripts");
        assert_eq!(blocking.severity, Severity::High);
        assert_eq!(blocking.title, "1 render-blocking scripts in <head>");
        assert_eq!(
            blocking.description,
            "1 scripts in <head> block rendering. First: /app.js"
        );
        assert_eq!(
            blocking.selector.as_deref(),
            Some("head > script[src]:not([async]):not([defer])")
        );
        let position = |check_id: &str| {
            findings
                .iter()
                .position(|finding| finding.check_id() == check_id)
                .unwrap()
        };
        assert!(position("perf-render-blocking-scripts") < position("perf-render-blocking-css"));
    }

    #[test]
    fn async_defer_and_module_head_scripts_do_not_block() {
        for markup in [
            r#"<script src="/a.js" async></script>"#,
            r#"<script src="/a.js" defer></script>"#,
            r#"<script type="module" src="/a.js"></script>"#,
        ] {
            let findings = page(&format!("<html><head>{markup}</head><body></body></html>"));
            assert!(
                !has(&findings, "perf-render-blocking-scripts"),
                "false positive on {markup}"
            );
        }
    }

    #[test]
    fn a_missing_viewport_is_high() {
        let findings = page("<html><head></head><body></body></html>");
        let viewport = find(&findings, "perf-missing-viewport");
        assert_eq!(viewport.severity, Severity::High);
        assert_eq!(viewport.title, "Missing viewport meta tag");
        assert_eq!(
            viewport.description,
            "No viewport meta tag found. The page won't render properly on mobile."
        );
        assert_eq!(
            viewport.recommendation,
            "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">."
        );
        assert_eq!(
            viewport.selector.as_deref(),
            Some("meta[name=\"viewport\"]")
        );
        assert_eq!(viewport.source_tool.as_deref(), Some("performance_scanner"));
        assert_eq!(viewport.category, Category::Performance);
    }

    #[test]
    fn images_without_dimensions_and_legacy_sources_are_flagged() {
        let findings = page(r#"<html><body><img src="/photo.png"></body></html>"#);
        let dimensions = find(&findings, "perf-img-no-dimensions");
        assert_eq!(dimensions.severity, Severity::Medium);
        assert_eq!(dimensions.title, "1 images without explicit dimensions");
        assert_eq!(
            dimensions.description,
            "1 images lack width/height attributes, causing layout shifts (CLS)."
        );
        assert_eq!(
            dimensions.selector.as_deref(),
            Some("img:not([width]):not([height])")
        );
        let formats = find(&findings, "perf-no-modern-images");
        assert_eq!(formats.severity, Severity::Low);
        assert_eq!(formats.title, "1 images using legacy formats");
        let hero = find(&findings, "perf-no-fetchpriority");
        assert_eq!(hero.severity, Severity::Low);

        let data_uri = page(r#"<html><body><img src="data:image/png;base64,AAAA"></body></html>"#);
        assert!(!has(&data_uri, "perf-img-no-dimensions"));
        assert!(!has(&data_uri, "perf-no-modern-images"));
        assert!(!has(&data_uri, "perf-no-fetchpriority"));

        let picture = page(
            r#"<html><body><picture><source type="image/webp" srcset="/a.webp"><img src="/a.png" width="1" height="1"></picture></body></html>"#,
        );
        assert!(!has(&picture, "perf-no-modern-images"));
    }

    #[test]
    fn below_fold_images_without_lazy_loading_are_flagged() {
        let findings = page(
            r#"<html><body>
            <img src="/one.png" width="1" height="1">
            <img src="/two.png" width="1" height="1">
            <img src="/three.png" width="1" height="1">
            </body></html>"#,
        );
        let lazy = find(&findings, "perf-no-lazy-loading");
        assert_eq!(lazy.severity, Severity::Medium);
        assert_eq!(lazy.title, "1 below-fold images without lazy loading");
        assert_eq!(
            lazy.description,
            "1 images below the fold lack loading='lazy'."
        );
        assert_eq!(lazy.selector.as_deref(), Some("img:not([loading])"));
        assert_eq!(lazy.evidence.as_deref(), Some("/three.png"));

        let header = page(
            r#"<html><body><header><img src="/logo.png"></header>
            <img src="/a.png" width="1" height="1">
            <img src="/b.png" width="1" height="1">
            <img src="/c.png" width="1" height="1"></body></html>"#,
        );
        assert_eq!(
            find(&header, "perf-no-lazy-loading").evidence.as_deref(),
            Some("/c.png")
        );
    }

    #[test]
    fn large_doms_flag_size_and_missing_content_visibility() {
        let findings = page(&format!(
            "<html><body>{}</body></html>",
            "<div></div>".repeat(3_001)
        ));
        let size = find(&findings, "perf-large-dom");
        assert_eq!(size.severity, Severity::Medium);
        assert_eq!(size.title, "Large DOM: 3004 elements");
        let visibility = find(&findings, "perf-no-content-visibility");
        assert_eq!(visibility.severity, Severity::Info);
        assert_eq!(
            visibility.title,
            "Large DOM (3004 nodes) without content-visibility"
        );

        let huge = page(&format!(
            "<html><body>{}</body></html>",
            "<div></div>".repeat(5_001)
        ));
        assert_eq!(find(&huge, "perf-large-dom").severity, Severity::High);

        let covered = page(&format!(
            r#"<html><head><style>.x{{content-visibility:auto}}</style></head><body>{}</body></html>"#,
            "<div></div>".repeat(501)
        ));
        assert!(!has(&covered, "perf-no-content-visibility"));
    }

    #[test]
    fn excessive_stylesheets_are_flagged() {
        let links: String = (0..9)
            .map(|index| format!(r#"<link rel="stylesheet" href="/s{index}.css">"#))
            .collect();
        let findings = page(&format!("<html><head>{links}</head><body></body></html>"));
        let css = find(&findings, "perf-excessive-stylesheets");
        assert_eq!(css.severity, Severity::Medium);
        assert_eq!(css.title, "9 external stylesheets");
        assert_eq!(
            css.description,
            "Loading 9 external CSS files increases HTTP requests."
        );
        assert_eq!(css.selector.as_deref(), Some("link[rel=\"stylesheet\"]"));

        let eight: String = (0..8)
            .map(|index| format!(r#"<link rel="stylesheet" href="/s{index}.css">"#))
            .collect();
        let findings = page(&format!("<html><head>{eight}</head><body></body></html>"));
        assert!(!has(&findings, "perf-excessive-stylesheets"));
    }

    #[test]
    fn large_inline_css_and_duplicated_selectors_are_flagged() {
        let css = ".a{color:red}".repeat(5_000);
        let findings = page(&format!(
            "<html><head><style>{css}</style></head><body></body></html>"
        ));
        let large = find(&findings, "perf-large-inline-css");
        assert_eq!(large.severity, Severity::Medium);
        assert_eq!(large.title, "Large inline CSS: 65KB");
        assert_eq!(
            large.description,
            "Inline <style> tags contain 65,000 characters of CSS."
        );
        let duplicate = find(&findings, "perf-css-duplication");
        assert_eq!(duplicate.severity, Severity::Low);
        assert_eq!(duplicate.title, "1 CSS selectors duplicated 3+ times");
        assert_eq!(
            duplicate.description,
            "1 CSS selector(s) appear 3 or more times in inline styles. Examples: '.a' (5000x). Duplicated rules increase CSS size and parse time."
        );
        assert_eq!(duplicate.selector.as_deref(), Some("style"));
    }

    #[test]
    fn duplicated_css_selectors_are_counted_per_selector() {
        let findings = page(
            r#"<html><head><style>.btn{color:red}.btn{color:blue}.btn{color:green}</style></head><body></body></html>"#,
        );
        let duplicate = find(&findings, "perf-css-duplication");
        assert_eq!(duplicate.title, "1 CSS selectors duplicated 3+ times");
        assert_eq!(
            duplicate.description,
            "1 CSS selector(s) appear 3 or more times in inline styles. Examples: '.btn' (3x). Duplicated rules increase CSS size and parse time."
        );
    }

    #[test]
    fn excessive_inline_styles_are_flagged() {
        let styled: String = (0..21)
            .map(|index| format!(r#"<div style="color:red" data-i="{index}"></div>"#))
            .collect();
        let findings = page(&format!("<html><body>{styled}</body></html>"));
        let inline = find(&findings, "perf-excessive-inline-styles");
        assert_eq!(inline.severity, Severity::Low);
        assert_eq!(inline.title, "21 elements with inline styles");
        assert_eq!(
            inline.description,
            "21 elements use inline style attributes."
        );
        assert_eq!(inline.selector.as_deref(), Some("[style]"));
    }

    #[test]
    fn excessive_scripts_are_flagged() {
        let scripts: String = (0..16)
            .map(|index| format!(r#"<script src="/s{index}.js"></script>"#))
            .collect();
        let findings = page(&format!("<html><body>{scripts}</body></html>"));
        let excessive = find(&findings, "perf-excessive-scripts");
        assert_eq!(excessive.severity, Severity::Medium);
        assert_eq!(excessive.title, "16 external scripts");
        assert_eq!(
            excessive.description,
            "Loading 16 external JavaScript files."
        );
        assert_eq!(excessive.selector.as_deref(), Some("script[src]"));

        let fifteen: String = (0..15)
            .map(|index| format!(r#"<script src="/s{index}.js"></script>"#))
            .collect();
        let findings = page(&format!("<html><body>{fifteen}</body></html>"));
        assert!(!has(&findings, "perf-excessive-scripts"));
    }

    #[test]
    fn large_inline_javascript_is_flagged() {
        let script = "var x=1;".repeat(15_000);
        let findings = page(&format!(
            "<html><body><script>{script}</script></body></html>"
        ));
        let large = find(&findings, "perf-large-inline-js");
        assert_eq!(large.severity, Severity::Medium);
        assert_eq!(large.title, "Large inline JavaScript: 120KB");
        assert_eq!(
            large.description,
            "Inline scripts total 120,000 characters."
        );
        assert_eq!(large.selector.as_deref(), Some("script:not([src])"));
    }

    #[test]
    fn page_weight_tiers_follow_the_source_thresholds() {
        let tiers = [
            "perf-page-weight-moderate",
            "perf-page-weight-large",
            "perf-page-weight-excessive",
        ];
        for (extra, expected_id, expected_severity, expected_title) in [
            (
                1_100_000usize,
                tiers[0],
                Severity::Low,
                "Moderate page weight",
            ),
            (2_500_000, tiers[1], Severity::Medium, "Large page weight"),
            (5_500_000, tiers[2], Severity::High, "Excessive page weight"),
        ] {
            let html = format!(
                "<html><head><meta name=\"viewport\" content=\"width=device-width\"><link rel=\"preconnect\" href=\"https://a.example\"><link rel=\"preload\" href=\"/x\"></head><body>{}</body></html>",
                "x".repeat(extra)
            );
            let findings = page(&html);
            let weight = find(&findings, expected_id);
            assert_eq!(weight.severity, expected_severity);
            assert_eq!(weight.title, expected_title);
            assert!(
                weight.description.starts_with("Total page weight is "),
                "unexpected description: {}",
                weight.description
            );
            let fired = tiers
                .iter()
                .filter(|check_id| has(&findings, check_id))
                .count();
            assert_eq!(fired, 1, "expected exactly one tier for {extra} bytes");
        }
    }

    #[test]
    fn missing_resource_hints_are_low() {
        let findings = page("<html><head></head><body></body></html>");
        let preconnect = find(&findings, "perf-no-preconnect");
        assert_eq!(preconnect.severity, Severity::Low);
        assert_eq!(
            preconnect.title,
            "No preconnect or dns-prefetch resource hints"
        );
        assert_eq!(
            preconnect.description,
            "No <link rel='preconnect'> or <link rel='dns-prefetch'> tags found. Resource hints can speed up connections to third-party domains."
        );
        assert_eq!(preconnect.selector.as_deref(), Some("head"));
        let preload = find(&findings, "perf-no-preload");
        assert_eq!(preload.severity, Severity::Low);
        assert_eq!(preload.title, "No preload resource hints");
        assert_eq!(
            preload.description,
            "No <link rel='preload'> tags found. Preloading critical resources can improve Largest Contentful Paint (LCP)."
        );

        let satisfied = page(
            r#"<html><head><link rel="dns-prefetch" href="https://cdn.example"><link rel="preload" href="/x"></head><body></body></html>"#,
        );
        assert!(!has(&satisfied, "perf-no-preconnect"));
        assert!(!has(&satisfied, "perf-no-preload"));
    }

    #[test]
    fn font_faces_without_display_are_flagged() {
        let findings = page(
            "<html><head><style>@font-face { font-family: X; src: url(x.woff2); }</style></head><body></body></html>",
        );
        let missing = find(&findings, "perf-font-no-display");
        assert_eq!(missing.severity, Severity::Medium);
        assert_eq!(missing.title, "@font-face missing font-display property");
        assert_eq!(
            missing.description,
            "A @font-face declaration lacks the font-display property. This can cause invisible text during font loading (FOIT)."
        );
        assert_eq!(missing.selector.as_deref(), Some("style"));
    }

    #[test]
    fn font_display_block_is_low() {
        let findings = page(
            "<html><head><style>@font-face { font-family: X; font-display: block; }</style></head><body></body></html>",
        );
        let block = find(&findings, "perf-font-display-block");
        assert_eq!(block.severity, Severity::Low);
        assert_eq!(block.title, "@font-face uses font-display: block");
        assert_eq!(
            block.description,
            "A @font-face declaration uses 'font-display: block', which causes invisible text while the font loads."
        );
        assert_eq!(block.selector.as_deref(), Some("style"));
    }

    #[test]
    fn font_service_links_without_display_swap_are_flagged() {
        let href = "https://fonts.googleapis.com/css2?family=Inter";
        let findings = page(&format!(
            r#"<html><head><link rel="stylesheet" href="{href}"></head><body></body></html>"#
        ));
        let link = find(&findings, "perf-font-link-no-display");
        assert_eq!(link.severity, Severity::Low);
        assert_eq!(link.title, "Font link missing display=swap parameter");
        assert_eq!(
            link.description,
            format!(
                "Font link '{href}' does not include the 'display=swap' parameter, which may cause invisible text during loading."
            )
        );
        assert_eq!(link.selector.as_deref(), Some("link[href*='fonts']"));

        let ok = page(
            r#"<html><head><link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter&display=swap"></head><body></body></html>"#,
        );
        assert!(!has(&ok, "perf-font-link-no-display"));

        let hint = page(
            r#"<html><head><link rel="preconnect" href="https://fonts.googleapis.com"></head><body></body></html>"#,
        );
        assert!(!has(&hint, "perf-font-link-no-display"));
    }

    #[test]
    fn hero_images_without_fetchpriority_are_flagged() {
        let findings = page(
            r#"<html><body><img class="hero" src="/hero.jpg" width="1" height="1"></body></html>"#,
        );
        let hero = find(&findings, "perf-no-fetchpriority");
        assert_eq!(hero.severity, Severity::Low);
        assert_eq!(hero.title, "No fetchpriority on hero/primary image");
        assert_eq!(
            hero.description,
            "The primary image does not have fetchpriority='high'. Setting this attribute helps the browser prioritize the Largest Contentful Paint (LCP) image."
        );
        assert_eq!(hero.selector.as_deref(), Some("img"));

        let prioritized = page(
            r#"<html><body><img class="hero" src="/hero.jpg" width="1" height="1" fetchpriority="high"></body></html>"#,
        );
        assert!(!has(&prioritized, "perf-no-fetchpriority"));

        let header_only =
            page(r#"<html><body><header><img src="/logo.png"></header></body></html>"#);
        assert!(!has(&header_only, "perf-no-fetchpriority"));
    }

    #[test]
    fn iframes_without_dimensions_are_flagged() {
        let findings =
            page(r#"<html><body><iframe src="https://example.org/embed"></iframe></body></html>"#);
        let iframe = find(&findings, "perf-iframe-no-dimensions");
        assert_eq!(iframe.severity, Severity::Medium);
        assert_eq!(iframe.title, "1 iframes without width/height dimensions");
        assert_eq!(
            iframe.description,
            "1 iframe elements lack explicit width/height attributes, which causes layout shifts (CLS)."
        );
        assert_eq!(
            iframe.selector.as_deref(),
            Some("iframe:not([width]):not([height])")
        );

        let sized = page(
            r#"<html><body><iframe src="https://example.org/embed" width="1" height="1"></iframe></body></html>"#,
        );
        assert!(!has(&sized, "perf-iframe-no-dimensions"));
    }

    #[test]
    fn critical_css_is_satisfied_by_an_inline_head_style() {
        let findings = page(
            r#"<html><head><link rel="stylesheet" href="/a.css"><style>.a{color:red}</style></head><body></body></html>"#,
        );
        assert!(!has(&findings, "perf-no-critical-css"));
    }

    #[test]
    fn excessive_third_party_scripts_are_flagged() {
        let scripts: String = (0..11)
            .map(|index| format!(r#"<script src="https://cdn{index}.example.org/x.js"></script>"#))
            .collect();
        let findings = page(&format!("<html><body>{scripts}</body></html>"));
        let third_party = find(&findings, "perf-excessive-third-party");
        assert_eq!(third_party.severity, Severity::Medium);
        assert_eq!(third_party.title, "11 third-party scripts loaded");
        assert_eq!(
            third_party.description,
            "11 scripts are loaded from third-party domains. Excessive third-party scripts increase page load time and create single points of failure."
        );
        assert_eq!(third_party.selector.as_deref(), Some("script[src]"));

        let first_party: String = (0..11)
            .map(|index| format!(r#"<script src="https://example.com/{index}.js"></script>"#))
            .collect();
        let findings = page(&format!("<html><body>{first_party}</body></html>"));
        assert!(!has(&findings, "perf-excessive-third-party"));
    }

    #[test]
    fn deep_dom_trees_are_flagged() {
        let nested = |levels: usize| {
            let mut html = String::from("<html><body>");
            for _ in 0..levels {
                html.push_str("<div>");
            }
            for _ in 0..levels {
                html.push_str("</div>");
            }
            html.push_str("</body></html>");
            html
        };

        let findings = page(&nested(21));
        let deep = find(&findings, "perf-deep-dom");
        assert_eq!(deep.severity, Severity::Medium);
        assert_eq!(deep.title, "Deep DOM tree (21 levels)");
        assert_eq!(
            deep.description,
            "The DOM tree is 21 levels deep. Deep DOMs increase style recalculation time and memory usage."
        );
        assert_eq!(deep.selector, None);

        let findings = page(&nested(26));
        assert_eq!(find(&findings, "perf-deep-dom").severity, Severity::High);

        let findings = page(&nested(16));
        let moderate = find(&findings, "perf-deep-dom");
        assert_eq!(moderate.severity, Severity::Low);
        assert_eq!(moderate.title, "Moderately deep DOM tree (16 levels)");
        assert_eq!(
            moderate.description,
            "The DOM tree is 16 levels deep. Consider simplifying the nesting structure."
        );
    }

    #[test]
    fn large_inline_svgs_are_flagged() {
        let svg = format!("<svg>{}</svg>", "x".repeat(6_000));
        let findings = page(&format!("<html><body>{svg}</body></html>"));
        let large = find(&findings, "perf-large-inline-svg");
        assert_eq!(large.severity, Severity::Low);
        assert_eq!(large.title, "1 inline SVG(s) exceed 5 KB");
        assert_eq!(large.selector.as_deref(), Some("svg"));

        let small = page(r#"<html><body><svg><rect width="1" height="1"/></svg></body></html>"#);
        assert!(!has(&small, "perf-large-inline-svg"));
    }

    #[test]
    fn module_scripts_require_modulepreload() {
        let findings =
            page(r#"<html><body><script type="module" src="/app.js"></script></body></html>"#);
        let module = find(&findings, "perf-no-modulepreload");
        assert_eq!(module.severity, Severity::Low);
        assert_eq!(module.title, "ES modules loaded without modulepreload");
        assert_eq!(
            module.description,
            "The page uses <script type='module'> but has no <link rel='modulepreload'> hints. Without modulepreload, the browser discovers module dependencies late, causing a waterfall of network requests."
        );
        assert_eq!(
            module.selector.as_deref(),
            Some("link[rel=\"modulepreload\"]")
        );

        let preloaded = page(
            r#"<html><head><link rel="modulepreload" href="/app.js"></head><body><script type="module" src="/app.js"></script></body></html>"#,
        );
        assert!(!has(&preloaded, "perf-no-modulepreload"));
    }

    #[test]
    fn synchronous_xhr_and_document_write_are_flagged() {
        let findings = page(
            r#"<html><body><script>var request = new XMLHttpRequest(); request.open("GET", "/api", false); document.write("<p>hi</p>");</script></body></html>"#,
        );
        let xhr = find(&findings, "perf-sync-xhr");
        assert_eq!(xhr.severity, Severity::Medium);
        assert_eq!(xhr.title, "Synchronous XMLHttpRequest detected");
        assert_eq!(
            xhr.description,
            "An inline script uses synchronous XMLHttpRequest (the third argument to open() is false). Synchronous XHR blocks the main thread and freezes the UI until the request completes."
        );
        assert_eq!(xhr.selector.as_deref(), Some("script"));

        let write = find(&findings, "perf-document-write");
        assert_eq!(write.severity, Severity::High);
        assert_eq!(write.title, "document.write() detected in inline script");
        assert_eq!(
            write.description,
            "An inline script uses document.write(). In modern browsers, document.write() can be blocked on slow connections, causes full page re-parsing, and is a significant performance anti-pattern."
        );
        assert_eq!(write.selector.as_deref(), Some("script"));

        let external = page(r#"<html><body><script src="/app.js"></script></body></html>"#);
        assert!(!has(&external, "perf-sync-xhr"));
        assert!(!has(&external, "perf-document-write"));
    }

    #[test]
    fn css_import_rules_are_flagged() {
        let findings = page(
            "<html><head><style>@import url(\"a.css\"); @import url(\"b.css\");</style></head><body></body></html>",
        );
        let imports = find(&findings, "perf-css-import-chain");
        assert_eq!(imports.severity, Severity::Medium);
        assert_eq!(imports.title, "2 CSS @import rule(s) in <style> blocks");
        assert_eq!(
            imports.description,
            "Found 2 @import rule(s) inside inline <style> blocks. CSS @import creates sequential request chains because the browser cannot discover the imported stylesheet until it finishes parsing the parent CSS."
        );
        assert_eq!(imports.selector.as_deref(), Some("style"));
    }

    #[test]
    fn css_import_ignores_external_stylesheets() {
        let findings =
            page(r#"<html><head><link rel="stylesheet" href="/a.css"></head><body></body></html>"#);
        assert!(!has(&findings, "perf-css-import-chain"));
    }

    #[test]
    fn every_ported_check_has_a_finding_shape_of_one_scanner() {
        let findings = page(r#"<html><body><img src="/photo.png"></body></html>"#);
        for finding in &findings {
            assert_eq!(finding.category, Category::Performance);
            assert_eq!(finding.source_tool.as_deref(), Some("performance_scanner"));
            assert_eq!(finding.scanner_count, 1);
            assert!(finding.confirmed_by.is_empty());
            assert_eq!(finding.url, "https://example.com/");
        }
    }
}
