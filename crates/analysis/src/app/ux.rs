//! Deterministic guards applied to LLM-authored UX claims before they are scored.
//!
//! Ported from the source's `src/domains/critique/services/scanners/ux_claims.py`
//! (`apply_page_guards` and the probes it consults). The UX reviewer is a language model; it once
//! ranked "No contact form or way to reach the business" critical on a page with `tel:` and
//! `mailto:` links, and listed praise as a finding. The guards act only on the structured fields
//! [`UxClaim`] carries, never on words in the title (which is how an earlier keyword filter
//! dropped "Password shown in clear text" as praise).
//!
//! The rules, in the source's order:
//!
//! - a `kind == "strength"` claim is dropped — it belonged in the strengths list, never the
//!   score — unless its severity is `high`/`critical`: praise is never that severe, so that
//!   conflict stays an issue;
//! - a `page` claim whose text carries a region qualifier ("above the fold", "pricing", "modal",
//!   en/nl) or whose evidence selector lands only inside a header/footer/dialog/pricing block is
//!   treated as `region`, which only ever PROTECTS it from the probe that would drop it;
//! - only a `page`-scoped claim with a known, non-`other` claim type is tested, against the ONE
//!   probe for that type. A `mailto:` link does not contradict "no contact form"; a region
//!   ("above the fold") or conditional ("zero-results state") claim is never auto-dropped, because
//!   the parsed page cannot see a fold or a state;
//! - a claim is verified when its evidence (a CSS selector that matches, or an exact text snippet
//!   found on the page) is present, or when its page-scoped absence claim is confirmed by its
//!   probe. An unverified claim keeps its severity except `critical`, which becomes `high`; the
//!   penalty lives in the caller's rubric, so this function only adjusts the severity.
//!
//! The contact probes are the source's: a phone number or email address — linked or written in
//! the visible text — or a real contact form disproves "no way to reach the business". A
//! newsletter, login, search or comment/reply/review form is not a contact form, and
//! IBAN/VAT/KvK/postcode digit runs are not phone numbers.
//!
//! Nothing here reads a clock, the network or the environment: two runs over the same document and
//! claims produce identical findings.
//!
//! Two mechanical adaptations are forced by this crate's dependencies. Rust's `regex` has no
//! lookaround, so the source's lookarounds are expressed as anchored captures plus explicit
//! boundary checks (same language, same matches for ASCII pages). Soupsieve selector matching is
//! `scraper`'s CSS engine; a selector one engine accepts and the other rejects is treated as "not
//! found", which is what the source did with a selector it could not parse.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::model::{Category, Document, Finding, Severity};

/// The source's `_llm_finding` stamps every LLM UX finding with this tool name.
const SOURCE_TOOL: &str = "ux_reviewer";

/// An exact-text snippet shorter than this ("Go", "Home") proves nothing.
const MIN_SNIPPET_CHARS: usize = 8;

/// A KvK is 8 digits and a VAT/RSIN 9 — fewer than 6 is not an identifier on its own.
const MIN_IDENTIFIER_DIGITS: usize = 6;

/// Input types a visitor does not type into.
const NON_ENTRY_TYPES: [&str; 7] = [
    "hidden", "submit", "button", "image", "reset", "checkbox", "radio",
];

/// Tags that can head a form (`_HEADING_TAGS`).
const HEADING_TAGS: [&str; 7] = ["h1", "h2", "h3", "h4", "h5", "h6", "legend"];

/// Elements that ARE a region: a selector landing inside one points at a region.
const REGION_TAGS: [&str; 3] = ["header", "footer", "dialog"];

/// The URL paths the source's `is_homepage_url` calls a site root.
const HOMEPAGE_PATHS: [&str; 5] = ["", "/", "/index.html", "/index.htm", "/index.php"];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex compiles")
}

fn ci(pattern: &str) -> Regex {
    Regex::new(&format!("(?i){pattern}")).expect("static regex compiles")
}

// --- contact form ------------------------------------------------------------

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| re(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+"));
static SEARCH_NAME_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"^(?:q|s|search|query)$"));
static MESSAGE_FIELD_RE: LazyLock<Regex> =
    LazyLock::new(|| ci(r"message|bericht|vraag|opmerking|toelichting|enquiry|inquiry"));
/// The source's `_NAME_FIELD_RE`, minus its two `user` lookbehinds, which
/// [`name_field_matches`] applies by hand.
static NAME_FIELD_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"name|naam"));
static EMAIL_FIELD_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"e-?mail"));

/// A submit label that says "contact us" -- including asking for a quote, an appointment or a
/// call back (en/nl) -- not "subscribe" or "log in".
static CONTACT_SUBMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"contact|get in touch|\bsend\b|verstuur|versturen|verzend|bericht|terugbel|bel (?:mij|me|ons) (?:terug|op)|call ?back|call me|offerte|\bquote\b|aanvra(?:ag|gen)|afspraak|appointment|gesprek|\b(?:book|schedule|plan) (?:a |an |een )?(?:call|meeting|demo|consult)",
    )
});

/// A submit label that signs up, logs in or searches. It vetoes the submit-only path even when it
/// borrows a contact word: "Nieuwsbrief aanvragen" is a sign-up.
static NON_CONTACT_SUBMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"nieuwsbrief|newsletter|subscribe|abonne|aanmeld|meld (?:je |u )?aan|inschrij|schrijf (?:je |u )?in|sign ?up|register|registre|log ?in|inlog|sign ?in|\bsearch\b|\bzoek",
    )
});

/// Words that say a form is FOR contacting the business (en/nl).
static CONTACT_INTENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"contact|get in touch|bericht|vraag|offerte|quote|message|enquiry|inquiry|\bsend\b|verstuur|versturen|verzend|terugbel|call ?back",
    )
});

/// A form that searches, or posts a comment/reply/review, is not a contact form.
static SEARCH_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"search|zoek"));
/// The source's `_FEEDBACK_MARKER_RE` without `review`; the `(?<![a-z])review` alternative is
/// applied by [`feedback_marker_matches`], since `regex` has no lookbehind.
static FEEDBACK_MARKER_RE: LazyLock<Regex> =
    LazyLock::new(|| ci(r"comment|repl(?:y|ies)|respond|reactie|reageer|recensie|beoordel"));
static REVIEW_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"review"));
static FEEDBACK_SUBMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"post (?:a |your )?(?:comment|reply|review)|\breply\b|submit (?:a |your )?review|reageer|plaats (?:een |je |uw )?(?:reactie|review|recensie)|review (?:plaatsen|schrijven|versturen)|beoordeling",
    )
});
static SEARCH_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"\b(?:search|zoek)"));
static SEARCH_SUBMIT_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"^\s*(?:search|zoek(?:en)?)\b"));

/// WordPress/Disqus comment plumbing: a hidden field that only a comment form carries.
static FEEDBACK_FIELD_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(r"^(?:comment|comments|reply|review|rating|comment_post_id|comment_parent|replytocom)$")
});

// --- phone numbers -----------------------------------------------------------

/// Business identifiers whose digits parse as a phone number once separators go:
/// "NL91 ABNA 0417 1643 00" leaves 0417164300, a well-formed Dutch number.
static IBAN_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){2,7}(?:\s?[A-Z0-9]{1,3})?\b"));
static VAT_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"\b[A-Z]{2}\s?\d{9}\s?B\s?\d{2}\b"));
static LABELLED_IDENTIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"\b(?:kvk|k\.v\.k\.?|coc|chamber of commerce|btw|vat|rsin|iban|bic|swift|ondernemingsnummer|handelsregister)(?:[\s.-]*(?:nummer|nr|no|number|id))?\.?\s*[:#]?\s*[A-Z]{0,2}[\d][\d .]{5,32}\d(?:\s?B\s?\d{2})?",
    )
});
static POSTCODE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"\b\d{4}\s?[A-Z]{2}\b"));

/// The source's `_PHONE_CANDIDATE_RE`. Its lookbehind and lookahead are the `(?:^|[^\w+])`
/// prefix and the [`is_word_char`] check on the character after the capture.
static PHONE_CANDIDATE_RE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|[^\w+])((?:\+|00)?\(?[0-9][0-9\s().-]{7,17}[0-9])"));
static PHONE_FORMAT_NATIONAL_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^0[1-9][0-9]{8}$"));
static PHONE_FORMAT_INTL_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\+31[1-9][0-9]{8}$"));
static PHONE_FORMAT_E164_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^\+[1-9][0-9]{7,13}$"));

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

// --- scope -------------------------------------------------------------------

/// Words that confine a claim to part of the page (en/nl). "No phone number above the fold" says
/// nothing about the footer, so it is never a page-level absence.
static REGION_QUALIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"above the fold|below the fold|boven de vouw|onder de vouw|\bhero\b|\bheader\b|\bfooter\b|\bmodal\b|\bpop-?up\b|\bdialog\b|pricing|\bprijzen|\bprijs|tarieven|checkout|check-out|afrekenen|\bkassa\b|mobile (?:menu|nav)|mobiele? menu|hamburger",
    )
});
static REGION_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| ci(r"modal|pricing|popup|dialog|prijzen|tarieven"));

/// An exact-text snippet that is only a tag name ("button") points at nothing.
static BARE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| re(r"^[a-zA-Z][\w-]*$"));

static BREADCRUMB_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"breadcrumb"));

/// One claim the UX reviewer made, with the structured fields the source's `output_schema` asks
/// for — never the free text of the title.
#[derive(Clone, Debug)]
pub struct UxClaim {
    /// `"issue"` or `"strength"`.
    pub kind: String,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    /// The source's claim-type vocabulary; `"other"` when unknown.
    pub claim_type: String,
    /// `"page"`, `"region"` or `"conditional"`.
    pub scope: String,
    /// CSS selector or exact text the reviewer cited.
    pub evidence: Option<String>,
    pub selector: Option<String>,
}

/// Apply the source's guards to LLM-authored claims. Returns only the claims that remain issues
/// (strengths are dropped, exactly as the source dropped them from scoring).
pub fn guard(document: &Document, claims: &[UxClaim]) -> Vec<Finding> {
    let html = Html::parse_document(&document.html);
    let text = visible_text(&html);
    let homepage = is_homepage_url(&document.url);
    let mut findings = Vec::new();

    for claim in claims {
        if claim.kind == "strength"
            && !matches!(claim.severity, Severity::Critical | Severity::High)
        {
            // Praise at medium and below is a strength, never a scored issue.
            continue;
        }

        // The source narrows a page claim to a region before deciding whether to probe it. This
        // only suppresses the probe; it never drops the claim.
        let page_scoped = claim.scope == "page" && !claims_a_region(claim, &html);
        let mut absence_confirmed = false;
        if let Some(probe) = claim_probe(&claim.claim_type).filter(|_| page_scoped) {
            if homepage && claim.claim_type == "missing_breadcrumbs" {
                // Breadcrumbs have no meaning on a site root.
                continue;
            }
            if probe(&html) {
                // The claimed-absent thing is on the page: the claim is contradicted.
                continue;
            }
            absence_confirmed = true;
        }

        let verified = absence_confirmed || evidence_found(claim.evidence.as_deref(), &html, &text);
        let severity = if !verified && claim.severity == Severity::Critical {
            Severity::High
        } else {
            claim.severity
        };

        findings.push(Finding {
            check_id: None,
            category: Category::Ux,
            severity,
            title: claim.title.clone(),
            description: claim.description.clone(),
            recommendation: claim.recommendation.clone(),
            url: document.url.clone(),
            selector: claim.selector.clone(),
            evidence: claim.evidence.clone(),
            source_tool: Some(SOURCE_TOOL.to_owned()),
            scanner_count: 1,
            confirmed_by: Vec::new(),
        });
    }

    findings
}

/// claim_type -> the one probe answering "is the claimed-absent thing present on the page?".
/// A claim is never contradicted by a DIFFERENT subject, so an unknown type gets no probe.
fn claim_probe(claim_type: &str) -> Option<fn(&Html) -> bool> {
    match claim_type {
        "missing_contact_form" => Some(has_contact_form),
        "missing_contact_method" => Some(has_contact_method),
        "missing_phone" => Some(has_phone),
        "missing_email" => Some(has_email),
        "missing_search" => Some(has_search),
        "missing_breadcrumbs" => Some(has_breadcrumbs),
        _ => None,
    }
}

/// `is_homepage_url`: true only for a URL with a host and a site-root path. A URL with no host (a
/// static artifact, or a bare path) is NOT assumed to be a homepage — unknown never suppresses a
/// finding.
fn is_homepage_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let has_host = parsed.host_str().is_some_and(|host| !host.is_empty());
    has_host && HOMEPAGE_PATHS.contains(&parsed.path().to_lowercase().as_str())
}

/// The finding's text or evidence confines it to one region of the page.
fn claims_a_region(claim: &UxClaim, html: &Html) -> bool {
    let text = format!(
        "{} {} {}",
        claim.title,
        claim.description,
        claim.evidence.as_deref().unwrap_or("")
    );
    if REGION_QUALIFIER_RE.is_match(&text) {
        return true;
    }
    let evidence = normalize_whitespace(claim.evidence.as_deref().unwrap_or(""));
    if evidence.is_empty() || BARE_TAG_RE.is_match(&evidence) {
        return false;
    }
    let Ok(selector) = Selector::parse(&evidence) else {
        return false;
    };
    let matched: Vec<ElementRef<'_>> = html.select(&selector).collect();
    !matched.is_empty() && matched.into_iter().all(in_region)
}

/// Whether an element sits inside a header, footer, dialog or modal/pricing/pop-up block.
fn in_region(element: ElementRef<'_>) -> bool {
    let mut current = Some(element);
    while let Some(node) = current {
        if REGION_TAGS.contains(&node.value().name()) || node.attr("role") == Some("dialog") {
            return true;
        }
        let attrs = format!(
            "{} {}",
            node.attr("id").unwrap_or(""),
            node.attr("class").unwrap_or("")
        );
        if REGION_ATTR_RE.is_match(&attrs) {
            return true;
        }
        current = node.parent().and_then(ElementRef::wrap);
    }
    false
}

/// `evidence` is a CSS selector that matches, or a text snippet on the page.
fn evidence_found(evidence: Option<&str>, html: &Html, text: &str) -> bool {
    let snippet = normalize_whitespace(evidence.unwrap_or(""));
    if snippet.chars().count() >= MIN_SNIPPET_CHARS {
        let haystack = normalize_whitespace(text).to_lowercase();
        if haystack.contains(&snippet.to_lowercase()) {
            return true;
        }
    }
    if BARE_TAG_RE.is_match(&snippet) {
        // "button" matches any button — it points at nothing.
        return false;
    }
    if snippet.is_empty() {
        return false;
    }
    match Selector::parse(&snippet) {
        Ok(selector) => html.select(&selector).next().is_some(),
        Err(_) => false,
    }
}

// --- document helpers --------------------------------------------------------

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The page's visible text: every text node except those inside script/style/noscript/template,
/// each trimmed, joined with a space — the source's `get_text(" ", strip=True)`.
fn visible_text(html: &Html) -> String {
    let mut parts = Vec::new();
    collect_visible_text(html.root_element(), &mut parts);
    parts.join(" ")
}

fn collect_visible_text(element: ElementRef<'_>, parts: &mut Vec<String>) {
    for child in element.children() {
        match child.value() {
            scraper::Node::Text(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_owned());
                }
            }
            scraper::Node::Element(element) => {
                if matches!(element.name(), "script" | "style" | "noscript" | "template") {
                    continue;
                }
                if let Some(element) = ElementRef::wrap(child) {
                    collect_visible_text(element, parts);
                }
            }
            _ => {}
        }
    }
}

/// All descendants of the root element, in document order.
fn descendent_elements<'a>(html: &'a Html) -> impl Iterator<Item = ElementRef<'a>> + 'a {
    html.root_element().descendent_elements()
}

/// The first element in document order matching `predicate`.
fn first_element<'a>(
    html: &'a Html,
    predicate: impl Fn(&ElementRef<'a>) -> bool,
) -> Option<ElementRef<'a>> {
    descendent_elements(html).find(|element| predicate(element))
}

/// The source's `get_text(" ", strip=True)` over one element.
fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_heading(name: &str) -> bool {
    HEADING_TAGS.contains(&name)
}

/// Every `href`, stripped and lowercased — the shape `_hrefs` produced.
fn hrefs(html: &Html) -> Vec<String> {
    descendent_elements(html)
        .filter(|element| element.value().name() == "a")
        .filter_map(|element| element.attr("href"))
        .map(|href| href.trim().to_lowercase())
        .collect()
}

// --- form probing ------------------------------------------------------------

/// A form a visitor uses to CONTACT the business.
///
/// Never a login (password), a newsletter (a lone email), a search, or a comment/reply/review
/// form. A free-text field alone is not enough — a WordPress comment box has one — so a form
/// qualifies when it says it is for contact (submit label, heading, aria/name, action or a field
/// label says contact/message/send/vraag/offerte, en/nl), or it takes a name, an email and a
/// free-text message.
fn is_contact_form<'a>(form: ElementRef<'a>, html: &'a Html) -> bool {
    let mut fields: Vec<ElementRef<'a>> = Vec::new();
    for field in form.descendent_elements() {
        let tag = field.value().name();
        if !matches!(tag, "input" | "textarea" | "select") {
            continue;
        }
        let kind = if tag == "input" {
            field.attr("type").unwrap_or("text").to_lowercase()
        } else {
            tag.to_owned()
        };
        if kind == "password" {
            return false;
        }
        if !NON_ENTRY_TYPES.contains(&kind.as_str()) {
            fields.push(field);
        }
    }
    if fields.is_empty() {
        return false;
    }

    let submit = submit_labels(form);
    let labels: Vec<String> = fields
        .iter()
        .map(|field| field_label(*field, html))
        .collect();
    if is_search_form(form, &fields, &labels, &submit) || is_feedback_form(form, &submit) {
        return false;
    }

    let is_email: Vec<bool> = fields
        .iter()
        .zip(labels.iter())
        .map(|(field, label)| {
            field
                .attr("type")
                .is_some_and(|kind| kind.to_lowercase() == "email")
                || EMAIL_FIELD_RE.is_match(label)
        })
        .collect();
    if fields.len() == 1 && is_email[0] {
        // Newsletter sign-up.
        return false;
    }

    let has_name = labels.iter().any(|label| name_field_matches(label));
    let message_fields: Vec<&String> = fields
        .iter()
        .zip(labels.iter())
        .filter(|(field, label)| {
            field.value().name() == "textarea" || MESSAGE_FIELD_RE.is_match(label)
        })
        .map(|(_, label)| label)
        .collect();

    if !message_fields.is_empty() {
        let mut context = vec![
            submit.clone(),
            form_identity(form),
            form.attr("action").unwrap_or("").to_owned(),
            form_heading(form, html),
        ];
        context.extend(message_fields.into_iter().cloned());
        let context = context.join(" ");
        return CONTACT_INTENT_RE.is_match(&context) || (has_name && is_email.iter().any(|x| *x));
    }

    has_name
        && is_email.iter().any(|x| *x)
        && CONTACT_SUBMIT_RE.is_match(&submit)
        && !NON_CONTACT_SUBMIT_RE.is_match(&submit)
}

/// A `form` that [`is_contact_form`] accepts.
fn has_contact_form(html: &Html) -> bool {
    descendent_elements(html)
        .filter(|element| element.value().name() == "form")
        .any(|form| is_contact_form(form, html))
}

fn is_search_form<'a>(
    form: ElementRef<'a>,
    entry_fields: &[ElementRef<'a>],
    labels: &[String],
    submit: &str,
) -> bool {
    if form.attr("role") == Some("search")
        || form
            .descendent_elements()
            .any(|el| el.value().name() == "search")
        || {
            let mut current = form.parent().and_then(ElementRef::wrap);
            let mut inside = false;
            while let Some(ancestor) = current {
                if ancestor.value().name() == "search" {
                    inside = true;
                    break;
                }
                current = ancestor.parent().and_then(ElementRef::wrap);
            }
            inside
        }
    {
        return true;
    }
    if form
        .descendent_elements()
        .any(|el| el.value().name() == "input" && el.attr("type") == Some("search"))
    {
        return true;
    }
    let identity = format!(
        "{} {}",
        form_identity(form),
        form.attr("action").unwrap_or("")
    );
    if SEARCH_MARKER_RE.is_match(&identity) {
        return true;
    }
    let names: Vec<&str> = entry_fields
        .iter()
        .map(|field| {
            field
                .attr("name")
                .or_else(|| field.attr("id"))
                .unwrap_or("")
        })
        .collect();
    if !names.is_empty() && names.iter().all(|name| SEARCH_NAME_RE.is_match(name)) {
        return true;
    }
    if labels.iter().any(|label| SEARCH_LABEL_RE.is_match(label)) {
        // placeholder="Search messages" is a search box, not a message field.
        return true;
    }
    SEARCH_SUBMIT_RE.is_match(submit)
}

/// A comment, reply or review form (WordPress `wp-comments-post.php` and kin).
fn is_feedback_form<'a>(form: ElementRef<'a>, submit: &str) -> bool {
    let identity = format!(
        "{} {}",
        form_identity(form),
        form.attr("action").unwrap_or("")
    );
    if feedback_marker_matches(&identity) {
        return true;
    }
    if FEEDBACK_SUBMIT_RE.is_match(submit) {
        return true;
    }
    form.descendent_elements().any(|field| {
        matches!(field.value().name(), "input" | "textarea" | "select")
            && field
                .attr("name")
                .is_some_and(|name| FEEDBACK_FIELD_NAME_RE.is_match(name))
            || field
                .attr("id")
                .is_some_and(|id| FEEDBACK_FIELD_NAME_RE.is_match(id))
    })
}

/// `_FEEDBACK_MARKER_RE`: the plain alternatives, plus `review` only when it is not preceded by a
/// letter (the source's `(?<![a-z])review`, case-insensitive).
fn feedback_marker_matches(value: &str) -> bool {
    if FEEDBACK_MARKER_RE.is_match(value) {
        return true;
    }
    REVIEW_RE.find_iter(value).any(|matched| {
        value[..matched.start()]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_ascii_alphabetic())
    })
}

/// `_NAME_FIELD_RE`: `name` unless preceded by `user`, `user_` or `user-` (en), or `naam`.
fn name_field_matches(label: &str) -> bool {
    NAME_FIELD_RE.find_iter(label).any(|matched| {
        if matched.as_str().to_lowercase() == "naam" {
            return true;
        }
        let before = label[..matched.start()].to_lowercase();
        !(before.ends_with("user") || before.ends_with("user_") || before.ends_with("user-"))
    })
}

/// Everything that names a form field: attributes and its `<label>`.
fn field_label<'a>(field: ElementRef<'a>, html: &'a Html) -> String {
    let mut parts: Vec<String> = [
        "name",
        "id",
        "autocomplete",
        "placeholder",
        "aria-label",
        "title",
    ]
    .iter()
    .map(|attr| field.attr(attr).unwrap_or("").to_owned())
    .collect();
    if let Some(label) = field.attr("id").filter(|id| !id.is_empty()).and_then(|id| {
        first_element(html, |element| {
            element.value().name() == "label" && element.attr("for") == Some(id)
        })
    }) {
        parts.push(element_text(label));
    }
    let mut current = field.parent().and_then(ElementRef::wrap);
    while let Some(ancestor) = current {
        if ancestor.value().name() == "label" {
            parts.push(element_text(ancestor));
            break;
        }
        current = ancestor.parent().and_then(ElementRef::wrap);
    }
    parts.join(" ")
}

fn submit_labels(form: ElementRef<'_>) -> String {
    let mut labels = Vec::new();
    for element in form.descendent_elements() {
        let tag = element.value().name();
        if !matches!(tag, "button" | "input") {
            continue;
        }
        let default = if tag == "button" { "submit" } else { "text" };
        let kind = element.attr("type").unwrap_or(default).to_lowercase();
        if kind == "submit" || kind == "image" {
            labels.push(element_text(element));
            labels.push(element.attr("value").unwrap_or("").to_owned());
            labels.push(element.attr("aria-label").unwrap_or("").to_owned());
        }
    }
    labels.join(" ")
}

/// The form's own name: id, class, name, aria-label, title.
fn form_identity(form: ElementRef<'_>) -> String {
    let mut parts: Vec<&str> = ["id", "name", "aria-label", "title"]
        .into_iter()
        .map(|attr| form.attr(attr).unwrap_or(""))
        .collect();
    parts.push(form.attr("class").unwrap_or(""));
    parts.join(" ")
}

/// The legend inside the form, its `aria-labelledby` target, or the heading before it.
fn form_heading<'a>(form: ElementRef<'a>, html: &'a Html) -> String {
    let mut parts = Vec::new();
    if let Some(labelledby) = form.attr("aria-labelledby") {
        for reference in labelledby.split_whitespace() {
            if let Some(target) =
                first_element(html, |element| element.attr("id") == Some(reference))
            {
                parts.push(element_text(target));
            }
        }
    }
    let heading = form
        .descendent_elements()
        .find(|element| is_heading(element.value().name()))
        .or_else(|| previous_heading(form, html));
    if let Some(heading) = heading {
        parts.push(element_text(heading));
    }
    parts.join(" ")
}

/// The nearest element before `form` in document order whose tag is a heading.
fn previous_heading<'a>(form: ElementRef<'a>, html: &'a Html) -> Option<ElementRef<'a>> {
    let mut previous = None;
    for element in descendent_elements(html) {
        if element == form {
            break;
        }
        if is_heading(element.value().name()) {
            previous = Some(element);
        }
    }
    previous
}

// --- phone and email probing -------------------------------------------------

fn normalize_phone(candidate: &str) -> String {
    let without_zero = candidate.replace("(0)", "");
    let plus = {
        let trimmed = without_zero.trim_start();
        trimmed.starts_with('+') || trimmed.starts_with("00")
    };
    let mut digits: String = without_zero
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if plus {
        if candidate.trim_start().starts_with("00") {
            digits = digits.chars().skip(2).collect();
        }
        digits.insert(0, '+');
    }
    digits
}

fn is_phone(candidate: &str) -> bool {
    let normalized = normalize_phone(candidate);
    PHONE_FORMAT_NATIONAL_RE.is_match(&normalized)
        || PHONE_FORMAT_INTL_RE.is_match(&normalized)
        || (!normalized.starts_with("+31") && PHONE_FORMAT_E164_RE.is_match(&normalized))
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Any phone candidate in `text` that normalises to a valid number. The lookahead of the source's
/// `_PHONE_CANDIDATE_RE` (`(?!\w)`) is the check on the character after the capture.
fn has_phone_candidate(text: &str) -> bool {
    PHONE_CANDIDATE_RE.captures_iter(text).any(|captures| {
        let Some(candidate) = captures.get(1) else {
            return false;
        };
        let trailing_is_word = text[candidate.end()..]
            .chars()
            .next()
            .is_some_and(is_word_char);
        !trailing_is_word && is_phone(candidate.as_str())
    })
}

/// Blank an identifier span, but hand back a valid phone number glued to its end.
///
/// "KvK 12345678 010 123 4567" matches as one digit run; tokenised at each space, the tail
/// "010 123 4567" is a phone number and "12345678" is still a whole KvK, so the tail survives.
fn strip_identifier(matched: &str) -> String {
    for whitespace in WHITESPACE_RE.find_iter(matched) {
        let head = &matched[..whitespace.start()];
        let tail = matched[whitespace.start()..].trim();
        let head_digits = head.chars().filter(|c| c.is_ascii_digit()).count();
        if head_digits >= MIN_IDENTIFIER_DIGITS && is_phone(tail) {
            return format!(" | {tail} ");
        }
    }
    " | ".to_owned()
}

/// Remove IBAN/VAT/KvK/postcode spans, in the source's order, from the visible text.
fn strip_identifiers(text: &str) -> String {
    let mut current = text.to_owned();
    for identifier in [&*IBAN_RE, &*VAT_RE, &*LABELLED_IDENTIFIER_RE, &*POSTCODE_RE] {
        current = identifier
            .replace_all(&current, |captures: &regex::Captures<'_>| {
                strip_identifier(captures.get(0).map(|m| m.as_str()).unwrap_or(""))
            })
            .into_owned();
    }
    current
}

/// A `tel:` link, or a well-formed NL/international number in the visible text.
fn has_phone(html: &Html) -> bool {
    if hrefs(html).iter().any(|href| href.starts_with("tel:")) {
        return true;
    }
    let text = strip_identifiers(&visible_text(html));
    has_phone_candidate(&text)
}

/// A `mailto:` link, or an email address in the visible text.
fn has_email(html: &Html) -> bool {
    hrefs(html).iter().any(|href| href.starts_with("mailto:"))
        || EMAIL_RE.is_match(&visible_text(html))
}

fn has_search(html: &Html) -> bool {
    descendent_elements(html).any(|element| {
        (element.value().name() == "input" && element.attr("type") == Some("search"))
            || element.attr("role") == Some("search")
            || element.value().name() == "search"
            || (element.value().name() == "input"
                && element
                    .attr("name")
                    .is_some_and(|name| SEARCH_NAME_RE.is_match(name)))
    })
}

fn jsonld_has_breadcrumbs(html: &Html) -> bool {
    for script in descendent_elements(html).filter(|element| {
        element.value().name() == "script" && element.attr("type") == Some("application/ld+json")
    }) {
        let raw: String = script.text().collect();
        if !raw.contains("BreadcrumbList") {
            continue;
        }
        if serde_json::from_str::<serde_json::Value>(&raw).is_ok() {
            return true;
        }
    }
    false
}

fn has_breadcrumbs(html: &Html) -> bool {
    descendent_elements(html).any(|element| {
        element
            .attr("aria-label")
            .is_some_and(|label| BREADCRUMB_RE.is_match(label))
            || element
                .attr("class")
                .is_some_and(|class| BREADCRUMB_RE.is_match(class))
    }) || jsonld_has_breadcrumbs(html)
}

/// A way to reach the business: a phone number, an email address, or a contact form. A "Contact"
/// nav link, a postal address, business identifiers, a newsletter/login/search/comment form do
/// not disprove "no way to reach the business".
fn has_contact_method(html: &Html) -> bool {
    has_phone(html) || has_email(html) || has_contact_form(html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, Document, Severity};

    fn doc(html: &str) -> Document {
        Document::new("https://example.com/contact", html)
    }

    fn page_claim(claim_type: &str, severity: Severity) -> UxClaim {
        UxClaim {
            kind: "issue".to_owned(),
            severity,
            title: "A UX claim".to_owned(),
            description: "Something the reviewer saw.".to_owned(),
            recommendation: "Fix it.".to_owned(),
            claim_type: claim_type.to_owned(),
            scope: "page".to_owned(),
            evidence: None,
            selector: None,
        }
    }

    #[test]
    fn phone_and_mailto_links_disprove_the_contact_probes() {
        let html = r#"<html><body>
            <a href="tel:+31201234567">Call us</a>
            <a href="mailto:hello@example.com">Mail us</a>
        </body></html>"#;
        let claims = [
            page_claim("missing_phone", Severity::Critical),
            page_claim("missing_email", Severity::Critical),
            page_claim("missing_contact_method", Severity::Critical),
        ];
        assert!(guard(&doc(html), &claims).is_empty());
    }

    #[test]
    fn written_phone_and_email_count_but_business_identifiers_do_not() {
        let written = r#"<p>Bel ons: 020 123 4567 of mail hello@example.com</p>"#;
        assert!(
            guard(
                &doc(written),
                &[page_claim("missing_contact_method", Severity::Critical)]
            )
            .is_empty()
        );

        let identifiers =
            r#"<p>KvK 12345678 &middot; IBAN NL91 ABNA 0417 1643 00 &middot; Postcode 1234 AB</p>"#;
        let findings = guard(
            &doc(identifiers),
            &[page_claim("missing_phone", Severity::Critical)],
        );
        assert_eq!(findings.len(), 1);
        // The probe found no phone: the page-scoped absence claim is verified and keeps its
        // severity. The identifier digits did not masquerade as a phone number.
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn a_newsletter_form_is_not_a_contact_form() {
        let newsletter = r#"<form action="/newsletter">
            <input type="email" name="email" placeholder="Your email">
            <input type="submit" value="Subscribe">
        </form>"#;
        let findings = guard(
            &doc(newsletter),
            &[page_claim("missing_contact_form", Severity::Critical)],
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Critical);
        assert_eq!(findings[0].category, Category::Ux);
        assert_eq!(findings[0].source_tool.as_deref(), Some("ux_reviewer"));
        assert_eq!(findings[0].url, "https://example.com/contact");

        let contact = r#"<form action="/contact">
            <input name="name">
            <input type="email" name="email">
            <textarea name="message"></textarea>
            <button type="submit">Send</button>
        </form>"#;
        assert!(
            guard(
                &doc(contact),
                &[page_claim("missing_contact_form", Severity::Critical)]
            )
            .is_empty()
        );
    }

    #[test]
    fn a_probe_only_answers_its_own_claim() {
        let html = r#"<a href="mailto:hello@example.com">Mail us</a>"#;
        // A mailto link does not contradict "no contact form"; the absence probe confirms it.
        let findings = guard(
            &doc(html),
            &[page_claim("missing_contact_form", Severity::Critical)],
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Critical);
    }

    #[test]
    fn a_high_strength_stays_an_issue_and_a_lesser_strength_is_dropped() {
        let mut high = page_claim("other", Severity::High);
        high.kind = "strength".to_owned();
        let findings = guard(&doc("<p>Hello world</p>"), &[high]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);

        let mut medium = page_claim("other", Severity::Medium);
        medium.kind = "strength".to_owned();
        assert!(guard(&doc("<p>Hello world</p>"), &[medium]).is_empty());
    }

    #[test]
    fn a_region_qualified_claim_is_never_dropped() {
        let html = r#"<html><body>
            <header>No phone here</header>
            <footer><p class="phone">Telefoon: 020 123 4567</p></footer>
        </body></html>"#;

        let mut qualified = page_claim("missing_phone", Severity::Critical);
        qualified.title = "No phone number above the fold".to_owned();
        let findings = guard(&doc(html), &[qualified]);
        assert_eq!(findings.len(), 1); // the footer phone cannot contradict a fold-scoped claim
        assert_eq!(findings[0].severity, Severity::High); // unverified, critical -> high

        let mut selected = page_claim("missing_phone", Severity::Critical);
        selected.title = "No phone number on the page".to_owned();
        selected.evidence = Some("p.phone".to_owned());
        let findings = guard(&doc(html), &[selected]);
        assert_eq!(findings.len(), 1); // the selector lands only inside a region
        assert_eq!(findings[0].severity, Severity::Critical); // its evidence was found
    }

    #[test]
    fn verified_claims_keep_their_severity_and_unverified_critical_becomes_high() {
        let html = "<p>Hello world, welcome to our bakery.</p>";

        let mut verified = page_claim("other", Severity::Critical);
        verified.evidence = Some("Hello world".to_owned());
        let findings = guard(&doc(html), &[verified]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Critical);

        let mut unverified = page_claim("other", Severity::Critical);
        unverified.evidence = Some("This sentence is absent".to_owned());
        let findings = guard(&doc(html), &[unverified]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn a_clean_set_produces_no_findings() {
        assert!(guard(&doc("<p>Hello</p>"), &[]).is_empty());
    }
}
