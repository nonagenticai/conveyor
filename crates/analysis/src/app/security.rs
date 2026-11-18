//! The security scanner — the HTML-only checks plus the checks over captured origin facts.
//!
//! Ported faithfully from the source's `scanners/security_scanner/`: the six HTML-only checks of
//! `scan_static` (mixed content, secrets in HTML, missing SRI, meta-refresh redirects,
//! external form actions, insecure form actions) and the header/TLS checks of `scan` that read
//! only the captured response headers and the result of an already-made TLS handshake. Check
//! ids, severities, titles, descriptions and recommendations are the source's, and
//! `source_tool` is the source's `security_scanner`.
//!
//! Not ported, because the library never makes a request:
//!
//! - exposed-file probing (`.env`, `.git/config`, `.DS_Store`, `wp-config.php`, `.htaccess`),
//! - the CORS request (`Origin` header probe),
//! - directory-listing probing,
//! - redirect-chain probing.
//!
//! The source's fetch-time outcomes (`sec-ssrf-blocked`, `sec-connection-failed`, `sec-timeout`,
//! `sec-scan-error`, `sec-check-errored`) belong to the caller's capture for the same reason.
//!
//! Two deliberate adaptations, both forced by the evidence shape:
//!
//! - An origin verdict needs a captured origin. Header checks run only when a response was
//!   recorded (a status or a header), and TLS verdicts need a page URL; a missing capture is
//!   silence, not a "missing header" finding. The HTML-only checks always run, exactly as
//!   `scan_static` ran them over a stored artifact.
//! - TLS expiry findings state whole days only: [`TlsEvidence`](crate::app::TlsEvidence) carries
//!   no `notAfter` timestamp and the crate reads no clock, so the source's interpolated
//!   certificate date cannot be reproduced. Everything else in those descriptions is verbatim.
//!
//! Output order is the source's execution order, and nothing here is time- or network-dependent:
//! two runs over the same evidence produce identical findings.

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::app::{HttpEvidence, PageEvidence};
use crate::model::{Category, Finding, Severity};

/// The source's `_sec_finding` stamps every finding with this tool name.
const SOURCE_TOOL: &str = "security_scanner";

/// Days-left thresholds for TLS expiry findings (the source's module constants).
const TLS_EXPIRY_HIGH_DAYS: i64 = 7;
const TLS_EXPIRY_WARN_DAYS: i64 = 21;

/// The source's `_SECRET_PATTERN`, verbatim (including its `IGNORECASE` flag).
const SECRET_PATTERN: &str = r#"(?i)(?:api[_\-]?key|secret[_\-]?key|api[_\-]?secret|token|password|passwd|access[_\-]?key|private[_\-]?key)\s*[:=]\s*['"]([A-Za-z0-9+/=_\-]{16,})['"]"#;

/// Hosts whose resources legitimately change without the URL changing, so SRI advice would be
/// wrong (`_DYNAMIC_RESOURCE_HOSTS`).
const DYNAMIC_RESOURCE_HOSTS: [&str; 10] = [
    "fonts.googleapis.com",
    "googletagmanager.com",
    "google-analytics.com",
    "js.stripe.com",
    "connect.facebook.net",
    "maps.googleapis.com",
    "www.google.com",
    "www.gstatic.com",
    "static.hotjar.com",
    "use.typekit.net",
];

/// Tags and attributes that load external resources, in the source's iteration order.
const RESOURCE_TAGS: [(&str, &str); 9] = [
    ("img", "src"),
    ("script", "src"),
    ("link", "href"),
    ("iframe", "src"),
    ("video", "src"),
    ("audio", "src"),
    ("source", "src"),
    ("object", "data"),
    ("embed", "src"),
];

/// One row of the source's `header_checks` table in `_check_security_headers`.
struct HeaderCheck {
    header: &'static str,
    severity: Severity,
    title: &'static str,
    description: &'static str,
    recommendation: &'static str,
    check_id: &'static str,
}

const HEADER_CHECKS: [HeaderCheck; 6] = [
    HeaderCheck {
        header: "content-security-policy",
        severity: Severity::High,
        title: "Missing Content-Security-Policy header",
        description: "No CSP header found. This leaves the site vulnerable to XSS attacks.",
        recommendation: "Add a Content-Security-Policy header to restrict resource loading.",
        check_id: "sec-missing-csp",
    },
    HeaderCheck {
        header: "strict-transport-security",
        severity: Severity::High,
        title: "Missing Strict-Transport-Security header",
        description: "No HSTS header found. The site may be vulnerable to protocol downgrade attacks.",
        recommendation: "Add Strict-Transport-Security: max-age=31536000; includeSubDomains.",
        check_id: "sec-missing-hsts",
    },
    HeaderCheck {
        header: "x-frame-options",
        severity: Severity::Medium,
        title: "Missing X-Frame-Options header",
        description: "No X-Frame-Options header. The page may be vulnerable to clickjacking.",
        recommendation: "Add X-Frame-Options: DENY or SAMEORIGIN.",
        check_id: "sec-missing-xfo",
    },
    HeaderCheck {
        header: "x-content-type-options",
        severity: Severity::Medium,
        title: "Missing X-Content-Type-Options header",
        description: "No X-Content-Type-Options header. Browsers may MIME-sniff responses.",
        recommendation: "Add X-Content-Type-Options: nosniff.",
        check_id: "sec-missing-xcto",
    },
    HeaderCheck {
        header: "referrer-policy",
        severity: Severity::Low,
        title: "Missing Referrer-Policy header",
        description: "No Referrer-Policy header. Full URLs may leak in referrer headers.",
        recommendation: "Add Referrer-Policy: strict-origin-when-cross-origin.",
        check_id: "sec-missing-referrer-policy",
    },
    HeaderCheck {
        header: "permissions-policy",
        severity: Severity::Low,
        title: "Missing Permissions-Policy header",
        description: "No Permissions-Policy header to restrict browser feature access.",
        recommendation: "Add Permissions-Policy to restrict camera, microphone, geolocation, etc.",
        check_id: "sec-missing-permissions-policy",
    },
];

const PROXY_ALTERNATIVE: &str = "Serve the site through a CDN or reverse proxy that can add response headers (for example Cloudflare in front of the custom domain), or move to a host that supports custom headers.";

const CSP_META_ALTERNATIVE: &str = "Add a <meta http-equiv=\"Content-Security-Policy\" content=\"...\"> tag to <head> (frame-ancestors and report-uri are not honoured in a meta tag), or serve the site through a CDN or reverse proxy that adds the header (for example Cloudflare).";

const HSTS_STATIC_HOST_ALTERNATIVE: &str = "Enable 'Enforce HTTPS' in the GitHub Pages settings (HTTP is then redirected). HSTS itself can only be sent by a CDN or reverse proxy in front of the site, for example Cloudflare.";

const REFERRER_META_ALTERNATIVE: &str =
    "Add <meta name=\"referrer\" content=\"strict-origin-when-cross-origin\"> to <head>.";

/// Analyse the captured page's security surface.
pub fn scan(evidence: &PageEvidence) -> Vec<Finding> {
    let mut findings = Vec::new();
    let http = &evidence.http;
    let url = page_url(evidence);
    let captured = response_captured(http);

    // The source's `scan` order: headers, cookies, server, TLS, CSP details, technology, HSTS
    // quality, CSP reporting, the cross-origin headers — then the HTML-only checks.
    if captured {
        findings.extend(missing_security_headers(http, url));
        findings.extend(cookie_findings(http, url));
        findings.extend(server_disclosure(http, url));
    }
    findings.extend(tls_findings(http, url));
    if captured {
        findings.extend(csp_details(http, url));
        findings.extend(technology_disclosure(http, url));
        findings.extend(hsts_quality(http, url));
        findings.extend(csp_reporting(http, url));
        findings.extend(cross_origin_headers(http, url));
    }

    let document = Html::parse_document(&evidence.document.html);
    findings.extend(mixed_content(&document, url));
    findings.extend(html_secrets(&evidence.document.html, url));
    findings.extend(subresource_integrity(&document, url));
    findings.extend(meta_refresh_redirect(&document, url));
    findings.extend(external_form_actions(&document, url));
    findings.extend(insecure_form_actions(&document, url));

    findings
}

/// Whether the capture recorded an actual response. `status == 0` is the model's "no response"
/// sentinel, so a header verdict on such an evidence would be invented.
fn response_captured(http: &HttpEvidence) -> bool {
    http.status != 0 || !http.headers.is_empty()
}

fn page_url(evidence: &PageEvidence) -> &str {
    if evidence.document.url.is_empty() {
        &evidence.http.final_url
    } else {
        &evidence.document.url
    }
}

/// A security finding with the source's `_sec_finding` defaults.
fn sec(
    check_id: &'static str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    url: &str,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Security,
        severity,
        title: title.into(),
        description: description.into(),
        recommendation: String::new(),
        url: url.to_owned(),
        selector: None,
        evidence: None,
        source_tool: Some(SOURCE_TOOL.to_owned()),
        scanner_count: 1,
        confirmed_by: Vec::new(),
    }
}

fn elements<'a>(document: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    let selector = Selector::parse(selector).expect("static selector");
    document.select(&selector).collect()
}

/// Collect up to five examples plus the source's `(and N more)` suffix.
fn sample_and_extra(items: &[String]) -> (String, String) {
    let sample = items.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    let extra = if items.len() > 5 {
        format!(" (and {} more)", items.len() - 5)
    } else {
        String::new()
    };
    (sample, extra)
}

// ---------------------------------------------------------------------------------------------
// Header checks
// ---------------------------------------------------------------------------------------------

/// `_check_security_headers`: the six headers every page should send, with the source's
/// static-host ("the platform cannot set headers") adjustment.
fn missing_security_headers(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let server = http.header("server").unwrap_or_default();
    let platform = match server.trim().to_ascii_lowercase().as_str() {
        "github.com" => Some("GitHub Pages"),
        _ => None,
    };

    HEADER_CHECKS
        .iter()
        .filter_map(|check| {
            if http.has(check.header) {
                return None;
            }
            let (severity, description, recommendation, evidence) = match platform {
                Some(platform) => {
                    // No session/cookies to downgrade on a static host, so HSTS is not HIGH.
                    let severity = if check.header == "strict-transport-security" {
                        Severity::Medium
                    } else {
                        check.severity
                    };
                    let description = format!(
                        "{} The site is served by {platform}, which does not allow custom response headers.",
                        check.description
                    );
                    let recommendation = static_host_alternative(check.header).to_owned();
                    (severity, description, recommendation, Some(format!("server: {server}")))
                }
                None => (
                    check.severity,
                    check.description.to_owned(),
                    check.recommendation.to_owned(),
                    None,
                ),
            };
            let mut finding =
                sec(check.check_id, severity, check.title, description, url)
                    .with_recommendation(recommendation);
            if let Some(evidence) = evidence {
                finding = finding.with_evidence(evidence);
            }
            Some(finding)
        })
        .collect()
}

fn static_host_alternative(header: &str) -> &'static str {
    match header {
        "content-security-policy" => CSP_META_ALTERNATIVE,
        "strict-transport-security" => HSTS_STATIC_HOST_ALTERNATIVE,
        "referrer-policy" => REFERRER_META_ALTERNATIVE,
        _ => PROXY_ALTERNATIVE,
    }
}

/// `_check_cookie_flags`: the source read `httpx`'s cookie jar; the captured `Set-Cookie`
/// headers carry the same facts, so each header is parsed as one cookie.
fn cookie_findings(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, value) in &http.headers {
        if !name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        let Some((cookie_name, rest)) = value.split_once('=') else {
            continue;
        };
        let cookie_name = cookie_name.trim();
        if cookie_name.is_empty() {
            continue;
        }
        let (mut secure, mut http_only, mut same_site) = (false, false, false);
        for attribute in rest.split(';').skip(1) {
            let mut parts = attribute.splitn(2, '=');
            let key = parts.next().unwrap_or_default().trim();
            let attribute_value = parts.next().map(str::trim);
            if key.eq_ignore_ascii_case("secure") {
                secure = true;
            } else if key.eq_ignore_ascii_case("httponly") {
                http_only = true;
            } else if key.eq_ignore_ascii_case("samesite") {
                // `get_nonstandard_attr` returns the value, and `""` is falsy in the source.
                same_site = attribute_value.is_some_and(|value| !value.is_empty());
            }
        }
        let mut missing = Vec::new();
        if !secure {
            missing.push("Secure");
        }
        if !http_only {
            missing.push("HttpOnly");
        }
        if !same_site {
            missing.push("SameSite");
        }
        if missing.is_empty() {
            continue;
        }
        let missing = missing.join(", ");
        findings.push(
            sec(
                "sec-cookie-flags",
                Severity::Medium,
                format!("Cookie '{cookie_name}' missing security flags"),
                format!("Cookie '{cookie_name}' is missing: {missing}."),
                url,
            )
            .with_recommendation(format!("Set {missing} flags on cookie '{cookie_name}'.")),
        );
    }
    findings
}

/// `_check_server_header`.
fn server_disclosure(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let server = http.header("server").unwrap_or_default();
    if server.is_empty() || !server.chars().any(|character| character.is_ascii_digit()) {
        return Vec::new();
    }
    vec![
        sec(
            "sec-server-disclosure",
            Severity::Low,
            "Server version disclosure",
            format!("Server header reveals version info: '{server}'."),
            url,
        )
        .with_recommendation("Remove version numbers from the Server header."),
    ]
}

/// `_check_csp_details`.
fn csp_details(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let csp = http.header("content-security-policy").unwrap_or_default();
    if csp.is_empty() {
        return findings;
    }

    if csp.contains("'unsafe-inline'") {
        findings.push(
            sec(
                "sec-csp-unsafe-inline",
                Severity::High,
                "CSP allows unsafe-inline",
                "The Content-Security-Policy contains 'unsafe-inline', which significantly weakens XSS protection.",
                url,
            )
            .with_recommendation(
                "Remove 'unsafe-inline' from CSP and use nonces or hashes instead.",
            )
            .with_evidence(csp.clone()),
        );
    }

    if csp.contains("'unsafe-eval'") {
        findings.push(
            sec(
                "sec-csp-unsafe-eval",
                Severity::High,
                "CSP allows unsafe-eval",
                "The Content-Security-Policy contains 'unsafe-eval', which allows JavaScript code execution from strings (e.g., eval()).",
                url,
            )
            .with_recommendation(
                "Remove 'unsafe-eval' from CSP and refactor code that relies on eval().",
            )
            .with_evidence(csp.clone()),
        );
    }

    for directive in csp.split(';') {
        let parts: Vec<&str> = directive.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        for source in &parts[1..] {
            if source.trim() == "*" {
                findings.push(
                    sec(
                        "sec-csp-wildcard",
                        Severity::Medium,
                        "CSP contains wildcard source",
                        format!(
                            "The CSP directive '{}' uses a wildcard (*), allowing resources from any origin.",
                            parts[0]
                        ),
                        url,
                    )
                    .with_recommendation(
                        "Replace wildcard sources with specific trusted domains.",
                    )
                    .with_evidence(csp.clone()),
                );
                // The source reports a wildcard only once.
                return findings;
            }
        }
    }

    findings
}

/// `_check_technology_disclosure`.
fn technology_disclosure(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut disclosed = Vec::new();
    for header in ["x-powered-by", "x-aspnet-version", "x-generator"] {
        if let Some(value) = http.header(header).filter(|value| !value.is_empty()) {
            disclosed.push(format!("{header}: {value}"));
        }
    }
    if disclosed.is_empty() {
        return Vec::new();
    }
    let disclosed = disclosed.join("; ");
    vec![
        sec(
            "sec-tech-disclosure",
            Severity::Low,
            "Technology stack disclosed in headers",
            format!(
                "Response headers reveal technology information: {disclosed}. This helps attackers fingerprint the server."
            ),
            url,
        )
        .with_recommendation(
            "Remove technology-disclosing headers like X-Powered-By, X-AspNet-Version, and X-Generator.",
        )
        .with_evidence(disclosed),
    ]
}

/// The source's `re.search(r"max-age=(\d+)", hsts, re.IGNORECASE)`: first `max-age=` that is
/// followed by at least one digit.
fn hsts_max_age(hsts: &str) -> Option<u128> {
    let lower = hsts.to_ascii_lowercase();
    let mut from = 0;
    while let Some(position) = lower[from..].find("max-age=") {
        let start = from + position + "max-age=".len();
        let digits: String = lower[start..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
        from = start;
    }
    None
}

/// `_check_hsts_quality`.
fn hsts_quality(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let hsts = http.header("strict-transport-security").unwrap_or_default();
    if hsts.is_empty() {
        return findings;
    }

    if let Some(max_age) = hsts_max_age(&hsts).filter(|max_age| *max_age < 31_536_000) {
        findings.push(
            sec(
                "sec-hsts-short-max-age",
                Severity::Medium,
                "HSTS max-age too short",
                format!(
                    "HSTS max-age is {max_age} seconds ({} days). Recommended minimum is 31536000 (1 year).",
                    max_age / 86_400
                ),
                url,
            )
            .with_recommendation(
                "Set Strict-Transport-Security max-age to at least 31536000 (1 year).",
            )
            .with_evidence(hsts.clone()),
        );
    }

    if !hsts.to_ascii_lowercase().contains("includesubdomains") {
        findings.push(
            sec(
                "sec-hsts-no-subdomains",
                Severity::Low,
                "HSTS missing includeSubDomains",
                "The HSTS header does not include the includeSubDomains directive. Subdomains are not protected by HSTS.",
                url,
            )
            .with_recommendation(
                "Add includeSubDomains to the Strict-Transport-Security header.",
            )
            .with_evidence(hsts),
        );
    }

    findings
}

/// `_check_csp_reporting`.
fn csp_reporting(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let csp = http.header("content-security-policy").unwrap_or_default();
    if csp.is_empty() {
        return Vec::new();
    }
    let lower = csp.to_ascii_lowercase();
    if lower.contains("report-uri") || lower.contains("report-to") {
        return Vec::new();
    }
    vec![
        sec(
            "sec-csp-no-reporting",
            Severity::Low,
            "CSP has no reporting directive",
            "The Content-Security-Policy header exists but does not include a report-uri or report-to directive. Without reporting, CSP violations go unnoticed.",
            url,
        )
        .with_recommendation(
            "Add a report-uri or report-to directive to the CSP header so you can monitor policy violations.",
        )
        .with_evidence(csp.chars().take(200).collect::<String>()),
    ]
}

/// `_check_corp`, `_check_coop`, `_check_coep`, `_check_dns_prefetch_control`.
fn cross_origin_headers(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !http.has("cross-origin-resource-policy") {
        findings.push(
            sec(
                "sec-missing-corp",
                Severity::Low,
                "Missing Cross-Origin-Resource-Policy header",
                "No Cross-Origin-Resource-Policy (CORP) header found. CORP prevents other origins from loading your resources, mitigating side-channel attacks like Spectre.",
                url,
            )
            .with_recommendation(
                "Add Cross-Origin-Resource-Policy: same-origin (or same-site / cross-origin as appropriate).",
            ),
        );
    }
    if !http.has("cross-origin-opener-policy") {
        findings.push(
            sec(
                "sec-missing-coop",
                Severity::Low,
                "Missing Cross-Origin-Opener-Policy header",
                "No Cross-Origin-Opener-Policy (COOP) header found. COOP isolates your browsing context from cross-origin documents, protecting against Spectre-like attacks.",
                url,
            )
            .with_recommendation(
                "Add Cross-Origin-Opener-Policy: same-origin to isolate the browsing context.",
            ),
        );
    }
    if !http.has("cross-origin-embedder-policy") {
        findings.push(
            sec(
                "sec-missing-coep",
                Severity::Low,
                "Missing Cross-Origin-Embedder-Policy header",
                "No Cross-Origin-Embedder-Policy (COEP) header found. COEP ensures all sub-resources are loaded with explicit permission, enabling cross-origin isolation.",
                url,
            )
            .with_recommendation(
                "Add Cross-Origin-Embedder-Policy: require-corp to enable cross-origin isolation.",
            ),
        );
    }
    if !http.has("x-dns-prefetch-control") {
        findings.push(
            sec(
                "sec-dns-prefetch",
                Severity::Info,
                "Missing X-DNS-Prefetch-Control header",
                "No X-DNS-Prefetch-Control header found. This header controls DNS prefetching, which can leak information about which links a user might visit.",
                url,
            )
            .with_recommendation(
                "Add X-DNS-Prefetch-Control: off to prevent unintended DNS prefetch requests, or explicitly set it to 'on' if prefetching is desired.",
            ),
        );
    }
    findings
}

// ---------------------------------------------------------------------------------------------
// TLS checks
// ---------------------------------------------------------------------------------------------

/// `_check_tls` over the captured handshake. No connection is opened here; `sec-no-https` only
/// needs the URL's scheme, the rest only needs the negotiated protocol and expiry days.
fn tls_findings(http: &HttpEvidence, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    if url.is_empty() {
        return findings;
    }
    let scheme = scheme_of(url);
    if scheme != "https" {
        findings.push(
            sec(
                "sec-no-https",
                Severity::High,
                "Not using HTTPS",
                "The site is served over HTTP, not HTTPS.",
                url,
            )
            .with_recommendation("Enable HTTPS with a valid TLS certificate."),
        );
        return findings;
    }

    let Some(tls) = http.tls.as_ref() else {
        // The handshake was not captured; the source's fail-soft path also yielded nothing.
        return findings;
    };

    if tls.protocol == "TLSv1" || tls.protocol == "TLSv1.1" {
        findings.push(
            sec(
                "sec-tls-outdated",
                Severity::High,
                format!("Outdated TLS version: {}", tls.protocol),
                format!(
                    "The server uses {}, which is deprecated and insecure.",
                    tls.protocol
                ),
                url,
            )
            .with_recommendation("Upgrade to TLS 1.2 or TLS 1.3."),
        );
    }

    findings.extend(tls_expiry_findings(tls.days_until_expiry, url));
    findings
}

/// The source's `_tls_expiry_findings`, minus the interpolated `notAfter` date: the evidence
/// carries days remaining, not the certificate timestamp, and this crate reads no clock.
fn tls_expiry_findings(days_remaining: i64, url: &str) -> Vec<Finding> {
    if days_remaining < 0 {
        return vec![
            sec(
                "sec-tls-expired",
                Severity::Critical,
                "TLS certificate expired",
                format!(
                    "The TLS certificate has expired ({} days ago).",
                    days_remaining.unsigned_abs()
                ),
                url,
            )
            .with_recommendation("Renew the TLS certificate immediately."),
        ];
    }
    if days_remaining <= TLS_EXPIRY_HIGH_DAYS {
        return vec![
            sec(
                "sec-tls-expiring-soon",
                Severity::High,
                "TLS certificate expiring soon",
                format!("The TLS certificate expires in {days_remaining} days."),
                url,
            )
            .with_recommendation("Renew the TLS certificate before it expires."),
        ];
    }
    if days_remaining <= TLS_EXPIRY_WARN_DAYS {
        return vec![
            sec(
                "sec-tls-expiring",
                Severity::Medium,
                "TLS certificate expiring",
                format!(
                    "The TLS certificate expires in {days_remaining} days and has not been renewed; automated renewal normally happens earlier."
                ),
                url,
            )
            .with_recommendation(
                "Check that automatic certificate renewal is working, or renew the certificate manually.",
            ),
        ];
    }
    Vec::new()
}

// ---------------------------------------------------------------------------------------------
// HTML-only checks (the source's `scan_static`)
// ---------------------------------------------------------------------------------------------

/// `_check_mixed_content`.
fn mixed_content(document: &Html, url: &str) -> Vec<Finding> {
    if scheme_of(url) != "https" {
        return Vec::new();
    }
    let mut mixed = Vec::new();
    for (tag, attribute) in RESOURCE_TAGS {
        let selector = format!("{tag}[{attribute}]");
        for element in elements(document, &selector) {
            if let Some(value) = element
                .value()
                .attr(attribute)
                .filter(|value| value.starts_with("http://"))
            {
                mixed.push(value.to_owned());
            }
        }
    }
    if mixed.is_empty() {
        return Vec::new();
    }
    let (list, extra) = sample_and_extra(&mixed);
    vec![
        sec(
            "sec-mixed-content",
            Severity::High,
            "Mixed content detected",
            format!(
                "Found {} resource(s) loaded over HTTP on an HTTPS page: {list}{extra}",
                mixed.len()
            ),
            url,
        )
        .with_recommendation(
            "Load all resources over HTTPS to prevent mixed content warnings and potential man-in-the-middle attacks.",
        ),
    ]
}

/// `_check_html_secrets`.
fn html_secrets(html: &str, url: &str) -> Vec<Finding> {
    let pattern = Regex::new(SECRET_PATTERN).expect("static pattern");
    let matches: Vec<String> = pattern
        .captures_iter(html)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_owned()))
        .collect();
    if matches.is_empty() {
        return Vec::new();
    }
    let count = matches.len().min(10);
    let redacted: Vec<String> = matches
        .iter()
        .take(count)
        .map(|value| {
            if value.len() > 8 {
                format!("{}...{}", &value[..4], &value[value.len() - 4..])
            } else {
                "****".to_owned()
            }
        })
        .collect();
    vec![
        sec(
            "sec-html-secret-exposed",
            Severity::Critical,
            format!("Potential secrets exposed in HTML ({count} found)"),
            format!(
                "Found {} potential secret(s) (API keys, tokens, or passwords) in the page HTML. These are visible to anyone viewing the page source.",
                matches.len()
            ),
            url,
        )
        .with_recommendation(
            "Move all secrets to server-side environment variables. Never embed API keys or tokens in client-side HTML.",
        )
        .with_evidence(format!("Redacted values: {}", redacted.join(", "))),
    ]
}

/// `_check_sri`.
fn subresource_integrity(document: &Html, url: &str) -> Vec<Finding> {
    let page_host = host_of(url);
    let mut candidates: Vec<(&str, &str)> = Vec::new();
    for element in elements(document, "script[src]") {
        candidates.push((
            element.value().attr("src").unwrap_or_default(),
            element.value().attr("integrity").unwrap_or_default(),
        ));
    }
    for element in elements(document, "link[rel]") {
        let is_stylesheet = element
            .value()
            .attr("rel")
            .is_some_and(|rel| rel.split_whitespace().any(|token| token == "stylesheet"));
        if is_stylesheet {
            candidates.push((
                element.value().attr("href").unwrap_or_default(),
                element.value().attr("integrity").unwrap_or_default(),
            ));
        }
    }

    let mut missing = Vec::new();
    for (reference, integrity) in candidates {
        // `integrity` is a valued attribute: an empty one verifies nothing.
        if !reference.starts_with("http") || !integrity.trim().is_empty() {
            continue;
        }
        let host = host_of(reference);
        if host.is_empty() || host == page_host || serves_dynamic_content(&host) {
            continue;
        }
        missing.push(reference.to_owned());
    }

    if missing.is_empty() {
        return Vec::new();
    }
    let (list, extra) = sample_and_extra(&missing);
    vec![
        sec(
            "sec-sri-missing",
            Severity::Medium,
            format!("Missing SRI on {} external resource(s)", missing.len()),
            format!(
                "{} external script(s)/stylesheet(s) from CDNs lack integrity attributes. Without SRI, a compromised CDN could inject malicious code.",
                missing.len()
            ),
            url,
        )
        .with_recommendation(
            "Add integrity and crossorigin attributes to all external <script> and <link> tags from third-party CDNs.",
        )
        .with_evidence(format!("Resources: {list}{extra}")),
    ]
}

fn serves_dynamic_content(host: &str) -> bool {
    DYNAMIC_RESOURCE_HOSTS.iter().any(|dynamic| {
        let dotted = format!(".{dynamic}");
        host == *dynamic || host.ends_with(dotted.as_str())
    })
}

/// `_check_meta_refresh_redirect`.
fn meta_refresh_redirect(document: &Html, url: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for element in elements(document, "meta[http-equiv]") {
        let is_refresh = element
            .value()
            .attr("http-equiv")
            .is_some_and(|value| value.eq_ignore_ascii_case("refresh"));
        if !is_refresh {
            continue;
        }
        let content = element
            .value()
            .attr("content")
            .unwrap_or_default()
            .to_lowercase();
        if !content.contains("url=") {
            continue;
        }
        findings.push(
            sec(
                "sec-meta-refresh-redirect",
                Severity::Medium,
                "Meta refresh redirect detected",
                "A <meta http-equiv='refresh'> tag redirects to another URL. Meta refresh redirects can be used for phishing and are not accessible to all users.",
                url,
            )
            .with_recommendation(
                "Use HTTP 301/302 redirects instead of meta refresh redirects for better security and accessibility.",
            )
            .with_selector("meta[http-equiv=\"refresh\"]")
            .with_evidence(content.chars().take(200).collect::<String>()),
        );
    }
    findings
}

/// `_check_form_action_external`.
fn external_form_actions(document: &Html, url: &str) -> Vec<Finding> {
    if url.is_empty() {
        return Vec::new();
    }
    let page_domain = netloc_of(url);
    let mut external = Vec::new();
    for element in elements(document, "form[action]") {
        let action = element.value().attr("action").unwrap_or_default().trim();
        if action.is_empty() || action.starts_with('#') {
            continue;
        }
        let action_domain = netloc_of(action);
        if !action_domain.is_empty() && action_domain != page_domain {
            external.push(action.to_owned());
        }
    }
    if external.is_empty() {
        return Vec::new();
    }
    let (list, extra) = sample_and_extra(&external);
    vec![
        sec(
            "sec-form-external-action",
            Severity::Medium,
            format!("{} form(s) submit to external domain(s)", external.len()),
            "One or more forms have action attributes pointing to external domains. This may indicate data exfiltration or phishing if unintended.",
            url,
        )
        .with_recommendation(
            "Verify that external form actions are intentional. Keep form submissions on the same origin where possible.",
        )
        .with_selector("form[action]")
        .with_evidence(format!("Actions: {list}{extra}")),
    ]
}

/// `_check_form_insecure`.
fn insecure_form_actions(document: &Html, url: &str) -> Vec<Finding> {
    let mut insecure = Vec::new();
    for element in elements(document, "form[action]") {
        let action = element.value().attr("action").unwrap_or_default().trim();
        if action.starts_with("http://") {
            insecure.push(action.to_owned());
        }
    }
    if insecure.is_empty() {
        return Vec::new();
    }
    let (list, extra) = sample_and_extra(&insecure);
    vec![
        sec(
            "sec-form-insecure",
            Severity::High,
            format!("{} form(s) submit over insecure HTTP", insecure.len()),
            "One or more forms have action attributes using http:// instead of https://. Form data sent over HTTP is transmitted in plain text and vulnerable to interception.",
            url,
        )
        .with_recommendation(
            "Change all form action URLs to use HTTPS to protect user data in transit.",
        )
        .with_selector("form[action^='http://']")
        .with_evidence(format!("Actions: {list}{extra}")),
    ]
}

// ---------------------------------------------------------------------------------------------
// URL helpers mirroring `urllib.parse.urlparse`
// ---------------------------------------------------------------------------------------------

/// `urlparse(url).scheme` — the token before `:` when it is a valid scheme, lowercased.
fn scheme_of(url: &str) -> String {
    let Some(colon) = url.find(':') else {
        return String::new();
    };
    let scheme = &url[..colon];
    let mut characters = scheme.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return String::new(),
    }
    if characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
    {
        scheme.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// `urlparse(url).netloc.lower()` — userinfo, host and port when the URL is absolute or
/// protocol-relative; empty for a relative reference.
fn netloc_of(url: &str) -> String {
    let after_scheme = if scheme_of(url).is_empty() {
        url
    } else {
        &url[url.find(':').unwrap_or_default() + 1..]
    };
    let Some(rest) = after_scheme.strip_prefix("//") else {
        return String::new();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..end].to_ascii_lowercase()
}

/// `urlparse(url).hostname.lower()` for the absolute HTTP(S) references the checks see.
fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .map(|host| host.to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::TlsEvidence;
    use crate::model::Document;

    fn evidence(url: &str, html: &str, headers: &[(&str, &str)], status: u16) -> PageEvidence {
        PageEvidence::new(
            Document::new(url, html),
            HttpEvidence {
                status,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
                final_url: url.to_owned(),
                redirects: Vec::new(),
                tls: None,
            },
        )
    }

    fn https_evidence(headers: &[(&str, &str)]) -> PageEvidence {
        evidence("https://example.test/page", "", headers, 200)
    }

    fn tls_page(protocol: &str, days_until_expiry: i64) -> PageEvidence {
        let mut page = evidence("https://example.test/", "", &[], 0);
        page.http.tls = Some(TlsEvidence {
            protocol: protocol.to_owned(),
            cipher_suite: "TLS_AES_256_GCM_SHA384".to_owned(),
            days_until_expiry,
        });
        page
    }

    fn check_ids(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(Finding::check_id).collect()
    }

    fn has(findings: &[Finding], check_id: &str) -> bool {
        findings
            .iter()
            .any(|finding| finding.check_id() == check_id)
    }

    fn find<'a>(findings: &'a [Finding], check_id: &str) -> &'a Finding {
        findings
            .iter()
            .find(|finding| finding.check_id() == check_id)
            .unwrap_or_else(|| panic!("no {check_id} in {:?}", check_ids(findings)))
    }

    #[test]
    fn an_empty_evidence_yields_no_findings() {
        // No URL and no response: nothing here is a fact about the page.
        assert!(scan(&PageEvidence::default()).is_empty());
    }

    #[test]
    fn a_clean_capture_yields_no_findings() {
        let html = r#"<script src="https://cdn.example.test/app.js" integrity="sha384-abc" crossorigin="anonymous"></script>
                <link rel="stylesheet" href="https://example.test/site.css">
                <form action="/submit"></form>"#;
        let headers = [
            (
                "content-security-policy",
                "default-src 'self'; report-uri /csp-report",
            ),
            (
                "strict-transport-security",
                "max-age=31536000; includeSubDomains",
            ),
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "strict-origin-when-cross-origin"),
            ("permissions-policy", "camera=()"),
            ("cross-origin-resource-policy", "same-origin"),
            ("cross-origin-opener-policy", "same-origin"),
            ("cross-origin-embedder-policy", "require-corp"),
            ("x-dns-prefetch-control", "off"),
            ("set-cookie", "session=abc; Secure; HttpOnly; SameSite=Lax"),
        ];
        let mut page = evidence("https://example.test/page", html, &headers, 200);
        page.http.tls = Some(TlsEvidence {
            protocol: "TLSv1.3".to_owned(),
            cipher_suite: "TLS_AES_256_GCM_SHA384".to_owned(),
            days_until_expiry: 90,
        });
        let findings = scan(&page);
        assert!(findings.is_empty(), "clean capture produced {findings:?}");
    }

    #[test]
    fn the_six_html_checks_fire() {
        let html = r#"
            <img src="http://cdn.example.test/a.png">
            <script src="https://cdn.other.test/lib.js"></script>
            <link rel="stylesheet" href="https://cdn.other.test/site.css">
            <script src="https://googletagmanager.com/gtm.js"></script>
            <script src="https://example.test/app.js"></script>
            <meta http-equiv="Refresh" content="0; URL=https://evil.test/">
            <form action="https://external.test/collect"></form>
            <form action="http://example.test/post"></form>
            <p>api_key = "abcdefghijklmnop1234"</p>
        "#;
        let page = evidence("https://example.test/page", html, &[], 0);
        let findings = scan(&page);

        assert_eq!(findings.len(), 6, "{:?}", check_ids(&findings));
        assert!(
            findings
                .iter()
                .all(|finding| finding.category == Category::Security
                    && finding.source_tool.as_deref() == Some("security_scanner"))
        );

        let mixed = find(&findings, "sec-mixed-content");
        assert_eq!(mixed.severity, Severity::High);
        assert_eq!(mixed.title, "Mixed content detected");
        assert_eq!(
            mixed.description,
            "Found 1 resource(s) loaded over HTTP on an HTTPS page: http://cdn.example.test/a.png"
        );

        let secrets = find(&findings, "sec-html-secret-exposed");
        assert_eq!(secrets.severity, Severity::Critical);
        assert_eq!(secrets.title, "Potential secrets exposed in HTML (1 found)");
        assert_eq!(
            secrets.evidence.as_deref(),
            Some("Redacted values: abcd...1234")
        );

        // The same-origin script and the dynamic-content host are both skipped by SRI.
        let sri = find(&findings, "sec-sri-missing");
        assert_eq!(sri.severity, Severity::Medium);
        assert_eq!(sri.title, "Missing SRI on 2 external resource(s)");
        assert_eq!(
            sri.evidence.as_deref(),
            Some("Resources: https://cdn.other.test/lib.js, https://cdn.other.test/site.css")
        );

        let meta = find(&findings, "sec-meta-refresh-redirect");
        assert_eq!(meta.severity, Severity::Medium);
        assert_eq!(
            meta.selector.as_deref(),
            Some("meta[http-equiv=\"refresh\"]")
        );
        assert_eq!(meta.evidence.as_deref(), Some("0; url=https://evil.test/"));

        let external = find(&findings, "sec-form-external-action");
        assert_eq!(external.title, "1 form(s) submit to external domain(s)");
        assert_eq!(
            external.evidence.as_deref(),
            Some("Actions: https://external.test/collect")
        );

        let insecure = find(&findings, "sec-form-insecure");
        assert_eq!(insecure.severity, Severity::High);
        assert_eq!(insecure.title, "1 form(s) submit over insecure HTTP");
        assert_eq!(
            insecure.evidence.as_deref(),
            Some("Actions: http://example.test/post")
        );
    }

    #[test]
    fn missing_headers_cookies_and_server_disclosure_fire() {
        let page = https_evidence(&[
            ("server", "nginx/1.25.3"),
            ("set-cookie", "session=abc; Path=/"),
        ]);
        let findings = scan(&page);

        assert_eq!(findings.len(), 12, "{:?}", check_ids(&findings));
        for check_id in [
            "sec-missing-csp",
            "sec-missing-hsts",
            "sec-missing-xfo",
            "sec-missing-xcto",
            "sec-missing-referrer-policy",
            "sec-missing-permissions-policy",
            "sec-cookie-flags",
            "sec-server-disclosure",
            "sec-missing-corp",
            "sec-missing-coop",
            "sec-missing-coep",
            "sec-dns-prefetch",
        ] {
            find(&findings, check_id);
        }
        assert_eq!(find(&findings, "sec-missing-hsts").severity, Severity::High);
        assert_eq!(
            find(&findings, "sec-missing-referrer-policy").severity,
            Severity::Low
        );
        assert_eq!(find(&findings, "sec-dns-prefetch").severity, Severity::Info);

        let cookie = find(&findings, "sec-cookie-flags");
        assert_eq!(cookie.title, "Cookie 'session' missing security flags");
        assert_eq!(
            cookie.description,
            "Cookie 'session' is missing: Secure, HttpOnly, SameSite."
        );
        assert_eq!(
            cookie.recommendation,
            "Set Secure, HttpOnly, SameSite flags on cookie 'session'."
        );

        assert_eq!(
            find(&findings, "sec-server-disclosure").description,
            "Server header reveals version info: 'nginx/1.25.3'."
        );
    }

    #[test]
    fn a_present_header_with_an_empty_value_is_not_missing() {
        let page = https_evidence(&[("content-security-policy", "")]);
        assert!(!has(&scan(&page), "sec-missing-csp"));
    }

    #[test]
    fn csp_weaknesses_fire() {
        let csp = "default-src * *; script-src 'unsafe-inline' 'unsafe-eval'";
        let page = https_evidence(&[("content-security-policy", csp)]);
        let findings = scan(&page);

        let inline = find(&findings, "sec-csp-unsafe-inline");
        assert_eq!(inline.severity, Severity::High);
        assert_eq!(inline.evidence.as_deref(), Some(csp));

        let eval = find(&findings, "sec-csp-unsafe-eval");
        assert_eq!(eval.severity, Severity::High);
        assert_eq!(eval.evidence.as_deref(), Some(csp));

        let wildcard = find(&findings, "sec-csp-wildcard");
        assert_eq!(wildcard.severity, Severity::Medium);
        assert_eq!(
            wildcard.description,
            "The CSP directive 'default-src' uses a wildcard (*), allowing resources from any origin."
        );
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.check_id() == "sec-csp-wildcard")
                .count(),
            1,
            "a wildcard is reported once"
        );

        assert_eq!(
            find(&findings, "sec-csp-no-reporting").severity,
            Severity::Low
        );
    }

    #[test]
    fn hsts_quality_fires() {
        let short = scan(&https_evidence(&[(
            "strict-transport-security",
            "max-age=300; includeSubDomains",
        )]));
        assert_eq!(
            find(&short, "sec-hsts-short-max-age").description,
            "HSTS max-age is 300 seconds (0 days). Recommended minimum is 31536000 (1 year)."
        );
        assert!(!has(&short, "sec-hsts-no-subdomains"));

        let no_subdomains = scan(&https_evidence(&[(
            "strict-transport-security",
            "max-age=31536000",
        )]));
        let finding = find(&no_subdomains, "sec-hsts-no-subdomains");
        assert_eq!(finding.severity, Severity::Low);
        assert!(!has(&no_subdomains, "sec-hsts-short-max-age"));
    }

    #[test]
    fn technology_headers_are_disclosed_once() {
        let page = https_evidence(&[
            ("x-powered-by", "PHP/8.1"),
            ("x-aspnet-version", "4.0.30319"),
            ("x-generator", "WordPress 6.4"),
        ]);
        let findings = scan(&page);
        let finding = find(&findings, "sec-tech-disclosure");
        assert_eq!(finding.severity, Severity::Low);
        assert_eq!(
            finding.evidence.as_deref(),
            Some("x-powered-by: PHP/8.1; x-aspnet-version: 4.0.30319; x-generator: WordPress 6.4")
        );
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.check_id() == "sec-tech-disclosure")
                .count(),
            1
        );
    }

    #[test]
    fn a_header_locked_static_host_gets_actionable_advice() {
        let page = https_evidence(&[("server", "GitHub.com")]);
        let findings = scan(&page);

        let hsts = find(&findings, "sec-missing-hsts");
        assert_eq!(
            hsts.severity,
            Severity::Medium,
            "HSTS is downgraded on a header-locked host"
        );
        assert!(hsts.description.ends_with(
            "The site is served by GitHub Pages, which does not allow custom response headers."
        ));
        assert_eq!(hsts.evidence.as_deref(), Some("server: GitHub.com"));
        assert!(hsts.recommendation.contains("Enforce HTTPS"));

        let csp = find(&findings, "sec-missing-csp");
        assert!(
            csp.recommendation
                .contains("<meta http-equiv=\"Content-Security-Policy\"")
        );

        let referrer = find(&findings, "sec-missing-referrer-policy");
        assert_eq!(
            referrer.recommendation,
            "Add <meta name=\"referrer\" content=\"strict-origin-when-cross-origin\"> to <head>."
        );

        let xfo = find(&findings, "sec-missing-xfo");
        assert!(
            xfo.recommendation
                .starts_with("Serve the site through a CDN or reverse proxy")
        );
        assert!(
            !has(&findings, "sec-server-disclosure"),
            "GitHub.com has no digit"
        );
    }

    #[test]
    fn tls_checks_fire() {
        let plain = evidence("http://example.test/", "", &[], 0);
        let plain_findings = scan(&plain);
        let not_https = find(&plain_findings, "sec-no-https");
        assert_eq!(not_https.severity, Severity::High);
        assert_eq!(not_https.title, "Not using HTTPS");
        assert!(!has(&plain_findings, "sec-tls-outdated"));

        let outdated = scan(&tls_page("TLSv1", 90));
        assert_eq!(
            find(&outdated, "sec-tls-outdated").title,
            "Outdated TLS version: TLSv1"
        );

        let expired = scan(&tls_page("TLSv1.3", -3));
        let expired_finding = find(&expired, "sec-tls-expired");
        assert_eq!(expired_finding.severity, Severity::Critical);
        assert_eq!(expired_finding.title, "TLS certificate expired");
        assert!(expired_finding.description.contains("3 days ago"));

        let soon = scan(&tls_page("TLSv1.3", 5));
        assert_eq!(
            find(&soon, "sec-tls-expiring-soon").severity,
            Severity::High
        );

        let expiring = scan(&tls_page("TLSv1.3", 14));
        let expiring_finding = find(&expiring, "sec-tls-expiring");
        assert_eq!(expiring_finding.severity, Severity::Medium);
        assert!(
            expiring_finding
                .description
                .ends_with("has not been renewed; automated renewal normally happens earlier.")
        );

        // The source's boundaries: 7 -> high, 8..=21 -> medium, 22+ -> no finding.
        assert!(has(&scan(&tls_page("TLSv1.3", 7)), "sec-tls-expiring-soon"));
        assert!(has(&scan(&tls_page("TLSv1.3", 8)), "sec-tls-expiring"));
        assert!(has(&scan(&tls_page("TLSv1.3", 21)), "sec-tls-expiring"));
        assert!(!has(&scan(&tls_page("TLSv1.3", 22)), "sec-tls-expiring"));

        let healthy = scan(&tls_page("TLSv1.3", 90));
        assert!(!has(&healthy, "sec-tls-expired"));
        assert!(!has(&healthy, "sec-tls-expiring"));
        assert!(!has(&healthy, "sec-tls-expiring-soon"));
        assert!(!has(&healthy, "sec-tls-outdated"));

        // An HTTPS page whose handshake was not captured is not reported as HTTP.
        let uncaptured = scan(&evidence("https://example.test/", "", &[], 0));
        assert!(
            uncaptured
                .iter()
                .all(|finding| !finding.check_id().starts_with("sec-tls-"))
        );
        assert!(!has(&uncaptured, "sec-no-https"));
    }

    #[test]
    fn output_is_deterministic() {
        let html = r#"<img src="http://cdn.example.test/a.png">
            <script src="https://cdn.other.test/lib.js"></script>
            <meta http-equiv="refresh" content="0; url=https://evil.test/">
            <form action="http://example.test/post"></form>"#;
        let page = evidence(
            "https://example.test/page",
            html,
            &[("content-security-policy", "default-src *")],
            200,
        );
        let first = serde_json::to_string(&scan(&page)).expect("findings serialise");
        let second = serde_json::to_string(&scan(&page)).expect("findings serialise");
        assert_eq!(first, second);
    }
}
