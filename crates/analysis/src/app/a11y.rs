//! The accessibility (WCAG) scanner — the source's twenty-three HTML check groups.
//!
//! Ported from `src/domains/critique/services/scanners/a11y/` (`scanner.py` plus `operable`,
//! `perceivable`, `robust`, `understandable` and the package's `base.py`) at the captured
//! revision. Every check `A11yScanner.scan` called is here, in the source's order, with the
//! source's `check_id`, severity, title, description and recommendation text. Thirty distinct
//! `check_id`s are produced by the twenty-three groups.
//!
//! The source's one deliberate absence is preserved: colour contrast is not checked here. It was
//! commented out of the source's `scan` because it needs computed styles from a real browser and
//! is covered by axe-core; this crate has neither a browser nor a computed-style engine.
//!
//! The source's `make_finding` metadata that has no field on [`Finding`] (`wcag_criterion`,
//! `false_positive_risk`, `remediation_effort`) is not carried — the WCAG reference that matters
//! is already inside the description text the source wrote. `selector` and `evidence` are set
//! exactly where the source set them.
//!
//! Two parser differences are load-bearing and are handled explicitly below: BeautifulSoup's
//! `get_text(strip=True)` trims each text node before concatenating, and `Tag.string` is non-null
//! only for an element with a single text (or comment) child. `scraper` offers neither, so
//! [`text_stripped`] and [`sole_string`] reproduce them.

use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

use crate::app::PageEvidence;
use crate::model::{Category, Finding, Severity};

/// Run every accessibility check over the captured document, in the source's order.
pub fn scan(evidence: &PageEvidence) -> Vec<Finding> {
    let url = evidence.document.url.as_str();
    let html = Html::parse_document(&evidence.document.html);
    let mut findings = Vec::new();
    findings.extend(check_lang(&html, url));
    findings.extend(check_alt_text(&html, url));
    findings.extend(check_form_labels(&html, url));
    findings.extend(check_skip_links(&html, url));
    findings.extend(check_heading_order(&html, url));
    findings.extend(check_aria_roles(&html, url));
    findings.extend(check_tabindex(&html, url));
    findings.extend(check_landmarks(&html, url));
    findings.extend(check_link_text(&html, url));
    // Colour contrast is checked by axe-core (computed styles in a real browser).
    findings.extend(check_video_captions(&html, url));
    findings.extend(check_button_names(&html, url));
    findings.extend(check_table_headers(&html, url));
    findings.extend(check_touch_targets(&html, url));
    findings.extend(check_keyboard_navigation(&html, url));
    findings.extend(check_autocomplete(&html, url));
    findings.extend(check_focus_trap(&html, url));
    findings.extend(check_empty_th(&html, url));
    findings.extend(check_iframe_title(&html, url));
    findings.extend(check_deprecated_motion(&html, url));
    findings.extend(check_lang_validity(&html, url));
    findings.extend(check_svg_accessibility(&html, url));
    findings.extend(check_aria_hidden_focusable(&html, url));
    findings.extend(check_aria_live(&html, url));
    findings
}

// ---------------------------------------------------------------------------
// Understandable: language, form labels, link text, headings, autocomplete.
// ---------------------------------------------------------------------------

/// `check_lang`: `<html>` exists but has no (or an empty) `lang` attribute.
fn check_lang(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(html_tag) = first(html, "html") {
        let lang = attr(&html_tag, "lang").unwrap_or("");
        if lang.is_empty() {
            findings.push(a11y_finding(
                Severity::High,
                "a11y-missing-lang",
                "Missing lang attribute",
                "The <html> element is missing a lang attribute (WCAG 3.1.1).",
                "Add lang=\"en\" (or appropriate language) to the <html> tag.",
                Some("html"),
                None,
                url,
            ));
        }
    }
    findings
}

/// `check_lang_validity`: a present `lang` that is not a plausible BCP 47 tag.
fn check_lang_validity(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(html_tag) = first(html, "html") else {
        return findings;
    };
    let lang = attr(&html_tag, "lang").unwrap_or("").trim().to_owned();
    if lang.is_empty() {
        // Missing lang is caught by check_lang.
        return findings;
    }
    if !invalid_lang_re().is_match(&lang) {
        let evidence = format!("lang=\"{lang}\"");
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-lang-invalid",
            format!("Invalid lang attribute value: '{lang}'"),
            format!(
                "The <html> lang attribute value '{lang}' does not match a valid BCP 47 language tag pattern. An invalid language code prevents assistive technologies from selecting the correct speech synthesizer (WCAG 3.1.1)."
            ),
            "Set the lang attribute to a valid ISO 639-1 language code (e.g. 'en', 'fr', 'de', 'en-US').",
            Some("html"),
            Some(&evidence),
            url,
        ));
    }
    findings
}

/// `check_form_labels`: one aggregate finding for every form control with no accessible name.
fn check_form_labels(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut unlabeled = 0usize;
    for input in all(html, "input, select, textarea") {
        let input_type = attr(&input, "type").unwrap_or("text");
        if matches!(
            input_type,
            "hidden" | "submit" | "button" | "reset" | "image"
        ) {
            continue;
        }
        let input_id = attr(&input, "id").unwrap_or("");
        let has_label = !input_id.is_empty() && has_label_for(html, input_id);
        let has_aria = truthy(&input, "aria-label") || truthy(&input, "aria-labelledby");
        let has_title = truthy(&input, "title");
        if !has_label && !has_aria && !has_title {
            let parent_is_label = input
                .parent()
                .and_then(ElementRef::wrap)
                .is_some_and(|parent| parent.value().name() == "label");
            if !parent_is_label {
                unlabeled += 1;
            }
        }
    }
    if unlabeled > 0 {
        findings.push(a11y_finding(
            Severity::High,
            "a11y-form-no-label",
            format!("{unlabeled} form inputs without labels"),
            format!("{unlabeled} form inputs lack associated labels (WCAG 1.3.1, 4.1.2)."),
            "Associate each input with a <label> element using for/id attributes.",
            Some("input:not([aria-label]):not([title])"),
            None,
            url,
        ));
    }
    findings
}

/// `check_link_text`: generic anchor text ("click here", "here", …).
fn check_link_text(html: &Html, url: &str) -> Vec<Finding> {
    const BAD_TEXTS: [&str; 6] = ["click here", "here", "read more", "more", "link", "this"];
    let mut findings = Vec::new();
    let mut bad_links: Vec<String> = Vec::new();
    for link in all(html, "a") {
        let text = text_stripped(&link).to_lowercase();
        if BAD_TEXTS.contains(&text.as_str()) && !truthy(&link, "aria-label") {
            bad_links.push(text);
        }
    }
    if !bad_links.is_empty() {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-generic-link-text",
            format!("{} links with non-descriptive text", bad_links.len()),
            format!(
                "Links use generic text like '{}' (WCAG 2.4.4).",
                bad_links[0]
            ),
            "Use descriptive link text that explains the destination or purpose.",
            Some("a"),
            None,
            url,
        ));
    }
    findings
}

/// `check_heading_order`: the first heading must be an H1 and levels must not skip.
fn check_heading_order(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let headings = all(html, "h1, h2, h3, h4, h5, h6");
    if headings.is_empty() {
        return findings;
    }
    let levels: Vec<u32> = headings
        .iter()
        .map(|heading| {
            heading.value().name()[1..]
                .parse::<u32>()
                .expect("the selector only matches h1 through h6")
        })
        .collect();
    if levels[0] != 1 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-heading-not-h1",
            format!("First heading is H{}, not H1", levels[0]),
            "Page content should begin with an H1 heading (WCAG 1.3.1).",
            "Start the page content with an H1 heading.",
            None,
            None,
            url,
        ));
    }
    for pair in levels.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        if current > previous + 1 {
            findings.push(a11y_finding(
                Severity::Medium,
                "a11y-heading-skip",
                format!("Heading level skip: H{previous} to H{current}"),
                format!("Heading hierarchy skips from H{previous} to H{current} (WCAG 1.3.1)."),
                "Use sequential heading levels without skipping.",
                None,
                None,
                url,
            ));
            break;
        }
    }
    findings
}

/// `check_autocomplete`: identity-looking inputs with no `autocomplete` attribute.
fn check_autocomplete(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut missing_autocomplete = 0usize;
    for input in all(html, "input") {
        let input_type = attr(&input, "type").unwrap_or("text").to_lowercase();
        if !matches!(
            input_type.as_str(),
            "text" | "email" | "tel" | "password" | "url"
        ) {
            continue;
        }
        if truthy(&input, "autocomplete") {
            continue;
        }
        let name = attr(&input, "name").unwrap_or("");
        let input_id = attr(&input, "id").unwrap_or("");
        if identity_field_re().is_match(name) || identity_field_re().is_match(input_id) {
            missing_autocomplete += 1;
        }
    }
    if missing_autocomplete > 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-autocomplete-missing",
            format!("{missing_autocomplete} identity inputs missing autocomplete"),
            format!(
                "{missing_autocomplete} input field(s) that appear to collect personal information lack the autocomplete attribute (WCAG 1.3.5). Autocomplete helps users fill forms faster and assists assistive technologies."
            ),
            "Add appropriate autocomplete values (e.g. autocomplete=\"email\", autocomplete=\"name\") to identity-related input fields.",
            Some("input"),
            None,
            url,
        ));
    }
    findings
}

// ---------------------------------------------------------------------------
// Perceivable: alt text, video captions, table headers, motion, SVG.
// ---------------------------------------------------------------------------

/// `check_alt_text`: images with no `alt` at all, and non-decorative images with empty alt.
fn check_alt_text(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut missing_alt = 0usize;
    let mut empty_alt = 0usize;
    for img in all(html, "img") {
        match attr(&img, "alt") {
            None => missing_alt += 1,
            Some(alt) => {
                if alt.trim().is_empty() {
                    let role = attr(&img, "role").unwrap_or("");
                    if role != "presentation" {
                        let src = attr(&img, "src").unwrap_or("");
                        if !src.is_empty() && !src.starts_with("data:") {
                            empty_alt += 1;
                        }
                    }
                }
            }
        }
    }
    if missing_alt > 0 {
        findings.push(a11y_finding(
            Severity::High,
            "a11y-img-missing-alt",
            format!("{missing_alt} images missing alt attribute"),
            format!("{missing_alt} images have no alt attribute at all (WCAG 1.1.1)."),
            "Add descriptive alt text to all images, or alt='' for decorative images.",
            Some("img:not([alt])"),
            None,
            url,
        ));
    }
    if empty_alt > 0 {
        findings.push(a11y_finding(
            Severity::Low,
            "a11y-img-empty-alt",
            format!("{empty_alt} images with empty alt text"),
            format!("{empty_alt} non-decorative images have empty alt text."),
            "Add descriptive alt text or mark as decorative with role='presentation'.",
            Some("img[alt=\"\"]"),
            None,
            url,
        ));
    }
    findings
}

/// `check_video_captions`: every `<video>` needs a captions/subtitles track, and it needs
/// `srclang`.
fn check_video_captions(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for video in all(html, "video") {
        let caption_tracks: Vec<ElementRef<'_>> = select_all(&video, "track")
            .into_iter()
            .filter(|track| matches!(attr(track, "kind"), Some("captions" | "subtitles")))
            .collect();
        if caption_tracks.is_empty() {
            let src = attr(&video, "src").unwrap_or("");
            let evidence = if src.is_empty() {
                None
            } else {
                Some(src.chars().take(100).collect::<String>())
            };
            findings.push(a11y_finding(
                Severity::High,
                "a11y-video-no-captions",
                "Video without captions",
                "A <video> element has no <track kind=\"captions\"> or <track kind=\"subtitles\"> child (WCAG 1.2.2).",
                "Add a <track kind=\"captions\" src=\"captions.vtt\" srclang=\"en\"> element to the video.",
                Some("video"),
                evidence.as_deref(),
                url,
            ));
        } else {
            for track in caption_tracks {
                if !truthy(&track, "srclang") {
                    findings.push(a11y_finding(
                        Severity::Medium,
                        "a11y-video-caption-no-lang",
                        "Video caption track missing srclang",
                        "A caption/subtitle track is missing the srclang attribute (WCAG 1.2.2).",
                        "Add srclang=\"en\" (or appropriate language code) to the <track> element.",
                        Some("track"),
                        None,
                        url,
                    ));
                }
            }
        }
    }
    findings
}

/// `check_table_headers`: data tables need header cells, scoped headers, and a caption.
fn check_table_headers(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for table in all(html, "table") {
        // Presentation tables are not data tables.
        if attr(&table, "role") == Some("presentation") {
            continue;
        }
        if select_all(&table, "tr").len() <= 1 {
            continue;
        }
        let header_cells = select_all(&table, "th");
        if header_cells.is_empty() {
            findings.push(a11y_finding(
                Severity::High,
                "a11y-table-no-headers",
                "Data table without header cells",
                "A data table has no <th> elements to identify column or row headers (WCAG 1.3.1).",
                "Add <th> elements to identify column and/or row headers.",
                Some("table"),
                None,
                url,
            ));
        } else {
            let without_scope = header_cells
                .iter()
                .filter(|cell| !truthy(cell, "scope"))
                .count();
            if without_scope > 0 {
                findings.push(a11y_finding(
                    Severity::Medium,
                    "a11y-table-header-no-scope",
                    "Table header cells missing scope attribute",
                    format!("{without_scope} <th> elements lack a scope attribute (WCAG 1.3.1)."),
                    "Add scope=\"col\" or scope=\"row\" to <th> elements to clarify header relationships.",
                    Some("th:not([scope])"),
                    None,
                    url,
                ));
            }
        }
        let has_caption = !select_all(&table, "caption").is_empty();
        let has_aria_label = truthy(&table, "aria-label") || truthy(&table, "aria-labelledby");
        if !has_caption && !has_aria_label {
            findings.push(a11y_finding(
                Severity::Low,
                "a11y-table-no-caption",
                "Data table without caption",
                "A data table has no <caption> element or aria-label to describe its purpose (WCAG 1.3.1).",
                "Add a <caption> element or aria-label to describe the table's content.",
                Some("table"),
                None,
                url,
            ));
        }
    }
    findings
}

/// `check_deprecated_motion`: `<marquee>` and `<blink>`.
fn check_deprecated_motion(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let marquees = all(html, "marquee").len();
    let blinks = all(html, "blink").len();
    if marquees + blinks > 0 {
        let mut elements = Vec::new();
        if marquees > 0 {
            elements.push(format!("{marquees} <marquee>"));
        }
        if blinks > 0 {
            elements.push(format!("{blinks} <blink>"));
        }
        let elements = elements.join(", ");
        findings.push(a11y_finding(
            Severity::High,
            "a11y-deprecated-motion",
            "Deprecated motion element(s) found",
            format!(
                "Found {elements} element(s). These deprecated elements create uncontrollable motion that can cause seizures or vestibular disorders (WCAG 2.3.1)."
            ),
            "Remove <marquee> and <blink> elements entirely. If animation is needed, use CSS with prefers-reduced-motion support.",
            Some("marquee, blink"),
            None,
            url,
        ));
    }
    findings
}

/// `check_svg_accessibility`: meaningful SVGs need a `<title>`, `aria-label` or
/// `aria-labelledby`; `role="presentation"`/`"none"` SVGs are exempt.
fn check_svg_accessibility(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut inaccessible_svgs = 0usize;
    for svg in all(html, "svg") {
        let role = attr(&svg, "role").unwrap_or("");
        if role == "presentation" || role == "none" {
            continue; // Decorative SVG, no label needed.
        }
        let has_title_child = !select_all(&svg, "title").is_empty();
        let has_aria_label = truthy(&svg, "aria-label");
        let has_aria_labelledby = truthy(&svg, "aria-labelledby");
        if !has_title_child && !has_aria_label && !has_aria_labelledby {
            inaccessible_svgs += 1;
        }
    }
    if inaccessible_svgs > 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-svg-no-title",
            format!("{inaccessible_svgs} SVG(s) without accessible name"),
            format!(
                "{inaccessible_svgs} <svg> element(s) lack a <title> child, aria-label, or aria-labelledby attribute. Screen readers cannot describe these images to users (WCAG 1.1.1)."
            ),
            "Add a <title> child element inside each meaningful <svg>, or use aria-label / aria-labelledby. For decorative SVGs, add role='presentation'.",
            Some("svg"),
            None,
            url,
        ));
    }
    findings
}

// ---------------------------------------------------------------------------
// Operable: skip links, tabindex, touch targets, keyboard navigation, focus traps.
// ---------------------------------------------------------------------------

/// `check_skip_links`: the first five links must include a skip/jump-to-content affordance.
fn check_skip_links(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let has_skip = all(html, "a").into_iter().take(5).any(|link| {
        let href = attr(&link, "href").unwrap_or("");
        skip_link_re().is_match(href) || skip_link_re().is_match(&text_concat(&link))
    });
    if !has_skip {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-no-skip-link",
            "No skip navigation link",
            "No skip-to-content link found (WCAG 2.4.1).",
            "Add a skip link as the first focusable element on the page.",
            None,
            None,
            url,
        ));
    }
    findings
}

/// `check_tabindex`: any element with a positive `tabindex`.
fn check_tabindex(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let bad_tabs = all(html, "[tabindex]")
        .into_iter()
        .filter(|element| {
            attr(element, "tabindex")
                .and_then(|value| value.trim().parse::<i64>().ok())
                .is_some_and(|value| value > 0)
        })
        .count();
    if bad_tabs > 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-positive-tabindex",
            format!("{bad_tabs} elements with positive tabindex"),
            "Positive tabindex values create confusing tab order (WCAG 2.4.3).",
            "Use tabindex=\"0\" for focusable elements or tabindex=\"-1\" for programmatic focus.",
            Some("[tabindex]"),
            None,
            url,
        ));
    }
    findings
}

/// `check_touch_targets`: a clickable element whose inline `width`/`height` is under 44px.
fn check_touch_targets(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut clickable: Vec<ElementRef<'_>> = all(html, "a");
    clickable.extend(all(html, "button"));
    clickable.extend(
        all(html, "input")
            .into_iter()
            .filter(|element| attr(element, "type") == Some("submit")),
    );
    clickable.extend(
        all(html, "input")
            .into_iter()
            .filter(|element| attr(element, "type") == Some("button")),
    );
    for element in clickable {
        let style = attr(&element, "style").unwrap_or("");
        if style.is_empty() || !has_small_inline_dimension(style) {
            continue;
        }
        let name = element.value().name();
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-small-touch-target",
            "Small touch target",
            format!(
                "A clickable <{name}> element has inline dimensions smaller than 44px, which may be difficult for touch users (WCAG 2.5.8). Note: this is a heuristic check based on inline styles only."
            ),
            "Ensure touch targets are at least 44x44 CSS pixels.",
            Some(name),
            None,
            url,
        ));
    }
    findings
}

/// `check_keyboard_navigation`: removed focus outlines (inline and embedded CSS) plus
/// clickable anchors with no keyboard handler.
fn check_keyboard_navigation(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = outline_removed_inline(html, url);
    findings.extend(outline_removed_in_styles(html, url));
    findings.extend(missing_keyboard_handlers(html, url));
    findings
}

fn outline_removed_inline(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for element in all(html, "[style]") {
        let style = attr(&element, "style").unwrap_or("");
        let Some(outline) = extract_style_value(style, "outline") else {
            continue;
        };
        if !outline.is_empty() && outline_removed_re().is_match(&outline) {
            let name = element.value().name();
            let selector = format!("{name}[style]");
            findings.push(a11y_finding(
                Severity::High,
                "a11y-outline-removed",
                "Focus outline removed via inline style",
                format!(
                    "A <{name}> element has outline:none or outline:0 in its inline style, which removes the keyboard focus indicator (WCAG 2.4.7)."
                ),
                "Do not remove focus outlines. Provide a visible focus indicator, either the default or a custom one.",
                Some(&selector),
                None,
                url,
            ));
        }
    }
    findings
}

fn outline_removed_in_styles(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for style_tag in all(html, "style") {
        if focus_outline_rule_re().is_match(&text_concat(&style_tag)) {
            findings.push(a11y_finding(
                Severity::High,
                "a11y-outline-removed",
                "Focus outline removed in embedded CSS",
                "A :focus rule in a <style> block sets outline to none or 0, removing the keyboard focus indicator (WCAG 2.4.7).",
                "Do not remove focus outlines. Provide a visible focus indicator, either the default or a custom one.",
                None,
                None,
                url,
            ));
        }
    }
    findings
}

fn missing_keyboard_handlers(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for element in all(html, "a, button, input") {
        if !truthy(&element, "onclick") || truthy(&element, "tabindex") {
            continue;
        }
        let has_keyboard = truthy(&element, "onkeydown")
            || truthy(&element, "onkeyup")
            || truthy(&element, "onkeypress");
        if has_keyboard {
            continue;
        }
        // Only flag an <a> without href: native interactives need no extra handlers.
        let name = element.value().name();
        if name == "a" && !truthy(&element, "href") {
            let selector = format!("{name}[onclick]");
            findings.push(a11y_finding(
                Severity::Medium,
                "a11y-no-keyboard-handler",
                "Interactive element lacks keyboard handler",
                format!(
                    "A <{name}> element has an onclick handler but no keyboard event handler and no href (WCAG 2.1.1)."
                ),
                "Add onkeydown/onkeypress handlers or use a native interactive element.",
                Some(&selector),
                None,
                url,
            ));
        }
    }
    findings
}

/// `check_focus_trap`: dialogs with no escape affordance, and `tabindex="-1"` elements that
/// also listen for keys. Both cases share the `a11y-potential-focus-trap` id in the source.
fn check_focus_trap(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for dialog in all(html, "[role=\"dialog\"]") {
        // `Tag.string` — a button counts only when its whole content is one text node.
        let close_button = select_all(&dialog, "button").into_iter().find(|button| {
            sole_string(button).is_some_and(|text| close_button_text_re().is_match(&text))
        });
        // `dialog.find(...)` searches descendants only, never the dialog itself.
        let has_close = close_button.is_some()
            || select_all(&dialog, "[aria-label]")
                .into_iter()
                .any(|element| {
                    attr(&element, "aria-label")
                        .is_some_and(|label| close_or_dismiss_re().is_match(label))
                });
        let has_aria_modal = attr(&dialog, "aria-modal") == Some("true");
        if !has_close && !has_aria_modal {
            findings.push(a11y_finding(
                Severity::High,
                "a11y-potential-focus-trap",
                "Potential focus trap in dialog",
                "A dialog element (role='dialog') lacks both a visible close button and aria-modal='true'. Users may become trapped in this dialog without a way to escape (WCAG 2.1.2).",
                "Add a visible close button and/or aria-modal='true' to dialog elements. Ensure Escape key closes the dialog.",
                Some("[role=\"dialog\"]"),
                None,
                url,
            ));
        }
    }
    for element in all(html, "[tabindex=\"-1\"]") {
        let has_key_handler = truthy(&element, "onkeydown") || truthy(&element, "onkeypress");
        if has_key_handler {
            let name = element.value().name();
            let selector = format!("{name}[tabindex=\"-1\"]");
            findings.push(a11y_finding(
                Severity::Medium,
                "a11y-potential-focus-trap",
                "Potential focus trap pattern detected",
                format!(
                    "A <{name}> element has tabindex='-1' combined with a keyboard event handler, which may indicate a focus trap (WCAG 2.1.2). This is a heuristic check."
                ),
                "Ensure users can navigate away from this element using standard keyboard navigation (Tab, Escape).",
                Some(&selector),
                None,
                url,
            ));
            break; // Only report once, as the source did.
        }
    }
    findings
}

// ---------------------------------------------------------------------------
// Robust: ARIA roles, landmarks, button names, empty headers, iframes, aria-hidden, aria-live.
// ---------------------------------------------------------------------------

/// `check_aria_roles`: a clickable `<div>`/`<span>` with neither `role` nor `tabindex`.
fn check_aria_roles(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for element in all(html, "div[onclick], span[onclick]") {
        if truthy(&element, "role") || truthy(&element, "tabindex") {
            continue;
        }
        let name = element.value().name();
        let selector = format!("{name}[onclick]");
        findings.push(a11y_finding(
            Severity::High,
            "a11y-clickable-no-role",
            format!("Clickable <{name}> without ARIA role"),
            format!("A <{name}> with onclick handler lacks role and tabindex (WCAG 4.1.2)."),
            format!("Add role=\"button\" and tabindex=\"0\" to clickable <{name}> elements."),
            Some(&selector),
            None,
            url,
        ));
        break; // Only the first offender is reported.
    }
    findings
}

/// `check_landmarks`: any landmark region at all, and specifically a main landmark.
fn check_landmarks(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let landmarks = all(html, "main, nav, header, footer, aside");
    let aria_landmarks = all(html, "[role]")
        .into_iter()
        .filter(|element| {
            attr(element, "role").is_some_and(|role| landmark_role_re().is_match(role))
        })
        .count();
    if landmarks.is_empty() && aria_landmarks == 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-no-landmarks",
            "No landmark regions found",
            "No semantic landmarks (<main>, <nav>, etc.) or ARIA landmarks found (WCAG 1.3.1).",
            "Use semantic HTML5 elements: <main>, <nav>, <header>, <footer>.",
            None,
            None,
            url,
        ));
    }
    let has_main = !all(html, "main").is_empty()
        || all(html, "[role]")
            .into_iter()
            .any(|element| attr(&element, "role") == Some("main"));
    if !has_main {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-no-main",
            "No main landmark",
            "No <main> element or role='main' found (WCAG 1.3.1).",
            "Wrap the primary content in a <main> element.",
            Some("main"),
            None,
            url,
        ));
    }
    findings
}

/// `check_button_names`: every `<button>` needs text, `aria-label`, `aria-labelledby` or
/// `title`.
fn check_button_names(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for button in all(html, "button") {
        let text_content = text_stripped(&button);
        let has_aria_label = truthy(&button, "aria-label");
        let has_aria_labelledby = truthy(&button, "aria-labelledby");
        let has_title = truthy(&button, "title");
        if text_content.is_empty() && !has_aria_label && !has_aria_labelledby && !has_title {
            findings.push(a11y_finding(
                Severity::High,
                "a11y-button-no-name",
                "Button without accessible name",
                "A <button> element has no text content, aria-label, aria-labelledby, or title attribute (WCAG 4.1.2).",
                "Add text content, aria-label, aria-labelledby, or title to the button for screen reader accessibility.",
                Some("button"),
                None,
                url,
            ));
        }
    }
    findings
}

/// `check_empty_th`: header cells with no text content.
fn check_empty_th(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let empty_ths = all(html, "th")
        .into_iter()
        .filter(|cell| text_stripped(cell).is_empty())
        .count();
    if empty_ths > 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-empty-th",
            format!("{empty_ths} empty table header cell(s)"),
            format!(
                "{empty_ths} <th> element(s) contain no text. Empty header cells provide no information to screen readers, making table data harder to understand (WCAG 1.3.1)."
            ),
            "Add descriptive text to all <th> elements, or convert empty headers to <td> if they are not actual headers.",
            Some("th:empty"),
            None,
            url,
        ));
    }
    findings
}

/// `check_iframe_title`: every `<iframe>` needs a non-empty `title`.
fn check_iframe_title(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let untitled = all(html, "iframe")
        .into_iter()
        .filter(|iframe| !truthy(iframe, "title"))
        .count();
    if untitled > 0 {
        findings.push(a11y_finding(
            Severity::Medium,
            "a11y-iframe-no-title",
            format!("{untitled} iframe(s) without title attribute"),
            format!(
                "{untitled} <iframe> element(s) lack a title attribute. Screen reader users need the title to understand the purpose of each iframe (WCAG 2.4.1)."
            ),
            "Add a descriptive title attribute to each <iframe> that explains its content or purpose.",
            Some("iframe:not([title])"),
            None,
            url,
        ));
    }
    findings
}

/// `check_aria_hidden_focusable`: `aria-hidden="true"` regions that still contain focusable
/// descendants.
fn check_aria_hidden_focusable(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let violations = all(html, "[aria-hidden=\"true\"]")
        .into_iter()
        .filter(|element| has_focusable_child(element))
        .count();
    if violations > 0 {
        findings.push(a11y_finding(
            Severity::High,
            "a11y-aria-hidden-focusable",
            format!("{violations} aria-hidden region(s) with focusable children"),
            format!(
                "{violations} element(s) with aria-hidden='true' contain focusable children (links, buttons, inputs, or elements with tabindex). This traps focus inside hidden content, confusing keyboard and screen reader users (WCAG 4.1.2)."
            ),
            "Remove focusable elements from aria-hidden regions, or remove the aria-hidden attribute. Use tabindex='-1' on children that must remain hidden.",
            Some("[aria-hidden=\"true\"]"),
            None,
            url,
        ));
    }
    findings
}

/// `check_aria_live`: no live region and no implicit live role anywhere on the page.
fn check_aria_live(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let live_regions = all(html, "[aria-live]");
    let implicit_live_roles = all(html, "[role]")
        .into_iter()
        .filter(|element| {
            attr(element, "role").is_some_and(|role| implicit_live_role_re().is_match(role))
        })
        .count();
    if live_regions.is_empty() && implicit_live_roles == 0 {
        findings.push(a11y_finding(
            Severity::Low,
            "a11y-no-aria-live",
            "No aria-live regions found",
            "The page contains no aria-live regions or elements with implicit live roles (alert, status, log). Live regions announce dynamic content changes to screen reader users (WCAG 4.1.3).",
            "Add aria-live='polite' or aria-live='assertive' regions for dynamic content that updates without page reload (e.g. notifications, form validation messages, chat).",
            None,
            None,
            url,
        ));
    }
    findings
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

/// The source's `_a11y_finding`: accessibility category, scanner name and defaults applied to
/// every finding this module emits.
#[allow(clippy::too_many_arguments)]
fn a11y_finding(
    severity: Severity,
    check_id: &str,
    title: impl Into<String>,
    description: impl Into<String>,
    recommendation: impl Into<String>,
    selector: Option<&str>,
    evidence: Option<&str>,
    url: &str,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Accessibility,
        severity,
        title: title.into(),
        description: description.into(),
        recommendation: recommendation.into(),
        url: url.to_owned(),
        selector: selector.map(str::to_owned),
        evidence: evidence.map(str::to_owned),
        source_tool: Some("a11y_scanner".to_owned()),
        scanner_count: 1,
        confirmed_by: Vec::new(),
    }
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a static selector is valid")
}

fn all<'a>(html: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    html.select(&selector(css)).collect()
}

fn first<'a>(html: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    html.select(&selector(css)).next()
}

/// Descendant elements matching `css`, scoped to `scope` (self excluded, like `find_all`).
fn select_all<'a>(scope: &ElementRef<'a>, css: &str) -> Vec<ElementRef<'a>> {
    scope.select(&selector(css)).collect()
}

fn attr<'a>(element: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name)
}

/// BeautifulSoup truthiness for an attribute: absent and `""` are both falsy.
fn truthy(element: &ElementRef<'_>, name: &str) -> bool {
    attr(element, name).is_some_and(|value| !value.is_empty())
}

/// `get_text()` — all descendant text concatenated without a separator.
fn text_concat(element: &ElementRef<'_>) -> String {
    element.text().collect()
}

/// `get_text(strip=True)` — each text node trimmed, then concatenated.
fn text_stripped(element: &ElementRef<'_>) -> String {
    element.text().map(str::trim).collect()
}

/// `Tag.string` — `Some` only when the element has exactly one text or comment child.
fn sole_string(element: &ElementRef<'_>) -> Option<String> {
    let mut children = element.children();
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    match child.value() {
        Node::Text(text) => Some(text.to_string()),
        Node::Comment(comment) => Some(comment.to_string()),
        _ => None,
    }
}

fn has_label_for(html: &Html, id: &str) -> bool {
    all(html, "label")
        .into_iter()
        .any(|label| attr(&label, "for") == Some(id))
}

/// The source's `_has_small_inline_dimension`: an inline `width` or `height` starting with a
/// pixel value below 44.
fn has_small_inline_dimension(style: &str) -> bool {
    for property in ["width", "height"] {
        let Some(dimension) = extract_style_value(style, property) else {
            continue;
        };
        let Some(captures) = pixel_dimension_re().captures(&dimension) else {
            continue;
        };
        let Ok(px) = captures[1].parse::<f64>() else {
            continue;
        };
        if px < 44.0 {
            return true;
        }
    }
    false
}

/// The source's `_extract_inline_style_value` for the properties this scanner reads. (Its
/// `color` special case — a negative lookbehind so `background-color` does not match — is not
/// needed: no ported check asks for `color`.)
fn extract_style_value(style: &str, property: &str) -> Option<String> {
    let pattern = match property {
        "outline" => style_outline_re(),
        "width" => style_width_re(),
        "height" => style_height_re(),
        _ => return None,
    };
    pattern
        .captures(style)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_owned())
}

/// The source's `_has_focusable_child`: a focusable tag or any descendant with `tabindex >= 0`.
fn has_focusable_child(element: &ElementRef<'_>) -> bool {
    const FOCUSABLE_TAGS: [&str; 5] = ["a", "button", "input", "select", "textarea"];
    // `descendants()` includes the element itself; BeautifulSoup's `descendants` does not.
    element.descendants().skip(1).any(|node| {
        let Node::Element(child) = node.value() else {
            return false;
        };
        if FOCUSABLE_TAGS.contains(&child.name()) {
            return true;
        }
        child
            .attr("tabindex")
            .is_some_and(|tabindex| tabindex.trim().parse::<i64>().is_ok_and(|value| value >= 0))
    })
}

// ---------------------------------------------------------------------------
// The source's regular expressions, compiled once.
// ---------------------------------------------------------------------------

fn skip_link_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)skip|jump|main.content").expect("static pattern"))
}

fn pixel_dimension_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(\d+(?:\.\d+)?)\s*px").expect("static pattern"))
}

fn outline_removed_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(none|0(px)?)$").expect("static pattern"))
}

fn focus_outline_rule_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?is):focus[^{]*\{[^}]*outline\s*:\s*(none|0(px)?)\s*;?[^}]*\}")
            .expect("static pattern")
    })
}

fn invalid_lang_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"^[a-zA-Z]{2,3}(-[a-zA-Z0-9]{2,8})*$").expect("static pattern"))
}

fn landmark_role_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(main|navigation|banner|contentinfo|complementary)$").expect("static pattern")
    })
}

fn implicit_live_role_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"^(alert|status|log|timer|marquee)$").expect("static pattern"))
}

fn identity_field_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)(name|email|phone|tel|address|postal|zip|city|country|cc-?number|card|fname|lname|first.?name|last.?name|username|login)",
        )
        .expect("static pattern")
    })
}

fn close_or_dismiss_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)close|dismiss").expect("static pattern"))
}

fn close_button_text_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)close|dismiss|cancel|x").expect("static pattern"))
}

fn style_outline_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)outline\s*:\s*([^;]+)").expect("static pattern"))
}

fn style_width_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)width\s*:\s*([^;]+)").expect("static pattern"))
}

fn style_height_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)height\s*:\s*([^;]+)").expect("static pattern"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::HttpEvidence;
    use crate::model::Document;

    fn scan_html(html: &str) -> Vec<Finding> {
        scan(&PageEvidence::new(
            Document::new("https://example.test/", html),
            HttpEvidence::default(),
        ))
    }

    fn has(findings: &[Finding], check_id: &str) -> bool {
        findings
            .iter()
            .any(|finding| finding.check_id() == check_id)
    }

    fn matching<'a>(findings: &'a [Finding], check_id: &str) -> Vec<&'a Finding> {
        findings
            .iter()
            .filter(|finding| finding.check_id() == check_id)
            .collect()
    }

    fn count(findings: &[Finding], check_id: &str) -> usize {
        matching(findings, check_id).len()
    }

    #[test]
    fn lang_attribute_checks() {
        let missing = scan_html("<html><body><p>Hello</p></body></html>");
        assert!(has(&missing, "a11y-missing-lang"));
        assert!(!has(&missing, "a11y-lang-invalid"));

        let invalid = scan_html(r#"<html lang="123"><body></body></html>"#);
        assert!(!has(&invalid, "a11y-missing-lang"));
        assert!(has(&invalid, "a11y-lang-invalid"));
        let finding = matching(&invalid, "a11y-lang-invalid")[0];
        assert_eq!(finding.title, "Invalid lang attribute value: '123'");
        assert_eq!(finding.evidence.as_deref(), Some("lang=\"123\""));

        let valid = scan_html(r#"<html lang="en-US"><body></body></html>"#);
        assert!(!has(&valid, "a11y-missing-lang"));
        assert!(!has(&valid, "a11y-lang-invalid"));
    }

    #[test]
    fn alt_text_checks() {
        let missing = scan_html(r#"<html lang="en"><body><img src="cat.png"></body></html>"#);
        assert!(has(&missing, "a11y-img-missing-alt"));
        assert!(!has(&missing, "a11y-img-empty-alt"));
        let finding = matching(&missing, "a11y-img-missing-alt")[0];
        assert_eq!(finding.title, "1 images missing alt attribute");
        assert_eq!(
            finding.description,
            "1 images have no alt attribute at all (WCAG 1.1.1)."
        );

        let empty = scan_html(r#"<html lang="en"><body><img src="cat.png" alt=""></body></html>"#);
        assert!(!has(&empty, "a11y-img-missing-alt"));
        assert!(has(&empty, "a11y-img-empty-alt"));
        assert_eq!(
            matching(&empty, "a11y-img-empty-alt")[0].title,
            "1 images with empty alt text"
        );

        // Boundary: an empty alt is allowed for decorative images (role=presentation), for
        // data: URIs, and for an image with no src at all.
        let allowed = scan_html(
            r#"<html lang="en"><body><img src="a.png" alt="" role="presentation"><img src="data:image/gif;base64,R0lGOD" alt=""><img alt=""></body></html>"#,
        );
        assert!(!has(&allowed, "a11y-img-missing-alt"));
        assert!(!has(&allowed, "a11y-img-empty-alt"));
    }

    #[test]
    fn form_label_checks() {
        let unlabeled = scan_html(
            r#"<html lang="en"><body><input id="q" type="text"><input type="text"></body></html>"#,
        );
        assert_eq!(count(&unlabeled, "a11y-form-no-label"), 1);
        assert_eq!(
            matching(&unlabeled, "a11y-form-no-label")[0].title,
            "2 form inputs without labels"
        );

        let labelled = scan_html(
            r#"<html lang="en"><body><label for="q">Query</label><input id="q" type="text"><label><input type="text"></label><input type="hidden"><input title="Search"></body></html>"#,
        );
        assert!(!has(&labelled, "a11y-form-no-label"));
    }

    #[test]
    fn skip_link_checks() {
        let missing =
            scan_html(r#"<html lang="en"><body><a href="/about">About us</a></body></html>"#);
        assert!(has(&missing, "a11y-no-skip-link"));

        let present = scan_html(
            r##"<html lang="en"><body><a href="#main-content">Jump</a><main>Body</main></body></html>"##,
        );
        assert!(!has(&present, "a11y-no-skip-link"));
    }

    #[test]
    fn heading_order_checks() {
        let wrong_first = scan_html(r#"<html lang="en"><body><h2>Sub</h2></body></html>"#);
        assert!(has(&wrong_first, "a11y-heading-not-h1"));
        assert_eq!(
            matching(&wrong_first, "a11y-heading-not-h1")[0].title,
            "First heading is H2, not H1"
        );

        let skip = scan_html(r#"<html lang="en"><body><h1>Title</h1><h3>Sub</h3></body></html>"#);
        assert!(!has(&skip, "a11y-heading-not-h1"));
        assert!(has(&skip, "a11y-heading-skip"));
        assert_eq!(
            matching(&skip, "a11y-heading-skip")[0].title,
            "Heading level skip: H1 to H3"
        );

        let ordered =
            scan_html(r#"<html lang="en"><body><h1>Title</h1><h2>Sub</h2></body></html>"#);
        assert!(!has(&ordered, "a11y-heading-not-h1"));
        assert!(!has(&ordered, "a11y-heading-skip"));
    }

    #[test]
    fn clickable_role_checks() {
        let missing = scan_html(
            r#"<html lang="en"><body><div onclick="go()">One</div><span onclick="go()">Two</span></body></html>"#,
        );
        // The source reports only the first offender.
        assert_eq!(count(&missing, "a11y-clickable-no-role"), 1);
        assert_eq!(
            matching(&missing, "a11y-clickable-no-role")[0].title,
            "Clickable <div> without ARIA role"
        );

        let with_role = scan_html(
            r#"<html lang="en"><body><div onclick="go()" role="button" tabindex="0">One</div></body></html>"#,
        );
        assert!(!has(&with_role, "a11y-clickable-no-role"));
    }

    #[test]
    fn positive_tabindex_checks() {
        let positive = scan_html(
            r#"<html lang="en"><body><div tabindex="2">One</div><span tabindex="1">Two</span></body></html>"#,
        );
        assert_eq!(
            matching(&positive, "a11y-positive-tabindex")[0].title,
            "2 elements with positive tabindex"
        );

        let fine = scan_html(
            r#"<html lang="en"><body><div tabindex="0">One</div><span tabindex="-1">Two</span><p tabindex="nope">Three</p></body></html>"#,
        );
        assert!(!has(&fine, "a11y-positive-tabindex"));
    }

    #[test]
    fn landmark_checks() {
        let none = scan_html(r#"<html lang="en"><body><p>Hello</p></body></html>"#);
        assert!(has(&none, "a11y-no-landmarks"));
        assert!(has(&none, "a11y-no-main"));

        let semantic = scan_html(r#"<html lang="en"><body><main>Hello</main></body></html>"#);
        assert!(!has(&semantic, "a11y-no-landmarks"));
        assert!(!has(&semantic, "a11y-no-main"));

        let aria = scan_html(r#"<html lang="en"><body><div role="main">Hello</div></body></html>"#);
        assert!(!has(&aria, "a11y-no-landmarks"));
        assert!(!has(&aria, "a11y-no-main"));
    }

    #[test]
    fn link_text_checks() {
        let generic = scan_html(
            r#"<html lang="en"><body><a href="/a">Click here</a><a href="/b">here</a></body></html>"#,
        );
        let finding = matching(&generic, "a11y-generic-link-text")[0];
        assert_eq!(finding.title, "2 links with non-descriptive text");
        assert_eq!(
            finding.description,
            "Links use generic text like 'click here' (WCAG 2.4.4)."
        );

        let labelled = scan_html(
            r#"<html lang="en"><body><a href="/a" aria-label="Full story">Read more</a></body></html>"#,
        );
        assert!(!has(&labelled, "a11y-generic-link-text"));
    }

    #[test]
    fn video_caption_checks() {
        let no_captions =
            scan_html(r#"<html lang="en"><body><video src="movie.mp4"></video></body></html>"#);
        assert!(has(&no_captions, "a11y-video-no-captions"));
        assert_eq!(
            matching(&no_captions, "a11y-video-no-captions")[0]
                .evidence
                .as_deref(),
            Some("movie.mp4")
        );

        let no_lang = scan_html(
            r#"<html lang="en"><body><video><track kind="captions" src="captions.vtt"></video></body></html>"#,
        );
        assert!(!has(&no_lang, "a11y-video-no-captions"));
        assert!(has(&no_lang, "a11y-video-caption-no-lang"));

        let complete = scan_html(
            r#"<html lang="en"><body><video><track kind="subtitles" src="captions.vtt" srclang="en"></video></body></html>"#,
        );
        assert!(!has(&complete, "a11y-video-no-captions"));
        assert!(!has(&complete, "a11y-video-caption-no-lang"));
    }

    #[test]
    fn button_name_checks() {
        let unnamed = scan_html(r#"<html lang="en"><body><button></button></body></html>"#);
        assert!(has(&unnamed, "a11y-button-no-name"));

        let named = scan_html(
            r#"<html lang="en"><body><button aria-label="Close"></button><button>Send</button><button title="Menu"></button></body></html>"#,
        );
        assert!(!has(&named, "a11y-button-no-name"));
    }

    #[test]
    fn table_header_checks() {
        let headerless = scan_html(
            r#"<html lang="en"><body><table><tr><td>a</td></tr><tr><td>b</td></tr></table></body></html>"#,
        );
        assert!(has(&headerless, "a11y-table-no-headers"));
        assert!(has(&headerless, "a11y-table-no-caption"));
        assert_eq!(count(&headerless, "a11y-table-header-no-scope"), 0);

        let no_scope = scan_html(
            r#"<html lang="en"><body><table><tr><th>Name</th></tr><tr><td>Ada</td></tr></table></body></html>"#,
        );
        assert!(!has(&no_scope, "a11y-table-no-headers"));
        assert!(has(&no_scope, "a11y-table-header-no-scope"));
        assert!(has(&no_scope, "a11y-table-no-caption"));
        assert_eq!(
            matching(&no_scope, "a11y-table-header-no-scope")[0].title,
            "Table header cells missing scope attribute"
        );

        let complete = scan_html(
            r#"<html lang="en"><body><table><caption>Users</caption><tr><th scope="col">Name</th></tr><tr><td>Ada</td></tr></table></body></html>"#,
        );
        assert!(!has(&complete, "a11y-table-no-headers"));
        assert!(!has(&complete, "a11y-table-header-no-scope"));
        assert!(!has(&complete, "a11y-table-no-caption"));

        let presentation = scan_html(
            r#"<html lang="en"><body><table role="presentation"><tr><td>a</td></tr><tr><td>b</td></tr></table></body></html>"#,
        );
        assert!(!has(&presentation, "a11y-table-no-headers"));
        assert!(!has(&presentation, "a11y-table-no-caption"));
    }

    #[test]
    fn touch_target_checks() {
        let small = scan_html(
            r#"<html lang="en"><body><a href="/a" style="width: 30px; height: 20px">Go</a></body></html>"#,
        );
        assert!(has(&small, "a11y-small-touch-target"));
        assert_eq!(
            matching(&small, "a11y-small-touch-target")[0]
                .selector
                .as_deref(),
            Some("a")
        );

        // 44px is the threshold, so 44 does not flag while 43.5 does.
        let boundary = scan_html(
            r#"<html lang="en"><body><a href="/a" style="width: 44px; height: 44px">Go</a><button style="height: 43.5px">Go</button></body></html>"#,
        );
        assert_eq!(count(&boundary, "a11y-small-touch-target"), 1);
        assert_eq!(
            matching(&boundary, "a11y-small-touch-target")[0]
                .selector
                .as_deref(),
            Some("button")
        );
    }

    #[test]
    fn keyboard_navigation_checks() {
        let inline = scan_html(
            r#"<html lang="en"><body><a href="/a" style="outline: none">Go</a></body></html>"#,
        );
        assert!(has(&inline, "a11y-outline-removed"));
        assert_eq!(
            matching(&inline, "a11y-outline-removed")[0].title,
            "Focus outline removed via inline style"
        );

        let embedded = scan_html(
            "<html lang=\"en\"><head><style>.x:focus { outline: 0 }</style></head><body></body></html>",
        );
        assert!(has(&embedded, "a11y-outline-removed"));
        assert_eq!(
            matching(&embedded, "a11y-outline-removed")[0].title,
            "Focus outline removed in embedded CSS"
        );

        let no_handler =
            scan_html(r#"<html lang="en"><body><a onclick="go()">Go</a></body></html>"#);
        assert!(has(&no_handler, "a11y-no-keyboard-handler"));

        let covered = scan_html(
            r#"<html lang="en"><body><a href="/a" onclick="go()">Go</a><a onclick="go()" onkeydown="goKey()">Go</a><button onclick="go()">Go</button></body></html>"#,
        );
        assert!(!has(&covered, "a11y-no-keyboard-handler"));
    }

    #[test]
    fn autocomplete_checks() {
        let missing = scan_html(
            r#"<html lang="en"><body><input type="email" name="user_email"></body></html>"#,
        );
        assert!(has(&missing, "a11y-autocomplete-missing"));
        assert_eq!(
            matching(&missing, "a11y-autocomplete-missing")[0].title,
            "1 identity inputs missing autocomplete"
        );

        let complete = scan_html(
            r#"<html lang="en"><body><input type="email" name="user_email" autocomplete="email"><input type="hidden" name="email"></body></html>"#,
        );
        assert!(!has(&complete, "a11y-autocomplete-missing"));
    }

    #[test]
    fn focus_trap_checks() {
        let dialog = scan_html(
            r#"<html lang="en"><body><div role="dialog"><button></button></div></body></html>"#,
        );
        assert!(has(&dialog, "a11y-potential-focus-trap"));
        assert_eq!(
            matching(&dialog, "a11y-potential-focus-trap")[0].title,
            "Potential focus trap in dialog"
        );

        let closed = scan_html(
            r#"<html lang="en"><body><div role="dialog"><button>Close</button></div><div role="dialog" aria-modal="true"></div><div role="dialog"><span aria-label="Dismiss"></span></div></body></html>"#,
        );
        assert!(!has(&closed, "a11y-potential-focus-trap"));

        // Faithful source quirk: the aria-label search is descendant-only, so a label on the
        // dialog itself does not count as a close affordance.
        let self_labelled = scan_html(
            r#"<html lang="en"><body><div role="dialog" aria-label="Close dialog"></div></body></html>"#,
        );
        assert!(has(&self_labelled, "a11y-potential-focus-trap"));

        let trap = scan_html(
            r#"<html lang="en"><body><div tabindex="-1" onkeydown="go()">Trap</div></body></html>"#,
        );
        assert_eq!(
            matching(&trap, "a11y-potential-focus-trap")[0].title,
            "Potential focus trap pattern detected"
        );

        let no_trap =
            scan_html(r#"<html lang="en"><body><div tabindex="-1">Fine</div></body></html>"#);
        assert!(!has(&no_trap, "a11y-potential-focus-trap"));
    }

    #[test]
    fn empty_table_header_checks() {
        let empty = scan_html(
            r#"<html lang="en"><body><table><tr><th> </th></tr><tr><td>Ada</td></tr></table></body></html>"#,
        );
        assert!(has(&empty, "a11y-empty-th"));
        assert_eq!(
            matching(&empty, "a11y-empty-th")[0].title,
            "1 empty table header cell(s)"
        );

        let filled = scan_html(
            r#"<html lang="en"><body><table><tr><th scope="col">Name</th></tr><tr><td>Ada</td></tr></table></body></html>"#,
        );
        assert!(!has(&filled, "a11y-empty-th"));
    }

    #[test]
    fn iframe_title_checks() {
        let untitled =
            scan_html(r#"<html lang="en"><body><iframe src="/map"></iframe></body></html>"#);
        assert!(has(&untitled, "a11y-iframe-no-title"));
        assert_eq!(
            matching(&untitled, "a11y-iframe-no-title")[0].title,
            "1 iframe(s) without title attribute"
        );

        let titled = scan_html(
            r#"<html lang="en"><body><iframe src="/map" title="Map"></iframe></body></html>"#,
        );
        assert!(!has(&titled, "a11y-iframe-no-title"));
    }

    #[test]
    fn deprecated_motion_checks() {
        let motion = scan_html(
            r#"<html lang="en"><body><marquee>News</marquee><blink>Alert</blink></body></html>"#,
        );
        assert!(has(&motion, "a11y-deprecated-motion"));
        assert_eq!(
            matching(&motion, "a11y-deprecated-motion")[0].description,
            "Found 1 <marquee>, 1 <blink> element(s). These deprecated elements create uncontrollable motion that can cause seizures or vestibular disorders (WCAG 2.3.1)."
        );
    }

    #[test]
    fn svg_accessibility_checks() {
        let unnamed = scan_html(r#"<html lang="en"><body><svg></svg></body></html>"#);
        assert!(has(&unnamed, "a11y-svg-no-title"));
        assert_eq!(
            matching(&unnamed, "a11y-svg-no-title")[0].title,
            "1 SVG(s) without accessible name"
        );

        let named = scan_html(
            r#"<html lang="en"><body><svg><title>Logo</title></svg><svg aria-label="Logo"></svg><svg aria-labelledby="logo-label"></svg><svg role="presentation"></svg><svg role="none"></svg></body></html>"#,
        );
        assert!(!has(&named, "a11y-svg-no-title"));
    }

    #[test]
    fn aria_hidden_focusable_checks() {
        let violation = scan_html(
            r#"<html lang="en"><body><div aria-hidden="true"><a href="/a">Link</a></div><div aria-hidden="true"><span tabindex="0">Focusable</span></div></body></html>"#,
        );
        assert!(has(&violation, "a11y-aria-hidden-focusable"));
        assert_eq!(
            matching(&violation, "a11y-aria-hidden-focusable")[0].title,
            "2 aria-hidden region(s) with focusable children"
        );

        let clean = scan_html(
            r#"<html lang="en"><body><div aria-hidden="true"><span>Hidden</span></div><div aria-hidden="true"><span tabindex="-1">Hidden</span></div></body></html>"#,
        );
        assert!(!has(&clean, "a11y-aria-hidden-focusable"));
    }

    #[test]
    fn aria_live_checks() {
        let missing = scan_html(r#"<html lang="en"><body><p>Static</p></body></html>"#);
        assert!(has(&missing, "a11y-no-aria-live"));

        let present =
            scan_html(r#"<html lang="en"><body><div aria-live="polite"></div></body></html>"#);
        assert!(!has(&present, "a11y-no-aria-live"));

        let role_present =
            scan_html(r#"<html lang="en"><body><div role="status"></div></body></html>"#);
        assert!(!has(&role_present, "a11y-no-aria-live"));
    }

    #[test]
    fn clean_page_has_no_findings() {
        let findings = scan_html(
            r##"<!doctype html>
<html lang="en">
<head><title>Example</title></head>
<body>
  <a href="#main">Skip to content</a>
  <main>
    <h1>Example</h1>
    <img src="cat.png" alt="A cat">
    <a href="/about">About us</a>
    <button>Send</button>
    <form><label for="q">Query</label><input id="q" type="search"></form>
    <table><caption>Users</caption><tr><th scope="col">Name</th></tr><tr><td>Ada</td></tr></table>
    <div aria-live="polite">Ready</div>
    <svg aria-label="Logo"><title>Logo</title></svg>
  </main>
</body>
</html>"##,
        );
        assert!(findings.is_empty(), "unexpected findings: {findings:#?}");
    }

    #[test]
    fn findings_carry_the_accessibility_contract() {
        let findings = scan_html(r#"<html lang="en"><body><img src="a.png"></body></html>"#);
        let finding = matching(&findings, "a11y-img-missing-alt")[0];
        assert_eq!(finding.category, Category::Accessibility);
        assert_eq!(finding.severity, Severity::High);
        assert_eq!(finding.source_tool.as_deref(), Some("a11y_scanner"));
        assert_eq!(finding.url, "https://example.test/");
        assert_eq!(finding.selector.as_deref(), Some("img:not([alt])"));
        assert_eq!(finding.evidence, None);
        assert_eq!(finding.scanner_count, 1);
        assert!(finding.confirmed_by.is_empty());
        assert_eq!(finding.title, "1 images missing alt attribute");
        assert_eq!(
            finding.description,
            "1 images have no alt attribute at all (WCAG 1.1.1)."
        );
        assert_eq!(
            finding.recommendation,
            "Add descriptive alt text to all images, or alt='' for decorative images."
        );
    }
}
