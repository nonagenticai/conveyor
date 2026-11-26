//! `analyse.app` — the application critique over **captured** page evidence.
//!
//! The source's `/api/critique` fetched the page itself and then ran five scanners over it. The
//! fetch, the live network probes and the LLM review are the caller's (the media executor and the
//! conveyor's inference router); this module owns the deterministic part: the accessibility,
//! performance and security checks over the bytes and origin facts the caller captured, plus the
//! source's category-weighted overall score.
//!
//! Nothing here reads a clock, the network or the environment. Two runs over the same
//! [`PageEvidence`] produce byte-identical reports.

pub mod a11y;
pub mod perf;
pub mod security;
pub mod ux;

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{Category, Document, Finding, Report, Scorecard, Severity};
use crate::rubric::{Rubric, dedupe, grade};

/// The TLS facts a capture observed. The source's TLS checks made their own handshake; the
/// library takes the result of one already made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsEvidence {
    /// Negotiated protocol version, e.g. `TLSv1.3`.
    pub protocol: String,
    /// Negotiated cipher suite name, e.g. `TLS_AES_256_GCM_SHA384`.
    pub cipher_suite: String,
    /// Whole days until the leaf certificate expires; negative when it already has.
    pub days_until_expiry: i64,
}

/// One redirect hop as the capture saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirect {
    pub status: u16,
    pub from: String,
    pub to: String,
}

/// The origin facts a capture observed for the page under analysis.
#[derive(Clone, Debug, Default)]
pub struct HttpEvidence {
    /// The final response's status.
    pub status: u16,
    /// Response headers, names lowercased. Repeated headers appear once per value.
    pub headers: Vec<(String, String)>,
    /// The URL the final response came from (after redirects).
    pub final_url: String,
    pub redirects: Vec<Redirect>,
    pub tls: Option<TlsEvidence>,
}

impl HttpEvidence {
    /// The value of a header by its lowercase name, joined with `", "` when repeated — the
    /// shape the source's `headers.get(...)` reads produced.
    pub fn header(&self, name: &str) -> Option<String> {
        let name = name.to_ascii_lowercase();
        let values: Vec<&str> = self
            .headers
            .iter()
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
            .collect();
        (!values.is_empty()).then(|| values.join(", "))
    }

    /// Whether a header is present at all, regardless of its value.
    pub fn has(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.headers.iter().any(|(key, _)| *key == name)
    }
}

/// Everything an app critique runs over: the captured document plus the origin facts.
#[derive(Clone, Debug, Default)]
pub struct PageEvidence {
    pub document: Document,
    pub http: HttpEvidence,
}

impl PageEvidence {
    pub fn new(document: Document, http: HttpEvidence) -> Self {
        Self { document, http }
    }
}

/// The source's category weights for the overall critique score (`ScoringSettings`).
///
/// The weights do not sum to 1; the source divided by the weights **present**, so a scan that
/// measured three categories weights those three. A complete scan that found nothing scores
/// `100.0`, never `0.0` — `overall_score_for_complete_scan` exists for exactly that reason.
fn category_weight(category: Category) -> f64 {
    match category {
        Category::Accessibility => 0.25,
        Category::Security => 0.25,
        Category::Performance => 0.20,
        Category::Seo => 0.15,
        Category::Ux => 0.15,
        Category::Cro => 0.15,
        _ => 0.10,
    }
}

/// Analyse one captured page: run the three deterministic scanner groups, deduplicate, score.
pub fn analyse(evidence: &PageEvidence) -> Report {
    analyse_with_ux(evidence, &[])
}

/// The same, plus the caller's LLM-authored UX claims through the source's deterministic guard.
///
/// The reviewer is a model; this crate owns only the guard that decides which of its claims are
/// issues, which are strengths, and which severity survives verification.
pub fn analyse_with_ux(evidence: &PageEvidence, claims: &[ux::UxClaim]) -> Report {
    let mut findings = Vec::new();
    findings.extend(a11y::scan(evidence));
    findings.extend(perf::scan(evidence));
    findings.extend(security::scan(evidence));
    findings.extend(ux::guard(&evidence.document, claims));
    let findings = dedupe(findings);
    let scorecard = scorecard(&findings);
    Report {
        url: evidence.document.url.clone(),
        findings,
        scorecard,
    }
}

/// Score a rendered artifact (the source's `/api/brand/score-artifact`): the SEO base scan, the
/// accessibility scanner and the performance scanner over the supplied HTML, with no origin facts.
///
/// The source's static artifact scorer ran exactly these three categories; the live-only checks
/// (headers, TLS, network probes) are not part of an artifact score and are not run here.
pub fn score_artifact(document: &Document) -> Report {
    let evidence = PageEvidence::new(document.clone(), HttpEvidence::default());
    let mut findings = crate::site::seo::scan(document);
    findings.extend(a11y::scan(&evidence));
    findings.extend(perf::scan(&evidence));
    let findings = dedupe(findings);
    let scorecard = scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}

/// The before/after delta of two rendered artifacts (the source's `/api/brand/score-delta`).
#[derive(Clone, Debug)]
pub struct ScoreDelta {
    pub before: Report,
    pub after: Report,
    /// Per-category `after - before`; a category absent on one side counts as `100.0` there —
    /// the same "clean until shown otherwise" rule the score itself uses.
    pub score_deltas: BTreeMap<String, f64>,
    pub overall_delta: f64,
    /// Check ids present only in the after report, sorted.
    pub new_findings: Vec<String>,
    /// Check ids present only in the before report, sorted.
    pub resolved_findings: Vec<String>,
    /// How many check ids appear on both sides.
    pub persistent_findings: usize,
}

/// Score both sides with the same static categories and diff them.
pub fn score_delta(before: &Document, after: &Document) -> ScoreDelta {
    let before_report = score_artifact(before);
    let after_report = score_artifact(after);
    let before_ids = check_ids(&before_report.findings);
    let after_ids = check_ids(&after_report.findings);
    let new_findings: Vec<String> = after_ids.difference(&before_ids).cloned().collect();
    let resolved_findings: Vec<String> = before_ids.difference(&after_ids).cloned().collect();
    let persistent_findings = before_ids.intersection(&after_ids).count();
    let categories: BTreeSet<String> = before_report
        .scorecard
        .buckets
        .keys()
        .chain(after_report.scorecard.buckets.keys())
        .cloned()
        .collect();
    let mut score_deltas = BTreeMap::new();
    for category in categories {
        let before_score = before_report
            .scorecard
            .buckets
            .get(&category)
            .copied()
            .unwrap_or(100.0);
        let after_score = after_report
            .scorecard
            .buckets
            .get(&category)
            .copied()
            .unwrap_or(100.0);
        score_deltas.insert(
            category,
            crate::rubric::round_1dp(after_score - before_score),
        );
    }
    ScoreDelta {
        overall_delta: crate::rubric::round_1dp(
            after_report.scorecard.overall_score - before_report.scorecard.overall_score,
        ),
        before: before_report,
        after: after_report,
        score_deltas,
        new_findings,
        resolved_findings,
        persistent_findings,
    }
}

fn check_ids(findings: &[Finding]) -> BTreeSet<String> {
    findings
        .iter()
        .filter(|finding| !finding.check_id().is_empty())
        .map(|finding| finding.check_id().to_owned())
        .collect()
}

/// The source's overall score: the decay score of every category that produced a finding,
/// weighted by that category's weight and divided by the weights present.
fn scorecard(findings: &[Finding]) -> Scorecard {
    let rubric = Rubric::default();
    let present: BTreeSet<Category> = findings.iter().map(|finding| finding.category).collect();
    let mut buckets = BTreeMap::new();
    let mut weighted_sum = 0.0;
    let mut total_weight = 0.0;
    for category in present {
        let score = rubric.category_score(findings, category);
        let weight = category_weight(category);
        buckets.insert(category.as_str().to_owned(), score);
        weighted_sum += score * weight;
        total_weight += weight;
    }
    let overall_score = if total_weight == 0.0 {
        // `overall_score_for_complete_scan`: a clean, complete scan is 100, not 0.
        100.0
    } else {
        crate::rubric::round_1dp(weighted_sum / total_weight)
    };
    Scorecard {
        overall_score,
        grade: grade(overall_score),
        buckets,
    }
}

/// One route the caller observed in the application's route table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RouteEvidence {
    /// The route path, e.g. `/portfolio`.
    pub path: String,
    /// Whether the application actually serves the route.
    #[serde(default)]
    pub served: bool,
}

/// The SPEC V2 §10.5 destinations: display name and the first path of the spec's route cell.
///
/// This is the coverage contract, not a live probe: the caller supplies the route table and the
/// captured pages, and the analysis says which destinations they do not cover.
pub const DESTINATIONS: [(&str, &str); 11] = [
    ("Organisation", "/organisation"),
    ("Procedures", "/procedures"),
    ("Authoring", "/authoring"),
    ("Portfolio", "/portfolio"),
    ("Meetings", "/meetings"),
    ("Programme", "/programme"),
    ("Architecture", "/design/adm"),
    ("Plan", "/plan"),
    ("Improve", "/improve"),
    ("Operations", "/operations"),
    ("Tools", "/tools"),
];

/// Measure the §10.5 destinations against a supplied route table and captured pages.
///
/// A destination is covered when a supplied route serves its path (or a path under it). When pages
/// are supplied, a served destination no page links to is reported as unreachable. Served routes
/// that are not a §10.5 destination are reported once, in path order. The report is a pure
/// function of its arguments; no clock, network or storage is read.
pub fn coverage(routes: &[RouteEvidence], pages: &[Document]) -> Report {
    let linked = linked_paths(pages);
    let mut findings = Vec::new();

    for (name, route) in DESTINATIONS {
        let matching: Vec<&RouteEvidence> = routes
            .iter()
            .filter(|evidence| route_matches(evidence, route))
            .collect();
        if matching.is_empty() {
            findings.push(coverage_finding(
                "app-destination-missing",
                Severity::High,
                format!("{name} has no route"),
                format!(
                    "The §10.5 destination {name} ({route}) is missing from the supplied route table."
                ),
                route.to_owned(),
            ));
        } else if !matching.iter().any(|evidence| evidence.served) {
            findings.push(coverage_finding(
                "app-destination-unserved",
                Severity::High,
                format!("{name} is not served"),
                format!(
                    "The §10.5 destination {name} ({route}) has a route row but the application does not serve it."
                ),
                route.to_owned(),
            ));
        } else if !pages.is_empty() && !linked.iter().any(|path| path_matches(path, route)) {
            findings.push(coverage_finding(
                "app-destination-unreachable",
                Severity::Info,
                format!("{name} is not linked from the captured pages"),
                format!(
                    "The §10.5 destination {name} ({route}) is served, but none of the supplied pages links to it."
                ),
                route.to_owned(),
            ));
        }
    }

    let mut unmapped: Vec<&str> = routes
        .iter()
        .filter(|evidence| evidence.served)
        .filter(|evidence| {
            !DESTINATIONS
                .iter()
                .any(|(_, route)| route_matches(evidence, route))
        })
        .map(|evidence| evidence.path.as_str())
        .collect();
    unmapped.sort_unstable();
    unmapped.dedup();
    if !unmapped.is_empty() {
        findings.push(coverage_finding(
            "app-route-unmapped",
            Severity::Info,
            "served routes outside the §10.5 destination table",
            "The supplied route table serves paths that no §10.5 destination claims.",
            unmapped.join(", "),
        ));
    }
    // No dedupe: each destination is a distinct item, and the rubric's dedupe keys on `check_id`,
    // so it would collapse two missing destinations into one finding.
    let scorecard = scorecard(&findings);
    Report {
        url: String::new(),
        findings,
        scorecard,
    }
}

fn coverage_finding(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    evidence: impl Into<String>,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Ux,
        severity,
        title: title.into(),
        description: description.into(),
        evidence: Some(evidence.into()),
        source_tool: Some("app_coverage".to_owned()),
        scanner_count: 1,
        ..Finding::default()
    }
}

fn normalise_path(path: &str) -> String {
    let path = path.trim();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    if path.len() > 1 {
        path.trim_end_matches('/').to_owned()
    } else {
        path.to_owned()
    }
}

fn path_matches(path: &str, destination: &str) -> bool {
    let path = normalise_path(path);
    path == destination || path.starts_with(&format!("{destination}/"))
}

fn route_matches(route: &RouteEvidence, destination: &str) -> bool {
    path_matches(&route.path, destination)
}

/// The path of every link the supplied pages carry, normalised and deduplicated.
///
/// A relative link without a leading `/` is skipped: without a base URL its target is unknowable,
/// and guessing one would invent coverage the pages do not prove.
fn linked_paths(pages: &[Document]) -> BTreeSet<String> {
    let selector = scraper::Selector::parse("a[href]").expect("a[href] is a valid selector");
    let mut paths = BTreeSet::new();
    for page in pages {
        let html = scraper::Html::parse_document(&page.html);
        for element in html.select(&selector) {
            let Some(href) = element.value().attr("href") else {
                continue;
            };
            let href = href.trim();
            let path = if href.starts_with("http://") || href.starts_with("https://") {
                match url::Url::parse(href) {
                    Ok(parsed) => parsed.path().to_owned(),
                    Err(_) => continue,
                }
            } else if href.starts_with('/') {
                href.to_owned()
            } else {
                continue;
            };
            paths.insert(normalise_path(&path));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Grade, Severity};

    fn finding(category: Category, severity: Severity) -> Finding {
        Finding {
            category,
            severity,
            title: "t".to_owned(),
            description: "d".to_owned(),
            ..Finding::default()
        }
    }

    #[test]
    fn a_clean_complete_scan_scores_one_hundred() {
        let report = scorecard(&[]);
        assert_eq!(report.overall_score, 100.0);
        assert_eq!(report.grade, Grade::APlus);
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn only_categories_with_findings_are_weighted() {
        // One critical accessibility finding and nothing else: accessibility is the only
        // category present, so its score IS the overall — the weights of absent categories must
        // not drag it toward 100 or 0.
        let report = scorecard(&[finding(Category::Accessibility, Severity::Critical)]);
        assert_eq!(report.buckets.len(), 1);
        assert_eq!(report.overall_score, report.buckets["accessibility"]);
    }

    #[test]
    fn present_categories_are_weighted_by_the_source_weights() {
        let findings = [
            finding(Category::Accessibility, Severity::Critical),
            finding(Category::Performance, Severity::Info),
        ];
        let report = scorecard(&findings);
        let rubric = Rubric::default();
        let a11y = rubric.category_score(&findings, Category::Accessibility);
        let perf = rubric.category_score(&findings, Category::Performance);
        let expected = crate::rubric::round_1dp((a11y * 0.25 + perf * 0.20) / 0.45);
        assert_eq!(report.overall_score, expected);
    }
}
