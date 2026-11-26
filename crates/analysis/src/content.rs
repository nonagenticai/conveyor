//! `analyse.content` — content-shaped analysis: the conversion (CRO) and marketing scanners over a
//! supplied document, scored by the shared rubric kernel.
//!
//! The source exposed these as `POST /api/cro/score` and `POST /api/marketing/scorecard`; the
//! deterministic scanner cores are here, and the optional LLM judge is the caller's (a conveyor
//! action supplies its result). No transport, no clock.

pub mod cro;
pub mod marketing;
pub mod quality;

use crate::model::{Category, Document, Finding, Report, Scorecard};
use crate::rubric::{Rubric, dedupe, grade, round_1dp};

/// Analyse one document for content-shaped quality: the CRO and marketing scanners.
pub fn analyse(document: &Document) -> Report {
    let mut findings = Vec::new();
    findings.extend(cro::scan(document));
    findings.extend(marketing::scan(document));
    let findings = dedupe(findings);
    let scorecard = scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}

/// The same, plus the heading, source and filler checks.
///
/// The quality checks join the conversion and marketing findings rather than replacing them: a
/// gate that cites `analyse.content` sees the whole content picture in one report.
pub fn analyse_quality(document: &Document) -> Report {
    let mut findings = analyse(document).findings;
    findings.extend(quality::scan(document));
    // The two sets have disjoint check ids, and each quality finding is a distinct item; dedupe
    // would collapse several filler phrases into one.
    let scorecard = scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}

/// The content scorecard: each present category scored by the decay law and weighted equally.
fn scorecard(findings: &[Finding]) -> Scorecard {
    let rubric = Rubric::default();
    let mut buckets = std::collections::BTreeMap::new();
    let mut weighted_sum = 0.0;
    let mut total_weight = 0.0;
    for category in [Category::Cro, Category::Marketing, Category::Content] {
        if findings.iter().any(|finding| finding.category == category) {
            let score = rubric.category_score(findings, category);
            buckets.insert(category.as_str().to_owned(), score);
            weighted_sum += score;
            total_weight += 1.0;
        }
    }
    let overall_score = if total_weight == 0.0 {
        100.0
    } else {
        round_1dp(weighted_sum / total_weight)
    };
    Scorecard {
        overall_score,
        grade: grade(overall_score),
        buckets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Grade;

    #[test]
    fn a_clean_content_scan_scores_one_hundred() {
        let report = scorecard(&[]);
        assert_eq!(report.overall_score, 100.0);
        assert_eq!(report.grade, Grade::APlus);
        assert!(report.buckets.is_empty());
    }
}
