//! The marketing scanner — the source's seventeen checks over a supplied document.
//!
//! A port of `src/domains/critique/services/scanners/marketing/`: `MarketingScanner.scan` runs the
//! competitive (3), brand (7), growth (5) and tracking (2) families, in that order. Every check is
//! absence detection — a page that presents itself well emits nothing — and every finding is
//! `category=marketing`, `source_tool=marketing_scanner`, in the source's `mkt-<dimension>-<check>`
//! namespace.
//!
//! `facts.py` and `scoring.py` are response assembly outside `scan()` (the facts block and the
//! six-dimension composite): `content.rs` scores these findings through the shared rubric and the
//! crate has no facts model. `checks.py` is the inventory — all seventeen ids below are in it.
//! The source's `false_positive_risk` / `remediation_effort` metadata has no field on
//! [`Finding`] and is not carried.

use std::sync::LazyLock;

use regex::{Captures, Regex};
use scraper::{ElementRef, Html, Selector};

use crate::model::{Category, Document, Finding, Severity};

const SOURCE_TOOL: &str = "marketing_scanner";

// --------------------------------------------------------------------------
// Compiled detectors. The patterns are the source's, character for character,
// except where the Rust `regex` crate has no look-around (see the phone block).
// --------------------------------------------------------------------------

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

fn ci(pattern: &str) -> Regex {
    Regex::new(&format!("(?i){pattern}")).expect("static regex")
}

fn sel(selector: &str) -> Selector {
    Selector::parse(selector).expect("static selector")
}

static COMPARISON_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"\b(vs\.?|versus|compare|comparison|alternative to|alternatives?|why choose|switch from|migrate from)\b",
    )
});

static DIFFERENTIATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"\b(only|unlike|first|leading|#\s*1|number one|best|most|fastest|unique|patented|proprietary|purpose-built|world[- ]class)\b",
    )
});

static REVIEW_PLATFORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"\bg2\b|capterra|trustpilot|trustradius|gartner|forrester|producthunt|product hunt|clutch\.co|glassdoor",
    )
});

static MEDIA_MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"forbes|techcrunch|\bwsj\b|wall street journal|as seen in|featured in|bloomberg|the verge|mashable|entrepreneur",
    )
});

static PRICE_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"[$£€]\s?\d|\bfree\b|/\s?(mo|month|yr|year)\b|per (month|year|user|seat)|\bpricing\b|\bplans?\b|\btiers?\b",
    )
});

static GROWTH_LOOP_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"refer(ral)?|affiliate|invite (a )?friend|share and|earn credit|reward(s)?|ambassador|partner program",
    )
});

static RETENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"newsletter|subscribe|sign up for|community|slack|discord|forum|\bblog\b|updates|changelog|webinar|academy|resources",
    )
});

static ABOUT_RE: LazyLock<Regex> =
    LazyLock::new(|| ci(r"about|our team|company|mission|our story|who we are|meet the team"));

static LOGO_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"logo|brand[- ]?mark|wordmark"));

static CONTACT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"mailto:|tel:|\bcontact us\b|\bget in touch\b|\b\d{1,4}\s+\w+\s+(street|st\.?|avenue|ave\.?|road|rd\.?|blvd)\b",
    )
});

/// Growth: analytics / tag-manager / pixel platforms. One row per vendor so the alternation can
/// be rebuilt from the same table the facts block used.
const ANALYTICS_VENDORS: [(&str, &[&str]); 14] = [
    (
        "Google Analytics",
        &[r"gtag\(", r"google-analytics\.com", r"ga\('create'"],
    ),
    (
        "Google Tag Manager",
        &[r"googletagmanager\.com", r"dataLayer"],
    ),
    ("Meta Pixel", &[r"fbq\(", r"connect\.facebook\.net"]),
    (
        "LinkedIn Insight",
        &[r"_linkedin_partner_id", r"snap\.licdn\.com"],
    ),
    ("Hotjar", &[r"static\.hotjar\.com", r"\bhj\("]),
    ("FullStory", &[r"fullstory\.com", r"\bFS\.identify"]),
    ("Microsoft Clarity", &[r"clarity\.ms", r"clarity\("]),
    (
        "Segment",
        &[r"segment\.com/analytics\.js", r"analytics\.track\("],
    ),
    ("Mixpanel", &[r"mixpanel"]),
    ("PostHog", &[r"posthog"]),
    ("Plausible", &[r"plausible\.io"]),
    ("Matomo", &[r"matomo"]),
    ("Amplitude", &[r"amplitude"]),
    ("Heap", &[r"heap\.io", r"\bheap\."]),
];

static ANALYTICS_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = ANALYTICS_VENDORS
        .iter()
        .flat_map(|(_, patterns)| patterns.iter().copied())
        .collect::<Vec<_>>()
        .join("|");
    ci(&alternatives)
});

static CONSENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"cookieconsent|cookie[- ]?consent|onetrust|cookiebot|osano|termly|iubenda|usercentrics|gdpr|\bconsent\b|cookie (policy|preferences|settings)|\baccept (all )?cookies\b",
    )
});

/// Growth: owned social profiles. One row per platform, as the source's facts table had.
const SOCIAL_PLATFORMS: [(&str, &str); 8] = [
    ("x", r"(twitter|x)\.com/"),
    ("linkedin", r"linkedin\.com/(company|in)/"),
    ("facebook", r"facebook\.com/"),
    ("instagram", r"instagram\.com/"),
    ("youtube", r"youtube\.com/(c/|channel/|@)"),
    ("github", r"github\.com/"),
    ("tiktok", r"tiktok\.com/@"),
    ("mastodon", r"mastodon"),
];

static SOCIAL_PROFILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    let alternatives = SOCIAL_PLATFORMS
        .iter()
        .map(|(_, pattern)| *pattern)
        .collect::<Vec<_>>()
        .join("|");
    ci(&alternatives)
});

// --------------------------------------------------------------------------
// Phone helpers, ported from `ux_claims.has_labelled_phone` — the contact check's second arm.
// `CONTACT_RE` already covers `tel:`/`mailto:`/contact links; this covers a number the page
// PRINTS and labels as a phone. The Rust `regex` crate has no look-around, so the source's
// `(?<![\w+])` / `(?!\w)` on the candidate and `(?=[\W]|$)` on the label are enforced in code.
// --------------------------------------------------------------------------

const PHONE_LABEL_WINDOW: usize = 40;

static PHONE_CANDIDATE_RE: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?:\+|00)?\(?\d[\d\s().-]{7,17}\d"));

static PHONE_NL_NATIONAL: LazyLock<Regex> = LazyLock::new(|| rx(r"^0[1-9]\d{8}$"));

static PHONE_NL_INTL: LazyLock<Regex> = LazyLock::new(|| rx(r"^\+31[1-9]\d{8}$"));

static PHONE_E164: LazyLock<Regex> = LazyLock::new(|| rx(r"^\+[1-9]\d{7,13}$"));

static PHONE_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| ci(r"\btel(?:ephone|efoon)\w*|\btel\b|\bphone\w*|\bmobiel\w*"));

static PHONE_PRODUCT_CONTEXT_RE: LazyLock<Regex> = LazyLock::new(|| ci(r"case|hoesje|accessoire"));

/// The source's label pattern. Two rewrites: `tel\.?(?=[\W]|$)` becomes
/// `tel\.?[^\w]` / `tel\.?$` (the lookahead's character is consumed), and the separator run
/// before an intervening word allows `0` so the consumed character is not counted twice.
/// Every other alternative already ends on `\b` or a word run, so `{0,4}` cannot invent a
/// label-to-word adjacency the source rejected.
static PHONE_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"(?:\btel\.?[^\w]|\btel\.?$|\btelefoon\w*|\btelephone\w*|\bphone\w*|\bmobiel\w*|\bmobile\b|\bgsm\b|\bwhatsapp\b|\bbel(?:\s+ons)?\b|\bcall(?:\s+us)?\b|\bcontact\w*|\bt\s*[:.]|[☎📞✆])(?:[\s:;.,\-–—()|]{0,4}\w{1,12}){0,2}[\s:;.,\-–—()|]{0,12}$",
    )
});

/// Identifier spans stripped before the label scan (IBAN, NL VAT, KvK/BTW, postcode). The
/// source's flags are preserved: rows 1 and 4 are case-sensitive, rows 2 and 3 are not.
static IDENTIFIER_RES: [LazyLock<Regex>; 4] = [
    LazyLock::new(|| rx(r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){2,7}(?:\s?[A-Z0-9]{1,3})?\b")),
    LazyLock::new(|| ci(r"\b[A-Z]{2}\s?\d{9}\s?B\s?\d{2}\b")),
    LazyLock::new(|| {
        ci(
            r"\b(?:kvk|k\.v\.k\.?|coc|chamber of commerce|btw|vat|rsin|iban|bic|swift|ondernemingsnummer|handelsregister)(?:[\s.-]*(?:nummer|nr|no|number|id))?\.?\s*[:#]?\s*[A-Z]{0,2}[\d][\d .]{5,32}\d(?:\s?B\s?\d{2})?",
        )
    }),
    LazyLock::new(|| rx(r"\b\d{4}\s?[A-Z]{2}\b")),
];

// --------------------------------------------------------------------------
// Finding factory and DOM helpers.
// --------------------------------------------------------------------------

/// A marketing finding. The source's `_marketing_finding` pins category, source tool and the
/// `false_positive_risk` / `remediation_effort` metadata; the crate's model carries the first
/// two and has no field for the last two.
fn marketing_finding(
    severity: Severity,
    title: &str,
    description: &str,
    url: &str,
    recommendation: &str,
    check_id: &str,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Marketing,
        severity,
        title: title.to_owned(),
        description: description.to_owned(),
        recommendation: recommendation.to_owned(),
        url: url.to_owned(),
        selector: None,
        evidence: None,
        source_tool: Some(SOURCE_TOOL.to_owned()),
        scanner_count: 1,
        confirmed_by: Vec::new(),
    }
}

fn elements<'a>(document: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    let selector = sel(selector);
    document.select(&selector).collect()
}

fn element_name(element: ElementRef<'_>) -> &str {
    element.value().name()
}

fn is_non_text(name: &str) -> bool {
    matches!(name, "script" | "style" | "noscript" | "template")
}

/// The source's `cro.base.page_text`: text under `<body>` (or the whole document when there is
/// no body), skipping text whose immediate parent is script/style/noscript/template, collapsed.
fn collect_page_text(element: ElementRef<'_>, out: &mut Vec<String>) {
    let skip_text = is_non_text(element_name(element));
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            if !skip_text {
                out.push(text.text.to_string());
            }
        } else if let Some(child_element) = ElementRef::wrap(child) {
            collect_page_text(child_element, out);
        }
    }
}

/// `ux_claims._visible_text`: the document with script/style/noscript/template subtrees removed.
fn collect_visible_text(element: ElementRef<'_>, out: &mut Vec<String>) {
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            out.push(text.text.to_string());
        } else if let Some(child_element) =
            ElementRef::wrap(child).filter(|candidate| !is_non_text(element_name(*candidate)))
        {
            collect_visible_text(child_element, out);
        }
    }
}

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| rx(r"\s+"));

fn page_text(document: &Html) -> String {
    let mut parts = Vec::new();
    let body_selector = sel("body");
    match document.select(&body_selector).next() {
        Some(body) => collect_page_text(body, &mut parts),
        None => collect_page_text(document.root_element(), &mut parts),
    }
    WHITESPACE_RE
        .replace_all(&parts.join(" "), " ")
        .trim()
        .to_owned()
}

fn visible_text(document: &Html) -> String {
    let mut parts = Vec::new();
    collect_visible_text(document.root_element(), &mut parts);
    parts
        .iter()
        .map(|part| part.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The source's `get_text(" ", strip=True)`: each text node trimmed, joined with a space.
fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn raw_hrefs(document: &Html) -> Vec<String> {
    elements(document, "a")
        .iter()
        .map(|link| link.attr("href").unwrap_or("").to_owned())
        .collect()
}

fn has_element(document: &Html, selector: &str) -> bool {
    let selector = sel(selector);
    document.select(&selector).next().is_some()
}

// --------------------------------------------------------------------------
// Phone helpers.
// --------------------------------------------------------------------------

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Every phone-shaped candidate span, with the source's look-around boundaries applied by hand.
fn phone_candidates(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for found in PHONE_CANDIDATE_RE.find_iter(text) {
        let before = text[..found.start()].chars().next_back();
        if matches!(before, Some(character) if is_word_char(character) || character == '+') {
            continue;
        }
        let after = text[found.end()..].chars().next();
        if matches!(after, Some(character) if is_word_char(character)) {
            continue;
        }
        spans.push((found.start(), found.end()));
    }
    spans
}

fn normalize_phone(candidate: &str) -> String {
    let without_zero_group = candidate.replace("(0)", "");
    let leading = without_zero_group.trim_start();
    let plus = leading.starts_with('+') || leading.starts_with("00");
    let digits: String = without_zero_group
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect();
    if !plus {
        return digits;
    }
    let body = if candidate.trim_start().starts_with("00") {
        &digits[digits.len().min(2)..]
    } else {
        digits.as_str()
    };
    format!("+{body}")
}

fn is_phone(candidate: &str) -> bool {
    let normalized = normalize_phone(candidate);
    if PHONE_NL_NATIONAL.is_match(&normalized) || PHONE_NL_INTL.is_match(&normalized) {
        return true;
    }
    // The source's third format is `\+(?!31)[1-9]\d{7,13}`; the Rust regex has no look-ahead.
    if normalized.starts_with("+31") {
        return false;
    }
    PHONE_E164.is_match(&normalized)
}

fn last_chars(text: &str, count: usize) -> String {
    let characters: Vec<char> = text.chars().collect();
    characters[characters.len().saturating_sub(count)..]
        .iter()
        .copied()
        .collect()
}

/// Blank an identifier span, but hand back a valid phone glued to its end (the source's
/// `_strip_identifier`: "KvK 12345678 010 123 4567" keeps the trailing phone).
fn strip_identifier(captures: &Captures<'_>) -> String {
    let span = captures.get(0).map_or("", |found| found.as_str());
    let characters: Vec<char> = span.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        if !character.is_whitespace() {
            continue;
        }
        let head: String = characters[..index].iter().copied().collect();
        let tail: String = characters[index..].iter().copied().collect();
        let digits = head
            .chars()
            .filter(|character| character.is_ascii_digit())
            .count();
        if digits >= 6 && is_phone(tail.trim()) {
            return format!(" | {} ", tail.trim());
        }
    }
    " | ".to_owned()
}

/// `ux_claims._attr_labelled_phone`: markup that names the element itself a phone, whose own
/// direct text holds a phone number. Product/category context disqualifies it (phone cases).
fn attr_labelled_phone(document: &Html) -> bool {
    const ATTRIBUTES: [&str; 5] = ["itemprop", "aria-label", "title", "class", "id"];
    elements(document, "*").iter().any(|element| {
        let attributes = ATTRIBUTES
            .iter()
            .map(|attribute| element.attr(attribute).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        if !PHONE_ATTR_RE.is_match(&attributes) || PHONE_PRODUCT_CONTEXT_RE.is_match(&attributes) {
            return false;
        }
        let own_text = element
            .children()
            .filter_map(|child| child.value().as_text())
            .map(|text| text.text.trim().to_owned())
            .collect::<Vec<_>>()
            .join(" ");
        phone_candidates(&own_text)
            .into_iter()
            .any(|(start, end)| is_phone(&own_text[start..end]))
    })
}

fn has_labelled_phone(document: &Html) -> bool {
    if elements(document, "a").iter().any(|link| {
        link.attr("href")
            .unwrap_or("")
            .trim()
            .to_lowercase()
            .starts_with("tel:")
    }) {
        return true;
    }
    if attr_labelled_phone(document) {
        return true;
    }
    let mut text = visible_text(document);
    for identifier in IDENTIFIER_RES.iter() {
        text = identifier.replace_all(&text, strip_identifier).into_owned();
    }
    for (start, end) in phone_candidates(&text) {
        if !is_phone(&text[start..end]) {
            continue;
        }
        let window = last_chars(&text[..start], PHONE_LABEL_WINDOW);
        if PHONE_PRODUCT_CONTEXT_RE.is_match(&window) {
            continue;
        }
        if PHONE_LABEL_RE.is_match(&window) {
            return true;
        }
    }
    false
}

// --------------------------------------------------------------------------
// competitive.py — 3 checks.
// --------------------------------------------------------------------------

fn fold_copy(document: &Html) -> String {
    elements(document, "h1, h2")
        .iter()
        .map(|heading| element_text(*heading))
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_comparison(document: &Html, text: &str) -> bool {
    COMPARISON_RE.is_match(text)
        || elements(document, "a")
            .iter()
            .any(|link| COMPARISON_RE.is_match(&element_text(*link)))
}

fn has_reputation(text: &str) -> bool {
    REVIEW_PLATFORM_RE.is_match(text) || MEDIA_MENTION_RE.is_match(text)
}

fn check_competitive(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let text = page_text(document);

    // 1. Differentiation clarity in the fold copy (highest-weight signal).
    if !DIFFERENTIATION_RE.is_match(&fold_copy(document)) {
        findings.push(marketing_finding(
            Severity::Medium,
            "No differentiation claim in the headline area",
            "The H1/H2 copy makes no differentiation or category claim (e.g. 'the only…', \
             'unlike…', 'purpose-built for…'). Visitors cannot tell how this offer differs \
             from alternatives in the first few seconds.",
            url,
            "State a concrete point of difference or category position in the headline or subhead.",
            "mkt-competitive-weak-differentiation",
        ));
    }

    // 2. Comparison / "vs" surface for bottom-funnel comparison intent.
    if !has_comparison(document, &text) {
        findings.push(marketing_finding(
            Severity::Low,
            "No comparison or alternatives content",
            "There is no comparison, 'vs', or 'alternative to' content. Buyers evaluating \
             options search for these; owning that surface captures comparison-stage intent.",
            url,
            "Add a comparison table or a 'vs / alternative to' section addressing the main \
             competitors.",
            "mkt-competitive-no-comparison",
        ));
    }

    // 3. Third-party reputation / authority (review platforms or media mentions).
    if !has_reputation(&text) {
        findings.push(marketing_finding(
            Severity::Low,
            "No third-party reputation signals",
            "No third-party review platform (G2, Capterra, Trustpilot…) or media mention is \
             referenced. Borrowed authority reassures skeptical buyers comparing vendors.",
            url,
            "Surface review-platform ratings or 'as featured in' media logos.",
            "mkt-competitive-no-reputation",
        ));
    }

    findings
}

// --------------------------------------------------------------------------
// brand.py — 7 checks.
// --------------------------------------------------------------------------

fn has_favicon(document: &Html) -> bool {
    elements(document, "link").iter().any(|link| {
        link.attr("rel")
            .is_some_and(|rel| rel.to_lowercase().contains("icon"))
    })
}

/// The logo `<img>` (by class/alt/id/src), else the first `<img>` inside the first `<header>`.
fn logo_img(document: &Html) -> bool {
    for image in elements(document, "img") {
        let haystack = ["class", "alt", "id", "src"]
            .iter()
            .map(|attribute| image.attr(attribute).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        if LOGO_RE.is_match(&haystack) {
            return true;
        }
    }
    let headers = elements(document, "header");
    headers.first().is_some_and(|header| {
        let image_selector = sel("img");
        header.select(&image_selector).next().is_some()
    })
}

fn about_link(document: &Html) -> bool {
    elements(document, "a").iter().any(|link| {
        ABOUT_RE.is_match(&element_text(*link))
            || ABOUT_RE.is_match(link.attr("href").unwrap_or(""))
    })
}

fn has_contact(document: &Html) -> bool {
    let haystack = format!("{} {}", page_text(document), raw_hrefs(document).join(" "));
    CONTACT_RE.is_match(&haystack) || has_labelled_phone(document)
}

fn check_brand_trust(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    // 1. Structured data (Schema.org JSON-LD) — rich-result + entity authority.
    if !has_element(document, r#"script[type="application/ld+json"]"#) {
        findings.push(marketing_finding(
            Severity::Medium,
            "No structured data (Schema.org)",
            "No JSON-LD structured data is present. Organization/Product schema unlocks rich \
             results and helps search engines model the brand as an entity.",
            url,
            "Add Organization (and Product/Article) JSON-LD structured data.",
            "mkt-brand-no-structured-data",
        ));
    }

    // 2. Open Graph image — controls how shared links present the brand.
    if !has_element(document, r#"meta[property="og:image"]"#) {
        findings.push(marketing_finding(
            Severity::Low,
            "No Open Graph image",
            "No og:image is set, so links shared on social/chat render without branded \
             imagery — a missed impression on every share.",
            url,
            "Set an og:image (1200x630) for branded link previews.",
            "mkt-brand-no-og-image",
        ));
    }

    // 3. Favicon — browser-tab brand presence.
    if !has_favicon(document) {
        findings.push(marketing_finding(
            Severity::Low,
            "No favicon",
            "No favicon link is declared. The browser tab and bookmarks show a generic icon \
             instead of the brand mark.",
            url,
            r#"Declare a favicon via <link rel="icon">."#,
            "mkt-brand-no-favicon",
        ));
    }

    // 4. Logo presence.
    if !logo_img(document) {
        findings.push(marketing_finding(
            Severity::Low,
            "No identifiable logo",
            "No logo image was detected in the header or by class/alt. A visible logo anchors \
             brand identity and trust.",
            url,
            "Place a logo image in the header with a descriptive alt.",
            "mkt-brand-no-logo",
        ));
    }

    // 5. About / team transparency link.
    if !about_link(document) {
        findings.push(marketing_finding(
            Severity::Info,
            "No about / team link",
            "No about, team, or company link was found. Transparency about who is behind the \
             offer builds credibility, especially for new visitors.",
            url,
            "Add an About / Team link exposing the company and people.",
            "mkt-brand-no-about",
        ));
    }

    // 6. Contact information — accessibility of a real business behind the brand.
    if !has_contact(document) {
        findings.push(marketing_finding(
            Severity::Low,
            "No contact information",
            "No email, phone, address, or contact link was found. Accessible contact details \
             signal a legitimate business and lower the trust barrier for high-consideration \
             buyers.",
            url,
            "Expose a contact email/phone or a clear 'Contact' link.",
            "mkt-brand-no-contact-info",
        ));
    }

    // 7. Social-card completeness — an og:image alone renders a poor share card.
    if has_element(document, r#"meta[property="og:image"]"#) {
        let has_og_title = has_element(document, r#"meta[property="og:title"]"#);
        let has_og_desc = has_element(document, r#"meta[property="og:description"]"#);
        let has_twitter_card = has_element(document, r#"meta[name="twitter:card"]"#);
        if !(has_og_title && has_og_desc && has_twitter_card) {
            findings.push(marketing_finding(
                Severity::Low,
                "Incomplete social-share card",
                "An og:image is set but the share card is missing one of og:title, \
                 og:description, or twitter:card — so shared links render inconsistently across \
                 platforms.",
                url,
                "Add og:title, og:description, and twitter:card meta tags.",
                "mkt-brand-incomplete-social-card",
            ));
        }
    }

    findings
}

// --------------------------------------------------------------------------
// growth.py — 5 checks.
// --------------------------------------------------------------------------

fn has_email_capture(document: &Html) -> bool {
    elements(document, "input").iter().any(|input| {
        let input_type = input.attr("type").unwrap_or("").to_lowercase();
        let name = format!(
            "{} {} {}",
            input.attr("name").unwrap_or(""),
            input.attr("id").unwrap_or(""),
            input.attr("placeholder").unwrap_or("")
        )
        .to_lowercase();
        input_type == "email" || name.contains("email") || name.contains("newsletter")
    })
}

fn social_links(document: &Html) -> Vec<String> {
    raw_hrefs(document)
        .into_iter()
        .filter(|href| SOCIAL_PROFILE_RE.is_match(href))
        .collect()
}

fn has_pricing(document: &Html, text: &str) -> bool {
    PRICE_RE.is_match(text)
        || elements(document, "a").iter().any(|link| {
            let href = link.attr("href").unwrap_or("").to_lowercase();
            href.contains("pric") || href.contains("plan")
        })
}

fn check_growth(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let text = page_text(document);

    // 1. Pricing / packaging visibility.
    if !has_pricing(document, &text) {
        findings.push(marketing_finding(
            Severity::Medium,
            "No visible pricing or packaging",
            "No pricing, plans, or price signals were found. Hidden pricing adds friction and \
             filters out self-serve buyers who want to qualify themselves before talking to \
             sales.",
            url,
            "Expose pricing or a plans/packaging section (or a clear 'contact for pricing' path).",
            "mkt-growth-no-pricing-visibility",
        ));
    }

    // 2. Growth loop (referral / advocacy).
    if !GROWTH_LOOP_RE.is_match(&text) {
        findings.push(marketing_finding(
            Severity::Low,
            "No growth loop (referral / advocacy)",
            "No referral, affiliate, or invite mechanism is present. Growth loops turn existing \
             users into an acquisition channel and lower CAC over time.",
            url,
            "Add a referral, affiliate, or invite-a-colleague loop.",
            "mkt-growth-no-growth-loop",
        ));
    }

    // 3. Retention / nurture hook.
    if !RETENTION_RE.is_match(&text) {
        findings.push(marketing_finding(
            Severity::Low,
            "No retention / nurture hook",
            "No newsletter, community, blog, or content hook was found. Visitors who are not \
             ready to buy leave with no way to stay in the funnel.",
            url,
            "Add a newsletter signup, community link, or content hub to nurture not-yet-ready \
             visitors.",
            "mkt-growth-no-retention-hook",
        ));
    }

    // 4. Email capture (lead nurture entry point).
    if !has_email_capture(document) {
        findings.push(marketing_finding(
            Severity::Low,
            "No email-capture affordance",
            "No email input or newsletter form was found. Email is the highest-ROI owned \
             channel; without a capture point, visitors who aren't ready to buy leave with no \
             way to be nurtured back.",
            url,
            "Add an email/newsletter capture form to a prominent surface.",
            "mkt-growth-no-email-capture",
        ));
    }

    // 5. Owned social-channel presence (multi-channel acquisition + credibility).
    if social_links(document).is_empty() {
        findings.push(marketing_finding(
            Severity::Low,
            "No owned social channels linked",
            "No links to owned social profiles (LinkedIn, X, YouTube, etc.) were found. Social \
             channels diversify acquisition and provide social proof of an active, credible \
             brand.",
            url,
            "Link the brand's active social profiles (typically in the footer).",
            "mkt-growth-no-social-presence",
        ));
    }

    findings
}

// --------------------------------------------------------------------------
// tracking.py — 2 checks, folded into the growth dimension.
// --------------------------------------------------------------------------

/// Every `<script>` src plus inline body — where trackers live.
fn script_haystack(document: &Html) -> String {
    let mut parts = Vec::new();
    for tag in elements(document, "script") {
        if let Some(src) = tag.attr("src").filter(|src| !src.is_empty()) {
            parts.push(src.to_owned());
        }
        let body = tag.text().collect::<String>();
        if !body.is_empty() {
            parts.push(body);
        }
    }
    parts.join("\n")
}

fn check_tracking(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let scripts = script_haystack(document);

    if !ANALYTICS_RE.is_match(&scripts) {
        findings.push(marketing_finding(
            Severity::Low,
            "No web analytics or tag manager detected",
            "No analytics, tag-manager, or conversion-pixel snippet was found in the static \
             HTML (e.g. GA4, Google Tag Manager, Meta Pixel, LinkedIn Insight, PostHog, \
             Segment). Without measurement a site cannot attribute channels, quantify \
             conversion, or run experiments. NOTE: trackers injected client-side by a SPA may \
             not be visible to a static fetch.",
            url,
            "Install a web-analytics platform and, if running paid acquisition, the relevant \
             conversion pixels — ideally via a tag manager. If already present via a JS bundle, \
             expose it in the initial HTML for reliability.",
            "mkt-growth-no-analytics",
        ));
    } else {
        // Consent only matters once tracking is actually running.
        let consent_haystack = format!("{}\n{}", scripts, page_text(document));
        if !CONSENT_RE.is_match(&consent_haystack) {
            findings.push(marketing_finding(
                Severity::Low,
                "Tracking present but no cookie-consent mechanism",
                "Analytics or tracking scripts were detected but no cookie-consent or \
                 privacy-preference mechanism was found. In many jurisdictions this is a \
                 compliance risk and it erodes visitor trust.",
                url,
                "Add a consent-management banner (e.g. OneTrust, Cookiebot, Usercentrics) \
                 gating non-essential tracking.",
                "mkt-growth-no-consent",
            ));
        }
    }

    findings
}

/// Scan one document for the source's seventeen go-to-market checks.
pub fn scan(document: &Document) -> Vec<Finding> {
    let parsed = Html::parse_document(&document.html);
    let url = document.url.as_str();
    let mut findings = Vec::new();
    findings.extend(check_competitive(&parsed, url));
    findings.extend(check_brand_trust(&parsed, url));
    findings.extend(check_growth(&parsed, url));
    findings.extend(check_tracking(&parsed, url));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_ids(html: &str) -> Vec<String> {
        scan(&Document::new("https://example.test/", html))
            .into_iter()
            .map(|finding| finding.check_id.unwrap_or_default())
            .collect()
    }

    fn assert_fires(html: &str, check_id: &str) {
        let ids = scan_ids(html);
        assert!(
            ids.iter().any(|id| id == check_id),
            "{check_id} did not fire; got {ids:?}"
        );
    }

    #[test]
    fn every_check_fires_on_the_page_that_lacks_its_signal() {
        let cases = [
            (
                "mkt-competitive-weak-differentiation",
                r#"<html><body><h1>Welcome to our shop</h1></body></html>"#,
            ),
            (
                "mkt-competitive-no-comparison",
                r#"<html><body><h1>The only platform purpose-built for teams</h1></body></html>"#,
            ),
            (
                "mkt-competitive-no-reputation",
                r#"<html><body><h1>The only platform purpose-built for teams</h1></body></html>"#,
            ),
            (
                "mkt-brand-no-structured-data",
                r#"<html><head></head><body></body></html>"#,
            ),
            (
                "mkt-brand-no-og-image",
                r#"<html><head><meta property="og:title" content="Acme"></head><body></body></html>"#,
            ),
            (
                "mkt-brand-no-favicon",
                r#"<html><head><link rel="stylesheet" href="/a.css"></head><body></body></html>"#,
            ),
            (
                "mkt-brand-no-logo",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-brand-no-about",
                r#"<html><body><a href="/pricing">Pricing</a></body></html>"#,
            ),
            (
                "mkt-brand-no-contact-info",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-brand-incomplete-social-card",
                r#"<html><head><meta property="og:image" content="/og.png"><meta property="og:title" content="Acme"></head><body></body></html>"#,
            ),
            (
                "mkt-growth-no-pricing-visibility",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-growth-no-growth-loop",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-growth-no-retention-hook",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-growth-no-email-capture",
                r#"<html><body><form><input type="text" name="q"></form></body></html>"#,
            ),
            (
                "mkt-growth-no-social-presence",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
            (
                "mkt-growth-no-analytics",
                r#"<html><body><p>Plain page</p></body></html>"#,
            ),
        ];
        for (check_id, html) in cases {
            assert_fires(html, check_id);
        }
    }

    #[test]
    fn the_consent_check_only_runs_once_analytics_is_present() {
        let html = r#"<html><body><script src="https://www.googletagmanager.com/gtm.js"></script></body></html>"#;
        let ids = scan_ids(html);
        assert!(ids.iter().any(|id| id == "mkt-growth-no-consent"));
        assert!(!ids.iter().any(|id| id == "mkt-growth-no-analytics"));
    }

    #[test]
    fn minimal_page_reports_checks_in_source_order() {
        assert_eq!(
            scan_ids("<html><body></body></html>"),
            vec![
                "mkt-competitive-weak-differentiation",
                "mkt-competitive-no-comparison",
                "mkt-competitive-no-reputation",
                "mkt-brand-no-structured-data",
                "mkt-brand-no-og-image",
                "mkt-brand-no-favicon",
                "mkt-brand-no-logo",
                "mkt-brand-no-about",
                "mkt-brand-no-contact-info",
                "mkt-growth-no-pricing-visibility",
                "mkt-growth-no-growth-loop",
                "mkt-growth-no-retention-hook",
                "mkt-growth-no-email-capture",
                "mkt-growth-no-social-presence",
                "mkt-growth-no-analytics",
            ]
        );
    }

    #[test]
    fn a_fully_presented_page_emits_no_findings() {
        let html = r#"<!doctype html>
<html>
<head>
<link rel="icon" href="/favicon.ico">
<meta property="og:image" content="https://example.test/og.png">
<meta property="og:title" content="Acme widgets">
<meta property="og:description" content="The leading widget platform">
<meta name="twitter:card" content="summary_large_image">
<script type="application/ld+json">{"@type": "Organization"}</script>
<script src="https://www.googletagmanager.com/gtag/js"></script>
</head>
<body>
<header><img src="/logo.png" alt="Acme logo"></header>
<h1>The leading widget platform</h1>
<p>Compare us vs the alternatives. Featured in Forbes.</p>
<a href="/about">About us</a>
<a href="mailto:hello@example.test">Contact us</a>
<a href="/pricing">Pricing</a>
<p>Refer a friend and subscribe to our newsletter.</p>
<form><input type="email" name="email"></form>
<a href="https://linkedin.com/company/acme">LinkedIn</a>
<a href="/cookies">Cookie consent</a>
</body>
</html>"#;
        let ids = scan_ids(html);
        assert!(ids.is_empty(), "expected no findings; got {ids:?}");
    }

    #[test]
    fn a_labelled_phone_number_is_contact_information() {
        let ids = scan_ids(r#"<html><body><p>Telefoon: 020 123 4567</p></body></html>"#);
        assert!(!ids.iter().any(|id| id == "mkt-brand-no-contact-info"));
    }

    #[test]
    fn findings_carry_the_source_metadata() {
        let findings = scan(&Document::new(
            "https://example.test/",
            "<html><body></body></html>",
        ));
        assert!(!findings.is_empty());
        for finding in &findings {
            assert_eq!(finding.category, Category::Marketing);
            assert_eq!(finding.source_tool.as_deref(), Some("marketing_scanner"));
            assert_eq!(finding.url, "https://example.test/");
            assert_eq!(finding.scanner_count, 1);
            assert!(finding.confirmed_by.is_empty());
        }
    }
}
