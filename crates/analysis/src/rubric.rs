//! The one scorecard kernel: per-bucket penalty decay, weighted mean, grading and dedupe.
//!
//! The source service implemented this loop three times (CRO, SEO, marketing) before extracting
//! it, and the extraction is the reason the three agree. This module is that extraction and
//! nothing else: a family is a `{weights, bucket_of}` configuration, never another copy of the
//! arithmetic.
//!
//! # The law
//!
//! For one bucket, with severity weights `30 / 15 / 7 / 3 / 0` and decay constant `k = 25`:
//!
//! ```text
//! score = floor + (100 - floor) · e^(−penalty / k)      floored at `score_floor` (50.0)
//! ```
//!
//! Rounded to one decimal, **half to even**, exactly as Python's `round(x, 1)` does. An empty
//! bucket scores `100.0` (nothing wrong), and a bucket still appears in the breakdown when it
//! has no findings. The overall score is the reference-weighted mean of the bucket scores.

use std::collections::BTreeMap;

use crate::model::{Category, Finding, Grade, Scorecard};

/// Cross-scanner equivalence: when two scanners flag the same underlying issue under different
/// `check_id`s, the right-hand id normalises to the left-hand one before dedupe.
pub const EQUIVALENT_CHECKS: &[(&str, &str)] = &[
    ("seo-images-missing-alt", "a11y-img-missing-alt"),
    ("seo-viewport-missing", "perf-missing-viewport"),
    ("seo-heading-hierarchy-skip", "a11y-heading-skip"),
];

/// Map an equivalent `check_id` to its canonical form.
pub fn normalize_check_id(check_id: &str) -> &str {
    for (left, right) in EQUIVALENT_CHECKS {
        if *left == check_id {
            return right;
        }
    }
    check_id
}

/// Score → letter grade. Ordered high to low; the first threshold met or exceeded wins.
pub const GRADE_THRESHOLDS: &[(f64, Grade)] = &[
    (95.0, Grade::APlus),
    (90.0, Grade::A),
    (80.0, Grade::B),
    (70.0, Grade::C),
    (60.0, Grade::D),
    (0.0, Grade::F),
];

pub fn grade(score: f64) -> Grade {
    for (threshold, grade) in GRADE_THRESHOLDS {
        if score >= *threshold {
            return *grade;
        }
    }
    Grade::F
}

/// Round to one decimal, ties to even — Python's `round(x, 1)`.
///
/// `f64::round` rounds ties *away from zero*, so it disagrees with the source on every boundary
/// value and would move a golden score. Rust's `{:.1}` formatting is a correctly-rounded decimal
/// conversion of the exact binary value (Grisu/Dragon), which is what `round()` does too; this
/// helper exists so no call site has to know that.
pub fn round_1dp(value: f64) -> f64 {
    format!("{value:.1}")
        .parse()
        .expect("a one-decimal rendering of a finite f64 parses back")
}

/// The severity penalty table and the two constants of the decay law.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeverityWeights {
    pub critical: i32,
    pub high: i32,
    pub medium: i32,
    pub low: i32,
    pub info: i32,
}

impl Default for SeverityWeights {
    fn default() -> Self {
        Self {
            critical: 30,
            high: 15,
            medium: 7,
            low: 3,
            info: 0,
        }
    }
}

impl SeverityWeights {
    pub fn weight(&self, severity: crate::model::Severity) -> i32 {
        use crate::model::Severity::*;
        match severity {
            Critical => self.critical,
            High => self.high,
            Medium => self.medium,
            Low => self.low,
            Info => self.info,
        }
    }
}

/// The tuning of one scorecard. [`Default`] is the source service's configuration, so a caller
/// that wants parity passes nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rubric {
    pub severity: SeverityWeights,
    pub decay_k: f64,
    pub score_floor: f64,
}

impl Default for Rubric {
    fn default() -> Self {
        Self {
            severity: SeverityWeights::default(),
            decay_k: 25.0,
            score_floor: 50.0,
        }
    }
}

impl Rubric {
    /// Score one category: the decay law over just that category's findings.
    ///
    /// An empty category scores `100.0` — the source's rule, and the reason a score never claims
    /// a problem it did not find.
    pub fn category_score(&self, findings: &[Finding], category: Category) -> f64 {
        let matching: Vec<&Finding> = findings.iter().filter(|f| f.category == category).collect();
        if matching.is_empty() {
            return 100.0;
        }
        self.score_slice(&matching)
    }

    fn score_slice(&self, findings: &[&Finding]) -> f64 {
        if findings.is_empty() {
            return 100.0;
        }
        let penalty: i32 = findings
            .iter()
            .map(|f| self.severity.weight(f.severity))
            .sum();
        self.score_for_penalty(penalty)
    }

    /// The decay law alone: one penalty in, one score out.
    ///
    /// Public because it is the parity surface — the fixture in `tests/fixtures/rubric_parity.json`
    /// is the absorbed service's own output for every penalty a finding set can reach, and a test
    /// compares this function against it directly rather than through a finding set that cannot
    /// express every integer.
    pub fn score_for_penalty(&self, penalty: i32) -> f64 {
        let score = self.score_floor
            + (100.0 - self.score_floor) * (-f64::from(penalty) / self.decay_k).exp();
        round_1dp(score).max(self.score_floor)
    }

    /// Decay-score each weighted bucket over just that bucket's findings.
    ///
    /// Every bucket in `weights` appears in the result, including one with no findings (which
    /// scores `100.0`). A finding whose bucket the resolver does not recognise contributes to
    /// its own bucket only if that bucket is in `weights`; a stray finding never invents a bucket,
    /// which is what keeps the breakdown's shape fixed by the rubric rather than by the page.
    pub fn bucket_scores<F>(
        &self,
        findings: &[Finding],
        category: Category,
        weights: &BTreeMap<String, f64>,
        bucket_of: F,
    ) -> BTreeMap<String, f64>
    where
        F: Fn(&str) -> String,
    {
        let in_category: Vec<&Finding> =
            findings.iter().filter(|f| f.category == category).collect();
        let mut scores = BTreeMap::new();
        for bucket in weights.keys() {
            let slice: Vec<&Finding> = in_category
                .iter()
                .copied()
                .filter(|f| &bucket_of(f.check_id()) == bucket)
                .collect();
            scores.insert(bucket.clone(), self.score_slice(&slice));
        }
        scores
    }

    /// The reference-weighted mean of the bucket scores, rounded to one decimal.
    pub fn weighted_overall(
        &self,
        buckets: &BTreeMap<String, f64>,
        weights: &BTreeMap<String, f64>,
    ) -> f64 {
        let total_weight: f64 = weights.values().sum();
        if total_weight == 0.0 {
            return 0.0;
        }
        let weighted: f64 = weights
            .iter()
            .map(|(bucket, weight)| buckets.get(bucket).copied().unwrap_or(100.0) * weight)
            .sum();
        round_1dp(weighted / total_weight)
    }

    /// A full scorecard: bucket scores, the weighted overall, and its grade.
    pub fn scorecard<F>(
        &self,
        findings: &[Finding],
        category: Category,
        weights: &BTreeMap<String, f64>,
        bucket_of: F,
    ) -> Scorecard
    where
        F: Fn(&str) -> String,
    {
        let buckets = self.bucket_scores(findings, category, weights, bucket_of);
        let overall_score = self.weighted_overall(&buckets, weights);
        Scorecard {
            overall_score,
            grade: grade(overall_score),
            buckets,
        }
    }
}

/// Deduplicate findings by `check_id` (normalised through [`EQUIVALENT_CHECKS`]), falling back to
/// `category | title | url` for a finding with no id.
///
/// The **more severe** finding survives; the survivors keep first-seen order; each survivor
/// records how many distinct scanners confirmed it and which ones, sorted.
pub fn dedupe(findings: Vec<Finding>) -> Vec<Finding> {
    let mut index: BTreeMap<String, (Finding, Vec<String>)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for finding in findings {
        let key = match &finding.check_id {
            Some(id) if !id.is_empty() => normalize_check_id(id).to_owned(),
            _ => format!(
                "{}|{}|{}",
                finding.category.as_str(),
                finding.title,
                finding.url
            ),
        };
        let source = finding
            .source_tool
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());

        match index.get_mut(&key) {
            Some((existing, sources)) => {
                if !sources.contains(&source) {
                    sources.push(source.clone());
                }
                if finding.severity.rank() < existing.severity.rank() {
                    *existing = finding;
                }
            }
            None => {
                order.push(key.clone());
                index.insert(key, (finding, vec![source]));
            }
        }
    }

    order
        .into_iter()
        .map(|key| {
            let (mut finding, mut sources) = index.remove(&key).expect("key was inserted above");
            sources.sort();
            finding.scanner_count = sources.len();
            finding.confirmed_by = sources;
            finding
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Severity;

    fn finding(severity: Severity) -> Finding {
        Finding::seo("seo-test", severity, "t", "d")
    }

    #[test]
    fn an_empty_category_scores_one_hundred() {
        let rubric = Rubric::default();
        assert_eq!(rubric.category_score(&[], Category::Seo), 100.0);
    }

    #[test]
    fn the_penalty_decays_exponentially() {
        let rubric = Rubric::default();
        // one critical = 30 penalty: 50 + 50·e^(−30/25) = 65.0597… → 65.1.
        // These expected values are Python's, not this crate's: see the parity test below.
        let score = rubric.category_score(&[finding(Severity::Critical)], Category::Seo);
        assert_eq!(score, 65.1);
        // one info = 0 penalty: the full 100 is lost to nothing
        let score = rubric.category_score(&[finding(Severity::Info)], Category::Seo);
        assert_eq!(score, 100.0);
        // two highs = 30 penalty, the same as one critical
        let score = rubric.category_score(
            &[finding(Severity::High), finding(Severity::High)],
            Category::Seo,
        );
        assert_eq!(score, 65.1);
        // high + medium = 22 penalty → 70.7
        let score = rubric.category_score(
            &[finding(Severity::High), finding(Severity::Medium)],
            Category::Seo,
        );
        assert_eq!(score, 70.7);
        // low = 3 penalty → 94.3
        let score = rubric.category_score(&[finding(Severity::Low)], Category::Seo);
        assert_eq!(score, 94.3);
    }

    #[test]
    fn a_score_never_falls_below_the_floor() {
        let rubric = Rubric::default();
        let many: Vec<Finding> = (0..20).map(|_| finding(Severity::Critical)).collect();
        assert_eq!(rubric.category_score(&many, Category::Seo), 50.0);
    }

    #[test]
    fn one_decimal_rounding_is_half_to_even() {
        // Python: round(0.15, 1) == 0.1, round(0.25, 1) == 0.2 — the binary value decides.
        // f64::round would give 0.2 and 0.3 if it were applied to the tenths.
        assert_eq!(round_1dp(0.15), 0.1);
        assert_eq!(round_1dp(0.25), 0.2);
        assert_eq!(round_1dp(0.35), 0.3);
        assert_eq!(round_1dp(2.5), 2.5);
        assert_eq!(round_1dp(87.3449), 87.3);
        assert_eq!(round_1dp(87.35), 87.3);
        assert_eq!(round_1dp(87.36), 87.4);
    }

    #[test]
    fn grades_follow_the_threshold_table() {
        assert_eq!(grade(100.0), Grade::APlus);
        assert_eq!(grade(95.0), Grade::APlus);
        assert_eq!(grade(94.9), Grade::A);
        assert_eq!(grade(90.0), Grade::A);
        assert_eq!(grade(89.9), Grade::B);
        assert_eq!(grade(80.0), Grade::B);
        assert_eq!(grade(70.0), Grade::C);
        assert_eq!(grade(60.0), Grade::D);
        assert_eq!(grade(59.9), Grade::F);
        assert_eq!(grade(0.0), Grade::F);
    }

    #[test]
    fn an_empty_bucket_scores_one_hundred_and_stays_in_the_breakdown() {
        let rubric = Rubric::default();
        let weights: BTreeMap<String, f64> =
            [("on_page".to_owned(), 1.0), ("technical".to_owned(), 1.0)]
                .into_iter()
                .collect();
        let card = rubric.scorecard(
            &[Finding::seo("seo-missing-title", Severity::High, "t", "d")],
            Category::Seo,
            &weights,
            |check_id| {
                if check_id.starts_with("seo-missing-title") {
                    "on_page".to_owned()
                } else {
                    "technical".to_owned()
                }
            },
        );
        assert_eq!(card.buckets.len(), 2);
        assert_eq!(card.buckets["technical"], 100.0);
        assert!(card.buckets["on_page"] < 100.0);
        // the mean of the two buckets, rounded once
        let expected = round_1dp((card.buckets["on_page"] + 100.0) / 2.0);
        assert_eq!(card.overall_score, expected);
        assert_eq!(card.grade, grade(expected));
    }

    #[test]
    fn dedupe_keeps_the_more_severe_finding_and_counts_scanners() {
        let mut viewport = Finding::seo(
            "seo-viewport-missing",
            Severity::Low,
            "missing viewport",
            "d",
        );
        let mut perf = Finding::seo(
            "perf-missing-viewport",
            Severity::High,
            "missing viewport",
            "d",
        );
        perf.source_tool = Some("perf_scanner".to_owned());
        viewport.source_tool = Some("seo_scanner".to_owned());

        let unique = dedupe(vec![viewport, perf]);
        assert_eq!(
            unique.len(),
            1,
            "the equivalence map makes these one finding"
        );
        assert_eq!(
            unique[0].severity,
            Severity::High,
            "the higher severity survives"
        );
        assert_eq!(unique[0].scanner_count, 2);
        assert_eq!(unique[0].confirmed_by, vec!["perf_scanner", "seo_scanner"]);
        assert_eq!(unique[0].check_id(), "perf-missing-viewport");
    }

    #[test]
    fn dedupe_without_a_check_id_falls_back_to_category_title_url() {
        let mut first = Finding::seo("", Severity::Low, "t", "d");
        first.check_id = None;
        first.url = "https://example.test/".to_owned();
        let mut second = first.clone();
        second.severity = Severity::Critical;

        let unique = dedupe(vec![first, second]);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].severity, Severity::Critical);
    }
}
