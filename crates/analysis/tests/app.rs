//! P5.1 — a captured page-evidence bundle analysed by `analyse.app` returns the accessibility,
//! performance and security findings the source's deterministic scanners produced, and the overall
//! score is the source's category-weighted mean.

use conveyor_analysis::app::ux::UxClaim;
use conveyor_analysis::app::{HttpEvidence, PageEvidence, TlsEvidence, analyse, analyse_with_ux};
use conveyor_analysis::model::{Category, Document, Severity};

fn bundle() -> PageEvidence {
    let html = r#"<html>
  <head><title>Acme</title></head>
  <body>
    <img src="/hero.png">
    <form action="http://insecure.test/submit" method="post">
      <input type="text" name="email">
      <button></button>
    </form>
    <script src="http://insecure.test/app.js"></script>
    <div style="color:#aaa;background:#fff">low contrast text</div>
  </body>
</html>"#;
    let http = HttpEvidence {
        status: 200,
        headers: vec![
            ("content-type".to_owned(), "text/html".to_owned()),
            ("server".to_owned(), "nginx/1.18.0".to_owned()),
        ],
        final_url: "https://acme.test/".to_owned(),
        redirects: Vec::new(),
        tls: Some(TlsEvidence {
            protocol: "TLSv1.1".to_owned(),
            cipher_suite: "TLS_RSA_WITH_AES_128_CBC_SHA".to_owned(),
            days_until_expiry: 3,
        }),
    };
    PageEvidence::new(Document::new("https://acme.test/", html), http)
}

#[test]
fn a_captured_bundle_produces_findings_in_each_deterministic_family() {
    let report = analyse(&bundle());
    let categories: Vec<Category> = report
        .findings
        .iter()
        .map(|finding| finding.category)
        .collect();
    assert!(
        categories.contains(&Category::Accessibility),
        "the a11y scanner must report on a page with no lang and a nameless image"
    );
    assert!(
        categories.contains(&Category::Performance),
        "the perf scanner must report on a page with no viewport and a render-blocking script"
    );
    assert!(
        categories.contains(&Category::Security),
        "the security scanner must report on mixed content and missing headers"
    );
    assert!(
        !report.findings.is_empty(),
        "an insecure, unlabelled page cannot produce a clean report"
    );
}

#[test]
fn the_report_is_a_pure_function_of_the_bundle() {
    let first = analyse(&bundle());
    let second = analyse(&bundle());
    assert_eq!(
        serde_json::to_string(&first).expect("the report serialises"),
        serde_json::to_string(&second).expect("the report serialises"),
        "two runs over the same evidence must be byte-identical"
    );
}

#[test]
fn the_overall_is_the_weighted_mean_of_the_categories_present() {
    let report = analyse(&bundle());
    let mut weighted = 0.0;
    let mut weights = 0.0;
    for (category, score) in &report.scorecard.buckets {
        let weight = match category.as_str() {
            "accessibility" => 0.25,
            "security" => 0.25,
            "performance" => 0.20,
            "seo" => 0.15,
            "ux" => 0.15,
            "cro" => 0.15,
            _ => 0.10,
        };
        weighted += score * weight;
        weights += weight;
    }
    assert!(weights > 0.0);
    assert_eq!(
        report.scorecard.overall_score,
        format!("{:.1}", weighted / weights)
            .parse::<f64>()
            .expect("a one-decimal rendering parses")
    );
}

#[test]
fn a_clean_bundle_scores_one_hundred() {
    let html = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Clean</title>
  <link rel="preconnect" href="https://cdn.example.test">
  <link rel="preload" href="/font.woff2" as="font" crossorigin>
  <link rel="dns-prefetch" href="//cdn.example.test">
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'; report-uri /csp">
</head>
<body>
  <a href="#main">Skip to content</a>
  <header><nav><a href="/">Home</a></nav></header>
  <main id="main"><h1>Clean</h1><p>Fine.</p><div aria-live="polite">Ready</div></main>
  <footer>Acme</footer>
</body>
</html>"##;
    let evidence = PageEvidence::new(
        Document::new("https://clean.test/", html),
        HttpEvidence {
            status: 200,
            headers: vec![
                (
                    "content-security-policy".to_owned(),
                    "default-src 'self'; report-uri /csp".to_owned(),
                ),
                (
                    "cross-origin-opener-policy".to_owned(),
                    "same-origin".to_owned(),
                ),
                (
                    "cross-origin-embedder-policy".to_owned(),
                    "require-corp".to_owned(),
                ),
                (
                    "cross-origin-resource-policy".to_owned(),
                    "same-origin".to_owned(),
                ),
                (
                    "strict-transport-security".to_owned(),
                    "max-age=31536000; includeSubDomains".to_owned(),
                ),
                ("x-frame-options".to_owned(), "DENY".to_owned()),
                ("x-content-type-options".to_owned(), "nosniff".to_owned()),
                (
                    "referrer-policy".to_owned(),
                    "strict-origin-when-cross-origin".to_owned(),
                ),
                ("permissions-policy".to_owned(), "camera=()".to_owned()),
                ("x-dns-prefetch-control".to_owned(), "off".to_owned()),
            ],
            final_url: "https://clean.test/".to_owned(),
            redirects: Vec::new(),
            tls: Some(TlsEvidence {
                protocol: "TLSv1.3".to_owned(),
                cipher_suite: "TLS_AES_256_GCM_SHA384".to_owned(),
                days_until_expiry: 300,
            }),
        },
    );
    let report = analyse(&evidence);
    assert!(
        report.findings.is_empty(),
        "the clean fixture must produce no finding; got {:?}",
        report
            .findings
            .iter()
            .map(|finding| finding.check_id())
            .collect::<Vec<_>>()
    );
    assert_eq!(report.scorecard.overall_score, 100.0);
}

fn claim(
    kind: &str,
    severity: Severity,
    title: &str,
    claim_type: &str,
    scope: &str,
    evidence: Option<&str>,
) -> UxClaim {
    UxClaim {
        kind: kind.to_owned(),
        severity,
        title: title.to_owned(),
        description: "d".to_owned(),
        recommendation: "r".to_owned(),
        claim_type: claim_type.to_owned(),
        scope: scope.to_owned(),
        evidence: evidence.map(str::to_owned),
        selector: None,
    }
}

fn evidence_with(html: &str) -> PageEvidence {
    PageEvidence::new(
        Document::new("https://acme.test/about", html),
        HttpEvidence::default(),
    )
}

#[test]
fn a_contradicted_ux_claim_is_dropped_not_scored() {
    let evidence = evidence_with(
        r##"<html lang="en"><body><a href="mailto:hello@acme.test">Email us</a></body></html>"##,
    );
    let claims = [claim(
        "issue",
        Severity::Critical,
        "No way to reach the business",
        "missing_contact_method",
        "page",
        None,
    )];
    let report = analyse_with_ux(&evidence, &claims);
    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.category != Category::Ux),
        "a mailto link disproves the claim; got {:?}",
        report
            .findings
            .iter()
            .map(|finding| finding.check_id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_unverified_critical_claim_is_demoted_to_high() {
    let evidence = evidence_with(r#"<html lang="en"><body><h1>Page</h1></body></html>"#);
    let claims = [claim(
        "issue",
        Severity::Critical,
        "Something vague",
        "other",
        "page",
        None,
    )];
    let report = analyse_with_ux(&evidence, &claims);
    let ux: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.category == Category::Ux)
        .collect();
    assert_eq!(ux.len(), 1, "the unverified claim is kept as an issue");
    assert_eq!(ux[0].severity, Severity::High, "critical becomes high");
}

#[test]
fn a_strength_is_not_an_issue() {
    let evidence = evidence_with(r#"<html lang="en"><body><h1>Page</h1></body></html>"#);
    let claims = [claim(
        "strength",
        Severity::Low,
        "Clear heading",
        "other",
        "page",
        None,
    )];
    let report = analyse_with_ux(&evidence, &claims);
    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.category != Category::Ux)
    );
}
