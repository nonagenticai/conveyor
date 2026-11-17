//! `analyse.report` — the deterministic report synthesis over a finding set.
//!
//! The source's report generator had one AI-written paragraph (the summary) and everything else
//! mechanical: the category scores, the weighted overall, the grade, and the Markdown document.
//! The summary is the caller's (a conveyor action supplies the LLM's text); this module owns the
//! mechanical part so two renderers cannot disagree about the same job's score.

use std::collections::BTreeMap;

use crate::model::{Category, Finding, Grade, Severity};
use crate::rubric::{Rubric, grade, round_1dp};

/// The mechanical summary of a finding set.
#[derive(Clone, Debug)]
pub struct Summary {
    /// One score per category that produced a finding, in the source's weighted-overall shape.
    pub scores: BTreeMap<String, f64>,
    /// The weighted overall over the categories present; `100.0` for a clean complete scan.
    pub overall_score: f64,
    pub grade: Grade,
}

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

/// Score every category that produced a finding and take the source's weighted mean.
pub fn summarise(findings: &[Finding]) -> Summary {
    let rubric = Rubric::default();
    let mut scores = BTreeMap::new();
    let mut weighted_sum = 0.0;
    let mut total_weight = 0.0;
    let mut present: Vec<Category> = findings.iter().map(|finding| finding.category).collect();
    present.sort();
    present.dedup();
    for category in present {
        let score = rubric.category_score(findings, category);
        let weight = category_weight(category);
        scores.insert(category.as_str().to_owned(), score);
        weighted_sum += score * weight;
        total_weight += weight;
    }
    let overall_score = if total_weight == 0.0 {
        100.0
    } else {
        round_1dp(weighted_sum / total_weight)
    };
    Summary {
        scores,
        overall_score,
        grade: grade(overall_score),
    }
}

/// The source's display name for a category (`code_quality` → `Code Quality`, `seo` → `SEO`).
pub fn display_category(category: &str) -> String {
    match category {
        "seo" => "SEO".to_owned(),
        "cro" => "CRO".to_owned(),
        "ux" => "UX".to_owned(),
        "code_quality" => "Code Quality".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Render the deterministic part of the source's `generate_markdown_report`.
///
/// `summary` is the caller's prose; everything around it is this crate's own arithmetic and the
/// source's fixed Markdown structure, so the same finding set renders the same document.
pub fn render_markdown(url: &str, findings: &[Finding], summary: &str) -> String {
    let scores = summarise(findings);
    let mut lines = vec![
        format!("# Website Critique Report: {url}"),
        String::new(),
        "## Summary".to_owned(),
        String::new(),
        summary.to_owned(),
        String::new(),
    ];
    if !scores.scores.is_empty() {
        lines.extend([
            "## Category Scores".to_owned(),
            String::new(),
            "| Category | Score |".to_owned(),
            "|----------|-------|".to_owned(),
        ]);
        for (category, score) in &scores.scores {
            lines.push(format!(
                "| {} | {} {score} |",
                display_category(category),
                grade(*score).as_str()
            ));
        }
        lines.extend([
            String::new(),
            format!("**Overall Score: {:.1}/100**", scores.overall_score),
            String::new(),
        ]);
    }
    if !findings.is_empty() {
        lines.extend(["## Findings".to_owned(), String::new()]);
        let mut by_category: BTreeMap<String, Vec<&Finding>> = BTreeMap::new();
        for finding in findings {
            by_category
                .entry(finding.category.as_str().to_owned())
                .or_default()
                .push(finding);
        }
        for (category, mut category_findings) in by_category {
            lines.push(format!(
                "### {} ({} findings)",
                display_category(&category),
                category_findings.len()
            ));
            lines.push(String::new());
            category_findings.sort_by_key(|finding| severity_order(finding.severity));
            for finding in category_findings {
                lines.extend([
                    format!(
                        "#### [{}] {}",
                        finding.severity.as_str().to_uppercase(),
                        finding.title
                    ),
                    String::new(),
                    finding.description.clone(),
                ]);
                if !finding.recommendation.is_empty() {
                    lines.extend([
                        String::new(),
                        format!("**Recommendation:** {}", finding.recommendation),
                    ]);
                }
                if !finding.url.is_empty() {
                    lines.extend([String::new(), format!("*URL: {}*", finding.url)]);
                }
                lines.push(String::new());
            }
        }
    }
    lines.extend([
        ("---").to_owned(),
        "*Generated by Critique Service*".to_owned(),
    ]);
    lines.join("\n")
}

fn severity_order(severity: Severity) -> u8 {
    severity.rank()
}

/// The deterministic diff between two finding sets (the source's `compare_critiques`): score
/// deltas are `after - before`, and findings are matched by their stable key.
#[derive(Clone, Debug)]
pub struct FindingDelta {
    pub score_deltas: BTreeMap<String, f64>,
    pub overall_delta: f64,
    /// Keys present only in the after set, sorted.
    pub new_findings: Vec<String>,
    /// Keys present only in the before set, sorted.
    pub resolved_findings: Vec<String>,
    pub persistent_findings: usize,
}

/// Compare two finding sets. A category absent on one side counts as `100.0` there.
pub fn compare(before: &[Finding], after: &[Finding]) -> FindingDelta {
    let before_summary = summarise(before);
    let after_summary = summarise(after);
    let before_keys = keys(before);
    let after_keys = keys(after);
    let new_findings: Vec<String> = after_keys.difference(&before_keys).cloned().collect();
    let resolved_findings: Vec<String> = before_keys.difference(&after_keys).cloned().collect();
    let persistent_findings = before_keys.intersection(&after_keys).count();
    let categories: std::collections::BTreeSet<String> = before_summary
        .scores
        .keys()
        .chain(after_summary.scores.keys())
        .cloned()
        .collect();
    let mut score_deltas = BTreeMap::new();
    for category in categories {
        let before_score = before_summary
            .scores
            .get(&category)
            .copied()
            .unwrap_or(100.0);
        let after_score = after_summary
            .scores
            .get(&category)
            .copied()
            .unwrap_or(100.0);
        score_deltas.insert(category, round_1dp(after_score - before_score));
    }
    FindingDelta {
        overall_delta: round_1dp(after_summary.overall_score - before_summary.overall_score),
        score_deltas,
        new_findings,
        resolved_findings,
        persistent_findings,
    }
}

fn keys(findings: &[Finding]) -> std::collections::BTreeSet<String> {
    findings
        .iter()
        .map(|finding| {
            let check_id = finding.check_id();
            if check_id.is_empty() {
                format!(
                    "{}|{}|{}",
                    finding.category.as_str(),
                    finding.title,
                    finding.url
                )
            } else {
                check_id.to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(category: Category, severity: Severity, title: &str) -> Finding {
        Finding {
            category,
            severity,
            title: title.to_owned(),
            description: "d".to_owned(),
            ..Finding::default()
        }
    }

    #[test]
    fn a_clean_scan_summarises_to_one_hundred() {
        let summary = summarise(&[]);
        assert_eq!(summary.overall_score, 100.0);
        assert_eq!(summary.grade, Grade::APlus);
        assert!(summary.scores.is_empty());
    }

    #[test]
    fn the_overall_is_the_weighted_mean_over_present_categories() {
        let findings = [
            finding(Category::Accessibility, Severity::Critical, "a"),
            finding(Category::Seo, Severity::Info, "b"),
        ];
        let summary = summarise(&findings);
        let rubric = Rubric::default();
        let a11y = rubric.category_score(&findings, Category::Accessibility);
        let seo = rubric.category_score(&findings, Category::Seo);
        assert_eq!(
            summary.overall_score,
            round_1dp((a11y * 0.25 + seo * 0.15) / 0.40)
        );
    }

    #[test]
    fn the_markdown_groups_findings_by_category_and_sorts_by_severity() {
        let findings = [
            finding(Category::Performance, Severity::Low, "slow"),
            finding(Category::Performance, Severity::Critical, "blocking"),
        ];
        let markdown = render_markdown("https://example.test", &findings, "Note.");
        let blocking = markdown
            .find("blocking")
            .expect("the critical finding renders");
        let slow = markdown.find("slow").expect("the low finding renders");
        assert!(blocking < slow, "critical sorts before low");
        assert!(markdown.contains("### Performance (2 findings)"));
        assert!(markdown.contains("## Category Scores"));
    }

    #[test]
    fn an_empty_report_has_no_findings_section() {
        let markdown = render_markdown("https://example.test", &[], "Clean.");
        assert!(!markdown.contains("## Findings"));
        assert!(markdown.contains("## Summary"));
    }

    #[test]
    fn comparing_identical_sets_yields_no_change() {
        let findings = [finding(Category::Accessibility, Severity::High, "a")];
        let delta = compare(&findings, &findings);
        assert_eq!(delta.overall_delta, 0.0);
        assert_eq!(delta.persistent_findings, 1);
        assert!(delta.new_findings.is_empty());
        assert!(delta.resolved_findings.is_empty());
    }

    #[test]
    fn a_new_finding_is_named_and_lowers_the_overall() {
        let before = [finding(Category::Accessibility, Severity::High, "a")];
        let mut added = finding(Category::Accessibility, Severity::Critical, "b");
        added.check_id = Some("a11y-added".to_owned());
        let after = [before[0].clone(), added];
        let delta = compare(&before, &after);
        assert_eq!(delta.new_findings, vec!["a11y-added".to_owned()]);
        assert!(delta.resolved_findings.is_empty());
        assert!(
            delta.overall_delta < 0.0,
            "one more severe finding cannot raise the score"
        );
    }
}
