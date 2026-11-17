//! The conversion (CRO) scanner — the source's eight check groups over a supplied document.
//!
//! The port is faithful to the absorbed service's `cro` package. `CROScanner.scan` called eight
//! modules in this order — form friction, hero, CTA, social proof, value proposition, objection
//! handling, technical friction, footer — and every check keeps the source's `check_id`,
//! severity, title, description, recommendation, selector and evidence. Findings carry
//! `category: Category::Cro` and `source_tool: "cro_scanner"`, the values `_cro_finding` pinned.
//!
//! `tech_friction` is the one composed module: the source re-tagged, one finding per check, the
//! output of six perf/a11y/seo checks under the `cro-tech-*` namespace. Those six predicates are
//! reproduced here rather than reusing this crate's SEO texts, because the wording that reaches a
//! caller is the CRO finding's wording, not the source scanner's.
//!
//! The source's `false_positive_risk` and `remediation_effort` metadata has no field in
//! [`Finding`](crate::model::Finding), so it is dropped; every other field is carried over.

use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

use crate::model::{Category, Document, Finding, Severity};

/// Scan one document for conversion-rate issues, in the source scanner's order.
pub fn scan(document: &Document) -> Vec<Finding> {
    let html = Html::parse_document(&document.html);
    let url = document.url.as_str();
    let mut findings = Vec::new();
    findings.extend(check_form_friction(&html, url));
    findings.extend(check_hero(&html, url));
    findings.extend(check_cta(&html, url));
    findings.extend(check_social_proof(&html, url));
    findings.extend(check_value_prop(&html, url));
    findings.extend(check_objection(&html, url));
    findings.extend(check_tech_friction(&html, url));
    findings.extend(check_footer(&html, url));
    findings
}

// ---------------------------------------------------------------------------
// DOM helpers. `scraper` parses like a browser, so the selectors answer the same
// questions the source's parser answered for these rules.
// ---------------------------------------------------------------------------

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a static selector is valid")
}

fn first<'a>(html: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    html.select(&selector(css)).next()
}

fn all<'a>(html: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    html.select(&selector(css)).collect()
}

/// Every element, including the root `<html>` — the source's `soup.find_all(True)`.
fn all_elements(html: &Html) -> Vec<ElementRef<'_>> {
    let root = html.root_element();
    std::iter::once(root)
        .chain(root.descendent_elements())
        .collect()
}

/// The source's `Tag.get_text(" ", strip=True)`: stripped, non-empty descendant text nodes
/// joined with a single space. Comments are not text (`bs4`'s default string types).
fn text_join(element: &ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `bs4`'s `Tag.string`: the single child string of an element, or nothing.
fn single_string(element: &ElementRef<'_>) -> Option<String> {
    let first = element.first_child()?;
    if first.next_sibling().is_some() {
        return None;
    }
    match first.value() {
        Node::Text(text) => Some(text.text.to_string()),
        Node::Comment(comment) => Some(comment.comment.to_string()),
        _ => None,
    }
}

/// The source's `cro.base.page_text`: visible page text (script/style/noscript/template
/// excluded), whitespace collapsed. `bs4` walked `NavigableString`s, which includes comments.
fn page_text(html: &Html) -> String {
    let body_id = first(html, "body").map(|body| body.id());
    let mut parts: Vec<String> = Vec::new();
    for node in html.tree.nodes() {
        if let Some(inner) = &body_id {
            let in_body = if node.id() == *inner {
                true
            } else {
                let mut cursor = node.parent();
                let mut found = false;
                while let Some(current) = cursor {
                    if current.id() == *inner {
                        found = true;
                        break;
                    }
                    cursor = current.parent();
                }
                found
            };
            if !in_body {
                continue;
            }
        }
        match node.value() {
            Node::Text(text) => {
                let excluded = node
                    .parent()
                    .and_then(ElementRef::wrap)
                    .is_some_and(|parent| {
                        matches!(
                            parent.value().name(),
                            "script" | "style" | "noscript" | "template"
                        )
                    });
                if !excluded {
                    parts.push(text.text.to_string());
                }
            }
            Node::Comment(comment) => parts.push(comment.comment.to_string()),
            _ => {}
        }
    }
    let joined = parts.join(" ");
    patterns()
        .whitespace
        .replace_all(&joined, " ")
        .trim()
        .to_owned()
}

/// `bs4`'s class-regex match: any class token matches, or the space-joined token list does.
fn class_matches(element: &ElementRef<'_>, pattern: &Regex) -> bool {
    let Some(classes) = element.attr("class") else {
        return false;
    };
    let tokens: Vec<&str> = classes.split_whitespace().collect();
    tokens.iter().any(|token| pattern.is_match(token)) || pattern.is_match(&tokens.join(" "))
}

/// `soup.find(attrs={"class": pattern}) is not None`.
fn class_present(html: &Html, pattern: &Regex) -> bool {
    all_elements(html)
        .iter()
        .any(|element| class_matches(element, pattern))
}

/// A CRO finding: the source's `_cro_finding` fields, minus the metadata the model lacks.
fn cro(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Cro,
        severity,
        title: title.into(),
        description: description.into(),
        source_tool: Some("cro_scanner".to_owned()),
        scanner_count: 1,
        ..Finding::default()
    }
}

// ---------------------------------------------------------------------------
// CTA collection — `cro.base.cta_elements` / `cta_text`.
// ---------------------------------------------------------------------------

fn cta_elements<'a>(root: &ElementRef<'a>) -> Vec<ElementRef<'a>> {
    let mut out: Vec<ElementRef<'a>> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let p = patterns();

    for button in root.select(&selector("button")) {
        if seen.insert(button.id()) {
            out.push(button);
        }
    }
    for input in root.select(&selector("input")) {
        let kind = input.attr("type").unwrap_or("").to_lowercase();
        if (kind == "submit" || kind == "button") && seen.insert(input.id()) {
            out.push(input);
        }
    }
    for anchor in root.select(&selector("a")) {
        let classes = anchor.attr("class").unwrap_or("");
        let role = anchor.attr("role").unwrap_or("");
        let text = text_join(&anchor);
        if (p.btn_class.is_match(classes) || role == "button" || p.action_verb.is_match(&text))
            && seen.insert(anchor.id())
        {
            out.push(anchor);
        }
    }
    out
}

fn cta_text(element: &ElementRef<'_>) -> String {
    if element.value().name() == "input" {
        element.attr("value").unwrap_or("").trim().to_owned()
    } else {
        text_join(element)
    }
}

fn parent_text(element: &ElementRef<'_>) -> String {
    element
        .parent()
        .and_then(ElementRef::wrap)
        .map(|parent| text_join(&parent))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// form_friction — the source's eight checks.
// ---------------------------------------------------------------------------

fn countable_fields<'a>(form: &ElementRef<'a>) -> Vec<ElementRef<'a>> {
    let mut fields: Vec<ElementRef<'a>> = Vec::new();
    for input in form.select(&selector("input")) {
        let kind = input.attr("type").unwrap_or("text").to_lowercase();
        if matches!(
            kind.as_str(),
            "hidden" | "submit" | "button" | "reset" | "image"
        ) {
            continue;
        }
        fields.push(input);
    }
    // The source collected selects and textareas after the inputs, in their own document order.
    fields.extend(form.select(&selector("select, textarea")));
    fields
}

fn primary_form<'a>(html: &'a Html) -> Option<(ElementRef<'a>, Vec<ElementRef<'a>>)> {
    let mut best: Option<(ElementRef<'a>, Vec<ElementRef<'a>>)> = None;
    for form in all(html, "form") {
        let fields = countable_fields(&form);
        let best_len = best
            .as_ref()
            .map_or(0, |(_, best_fields)| best_fields.len());
        if fields.len() > best_len {
            best = Some((form, fields));
        }
    }
    best
}

fn has_label(form: &ElementRef<'_>, field: &ElementRef<'_>) -> bool {
    if field
        .attr("aria-label")
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }
    if field
        .attr("aria-labelledby")
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }
    if field
        .attr("id")
        .filter(|value| !value.is_empty())
        .is_some_and(|id| {
            form.select(&selector("label[for]"))
                .any(|label| label.attr("for") == Some(id))
        })
    {
        return true;
    }
    let form_id = form.id();
    let mut cursor = field.parent();
    while let Some(current) = cursor {
        if current.id() == form_id {
            return false;
        }
        let next = current.parent();
        if ElementRef::wrap(current).is_some_and(|element| element.value().name() == "label") {
            return true;
        }
        cursor = next;
    }
    false
}

fn check_field_count(n: usize, url: &str) -> Vec<Finding> {
    let (severity, note) = if n >= 10 {
        (Severity::Critical, "far above the ~5-field sweet spot")
    } else if n >= 7 {
        (Severity::High, "past the 5-to-7 conversion cliff")
    } else if n == 6 {
        (Severity::Medium, "just above the ideal 3-5 range")
    } else if n >= 4 {
        (Severity::Low, "slightly above the 3-field ideal")
    } else {
        return Vec::new();
    };
    vec![
        cro(
            "cro-form-field-count",
            severity,
            format!("Conversion form has {n} fields"),
            format!(
                "The primary form asks for {n} fields ({note}). Every field reduces \
                 completion — Marketo saw +34% conversion cutting 9 fields to 5, and \
                 completion drops sharply beyond ~5-7 fields."
            ),
        )
        .with_url(url)
        .with_recommendation(
            "Remove every non-essential field; defer the rest to after signup or \
             a later step. Aim for 3-5 fields.",
        )
        .with_selector("form")
        .with_evidence(format!("{n} countable fields")),
    ]
}

fn check_field_types(fields: &[ElementRef<'_>], url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let textareas = fields
        .iter()
        .filter(|field| field.value().name() == "textarea")
        .count();
    if textareas > 0 {
        findings.push(
            cro(
                "cro-form-textarea",
                Severity::Low,
                format!("{textareas} free-text textarea(s) in the form"),
                "Open textareas are among the highest-friction fields (HubSpot). \
                 They demand effort and deter completion.",
            )
            .with_url(url)
            .with_recommendation("Make free-text fields optional, or drop them entirely.")
            .with_selector("form textarea"),
        );
    }
    let selects = fields
        .iter()
        .filter(|field| field.value().name() == "select")
        .count();
    if selects >= 2 {
        findings.push(
            cro(
                "cro-form-multiple-selects",
                Severity::Low,
                format!("{selects} dropdown fields"),
                "Multiple <select> dropdowns measurably depress form completion \
                 (HubSpot). Prefer sensible defaults or smart detection.",
            )
            .with_url(url)
            .with_recommendation("Reduce dropdowns; pre-fill or infer values where possible.")
            .with_selector("form select"),
        );
    }
    findings
}

fn check_placeholder_labels(
    form: &ElementRef<'_>,
    fields: &[ElementRef<'_>],
    url: &str,
) -> Vec<Finding> {
    let offenders = fields
        .iter()
        .filter(|field| {
            field.value().name() == "input"
                && field
                    .attr("placeholder")
                    .is_some_and(|placeholder| !placeholder.is_empty())
                && !has_label(form, field)
        })
        .count();
    if offenders == 0 {
        return Vec::new();
    }
    vec![
        cro(
            "cro-form-placeholder-label",
            Severity::High,
            format!("{offenders} field(s) use a placeholder as the only label"),
            "Placeholder-only labels disappear on focus, causing recall errors and \
             failed validation — worst on mobile (Baymard).",
        )
        .with_url(url)
        .with_recommendation("Add a persistent <label> above every field.")
        .with_selector("form input[placeholder]"),
    ]
}

fn check_input_types(inputs: &[ElementRef<'_>], url: &str) -> Vec<Finding> {
    let p = patterns();
    let mut findings = Vec::new();
    for field in inputs {
        let name_id = format!(
            "{} {} {}",
            field.attr("name").unwrap_or(""),
            field.attr("id").unwrap_or(""),
            field.attr("placeholder").unwrap_or("")
        );
        if p.email_field.is_match(&name_id)
            && field.attr("type").unwrap_or("text").to_lowercase() != "email"
        {
            findings.push(
                cro(
                    "cro-form-input-type",
                    Severity::Low,
                    "Email field is not type=email",
                    "An email field without type=email shows the wrong mobile \
                     keyboard and skips built-in validation (Baymard).",
                )
                .with_url(url)
                .with_recommendation("Set type=\"email\" and autocomplete=\"email\".")
                .with_selector("form input"),
            );
            break;
        }
    }
    if inputs.len() >= 3
        && !inputs.iter().any(|field| {
            field
                .attr("autocomplete")
                .is_some_and(|value| !value.is_empty())
        })
    {
        findings.push(
            cro(
                "cro-form-autocomplete-missing",
                Severity::Low,
                "No autocomplete attributes on form fields",
                "Without autocomplete tokens the browser cannot autofill, adding \
                 typing friction (Baymard).",
            )
            .with_url(url)
            .with_recommendation("Add autocomplete=\"email name tel address-line1\" tokens.")
            .with_selector("form input"),
        );
    }
    findings
}

fn check_required_friction(inputs: &[ElementRef<'_>], url: &str) -> Vec<Finding> {
    let p = patterns();
    for field in inputs {
        let required =
            field.attr("required").is_some() || field.attr("aria-required") == Some("true");
        let name_id = format!(
            "{} {}",
            field.attr("name").unwrap_or(""),
            field.attr("id").unwrap_or("")
        );
        if required && (p.phone_field.is_match(&name_id) || p.company_field.is_match(&name_id)) {
            return vec![
                cro(
                    "cro-form-required-friction",
                    Severity::Medium,
                    "High-friction field marked required",
                    "Requiring phone/company early is a top abandonment driver \
                     (Expedia removed one such field for +$12M/yr).",
                )
                .with_url(url)
                .with_recommendation("Make phone/company optional or collect them later.")
                .with_selector("form input[required]"),
            ];
        }
    }
    Vec::new()
}

fn check_multistep(form: &ElementRef<'_>, n: usize, url: &str) -> Vec<Finding> {
    let has_step = form.select(&selector("fieldset")).next().is_some()
        || form
            .select(&selector("[class]"))
            .any(|element| class_matches(&element, &patterns().step_class))
        || form.select(&selector("progress")).next().is_some();
    if n <= 7 || has_step {
        return Vec::new();
    }
    vec![
        cro(
            "cro-form-no-multistep",
            Severity::Low,
            "Long single-step form",
            format!(
                "A {n}-field single-step form feels daunting. Multi-step forms convert \
                 up to ~3x better (Formstack/Venture Harbour)."
            ),
        )
        .with_url(url)
        .with_recommendation("Split the form into 2-3 progressive steps with a progress bar.")
        .with_selector("form"),
    ]
}

fn check_form_friction(html: &Html, url: &str) -> Vec<Finding> {
    let Some((form, fields)) = primary_form(html) else {
        return Vec::new();
    };
    if fields.is_empty() {
        return Vec::new();
    }
    let n = fields.len();
    let inputs: Vec<ElementRef<'_>> = fields
        .iter()
        .copied()
        .filter(|field| field.value().name() == "input")
        .collect();

    let mut findings = Vec::new();
    findings.extend(check_field_count(n, url));
    findings.extend(check_field_types(&fields, url));
    findings.extend(check_placeholder_labels(&form, &fields, url));
    findings.extend(check_input_types(&inputs, url));
    findings.extend(check_required_friction(&inputs, url));
    findings.extend(check_multistep(&form, n, url));
    findings
}

// ---------------------------------------------------------------------------
// hero — the source's six checks.
// ---------------------------------------------------------------------------

fn nav_link_elements(html: &Html) -> Vec<ElementRef<'_>> {
    let mut links = Vec::new();
    for container in all(html, "nav, header") {
        links.extend(container.select(&selector("a")));
    }
    links
}

fn check_hero(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let p = patterns();

    let h1s = all(html, "h1");
    if h1s.is_empty() {
        findings.push(
            cro(
                "cro-hero-no-h1",
                Severity::High,
                "No H1 headline",
                "The page has no <h1>. Visitors decide within ~5 seconds; a clear \
                 headline stating the value proposition is the single most important \
                 above-fold element.",
            )
            .with_url(url)
            .with_recommendation("Add one <h1> that states the core benefit in plain language.")
            .with_selector("h1"),
        );
    } else if h1s.len() > 1 {
        findings.push(
            cro(
                "cro-hero-multiple-h1",
                Severity::Low,
                format!("{} H1 headlines", h1s.len()),
                "Multiple <h1>s dilute the single-message focus a hero needs.",
            )
            .with_url(url)
            .with_recommendation("Keep exactly one <h1>; demote the rest to <h2>.")
            .with_selector("h1"),
        );
    }

    if let Some(h1) = h1s.first() {
        let text = text_join(h1);
        let words = text.split_whitespace().count();
        let chars = text.chars().count();
        if words > 0 && (!(3..=12).contains(&words) || chars > 70) {
            findings.push(
                cro(
                    "cro-hero-headline-length",
                    Severity::Low,
                    "Headline length is off-target",
                    format!(
                        "The H1 is {words} words / {chars} chars. Punchy hero headlines \
                         are ~6-12 words and scannable in one glance."
                    ),
                )
                .with_url(url)
                .with_recommendation("Tighten the headline to ~6-12 words focused on the benefit.")
                .with_selector("h1")
                .with_evidence(text.chars().take(120).collect::<String>()),
            );
        }
    }

    let root = html.root_element();
    if cta_elements(&root).is_empty() {
        findings.push(
            cro(
                "cro-hero-no-cta",
                Severity::High,
                "No call-to-action found",
                "No button or action link was detected. A landing page with no clear \
                 CTA cannot convert.",
            )
            .with_url(url)
            .with_recommendation("Add a prominent primary CTA button above the fold.")
            .with_selector("a, button"),
        );
    }

    let nav_links = nav_link_elements(html);
    if nav_links.len() > 6 {
        findings.push(
            cro(
                "cro-hero-nav-distraction",
                Severity::Medium,
                format!("{} competing nav links", nav_links.len()),
                format!(
                    "The header/nav offers {} links, pulling attention away from the \
                     conversion goal. Dedicated landing pages convert best near a \
                     1:1 attention ratio (Unbounce).",
                    nav_links.len()
                ),
            )
            .with_url(url)
            .with_recommendation("Trim top navigation on conversion pages to the essentials.")
            .with_selector("nav a, header a"),
        );
    }

    if !p.trust_badge.is_match(&page_text(html)) {
        findings.push(
            cro(
                "cro-hero-no-trust-signal",
                Severity::Low,
                "No trust signal detected",
                "No trust badge, guarantee, or security/authority mark was found. \
                 Above-fold trust cues reduce first-impression risk.",
            )
            .with_url(url)
            .with_recommendation("Add a trust badge, guarantee, or recognised security mark."),
        );
    }

    findings
}

// ---------------------------------------------------------------------------
// cta — the source's four checks.
// ---------------------------------------------------------------------------

const LONG_PAGE_CHARS: usize = 6000;

fn check_cta(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let p = patterns();
    let root = html.root_element();
    let ctas = cta_elements(&root);
    if ctas.is_empty() {
        return findings; // absence handled by check_hero
    }

    let texts: Vec<String> = ctas.iter().map(cta_text).collect();
    let non_empty: Vec<&str> = texts
        .iter()
        .map(String::as_str)
        .filter(|text| !text.is_empty())
        .collect();

    let has_action_verb = non_empty.iter().any(|text| p.action_verb.is_match(text));
    let has_generic = non_empty
        .iter()
        .any(|text| p.generic_cta.is_match(text.trim()));
    if !non_empty.is_empty() && !has_action_verb && has_generic {
        findings.push(
            cro(
                "cro-cta-generic-text",
                Severity::Medium,
                "CTAs use generic copy",
                "Every CTA uses generic text (e.g. 'Submit', 'Click here', 'Continue'). \
                 Action-verb, benefit-led copy ('Get my free quote') converts better.",
            )
            .with_url(url)
            .with_recommendation("Rewrite CTAs to start with an action verb tied to the benefit.")
            .with_selector("a, button")
            .with_evidence(
                non_empty
                    .iter()
                    .take(5)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        );
    }

    if !non_empty.is_empty() && !non_empty.iter().any(|text| p.first_person.is_match(text)) {
        findings.push(
            cro(
                "cro-cta-no-first-person",
                Severity::Low,
                "No first-person CTA copy",
                "None of the CTAs use first-person phrasing. 'Start my free trial' has \
                 out-clicked 'Start your free trial' in repeated tests (ContentVerve).",
            )
            .with_url(url)
            .with_recommendation("Test first-person CTA copy ('my' instead of 'your').")
            .with_selector("a, button"),
        );
    }

    if !ctas
        .iter()
        .any(|cta| p.microcopy.is_match(&parent_text(cta)))
    {
        findings.push(
            cro(
                "cro-cta-no-microcopy",
                Severity::Low,
                "No reassurance microcopy near CTAs",
                "No risk-reducing microcopy (e.g. 'No credit card required', \
                 'Cancel anytime') was found beside a CTA.",
            )
            .with_url(url)
            .with_recommendation("Add a short reassurance line directly under the primary CTA.")
            .with_selector("a, button"),
        );
    }

    if ctas.len() == 1 && page_text(html).chars().count() > LONG_PAGE_CHARS {
        findings.push(
            cro(
                "cro-cta-single-on-long-page",
                Severity::Low,
                "Only one CTA on a long page",
                "A long page presents the CTA once. Repeating the same primary CTA as \
                 the visitor scrolls captures intent at multiple points.",
            )
            .with_url(url)
            .with_recommendation("Repeat the primary CTA every ~1-1.5 viewports on long pages.")
            .with_selector("a, button"),
        );
    }

    findings
}

// ---------------------------------------------------------------------------
// social_proof — the source's five checks over its shared signal predicate.
// ---------------------------------------------------------------------------

struct ProofSignals {
    testimonials: bool,
    named: bool,
    logos: bool,
    ratings: bool,
    metrics: bool,
}

fn has_review_schema(html: &Html) -> bool {
    all(html, "script").iter().any(|script| {
        if script.attr("type") != Some("application/ld+json") {
            return false;
        }
        single_string(script).is_some_and(|content| {
            content.contains("\"Review\"")
                || content.contains("\"AggregateRating\"")
                || content.contains("\"ratingValue\"")
        })
    })
}

fn proof_signals(html: &Html, text: &str) -> ProofSignals {
    let p = patterns();
    let review_schema = has_review_schema(html);
    let testimonials = first(html, "blockquote").is_some()
        || class_present(html, &p.testimonial_class)
        || review_schema;
    let logos = all_elements(html).iter().any(|container| {
        class_matches(container, &p.logo_class) && container.select(&selector("img")).count() >= 3
    });
    ProofSignals {
        testimonials,
        named: p.named_attribution.is_match(text),
        logos,
        ratings: class_present(html, &p.rating_class) || text.contains('★') || review_schema,
        metrics: p.count.is_match(text) || p.media_mention.is_match(text),
    }
}

fn check_social_proof(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let signals = proof_signals(html, &page_text(html));

    if !signals.testimonials {
        findings.push(
            cro(
                "cro-social-no-testimonials",
                Severity::Medium,
                "No testimonials or reviews",
                "No testimonial, review block, or Review schema was found. Customer \
                 proof is one of the strongest conversion levers.",
            )
            .with_url(url)
            .with_recommendation("Add 2-3 named customer testimonials, ideally with photos."),
        );
    } else if !signals.named {
        findings.push(
            cro(
                "cro-social-unnamed-testimonials",
                Severity::Low,
                "Testimonials lack named attribution",
                "Testimonials were detected but no clear name/role attribution \
                 ('— Jane Doe, CEO') was found. Anonymous quotes read as fabricated.",
            )
            .with_url(url)
            .with_recommendation("Attribute each testimonial to a real name, role, and company."),
        );
    }

    if !signals.logos {
        findings.push(
            cro(
                "cro-social-no-logos",
                Severity::Low,
                "No client/logo trust strip",
                "No 'trusted by' logo cluster was detected. Recognisable customer or \
                 press logos borrow authority.",
            )
            .with_url(url)
            .with_recommendation("Add a logo strip of known customers or media mentions."),
        );
    }

    if !signals.ratings {
        findings.push(
            cro(
                "cro-social-no-ratings",
                Severity::Low,
                "No star ratings",
                "No star-rating UI or ratingValue schema was found.",
            )
            .with_url(url)
            .with_recommendation("Surface an aggregate star rating with a review count."),
        );
    }

    if !signals.metrics {
        findings.push(
            cro(
                "cro-social-no-metrics",
                Severity::Low,
                "No quantified proof or media mentions",
                "No customer/usage counts ('12,000+ customers') or media mentions \
                 were found. Specific numbers make proof concrete.",
            )
            .with_url(url)
            .with_recommendation(
                "Add a headline metric (users, reviews, results) or press mentions.",
            ),
        );
    }

    findings
}

// ---------------------------------------------------------------------------
// value_prop — the source's five checks, including its Flesch-Kincaid grade.
// ---------------------------------------------------------------------------

const LONG_PARAGRAPH_WORDS: usize = 60;
const MIN_WORDS_FOR_GRADE: usize = 120;
const MAX_READING_GRADE: f64 = 9.0;

fn count_syllables(word: &str) -> usize {
    let groups = patterns().vowel_group.find_iter(word).count();
    let count = if word.ends_with('e') && groups > 1 {
        groups - 1
    } else {
        groups
    };
    count.max(1)
}

fn flesch_kincaid_grade(text: &str) -> Option<f64> {
    let p = patterns();
    let lowered = text.to_lowercase();
    let words: Vec<&str> = p.word.find_iter(&lowered).map(|m| m.as_str()).collect();
    if words.len() < MIN_WORDS_FOR_GRADE {
        return None;
    }
    let sentences = p
        .sentence
        .split(text)
        .filter(|piece| !piece.trim().is_empty())
        .count()
        .max(1);
    let syllables: usize = words.iter().map(|word| count_syllables(word)).sum();
    Some(
        0.39 * (words.len() as f64 / sentences as f64)
            + 11.8 * (syllables as f64 / words.len() as f64)
            - 15.59,
    )
}

fn check_value_prop(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let p = patterns();

    let headline = first(html, "h1")
        .map(|h1| text_join(&h1))
        .unwrap_or_default();
    let lead_paras: Vec<String> = all(html, "p").iter().take(5).map(text_join).collect();
    let mut surface = headline;
    for paragraph in &lead_paras {
        surface.push(' ');
        surface.push_str(paragraph);
    }

    if !surface.trim().is_empty() && !p.quant_benefit.is_match(&surface) {
        findings.push(
            cro(
                "cro-valueprop-no-quantified-benefit",
                Severity::Low,
                "Value proposition is not quantified",
                "The headline and lead copy contain no specific numbers (%, $, time \
                 saved, multiples). Ultra-specific claims are more persuasive than \
                 vague ones.",
            )
            .with_url(url)
            .with_recommendation("Add a concrete metric ('Cut onboarding time by 40%')."),
        );
    }

    let bullets = all(html, "li").len();
    if bullets < 3 {
        findings.push(
            cro(
                "cro-valueprop-few-bullets",
                Severity::Low,
                "Few or no benefit bullets",
                format!(
                    "Only {bullets} list item(s) found. Scannable bullet lists let \
                     visitors grasp benefits without reading prose (79% scan, NN/G)."
                ),
            )
            .with_url(url)
            .with_recommendation("Summarise the top 3-5 benefits as a bulleted list."),
        );
    }

    let long_paras = all(html, "p")
        .iter()
        .filter(|paragraph| text_join(paragraph).split_whitespace().count() > LONG_PARAGRAPH_WORDS)
        .count();
    if long_paras > 0 {
        findings.push(
            cro(
                "cro-valueprop-long-paragraph",
                Severity::Low,
                format!("{long_paras} long paragraph(s)"),
                format!(
                    "{long_paras} paragraph(s) exceed {LONG_PARAGRAPH_WORDS} words. \
                     Dense text is skipped by scanning readers."
                ),
            )
            .with_url(url)
            .with_recommendation("Split long paragraphs; one idea each, ~40 words max."),
        );
    }

    if first(html, "h2, h3").is_none() {
        findings.push(
            cro(
                "cro-valueprop-no-subheads",
                Severity::Low,
                "No subheadings",
                "No <h2>/<h3> subheadings were found. Subheads anchor the F-pattern \
                 scan and let visitors navigate the message.",
            )
            .with_url(url)
            .with_recommendation("Break the page into sections with descriptive subheads."),
        );
    }

    findings.extend(check_reading_grade(html, url));
    findings
}

fn check_reading_grade(html: &Html, url: &str) -> Vec<Finding> {
    let body = first(html, "body").unwrap_or_else(|| html.root_element());
    let grade = match flesch_kincaid_grade(&text_join(&body)) {
        Some(grade) if grade > MAX_READING_GRADE => grade,
        _ => return Vec::new(),
    };
    vec![
        cro(
            "cro-valueprop-reading-grade",
            Severity::Low,
            format!("Reading grade {grade:.0} is high"),
            format!(
                "The copy reads at a grade-{grade:.0} level. Landing-page copy converts \
                 best around grade 7-8; complex language loses scanning readers."
            ),
        )
        .with_url(url)
        .with_recommendation("Simplify sentences and vocabulary toward a grade 7-8 reading level.")
        .with_evidence(format!("Flesch-Kincaid grade {grade:.1}")),
    ]
}

// ---------------------------------------------------------------------------
// objection — the source's four checks over its shared signal predicate.
// ---------------------------------------------------------------------------

struct ObjectionSignals {
    faq: bool,
    risk_reversal: bool,
    security_assurance: bool,
    contact: bool,
}

fn objection_signals(html: &Html, text: &str) -> ObjectionSignals {
    let p = patterns();
    let anchors = all(html, "a");
    let hrefs: Vec<String> = anchors
        .iter()
        .map(|anchor| anchor.attr("href").unwrap_or("").to_owned())
        .collect();

    let has_faq = class_present(html, &p.faq_class)
        || first(html, "details").is_some()
        || text.to_lowercase().contains("frequently asked");
    let has_privacy_link = hrefs.iter().any(|href| {
        let lower = href.to_lowercase();
        lower.contains("privacy") || lower.contains("security")
    });
    let has_contact = hrefs
        .iter()
        .any(|href| href.starts_with("mailto:") || href.starts_with("tel:"))
        || anchors
            .iter()
            .any(|anchor| p.objection_contact.is_match(&text_join(anchor)));

    ObjectionSignals {
        faq: has_faq,
        risk_reversal: p.guarantee.is_match(text) || p.free_trial.is_match(text),
        security_assurance: p.security.is_match(text) || has_privacy_link,
        contact: has_contact,
    }
}

fn check_objection(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let signals = objection_signals(html, &page_text(html));

    if !signals.faq {
        findings.push(
            cro(
                "cro-objection-no-faq",
                Severity::Low,
                "No FAQ section",
                "No FAQ or accordion was found. An FAQ pre-empts the objections that \
                 otherwise stall a decision.",
            )
            .with_url(url)
            .with_recommendation("Add an FAQ answering the top 4-6 buyer objections."),
        );
    }
    if !signals.risk_reversal {
        findings.push(
            cro(
                "cro-objection-no-risk-reversal",
                Severity::Low,
                "No risk reversal",
                "No guarantee, refund, or free-trial language was found. Risk reversal \
                 shifts perceived risk off the visitor.",
            )
            .with_url(url)
            .with_recommendation(
                "Offer a money-back guarantee or free trial and state it clearly.",
            ),
        );
    }
    if !signals.security_assurance {
        findings.push(
            cro(
                "cro-objection-no-trust-links",
                Severity::Low,
                "No security or privacy assurance",
                "No security/privacy language or privacy link was found. Visitors \
                 handing over data look for reassurance it is safe.",
            )
            .with_url(url)
            .with_recommendation(
                "Add a privacy link and a short security/data-handling statement.",
            ),
        );
    }
    if !signals.contact {
        findings.push(
            cro(
                "cro-objection-no-legitimacy",
                Severity::Low,
                "No contact or legitimacy signals",
                "No contact link, email/phone link, or About/Support link was found. \
                 Reachability signals a real, accountable business.",
            )
            .with_url(url)
            .with_recommendation("Add visible contact details and an About/Contact link."),
        );
    }

    findings
}

// ---------------------------------------------------------------------------
// tech_friction — the source's six re-tagged checks, one finding each.
// ---------------------------------------------------------------------------

fn tech_render_blocking(html: &Html, url: &str) -> Option<Finding> {
    let head = first(html, "head")?;
    let mut blocking: Vec<String> = Vec::new();
    for script in head.select(&selector("script")) {
        let Some(src) = script.attr("src").filter(|src| !src.is_empty()) else {
            continue;
        };
        if script.attr("async").is_some() || script.attr("defer").is_some() {
            continue;
        }
        if script.attr("type").unwrap_or("").trim().to_lowercase() == "module" {
            continue;
        }
        blocking.push(src.to_owned());
    }
    if blocking.is_empty() {
        return None;
    }
    let count = blocking.len();
    let first_src = &blocking[0];
    Some(
        cro(
            "cro-tech-render-blocking",
            Severity::High,
            format!("{count} render-blocking scripts in <head>"),
            format!("{count} scripts in <head> block rendering. First: {first_src}"),
        )
        .with_url(url)
        .with_recommendation("Add 'async' or 'defer' attribute to scripts in <head>.")
        .with_selector("head > script[src]:not([async]):not([defer])")
        .with_evidence("via perf-render-blocking-scripts"),
    )
}

fn tech_cls_images(html: &Html, url: &str) -> Option<Finding> {
    let missing = all(html, "img")
        .into_iter()
        .filter(|image| {
            if image.attr("width").is_some() && image.attr("height").is_some() {
                return false;
            }
            image
                .attr("src")
                .is_some_and(|src| !src.is_empty() && !src.starts_with("data:"))
        })
        .count();
    if missing == 0 {
        return None;
    }
    Some(
        cro(
            "cro-tech-cls-images",
            Severity::Medium,
            format!("{missing} images without explicit dimensions"),
            format!("{missing} images lack width/height attributes, causing layout shifts (CLS)."),
        )
        .with_url(url)
        .with_recommendation("Add explicit width and height attributes to all images.")
        .with_selector("img:not([width]):not([height])")
        .with_evidence("via perf-img-no-dimensions"),
    )
}

fn tech_large_dom(html: &Html, url: &str) -> Option<Finding> {
    let count = all_elements(html).len();
    if count <= 3000 {
        return None;
    }
    let severity = if count > 5000 {
        Severity::High
    } else {
        Severity::Medium
    };
    Some(
        cro(
            "cro-tech-large-dom",
            severity,
            format!("Large DOM: {count} elements"),
            format!(
                "The DOM contains {count} elements. Large DOMs slow \
                 rendering and increase memory usage."
            ),
        )
        .with_url(url)
        .with_recommendation(
            "Reduce DOM size by removing unnecessary wrappers, \
             using virtualization, or lazy loading content.",
        )
        .with_evidence("via perf-large-dom"),
    )
}

fn tech_no_viewport(html: &Html, url: &str) -> Option<Finding> {
    let present = all(html, "meta")
        .iter()
        .any(|meta| meta.attr("name") == Some("viewport"));
    if present {
        return None;
    }
    Some(
        cro(
            "cro-tech-no-viewport",
            Severity::Medium,
            "Missing viewport meta tag",
            "No viewport meta tag found. Google uses mobile-first \
             indexing, so pages without a viewport tag may rank lower.",
        )
        .with_url(url)
        .with_recommendation(
            "Add <meta name=\"viewport\" content=\"width=device-width, \
             initial-scale=1\"> to the page head.",
        )
        .with_selector("meta[name=\"viewport\"]")
        .with_evidence("via seo-viewport-missing"),
    )
}

/// Python's `str(float)` for the plausible font-size range (10 -> "10.0", 9.5 -> "9.5").
fn py_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn tech_small_fonts(html: &Html, url: &str) -> Option<Finding> {
    let p = patterns();
    let mut small: Vec<String> = Vec::new();

    for element in all_elements(html) {
        if let Some(style) = element.attr("style") {
            for captures in p.font_size.captures_iter(style) {
                match captures[1].parse::<f64>() {
                    Ok(size) if size < 12.0 => {
                        small.push(format!("{}px (inline)", py_float(size)));
                    }
                    _ => {}
                }
            }
        }
    }
    for style_element in all(html, "style") {
        let Some(css) = single_string(&style_element) else {
            continue;
        };
        for captures in p.font_size.captures_iter(&css) {
            match captures[1].parse::<f64>() {
                Ok(size) if size < 12.0 => {
                    small.push(format!("{}px (style block)", py_float(size)));
                }
                _ => {}
            }
        }
    }

    if small.is_empty() {
        return None;
    }
    let count = small.len();
    Some(
        cro(
            "cro-tech-small-fonts",
            Severity::Low,
            format!("{count} font-size value(s) below 12px"),
            "Font sizes below 12px are hard to read on mobile \
             devices and may trigger a mobile usability warning \
             in Google Search Console.",
        )
        .with_url(url)
        .with_recommendation(
            "Use a minimum font-size of 12px (ideally 16px) for \
             body text to ensure readability on mobile devices.",
        )
        .with_evidence("via seo-mobile-font-size"),
    )
}

fn clickable_elements<'a>(html: &'a Html) -> Vec<ElementRef<'a>> {
    let mut out = Vec::new();
    out.extend(all(html, "a"));
    out.extend(all(html, "button"));
    out.extend(
        all(html, "input")
            .into_iter()
            .filter(|element| element.attr("type") == Some("submit")),
    );
    out.extend(
        all(html, "input")
            .into_iter()
            .filter(|element| element.attr("type") == Some("button")),
    );
    out
}

fn small_inline_dimension(style: &str) -> bool {
    let p = patterns();
    for pattern in [&p.width_style, &p.height_style] {
        let Some(captures) = pattern.captures(style) else {
            continue;
        };
        let value = captures[1].trim();
        let Some(pixels) = p.px_prefix.captures(value) else {
            continue;
        };
        if pixels[1].parse::<f64>().is_ok_and(|size| size < 44.0) {
            return true;
        }
    }
    false
}

fn tech_small_touch_target(html: &Html, url: &str) -> Option<Finding> {
    for element in clickable_elements(html) {
        let Some(style) = element.attr("style") else {
            continue;
        };
        if style.is_empty() || !small_inline_dimension(style) {
            continue;
        }
        let tag = element.value().name();
        return Some(
            cro(
                "cro-tech-small-touch-target",
                Severity::Medium,
                "Small touch target",
                format!(
                    "A clickable <{tag}> element has inline dimensions \
                     smaller than 44px, which may be difficult for touch users \
                     (WCAG 2.5.8). Note: this is a heuristic check based on \
                     inline styles only."
                ),
            )
            .with_url(url)
            .with_recommendation("Ensure touch targets are at least 44x44 CSS pixels.")
            .with_selector(tag)
            .with_evidence("via a11y-small-touch-target"),
        );
    }
    None
}

fn check_tech_friction(html: &Html, url: &str) -> Vec<Finding> {
    let checks: [Option<Finding>; 6] = [
        tech_render_blocking(html, url),
        tech_cls_images(html, url),
        tech_large_dom(html, url),
        tech_no_viewport(html, url),
        tech_small_fonts(html, url),
        tech_small_touch_target(html, url),
    ];
    checks.into_iter().flatten().collect()
}

// ---------------------------------------------------------------------------
// footer — the source's two checks over the closing region.
// ---------------------------------------------------------------------------

fn bottom_region<'a>(html: &'a Html) -> Option<ElementRef<'a>> {
    if let Some(footer) = first(html, "footer") {
        return Some(footer);
    }
    let body = first(html, "body")?;
    let sections: Vec<ElementRef<'_>> = body
        .child_elements()
        .filter(|element| matches!(element.value().name(), "section" | "div"))
        .collect();
    sections.last().copied()
}

fn check_footer(html: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(region) = bottom_region(html) else {
        return findings;
    };
    let p = patterns();

    if cta_elements(&region).is_empty() {
        findings.push(
            cro(
                "cro-footer-no-final-cta",
                Severity::Low,
                "No closing CTA",
                "The bottom of the page has no call-to-action. Visitors who read to the \
                 end are the warmest — give them a final way to convert.",
            )
            .with_url(url)
            .with_recommendation("Repeat the primary CTA in a closing section or the footer.")
            .with_selector("footer"),
        );
    }

    let region_text = text_join(&region);
    let has_contact_link = region.select(&selector("a")).any(|anchor| {
        let href = anchor.attr("href").unwrap_or("");
        href.starts_with("mailto:")
            || href.starts_with("tel:")
            || p.footer_contact.is_match(&text_join(&anchor))
    });
    if !has_contact_link && !p.footer_contact.is_match(&region_text) {
        findings.push(
            cro(
                "cro-footer-no-contact",
                Severity::Low,
                "No contact/support link in footer",
                "The footer offers no contact or support link. Easy reachability builds \
                 trust and captures late-stage questions.",
            )
            .with_url(url)
            .with_recommendation("Add contact/support links (and an email/phone) to the footer.")
            .with_selector("footer"),
        );
    }

    findings
}

// ---------------------------------------------------------------------------
// Patterns — the source's compiled-once token regexes.
// ---------------------------------------------------------------------------

struct Patterns {
    whitespace: Regex,
    action_verb: Regex,
    generic_cta: Regex,
    first_person: Regex,
    microcopy: Regex,
    trust_badge: Regex,
    guarantee: Regex,
    free_trial: Regex,
    security: Regex,
    media_mention: Regex,
    count: Regex,
    quant_benefit: Regex,
    named_attribution: Regex,
    testimonial_class: Regex,
    logo_class: Regex,
    rating_class: Regex,
    faq_class: Regex,
    step_class: Regex,
    btn_class: Regex,
    phone_field: Regex,
    company_field: Regex,
    email_field: Regex,
    objection_contact: Regex,
    footer_contact: Regex,
    font_size: Regex,
    word: Regex,
    sentence: Regex,
    vowel_group: Regex,
    width_style: Regex,
    height_style: Regex,
    px_prefix: Regex,
}

static PATTERNS: OnceLock<Patterns> = OnceLock::new();

fn patterns() -> &'static Patterns {
    PATTERNS.get_or_init(|| Patterns {
        whitespace: Regex::new(r"\s+").expect("static pattern"),
        action_verb: Regex::new(concat!(
            r"(?i)\b(get|start|try|download|book|claim|build|create|join|grab|unlock|",
            r"buy|order|subscribe|register|reserve|request|discover|see)\b"
        ))
        .expect("static pattern"),
        generic_cta: Regex::new(
            r"(?i)^(submit|click here|continue|next|go|ok|enter|more|read more|learn more)$",
        )
        .expect("static pattern"),
        first_person: Regex::new(r"(?i)\b(my|me|i)\b").expect("static pattern"),
        microcopy: Regex::new(concat!(
            r"(?i)no credit card|free|cancel anytime|money[- ]back|30[- ]day|",
            r"no obligation|instant|risk[- ]free|secure"
        ))
        .expect("static pattern"),
        trust_badge: Regex::new(concat!(
            r"(?i)norton|mcafee|\bssl\b|trustpilot|\bg2\b|capterra|verisign|secured?|",
            r"guarantee|gdpr|soc\s*2|iso\s*27001|\bpci\b|badge|award|certified"
        ))
        .expect("static pattern"),
        guarantee: Regex::new(concat!(
            r"(?i)money[- ]back|refund|guarantee|risk[- ]free|no questions asked|",
            r"satisfaction guaranteed|full refund"
        ))
        .expect("static pattern"),
        free_trial: Regex::new(
            r"(?i)free trial|no credit card|cancel anytime|start free|free forever|free plan",
        )
        .expect("static pattern"),
        security: Regex::new(concat!(
            r"(?i)\bssl\b|encrypt|gdpr|soc\s*2|iso\s*27001|\bpci\b|privacy|secure|",
            r"data protection|hipaa"
        ))
        .expect("static pattern"),
        media_mention: Regex::new(concat!(
            r"(?i)forbes|techcrunch|\bwsj\b|wall street journal|as seen in|featured in|",
            r"bloomberg|the verge|mashable|entrepreneur"
        ))
        .expect("static pattern"),
        count: Regex::new(concat!(
            r"(?i)\d[\d,]*\+?\s*(reviews?|customers?|users?|clients?|companies|downloads?|",
            r"members?|businesses|teams)"
        ))
        .expect("static pattern"),
        quant_benefit: Regex::new(concat!(
            r"(?i)\d+\s*%|[$£€]\s*\d|\bx\d+|\d+x\b|\d[\d,]*\s*(hours?|days?|minutes?|",
            r"seconds?|hrs?|customers?|users?)"
        ))
        .expect("static pattern"),
        named_attribution: Regex::new(
            r"[\u{2014}\u{2013}-]\s*[A-Z][a-z]+\s+[A-Z]|,\s*(ceo|cto|founder|director|manager|head of|vp)\b",
        )
        .expect("static pattern"),
        testimonial_class: Regex::new(r"(?i)testimonial|review|quote|customer-story")
            .expect("static pattern"),
        logo_class: Regex::new(r"(?i)logo|clients?|trusted[- ]by|brands?|partners?|as[- ]seen")
            .expect("static pattern"),
        rating_class: Regex::new(r"(?i)star|rating|review-score").expect("static pattern"),
        faq_class: Regex::new(r"(?i)faq|accordion|questions?").expect("static pattern"),
        step_class: Regex::new(r"(?i)step|progress|wizard|stage").expect("static pattern"),
        btn_class: Regex::new(r"(?i)btn|button|cta|call-to-action").expect("static pattern"),
        phone_field: Regex::new(r"(?i)phone|tel|mobile").expect("static pattern"),
        company_field: Regex::new(r"(?i)company|organi[sz]ation|business").expect("static pattern"),
        email_field: Regex::new(r"(?i)email|e-mail").expect("static pattern"),
        objection_contact: Regex::new(r"(?i)contact|about|support|help").expect("static pattern"),
        footer_contact: Regex::new(r"(?i)contact|support|help|email|phone").expect("static pattern"),
        font_size: Regex::new(r"(?i)font-size\s*:\s*(\d+(?:\.\d+)?)\s*px")
            .expect("static pattern"),
        word: Regex::new(r"[A-Za-z]+").expect("static pattern"),
        sentence: Regex::new(r"[.!?]+").expect("static pattern"),
        vowel_group: Regex::new(r"[aeiouy]+").expect("static pattern"),
        width_style: Regex::new(r"(?i)width\s*:\s*([^;]+)").expect("static pattern"),
        height_style: Regex::new(r"(?i)height\s*:\s*([^;]+)").expect("static pattern"),
        px_prefix: Regex::new(r"^(\d+(?:\.\d+)?)\s*px").expect("static pattern"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_html(html: &str) -> Vec<Finding> {
        scan(&Document::new("https://example.test/page", html))
    }

    fn ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(Finding::check_id).collect()
    }

    fn has(findings: &[Finding], id: &str) -> bool {
        ids(findings).contains(&id)
    }

    fn only<'a>(findings: &'a [Finding], id: &str) -> &'a Finding {
        findings
            .iter()
            .find(|finding| finding.check_id() == id)
            .unwrap_or_else(|| panic!("missing {id}; got {:?}", ids(findings)))
    }

    fn form_with(fields: &str) -> String {
        format!("<html><body><form>{fields}</form></body></html>")
    }

    #[test]
    fn every_finding_carries_the_cro_contract() {
        let findings = scan_html(r#"<html><body><h1>Hi</h1></body></html>"#);
        assert!(!findings.is_empty());
        for finding in &findings {
            assert_eq!(finding.category, Category::Cro);
            assert_eq!(finding.source_tool.as_deref(), Some("cro_scanner"));
            assert_eq!(finding.url, "https://example.test/page");
            assert_eq!(finding.scanner_count, 1);
            assert!(finding.check_id().starts_with("cro-"));
        }
    }

    // -- form friction ------------------------------------------------------

    #[test]
    fn form_field_count_scales_with_the_cliff() {
        let inputs = |n: usize| {
            (0..n)
                .map(|index| format!("<input name=\"f{index}\" type=\"text\">"))
                .collect::<String>()
        };
        let findings = scan_html(&form_with(&inputs(4)));
        let finding = only(&findings, "cro-form-field-count");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Conversion form has 4 fields");
        assert_eq!(finding.evidence.as_deref(), Some("4 countable fields"));
        assert_eq!(finding.selector.as_deref(), Some("form"));

        for (n, severity) in [
            (6, Severity::Medium),
            (7, Severity::High),
            (10, Severity::Critical),
        ] {
            let findings = scan_html(&form_with(&inputs(n)));
            assert_eq!(
                only(&findings, "cro-form-field-count").severity,
                severity,
                "n={n}"
            );
        }

        let ideal = scan_html(&form_with(&inputs(3)));
        assert!(!has(&ideal, "cro-form-field-count"));
    }

    #[test]
    fn a_textarea_is_free_text_friction() {
        let findings = scan_html(&form_with(
            r#"<input name="a"><textarea name="msg"></textarea>"#,
        ));
        let finding = only(&findings, "cro-form-textarea");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "1 free-text textarea(s) in the form");
        assert_eq!(finding.selector.as_deref(), Some("form textarea"));
    }

    #[test]
    fn two_or_more_dropdowns_are_friction() {
        let findings = scan_html(&form_with(
            r#"<select name="a"></select><select name="b"></select>"#,
        ));
        let finding = only(&findings, "cro-form-multiple-selects");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "2 dropdown fields");

        let one = scan_html(&form_with(r#"<select name="a"></select>"#));
        assert!(!has(&one, "cro-form-multiple-selects"));
    }

    #[test]
    fn a_placeholder_without_a_label_is_high() {
        let findings = scan_html(&form_with(
            r#"<input name="nickname" placeholder="Nickname">"#,
        ));
        let finding = only(&findings, "cro-form-placeholder-label");
        assert_eq!(finding.severity, Severity::High);
        assert_eq!(
            finding.title,
            "1 field(s) use a placeholder as the only label"
        );

        let labelled = scan_html(&form_with(
            r#"<label for="nickname">Nickname</label><input id="nickname" name="nickname" placeholder="Nickname">"#,
        ));
        assert!(!has(&labelled, "cro-form-placeholder-label"));
    }

    #[test]
    fn an_email_field_must_be_type_email() {
        let findings = scan_html(&form_with(r#"<input name="email" type="text">"#));
        let finding = only(&findings, "cro-form-input-type");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Email field is not type=email");

        let correct = scan_html(&form_with(r#"<input name="email" type="email">"#));
        assert!(!has(&correct, "cro-form-input-type"));
    }

    #[test]
    fn three_inputs_without_autocomplete_are_flagged() {
        let findings = scan_html(&form_with(
            r#"<input name="a"><input name="b"><input name="c">"#,
        ));
        let finding = only(&findings, "cro-form-autocomplete-missing");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "No autocomplete attributes on form fields");

        let with_token = scan_html(&form_with(
            r#"<input name="a" autocomplete="name"><input name="b"><input name="c">"#,
        ));
        assert!(!has(&with_token, "cro-form-autocomplete-missing"));
    }

    #[test]
    fn a_required_phone_field_is_medium() {
        let findings = scan_html(&form_with(r#"<input name="phone" required>"#));
        let finding = only(&findings, "cro-form-required-friction");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "High-friction field marked required");
        assert_eq!(finding.selector.as_deref(), Some("form input[required]"));
    }

    #[test]
    fn a_long_single_step_form_is_flagged_unless_it_has_steps() {
        let inputs = r#"<input name="a"><input name="b"><input name="c"><input name="d"><input name="e"><input name="f"><input name="g"><input name="h">"#;
        let findings = scan_html(&form_with(inputs));
        let finding = only(&findings, "cro-form-no-multistep");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Long single-step form");
        assert!(finding.description.contains("A 8-field single-step form"));

        let stepped = scan_html(&format!(
            "<html><body><form><fieldset>{inputs}</fieldset></form></body></html>"
        ));
        assert!(!has(&stepped, "cro-form-no-multistep"));
    }

    #[test]
    fn the_primary_form_is_the_one_with_the_most_fields() {
        let findings = scan_html(
            r#"<html><body>
                 <form><input name="q"></form>
                 <form><input name="a"><input name="b"><input name="c"><input name="d"></form>
               </body></html>"#,
        );
        assert_eq!(
            only(&findings, "cro-form-field-count").title,
            "Conversion form has 4 fields"
        );
    }

    // -- hero ---------------------------------------------------------------

    #[test]
    fn hero_absence_checks_fire_on_a_bare_page() {
        let findings = scan_html(r#"<html><body><p>A plain page.</p></body></html>"#);
        let h1 = only(&findings, "cro-hero-no-h1");
        assert_eq!(h1.severity, Severity::High);
        assert_eq!(h1.title, "No H1 headline");
        assert_eq!(only(&findings, "cro-hero-no-cta").severity, Severity::High);
        assert_eq!(
            only(&findings, "cro-hero-no-trust-signal").severity,
            Severity::Low
        );
    }

    #[test]
    fn multiple_h1s_are_flagged_when_the_headline_length_is_fine() {
        let findings = scan_html(
            "<html><body><h1>One headline here</h1><h1>Two headline here</h1></body></html>",
        );
        let finding = only(&findings, "cro-hero-multiple-h1");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "2 H1 headlines");
        assert!(!has(&findings, "cro-hero-headline-length"));
    }

    #[test]
    fn an_off_target_headline_length_quotes_the_measurement() {
        let findings = scan_html("<html><body><button>Go</button><h1>Hi</h1></body></html>");
        let finding = only(&findings, "cro-hero-headline-length");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Headline length is off-target");
        assert!(finding.description.contains("The H1 is 1 words / 2 chars."));
        assert_eq!(finding.evidence.as_deref(), Some("Hi"));
    }

    #[test]
    fn more_than_six_nav_links_distract() {
        let links = (0..7)
            .map(|index| format!("<a href=\"/{index}\">link</a>"))
            .collect::<String>();
        let findings = scan_html(&format!(
            "<html><body><nav>{links}</nav><h1>Welcome to the best product</h1></body></html>"
        ));
        let finding = only(&findings, "cro-hero-nav-distraction");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "7 competing nav links");
    }

    // -- cta ----------------------------------------------------------------

    #[test]
    fn generic_only_cta_copy_is_medium() {
        let findings =
            scan_html(r#"<html><body><a href="/go" class="btn">Submit</a></body></html>"#);
        let finding = only(&findings, "cro-cta-generic-text");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "CTAs use generic copy");
        assert_eq!(finding.evidence.as_deref(), Some("Submit"));
    }

    #[test]
    fn missing_first_person_copy_is_flagged() {
        let findings = scan_html("<html><body><button>Start your trial</button></body></html>");
        let finding = only(&findings, "cro-cta-no-first-person");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "No first-person CTA copy");

        let mine = scan_html("<html><body><button>Start my trial</button></body></html>");
        assert!(!has(&mine, "cro-cta-no-first-person"));
    }

    #[test]
    fn reassurance_microcopy_beside_a_cta_suppresses_the_finding() {
        let bare = scan_html("<html><body><div><button>Get started</button></div></body></html>");
        let finding = only(&bare, "cro-cta-no-microcopy");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "No reassurance microcopy near CTAs");

        let reassured = scan_html(
            "<html><body><div><button>Get started</button><span>No credit card required</span></div></body></html>",
        );
        assert!(!has(&reassured, "cro-cta-no-microcopy"));
    }

    #[test]
    fn one_cta_on_a_long_page_is_flagged() {
        let long_text = "a".repeat(6100);
        let findings = scan_html(&format!(
            "<html><body><button>Get started</button><div>{long_text}</div></body></html>"
        ));
        let finding = only(&findings, "cro-cta-single-on-long-page");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Only one CTA on a long page");

        let short_text = "a".repeat(100);
        let short = scan_html(&format!(
            "<html><body><button>Get started</button><div>{short_text}</div></body></html>"
        ));
        assert!(!has(&short, "cro-cta-single-on-long-page"));
    }

    // -- social proof -------------------------------------------------------

    #[test]
    fn social_absence_checks_fire_on_a_bare_page() {
        let findings = scan_html("<html><body><p>Nothing much here.</p></body></html>");
        assert_eq!(
            only(&findings, "cro-social-no-testimonials").severity,
            Severity::Medium
        );
        assert_eq!(
            only(&findings, "cro-social-no-logos").severity,
            Severity::Low
        );
        assert_eq!(
            only(&findings, "cro-social-no-ratings").severity,
            Severity::Low
        );
        assert_eq!(
            only(&findings, "cro-social-no-metrics").severity,
            Severity::Low
        );
    }

    #[test]
    fn an_unnamed_testimonial_is_flagged_but_a_named_one_is_not() {
        let unnamed = scan_html("<html><body><blockquote>Great product</blockquote></body></html>");
        let finding = only(&unnamed, "cro-social-unnamed-testimonials");
        assert_eq!(finding.severity, Severity::Low);
        assert!(!has(&unnamed, "cro-social-no-testimonials"));

        let named = scan_html(
            "<html><body><blockquote>Great product. — Jane Doe</blockquote></body></html>",
        );
        assert!(!has(&named, "cro-social-unnamed-testimonials"));
    }

    #[test]
    fn review_schema_counts_as_testimonials_and_ratings() {
        let findings = scan_html(
            r#"<html><body><script type="application/ld+json">{"@type":"Review","ratingValue":5}</script></body></html>"#,
        );
        assert!(!has(&findings, "cro-social-no-testimonials"));
        assert!(!has(&findings, "cro-social-no-ratings"));
    }

    #[test]
    fn a_three_logo_strip_suppresses_the_logo_finding() {
        let findings = scan_html(
            r#"<html><body><div class="clients"><img src="/1.png"><img src="/2.png"><img src="/3.png"></div></body></html>"#,
        );
        assert!(!has(&findings, "cro-social-no-logos"));

        let two = scan_html(
            r#"<html><body><div class="clients"><img src="/1.png"><img src="/2.png"></div></body></html>"#,
        );
        assert!(has(&two, "cro-social-no-logos"));
    }

    // -- value proposition --------------------------------------------------

    #[test]
    fn an_unquantified_value_proposition_is_flagged() {
        let findings = scan_html(
            "<html><body><h1>Welcome to our product</h1><p>We help teams work better.</p></body></html>",
        );
        let finding = only(&findings, "cro-valueprop-no-quantified-benefit");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Value proposition is not quantified");

        let quantified = scan_html("<html><body><h1>Cut onboarding by 40%</h1></body></html>");
        assert!(!has(&quantified, "cro-valueprop-no-quantified-benefit"));
    }

    #[test]
    fn fewer_than_three_bullets_are_flagged() {
        let findings = scan_html("<html><body><ul><li>One</li><li>Two</li></ul></body></html>");
        let finding = only(&findings, "cro-valueprop-few-bullets");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Few or no benefit bullets");
        assert!(
            finding
                .description
                .starts_with("Only 2 list item(s) found.")
        );
    }

    #[test]
    fn a_paragraph_over_sixty_words_is_flagged() {
        let text = "word ".repeat(61);
        let findings = scan_html(&format!("<html><body><p>{text}</p></body></html>"));
        let finding = only(&findings, "cro-valueprop-long-paragraph");
        assert_eq!(finding.title, "1 long paragraph(s)");
        assert_eq!(finding.severity, Severity::Low);
    }

    #[test]
    fn missing_subheads_are_flagged() {
        let findings = scan_html("<html><body><p>No headings here.</p></body></html>");
        assert_eq!(
            only(&findings, "cro-valueprop-no-subheads").severity,
            Severity::Low
        );

        let with_h2 = scan_html("<html><body><h2>Section</h2></body></html>");
        assert!(!has(&with_h2, "cro-valueprop-no-subheads"));
    }

    #[test]
    fn a_high_reading_grade_is_flagged() {
        let words = "indispensable ".repeat(120);
        let findings = scan_html(&format!("<html><body><p>{words}</p></body></html>"));
        let finding = only(&findings, "cro-valueprop-reading-grade");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "Reading grade 78 is high");
        assert_eq!(
            finding.evidence.as_deref(),
            Some("Flesch-Kincaid grade 78.4")
        );

        let short = scan_html("<html><body><p>Short copy.</p></body></html>");
        assert!(!has(&short, "cro-valueprop-reading-grade"));
    }

    // -- objection ----------------------------------------------------------

    #[test]
    fn objection_absence_checks_fire_on_a_bare_page() {
        let findings = scan_html("<html><body><p>Nothing much here.</p></body></html>");
        assert_eq!(
            only(&findings, "cro-objection-no-faq").title,
            "No FAQ section"
        );
        assert_eq!(
            only(&findings, "cro-objection-no-risk-reversal").title,
            "No risk reversal"
        );
        assert_eq!(
            only(&findings, "cro-objection-no-trust-links").title,
            "No security or privacy assurance"
        );
        assert_eq!(
            only(&findings, "cro-objection-no-legitimacy").title,
            "No contact or legitimacy signals"
        );
        for id in [
            "cro-objection-no-faq",
            "cro-objection-no-risk-reversal",
            "cro-objection-no-trust-links",
            "cro-objection-no-legitimacy",
        ] {
            assert_eq!(only(&findings, id).severity, Severity::Low);
        }
    }

    #[test]
    fn faq_risk_reversal_security_and_contact_all_suppress_their_findings() {
        let findings = scan_html(
            r#"<html><body>
                 <div class="faq">Questions? We have answers.</div>
                 <p>Every plan has a money-back guarantee.</p>
                 <a href="/privacy">Privacy</a>
                 <a href="mailto:hello@example.test">Mail us</a>
               </body></html>"#,
        );
        for id in [
            "cro-objection-no-faq",
            "cro-objection-no-risk-reversal",
            "cro-objection-no-trust-links",
            "cro-objection-no-legitimacy",
        ] {
            assert!(!has(&findings, id), "unexpected {id}");
        }
    }

    // -- technical friction -------------------------------------------------

    #[test]
    fn a_render_blocking_script_in_head_is_high() {
        let findings =
            scan_html("<html><head><script src=\"/app.js\"></script></head><body></body></html>");
        let finding = only(&findings, "cro-tech-render-blocking");
        assert_eq!(finding.severity, Severity::High);
        assert_eq!(finding.title, "1 render-blocking scripts in <head>");
        assert_eq!(
            finding.description,
            "1 scripts in <head> block rendering. First: /app.js"
        );
        assert_eq!(
            finding.evidence.as_deref(),
            Some("via perf-render-blocking-scripts")
        );

        let deferred = scan_html(
            "<html><head>\
               <script src=\"/a.js\" async></script>\
               <script src=\"/b.js\" defer></script>\
               <script src=\"/c.js\" type=\"module\"></script>\
             </head><body></body></html>",
        );
        assert!(!has(&deferred, "cro-tech-render-blocking"));
    }

    #[test]
    fn an_image_without_dimensions_is_flagged() {
        let findings = scan_html(r#"<html><body><img src="/a.png"></body></html>"#);
        let finding = only(&findings, "cro-tech-cls-images");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "1 images without explicit dimensions");
        assert_eq!(
            finding.evidence.as_deref(),
            Some("via perf-img-no-dimensions")
        );

        let data_uri =
            scan_html(r#"<html><body><img src="data:image/png;base64,AAAA"></body></html>"#);
        assert!(!has(&data_uri, "cro-tech-cls-images"));

        let sized =
            scan_html(r#"<html><body><img src="/a.png" width="10" height="10"></body></html>"#);
        assert!(!has(&sized, "cro-tech-cls-images"));
    }

    #[test]
    fn a_dom_over_three_thousand_elements_is_flagged() {
        let mut html = String::from("<html><head></head><body>");
        for _ in 0..3001 {
            html.push_str("<div></div>");
        }
        html.push_str("</body></html>");
        let findings = scan_html(&html);
        let finding = only(&findings, "cro-tech-large-dom");
        assert_eq!(finding.severity, Severity::Medium);
        assert!(finding.title.starts_with("Large DOM: "));
        assert_eq!(finding.evidence.as_deref(), Some("via perf-large-dom"));

        let small = scan_html("<html><body><div></div></body></html>");
        assert!(!has(&small, "cro-tech-large-dom"));
    }

    #[test]
    fn a_missing_viewport_meta_is_flagged() {
        let findings = scan_html("<html><head></head><body></body></html>");
        let finding = only(&findings, "cro-tech-no-viewport");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "Missing viewport meta tag");
        assert_eq!(
            finding.evidence.as_deref(),
            Some("via seo-viewport-missing")
        );

        let present = scan_html(
            "<html><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></head><body></body></html>",
        );
        assert!(!has(&present, "cro-tech-no-viewport"));
    }

    #[test]
    fn font_sizes_below_twelve_pixels_are_flagged() {
        let inline =
            scan_html("<html><body><div style=\"font-size: 10px\">tiny</div></body></html>");
        let finding = only(&inline, "cro-tech-small-fonts");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(finding.title, "1 font-size value(s) below 12px");
        assert_eq!(
            finding.evidence.as_deref(),
            Some("via seo-mobile-font-size")
        );

        let block = scan_html(
            "<html><head><style>.x { font-size: 9.5px }</style></head><body></body></html>",
        );
        assert!(has(&block, "cro-tech-small-fonts"));

        let readable =
            scan_html("<html><body><div style=\"font-size: 16px\">fine</div></body></html>");
        assert!(!has(&readable, "cro-tech-small-fonts"));
    }

    #[test]
    fn a_small_inline_touch_target_is_flagged() {
        let findings =
            scan_html(r#"<html><body><a href="/x" style="width: 20px">x</a></body></html>"#);
        let finding = only(&findings, "cro-tech-small-touch-target");
        assert_eq!(finding.severity, Severity::Medium);
        assert_eq!(finding.title, "Small touch target");
        assert_eq!(finding.selector.as_deref(), Some("a"));
        assert!(finding.description.contains("A clickable <a> element"));
        assert_eq!(
            finding.evidence.as_deref(),
            Some("via a11y-small-touch-target")
        );

        let large = scan_html(
            r#"<html><body><a href="/x" style="width: 100px; height: 100px">x</a></body></html>"#,
        );
        assert!(!has(&large, "cro-tech-small-touch-target"));
    }

    // -- footer -------------------------------------------------------------

    #[test]
    fn a_footer_without_a_cta_or_contact_is_flagged_twice() {
        let findings = scan_html("<html><body><footer><div>hello</div></footer></body></html>");
        let cta = only(&findings, "cro-footer-no-final-cta");
        assert_eq!(cta.severity, Severity::Low);
        assert_eq!(cta.title, "No closing CTA");
        assert_eq!(
            cta.description,
            "The bottom of the page has no call-to-action. Visitors who read to the \
             end are the warmest — give them a final way to convert."
        );
        let contact = only(&findings, "cro-footer-no-contact");
        assert_eq!(contact.severity, Severity::Low);
        assert_eq!(contact.title, "No contact/support link in footer");
    }

    #[test]
    fn a_footer_with_a_cta_and_a_contact_link_is_clean() {
        let findings = scan_html(
            r#"<html><body><footer>
                 <a href="mailto:hello@example.test">Mail us</a>
                 <button>Get started</button>
               </footer></body></html>"#,
        );
        assert!(!has(&findings, "cro-footer-no-final-cta"));
        assert!(!has(&findings, "cro-footer-no-contact"));
    }

    #[test]
    fn the_last_top_level_body_section_stands_in_for_a_missing_footer() {
        let findings = scan_html(
            r#"<html><body>
                 <div>first</div>
                 <div><button>Get started</button><a href="mailto:a@example.test">Mail</a></div>
               </body></html>"#,
        );
        assert!(!has(&findings, "cro-footer-no-final-cta"));
        assert!(!has(&findings, "cro-footer-no-contact"));
    }

    // -- ordering and the clean page ---------------------------------------

    #[test]
    fn findings_keep_the_source_module_order() {
        let findings = scan_html(
            "<html><body><form><input name=\"a\"><input name=\"b\"><input name=\"c\"><input name=\"d\"></form></body></html>",
        );
        let order = ids(&findings);
        let form = order
            .iter()
            .position(|id| id.starts_with("cro-form-"))
            .expect("form findings");
        let hero = order
            .iter()
            .position(|id| id.starts_with("cro-hero-"))
            .expect("hero findings");
        assert!(form < hero, "form module must run before hero: {order:?}");
    }

    #[test]
    fn a_clean_page_emits_no_findings() {
        let clean = r##"<!DOCTYPE html>
<html><head>
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Onboarding</title>
</head>
<body>
<header><nav><a href="#main">Skip</a></nav></header>
<main>
<h1>Cut onboarding time by 40% today</h1>
<p>We help teams ship faster. <a href="/signup" class="btn">Get my free trial</a> No credit card required.</p>
<h2>Why teams choose us</h2>
<ul><li>Faster setup</li><li>Clear reports</li><li>Happy teams</li></ul>
<div class="logos"><img src="/a.png" width="50" height="50" alt="A"><img src="/b.png" width="50" height="50" alt="B"><img src="/c.png" width="50" height="50" alt="C"></div>
<blockquote>Brilliant from day one. — Jane Doe, CEO</blockquote>
<div class="rating">5 stars</div>
<p>Trusted by 1,000 customers worldwide.</p>
<div class="faq">Questions? We have answers.</div>
<p>Every plan includes a money-back guarantee and privacy protection.</p>
</main>
<footer><a href="mailto:hello@example.com">Email us</a><button>Get started</button></footer>
</body></html>"##;
        let findings = scan_html(clean);
        assert!(
            findings.is_empty(),
            "a clean page must emit no findings, got {:?}",
            ids(&findings)
        );
    }
}
